use crate::ident::{Ident, IdentError};
use serde::{Deserialize, Serialize};

macro_rules! identity {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, IdentError> {
                let value = value.into();
                if value.len() > 128 {
                    return Err(IdentError::TooLong { kind: $kind });
                }
                Ident::parse($kind, value).map(Self)
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }
    };
}
identity!(InstanceId, "instance id");
identity!(IncarnationId, "incarnation id");
identity!(PlacementId, "placement id");
identity!(SingletonId, "singleton id");
