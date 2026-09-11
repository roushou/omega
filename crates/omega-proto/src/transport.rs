//! Framed byte streams and bounded duplex I/O carrying [`Frame`]s.

use prost::Message;
use std::collections::VecDeque;
use std::io;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::Framed;

use crate::codec::FrameCodec;

use crate::CodecError;
use crate::omega::Frame;

/// A framed transport over an async byte stream.
#[derive(Debug)]
pub struct Transport<S> {
    inner: Framed<S, FrameCodec>,
}

impl<S> Transport<S> {
    pub fn new(io: S) -> Self {
        Self {
            inner: Framed::new(io, FrameCodec),
        }
    }

    pub fn into_inner(self) -> S {
        self.inner.into_inner()
    }
}

impl<S> Transport<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// A connection that continues receiving while a write is pending.
    pub fn duplex(self) -> Duplex<S> {
        let (reader, writer) = self.split();
        Duplex {
            reader,
            writer,
            incoming: VecDeque::new(),
            bytes: 0,
            interrupted: false,
        }
    }

    /// Split into independent read and write halves, so a caller can `select!`
    /// over reads while writing from another branch.
    pub fn split(self) -> (ReadHalf<S>, WriteHalf<S>) {
        let (write, read) = self.inner.split();
        (ReadHalf { inner: read }, WriteHalf { inner: write })
    }
}

impl<S> Transport<S>
where
    S: AsyncWrite + Unpin,
{
    pub async fn send(&mut self, frame: Frame) -> Result<(), CodecError> {
        tokio::time::timeout(crate::Handshake::TIMEOUT, self.inner.send(frame))
            .await
            .map_err(|_| {
                crate::CodecError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "socket write deadline exceeded",
                ))
            })?
    }
}

impl<S> Transport<S>
where
    S: AsyncRead + Unpin,
{
    /// Receive one frame. `Ok(None)` is a clean EOF.
    pub async fn recv(&mut self) -> Result<Option<Frame>, CodecError> {
        match self.inner.next().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

/// A framed connection with bounded receive progress during writes.
///
/// A cancelled or failed write poisons the connection: its partially written
/// frame cannot be followed safely by another frame.
#[derive(Debug)]
pub struct Duplex<S> {
    reader: ReadHalf<S>,
    writer: WriteHalf<S>,
    incoming: VecDeque<(Frame, usize)>,
    bytes: usize,
    interrupted: bool,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Duplex<S> {
    const INBOX_COUNT: usize = 32;
    const INBOX_BYTES: usize = 8 * 1024 * 1024;

    pub async fn send(&mut self, frame: Frame) -> Result<(), CodecError> {
        if self.interrupted {
            return Err(CodecError::InterruptedWrite);
        }
        self.interrupted = true;
        let writing = self.writer.send(frame);
        tokio::pin!(writing);
        let result = loop {
            tokio::select! {
                biased;
                result = &mut writing => break result,
                received = self.reader.recv() => {
                    match received {
                        Ok(Some(frame)) => {
                            let size = frame.encoded_len();
                            if self.incoming.len() >= Self::INBOX_COUNT || self.bytes + size > Self::INBOX_BYTES {
                                break Err(CodecError::ReceiveCapacity);
                            }
                            self.bytes += size;
                            self.incoming.push_back((frame, size));
                        }
                        Ok(None) => break Err(CodecError::Io(io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed during a write"))),
                        Err(error) => break Err(error),
                    }
                }
            }
        };
        if result.is_ok() {
            self.interrupted = false;
        }
        result
    }

    pub async fn recv(&mut self) -> Result<Option<Frame>, CodecError> {
        if self.interrupted {
            return Err(CodecError::InterruptedWrite);
        }
        if let Some((frame, size)) = self.incoming.pop_front() {
            self.bytes -= size;
            return Ok(Some(frame));
        }
        self.reader.recv().await
    }
}

/// The read half of a split [`Transport`].
#[derive(Debug)]
pub struct ReadHalf<S> {
    inner: SplitStream<Framed<S, FrameCodec>>,
}

impl<S> ReadHalf<S>
where
    S: AsyncRead + Unpin,
{
    pub async fn recv(&mut self) -> Result<Option<Frame>, CodecError> {
        match self.inner.next().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

/// The write half of a split [`Transport`].
#[derive(Debug)]
pub struct WriteHalf<S> {
    inner: SplitSink<Framed<S, FrameCodec>, Frame>,
}

impl<S> WriteHalf<S>
where
    S: AsyncWrite + Unpin,
{
    pub async fn send(&mut self, frame: Frame) -> Result<(), CodecError> {
        tokio::time::timeout(crate::Handshake::TIMEOUT, self.inner.send(frame))
            .await
            .map_err(|_| {
                crate::CodecError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "socket write deadline exceeded",
                ))
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct Fixture;
    impl Fixture {
        fn frame(stream: u64, size: usize) -> Frame {
            Frame::reply(
                stream,
                crate::omega::result::Outcome::Value(crate::IntoValue::into_value(
                    "x".repeat(size),
                )),
            )
        }
    }

    #[tokio::test(start_paused = true)]
    async fn simultaneous_large_writes_keep_both_read_halves_moving() {
        let (a, b) = tokio::io::duplex(64);
        let mut a = Transport::new(a).duplex();
        let mut b = Transport::new(b).duplex();
        let first = Fixture::frame(1, 600_000);
        let second = Fixture::frame(2, 900_000);
        let (received_a, received_b) = tokio::join!(
            async {
                a.send(first.clone()).await.unwrap();
                a.recv().await.unwrap().unwrap()
            },
            async {
                b.send(second.clone()).await.unwrap();
                b.recv().await.unwrap().unwrap()
            },
        );
        assert_eq!(received_a, second);
        assert_eq!(received_b, first);
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_a_partial_write_prevents_reuse_of_the_connection() {
        let (a, _b) = tokio::io::duplex(64);
        let mut a = Transport::new(a).duplex();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), a.send(Fixture::frame(1, 600_000)))
                .await
                .is_err()
        );
        assert!(matches!(a.recv().await, Err(CodecError::InterruptedWrite)));
        assert!(matches!(
            a.send(Fixture::frame(3, 1)).await,
            Err(CodecError::InterruptedWrite)
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn receive_count_is_bounded_while_a_peer_refuses_to_read() {
        let (a, b) = tokio::io::duplex(64);
        let mut a = Transport::new(a).duplex();
        let mut b = Transport::new(b);
        let (result, _) = tokio::join!(a.send(Fixture::frame(1, 600_000)), async {
            for stream in 0..33 {
                b.send(Fixture::frame(stream, 0)).await.unwrap();
            }
        },);
        assert!(matches!(result, Err(CodecError::ReceiveCapacity)));
        assert_eq!(
            a.incoming.len(),
            Duplex::<tokio::io::DuplexStream>::INBOX_COUNT
        );
    }

    #[tokio::test(start_paused = true)]
    async fn receive_bytes_are_bounded_independently_of_frame_count() {
        let (a, b) = tokio::io::duplex(64);
        let mut a = Transport::new(a).duplex();
        let mut b = Transport::new(b);
        let (result, _) = tokio::join!(a.send(Fixture::frame(1, 600_000)), async {
            for stream in 0..3 {
                b.send(Fixture::frame(stream, 3_000_000)).await.unwrap();
            }
        },);
        assert!(matches!(result, Err(CodecError::ReceiveCapacity)));
        assert!(a.bytes <= Duplex::<tokio::io::DuplexStream>::INBOX_BYTES);
        assert_eq!(a.incoming.len(), 2);
    }
}
