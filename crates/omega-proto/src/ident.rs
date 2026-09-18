//! Validated identifiers for plugins, surfaces, modules, and instances.
//! Parse identifiers at input boundaries and retain their typed values internally.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Shared identifier validation; public newtypes select their identifier kind.
pub(crate) struct Ident;

impl Ident {
    pub(crate) fn validate(kind: &'static str, name: String) -> Result<String, IdentError> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(IdentError::InvalidCharacters { kind, name });
        }
        if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(IdentError::InvalidStart { kind, name });
        }
        Ok(name)
    }
}

/// Plugin identifier using lowercase ASCII letters, digits, hyphens, and underscores.
/// Must start with a letter. Serializes as a string.
/// Parse borrowed text with [`str::parse`], or validate an owned string with
/// [`TryFrom<String>`] to retain its allocation. Both return [`IdentError`] on
/// invalid input; deserialization applies the same rules.
///
/// ```
/// use omega_proto::PluginName;
/// let parsed: PluginName = "battery".parse()?;
/// let converted = PluginName::try_from(String::from("battery"))?;
/// assert_eq!(parsed, converted);
/// # Ok::<(), omega_proto::IdentError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PluginName(String);

impl std::str::FromStr for PluginName {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for PluginName {
    type Error = IdentError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for PluginName {
    type Error = IdentError;

    /// Validate and wrap a plugin name.
    fn try_from(name: String) -> Result<Self, Self::Error> {
        Ident::validate("plugin name", name).map(Self)
    }
}

impl PluginName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Surface identifier, unique within its declaring plugin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SurfaceId(String);

impl std::str::FromStr for SurfaceId {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for SurfaceId {
    type Error = IdentError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for SurfaceId {
    type Error = IdentError;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        Ident::validate("surface id", id).map(Self)
    }
}

impl SurfaceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SurfaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Command identifier, unique within its declaring plugin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct CommandId(String);

impl std::str::FromStr for CommandId {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for CommandId {
    type Error = IdentError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for CommandId {
    type Error = IdentError;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        Ident::validate("command id", id).map(Self)
    }
}

impl CommandId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A bar module's id: one instance of a surface, named by the state document
/// so the same widget can appear twice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ModuleId(String);

impl std::str::FromStr for ModuleId {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for ModuleId {
    type Error = IdentError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for ModuleId {
    type Error = IdentError;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        Ident::validate("module id", id).map(Self)
    }
}

impl ModuleId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Invalid identifier with its kind and validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentError {
    #[error("{kind} exceeds 128 bytes")]
    TooLong { kind: &'static str },
    #[error("{kind} must be lowercase letters, digits, hyphens and underscores: {name:?}")]
    InvalidCharacters { kind: &'static str, name: String },
    #[error("{kind} must start with a lowercase letter: {name:?}")]
    InvalidStart { kind: &'static str, name: String },
}

impl<'de> Deserialize<'de> for PluginName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for SurfaceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for CommandId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ModuleId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{
        IncarnationId, InstanceId, PlacementId, RendererFingerprint, SingletonId,
    };
    use std::str::FromStr;

    struct Conversions;

    impl Conversions {
        fn agree<T>(valid: &str, invalid: &str, value: fn(&T) -> &str)
        where
            T: fmt::Debug
                + PartialEq
                + FromStr
                + TryFrom<String, Error = <T as FromStr>::Err>
                + for<'a> TryFrom<&'a str, Error = <T as FromStr>::Err>,
            <T as FromStr>::Err: fmt::Debug + fmt::Display,
        {
            let owned = valid.to_owned();
            let allocation = owned.as_ptr();
            let converted = T::try_from(owned).unwrap();
            assert_eq!(
                value(&converted).as_ptr(),
                allocation,
                "owned conversion must retain its string"
            );
            assert_eq!(value(&converted), valid);
            assert_eq!(converted, valid.parse::<T>().unwrap());
            assert_eq!(converted, T::try_from(valid).unwrap());

            let error = invalid.parse::<T>().unwrap_err().to_string();
            assert_eq!(error, T::try_from(invalid).unwrap_err().to_string());
            assert_eq!(
                error,
                T::try_from(invalid.to_owned()).unwrap_err().to_string()
            );
        }
    }

    #[test]
    fn identifier_entry_points_share_validation_and_preserve_owned_storage() {
        Conversions::agree("battery", "Battery", PluginName::as_str);
        Conversions::agree("panel", "", SurfaceId::as_str);
        Conversions::agree("left-clock", "left.clock", ModuleId::as_str);
        Conversions::agree("instance-1", &"a".repeat(129), InstanceId::as_str);
        Conversions::agree("incarnation-1", "1", IncarnationId::as_str);
        Conversions::agree("bar-clock", "bar.clock", PlacementId::as_str);
        Conversions::agree("launcher", " launcher", SingletonId::as_str);
        #[cfg(feature = "json")]
        Conversions::agree("empty-state", "Empty", crate::preview::CaseId::as_str);
        Conversions::agree(
            &"a".repeat(64),
            &"A".repeat(64),
            RendererFingerprint::as_str,
        );
        Conversions::agree(" Work Space ", "\n", crate::WorkspaceName::as_str);
        Conversions::agree(
            "firefox.desktop",
            "../firefox.desktop",
            crate::ApplicationId::as_str,
        );
        Conversions::agree("vlc.instance_1", "vlc.123", crate::PlayerId::as_str);
        Conversions::agree(
            "/org/bluez/hci0/dev_60_AB_D2_25_8C_49",
            "60:AB:D2:25:8C:49",
            crate::BluetoothDeviceId::as_str,
        );
    }

    #[test]
    fn deserialization_keeps_identifier_rules_and_instance_length_limits() {
        assert!(serde_json::from_str::<PluginName>("\"Invalid\"").is_err());
        assert!(serde_json::from_str::<SurfaceId>("\"\"").is_err());
        assert!(serde_json::from_str::<ModuleId>("\"a.b\"").is_err());
        let long = "a".repeat(129);
        let json = serde_json::to_string(&long).unwrap();
        assert_eq!(
            serde_json::from_str::<PluginName>(&json).unwrap(),
            long.parse::<PluginName>().unwrap()
        );
        assert!(serde_json::from_str::<InstanceId>(&json).is_err());
        assert!(serde_json::from_str::<IncarnationId>(&json).is_err());
        assert!(serde_json::from_str::<PlacementId>(&json).is_err());
        assert!(serde_json::from_str::<SingletonId>(&json).is_err());
    }
}
