use serde::{Deserialize, Serialize};
use std::{fmt, io};

pub(super) const VERSION: u32 = 1;

/// Identifies one retained change record, independently of stage IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ChangeId(String);

impl std::str::FromStr for ChangeId {
    type Err = io::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for ChangeId {
    type Error = io::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for ChangeId {
    type Error = io::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value == "."
            || value == ".."
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid change identifier",
            ));
        }
        Ok(Self(value))
    }
}

impl ChangeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for ChangeId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// An unfinished state requires inspection, not an assumption about effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Prepared,
    Applying,
    Applied,
    Restoring,
    Restored,
}

/// Readable metadata shared by all change kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub id: ChangeId,
    pub kind: String,
    pub state: State,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Record<C> {
    pub(super) version: u32,
    pub(super) receipt: Receipt,
    pub(super) change: C,
}

// Scan metadata without allocating the potentially large change payload.
#[derive(Deserialize)]
pub(super) struct RecordHeader {
    pub(super) version: u32,
    pub(super) receipt: Receipt,
    #[serde(rename = "change")]
    _change: serde::de::IgnoredAny,
}
