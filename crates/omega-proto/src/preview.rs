//! Bounded development transport. These messages never enter daemon dispatch.
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

/// Preview protocol version, independent of the production connection version.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CaseId(String);
impl CaseId {
    pub fn parse(value: impl Into<String>) -> Result<Self, crate::IdentError> {
        let value = value.into();
        crate::UnitName::parse(value.clone())?;
        Ok(Self(value))
    }
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
impl EffectId {
    pub fn parse(value: u64) -> Result<Self, PreviewError> {
        std::num::NonZeroU64::new(value)
            .map(Self)
            .ok_or(PreviewError::InvalidEffect)
    }
    pub fn get(self) -> u64 {
        self.0.get()
    }
}
