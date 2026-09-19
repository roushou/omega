use serde::{Deserialize, Serialize};
use std::{fmt, num::NonZeroU64};

/// Stable configured executable identity, independent of its Cargo package.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostId(crate::PluginName);
impl std::str::FromStr for HostId {
    type Err = crate::IdentError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl TryFrom<String> for HostId {
    type Error = crate::IdentError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        crate::PluginName::try_from(value).map(Self)
    }
}

impl HostId {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for HostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A process incarnation within one daemon lifetime; never an OS PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProcessId(NonZeroU64);
impl ProcessId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl TryFrom<u64> for ProcessId {
    type Error = std::num::TryFromIntError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        NonZeroU64::try_from(value).map(Self)
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An admitted execution within one daemon lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InvocationId(NonZeroU64);
impl InvocationId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl TryFrom<u64> for InvocationId {
    type Error = std::num::TryFromIntError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        NonZeroU64::try_from(value).map(Self)
    }
}

impl fmt::Display for InvocationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
