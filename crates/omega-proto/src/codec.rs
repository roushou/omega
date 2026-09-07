//! Length-delimited frame codec.
//!
//! The wire format is `varint length ++ protobuf(Frame)`. [`FrameCodec`] is the
//! single unit of framing; every transport in Omega is
//! [`tokio_util::codec::Framed`] over this codec.

use bytes::{Buf, BytesMut};
use prost::Message;
use tokio_util::codec::{Decoder, Encoder};

use crate::error::CodecError;
use crate::omega::Frame;

/// Upper bound on a single frame. The daemon rejects anything larger; this
/// keeps a misbehaving peer from forcing unbounded allocation.
pub const MAX_FRAME_LEN: usize = 4 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct FrameCodec;

impl FrameCodec {
    /// Peek the varint length prefix without consuming it, so a partial
    /// prefix never loses data. Returns `(prefix_bytes, body_len)`.
    fn peek_length(&self, src: &BytesMut) -> Result<Option<(usize, usize)>, CodecError> {
        let mut len = 0usize;
        let mut shift = 0u32;
        let mut i = 0;

        loop {
            if i >= src.len() {
                return Ok(None);
            }
            let b = src[i];
            i += 1;

            if shift == 63 && (b & 0xfe) != 0 {
                return Err(CodecError::PrefixOverflow);
            }
            len |= ((b & 0x7f) as usize) << shift;

            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }

        if len > MAX_FRAME_LEN {
            return Err(CodecError::FrameTooLong(len));
        }

        Ok(Some((i, len)))
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = CodecError;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode_length_delimited(dst)?;
        Ok(())
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = CodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, Self::Error> {
        let (prefix_len, body_len) = match self.peek_length(src)? {
            Some(v) => v,
            None => return Ok(None),
        };
        if src.len() < prefix_len + body_len {
            return Ok(None);
        }

        src.advance(prefix_len);
        let frame = Frame::decode(&src[..body_len])?;
        src.advance(body_len);
        Ok(Some(frame))
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, Self::Error> {
        match self.decode(src)? {
            Some(frame) => Ok(Some(frame)),
            None if src.is_empty() => Ok(None),
            None => Err(CodecError::Truncated),
        }
    }
}
