use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

/// Partial input belongs to the connection, so cancelling a read loses no bytes.
pub(super) struct Requests<R> {
    reader: BufReader<R>,
    partial: Vec<u8>,
    pending: std::collections::VecDeque<String>,
    bytes: usize,
    pub(super) closed: bool,
}
impl<R: AsyncRead + Unpin> Requests<R> {
    const LIMIT: usize = omega_proto::MAX_FRAME_LEN;

    pub(super) fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            partial: Vec::new(),
            pending: Default::default(),
            bytes: 0,
            closed: false,
        }
    }

    pub(super) async fn next_line(&mut self) -> io::Result<Option<String>> {
        if let Some(line) = self.pending.pop_front() {
            self.bytes -= line.len();
            return Ok(Some(line));
        }
        self.read_line().await
    }

    pub(super) async fn buffer_next(&mut self) -> io::Result<()> {
        if let Some(line) = self.read_line().await? {
            if self.pending.len() >= 32 || self.bytes + line.len() > 8 * 1024 * 1024 {
                return Err(io::Error::other(
                    "observation receive capacity exhausted while writing",
                ));
            }
            self.bytes += line.len();
            self.pending.push_back(line);
        }
        Ok(())
    }

    async fn read_line(&mut self) -> io::Result<Option<String>> {
        if self.closed {
            return Ok(None);
        }
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                self.closed = true;
                return if self.partial.is_empty() {
                    Ok(None)
                } else {
                    self.finish().map(Some)
                };
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let length = newline.unwrap_or(available.len());
            if length > Self::LIMIT - self.partial.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "observation request exceeds line limit",
                ));
            }
            self.partial.extend_from_slice(&available[..length]);
            self.reader.consume(length + usize::from(newline.is_some()));
            if newline.is_some() {
                return self.finish().map(Some);
            }
        }
    }

    fn finish(&mut self) -> io::Result<String> {
        let mut bytes = std::mem::take(&mut self.partial);
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn buffered_lines_are_ordered_and_bounded_even_when_empty() {
        let input = "\n".repeat(33);
        let mut requests = Requests::new(input.as_bytes());
        for _ in 0..32 {
            requests.buffer_next().await.unwrap();
        }
        assert!(requests.buffer_next().await.is_err());
        assert_eq!(requests.pending.len(), 32);
        for _ in 0..32 {
            assert_eq!(requests.next_line().await.unwrap().unwrap(), "");
        }
    }

    #[tokio::test]
    async fn buffered_line_bytes_have_an_independent_limit() {
        let input = ("x".repeat(3_000_000) + "\n").repeat(3);
        let mut requests = Requests::new(input.as_bytes());
        requests.buffer_next().await.unwrap();
        requests.buffer_next().await.unwrap();
        assert!(requests.buffer_next().await.is_err());
        assert_eq!(requests.bytes, 6_000_000);
    }

    #[tokio::test(start_paused = true)]
    async fn interrupted_partial_requests_preserve_bytes_and_split_utf8() {
        let (mut writer, reader) = tokio::io::duplex(64);
        let mut requests = Requests::new(reader);
        writer.write_all(b"{\"message\":\"\xc3").await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), requests.next_line())
                .await
                .is_err()
        );
        writer.write_all(b"\xa9\"}\r\nsecond\n").await.unwrap();
        assert_eq!(
            requests.next_line().await.unwrap().unwrap(),
            "{\"message\":\"é\"}"
        );
        assert_eq!(requests.next_line().await.unwrap().unwrap(), "second");
        drop(writer);
        assert!(requests.next_line().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn oversized_unterminated_input_is_rejected_before_eof() {
        let bytes = vec![b'x'; Requests::<&[u8]>::LIMIT + 1];
        let mut requests = Requests::new(bytes.as_slice());
        assert_eq!(
            requests.next_line().await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(requests.partial.len() <= Requests::<&[u8]>::LIMIT);
    }

    #[tokio::test]
    async fn exact_limit_and_final_unterminated_lines_remain_valid() {
        let mut bytes = vec![b'x'; Requests::<&[u8]>::LIMIT];
        bytes.extend_from_slice(b"\nlast");
        let mut requests = Requests::new(bytes.as_slice());
        assert_eq!(
            requests.next_line().await.unwrap().unwrap().len(),
            Requests::<&[u8]>::LIMIT
        );
        assert_eq!(requests.next_line().await.unwrap().unwrap(), "last");
        assert!(requests.next_line().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn malformed_utf8_is_not_lossily_decoded() {
        let mut requests = Requests::new(b"\xff\n".as_slice());
        assert_eq!(
            requests.next_line().await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
