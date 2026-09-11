use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, io, str::FromStr};

/// One directory name inside the generation store.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct GenerationId(String);

impl GenerationId {
    pub fn parse(value: impl Into<String>) -> io::Result<Self> {
        let value = value.into();
        if value.is_empty()
            || value == "."
            || value == ".."
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid generation identifier",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for GenerationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl FromStr for GenerationId {
    type Err = io::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl<'de> Deserialize<'de> for GenerationId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
