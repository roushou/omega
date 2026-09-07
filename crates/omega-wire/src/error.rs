use crate::omega::frame;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("frame length {0} exceeds MAX_FRAME_LEN")]
    FrameTooLong(usize),
    #[error("length prefix overflows 64 bits")]
    PrefixOverflow,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("connection closed mid-frame")]
    Truncated,
    #[error("encode error: {0}")]
    Encode(#[from] prost::EncodeError),
    #[error("decode error: {0}")]
    Decode(#[from] prost::DecodeError),
}

#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("connection closed before {0}")]
    ClosedBefore(&'static str),
    #[error("expected {expected}, got {got:?}")]
    Unexpected {
        expected: &'static str,
        got: Box<Option<frame::Body>>,
    },
    #[error("protocol mismatch: peer speaks v{peer}, we speak v{ours}")]
    VersionMismatch { peer: u32, ours: u32 },
}
