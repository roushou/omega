use crate::ident::{Ident, IdentError};
use serde::{Deserialize, Serialize};

macro_rules! identity {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl TryFrom<String> for $name {
            type Error = IdentError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                if value.len() > 128 {
                    return Err(IdentError::TooLong { kind: $kind });
                }
                Ident::validate($kind, value).map(Self)
            }
        }
        impl std::str::FromStr for $name {
            type Err = IdentError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(value.to_owned())
            }
        }
        impl TryFrom<&str> for $name {
            type Error = IdentError;
            fn try_from(value: &str) -> Result<Self, Self::Error> {
                value.parse()
            }
        }
        impl $name {
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
                Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }
    };
}
identity!(InstanceId, "instance id");
identity!(IncarnationId, "incarnation id");
identity!(PlacementId, "placement id");
identity!(SingletonId, "singleton id");
