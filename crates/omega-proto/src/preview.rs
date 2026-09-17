//! Bounded development transport. These messages never enter daemon dispatch.
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

/// Preview protocol version, independent of the production connection version.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CaseId(String);
impl std::str::FromStr for CaseId {
    type Err = crate::IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for CaseId {
    type Error = crate::IdentError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for CaseId {
    type Error = crate::IdentError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        crate::ident::Ident::validate("preview case id", value).map(Self)
    }
}

impl CaseId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for CaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("preview transport: {0}")]
    Transport(#[from] tokio_util::codec::LinesCodecError),
    #[error("preview JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("preview effect identity must be nonzero")]
    InvalidEffect,
    #[error("preview message exceeds the frame budget")]
    TooLarge,
}
#[derive(Debug)]
pub struct Reader<R>(FramedRead<R, LinesCodec>);
impl<R: AsyncRead + Unpin> Reader<R> {
    pub fn new(reader: R) -> Self {
        Self(FramedRead::new(
            reader,
            LinesCodec::new_with_max_length(crate::MAX_FRAME_LEN),
        ))
    }
    pub async fn receive<T: serde::de::DeserializeOwned>(
        &mut self,
    ) -> Result<Option<T>, PreviewError> {
        self.0
            .next()
            .await
            .transpose()?
            .map(|line| serde_json::from_str(&line).map_err(Into::into))
            .transpose()
    }
}
#[derive(Debug)]
pub struct Writer<W>(FramedWrite<W, LinesCodec>);
impl<W: AsyncWrite + Unpin> Writer<W> {
    pub fn new(writer: W) -> Self {
        Self(FramedWrite::new(
            writer,
            LinesCodec::new_with_max_length(crate::MAX_FRAME_LEN),
        ))
    }
    pub async fn send<T: serde::Serialize>(&mut self, value: &T) -> Result<(), PreviewError> {
        let line = serde_json::to_string(value)?;
        if line.len() > crate::MAX_FRAME_LEN {
            return Err(PreviewError::TooLarge);
        }
        self.0.send(line).await?;
        Ok(())
    }
}

/// Identity of one outstanding simulated effect within a preview session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EffectId(std::num::NonZeroU64);
impl TryFrom<u64> for EffectId {
    type Error = PreviewError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        std::num::NonZeroU64::new(value)
            .map(Self::from)
            .ok_or(PreviewError::InvalidEffect)
    }
}

impl EffectId {
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl From<std::num::NonZeroU64> for EffectId {
    fn from(value: std::num::NonZeroU64) -> Self {
        Self(value)
    }
}
