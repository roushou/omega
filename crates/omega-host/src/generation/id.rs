use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, io, str::FromStr};

/// One directory name inside the generation store.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct GenerationId(String);

impl TryFrom<String> for GenerationId {
    type Error = io::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
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
}

impl GenerationId {
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
        Self::try_from(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for GenerationId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<&str> for GenerationId {
    type Error = io::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}
