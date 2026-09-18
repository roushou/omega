//! Storage identifiers and validation shared by manifest and request boundaries.
use crate::omega::{StorageDescriptor, StorageQuery};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct StorageId(String);

#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid storage contract: {0}")]
pub struct StorageError(pub String);

impl TryFrom<String> for StorageId {
    type Error = StorageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > 128
            || value
                .split('.')
                .any(|part| crate::ident::Ident::validate("storage id", part.into()).is_err())
        {
            return Err(StorageError(format!("invalid storage id {value:?}")));
        }
        Ok(Self(value))
    }
}

impl std::str::FromStr for StorageId {
    type Err = StorageError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl fmt::Display for StorageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StorageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for StorageId {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Self::try_from(<String as serde::Deserialize>::deserialize(de)?)
            .map_err(serde::de::Error::custom)
    }
}

impl StorageDescriptor {
    pub const CODEC: &'static str = "json-v1";
    pub const MAX_ENTRIES: u32 = 10_000;
    pub const MAX_VALUE_BYTES: u32 = 16 * 1024;
    pub const MAX_TOTAL_BYTES: u32 = 8 * 1024 * 1024;

    pub fn validate(&self) -> Result<StorageId, StorageError> {
        let id = self.id.parse()?;
        if self.codec != Self::CODEC
            || self.schema_version != u32::from(self.persistent)
            || self.max_entries == 0
            || self.max_entries > Self::MAX_ENTRIES
            || self.max_value_bytes == 0
            || self.max_value_bytes > Self::MAX_VALUE_BYTES
            || self.max_total_bytes == 0
            || self.max_total_bytes > Self::MAX_TOTAL_BYTES
        {
            return Err(StorageError(format!(
                "unsupported descriptor for {}",
                self.id
            )));
        }
        Ok(id)
    }

    pub fn compatible(&self, other: &Self) -> bool {
        let mut left = self.clone();
        let mut right = other.clone();
        left.writable = false;
        right.writable = false;
        left == right
    }
}

impl StorageQuery {
    pub const DEFAULT_LIMIT: u32 = 50;
    pub const MAX_LIMIT: u32 = 100;

    pub fn accepts(&self, key: &str) -> bool {
        self.key.as_deref().is_none_or(|wanted| key == wanted)
            && self.after.as_deref().is_none_or(|after| key > after)
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.limit == 0
            || self.limit > Self::MAX_LIMIT
            || (self.key.is_some() && self.after.is_some())
        {
            return Err(StorageError(
                "query needs a limit of 1..=100 and either key or after".into(),
            ));
        }
        for key in self.key.iter().chain(self.after.iter()) {
            StorageKey::validate(key)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct StorageKey;
impl StorageKey {
    pub fn validate(key: &str) -> Result<(), StorageError> {
        if key.is_empty() || key.len() > 256 || key.chars().any(char::is_control) {
            Err(StorageError(
                "key must contain 1..=256 bytes without control characters".into(),
            ))
        } else {
            Ok(())
        }
    }
}

impl crate::IntoValue for crate::omega::StoragePage {
    fn into_value(self) -> crate::omega::Value {
        use prost::Message;
        crate::omega::Value {
            kind: Some(crate::omega::value::Kind::BytesValue(self.encode_to_vec())),
        }
    }
}

impl TryFrom<&crate::omega::Value> for crate::omega::StoragePage {
    type Error = StorageError;
    fn try_from(value: &crate::omega::Value) -> Result<Self, Self::Error> {
        use prost::Message;
        match &value.kind {
            Some(crate::omega::value::Kind::BytesValue(bytes)) => {
                Self::decode(bytes.as_slice()).map_err(|e| StorageError(e.to_string()))
            }
            _ => Err(StorageError("expected a storage page".into())),
        }
    }
}

/// Compatible declarations for a build; access is the union of handle declarations.
#[derive(Debug)]
pub struct StorageContracts;
impl StorageContracts {
    pub fn collect<'a>(
        descriptors: impl IntoIterator<Item = &'a StorageDescriptor>,
    ) -> Result<std::collections::BTreeMap<StorageId, StorageDescriptor>, StorageError> {
        let mut result = std::collections::BTreeMap::<StorageId, StorageDescriptor>::new();
        for descriptor in descriptors {
            let id = descriptor.validate()?;
            if let Some(prior) = result.get_mut(&id) {
                if !prior.compatible(descriptor) {
                    return Err(StorageError(format!("conflicting declarations for {id}")));
                }
                prior.writable |= descriptor.writable;
            } else {
                result.insert(id, descriptor.clone());
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture;
    impl Fixture {
        fn descriptor() -> StorageDescriptor {
            StorageDescriptor {
                id: "example.tasks".into(),
                codec: "json-v1".into(),
                max_entries: 100,
                max_value_bytes: 1024,
                max_total_bytes: 4096,
                ..Default::default()
            }
        }
    }
    #[test]
    fn identifier_boundaries_and_deserialization_share_validation() {
        for invalid in ["", ".", "../tasks", "tasks/other", "a..b", "Upper", "a.\nb"] {
            assert!(invalid.parse::<StorageId>().is_err());
            assert!(serde_json::from_value::<StorageId>(serde_json::json!(invalid)).is_err());
        }
        let text = String::from("example.tasks");
        let pointer = text.as_ptr();
        assert_eq!(
            StorageId::try_from(text).unwrap().as_str().as_ptr(),
            pointer
        );
    }
    #[test]
    fn declarations_union_access_and_reject_conflicting_resource_policies() {
        let read = Fixture::descriptor();
        let mut write = read.clone();
        write.writable = true;
        let merged = StorageContracts::collect([&read, &write]).unwrap();
        assert!(merged.values().next().unwrap().writable);
        write.max_entries -= 1;
        assert!(StorageContracts::collect([&read, &write]).is_err());
        write = read.clone();
        write.persistent = true;
        assert!(write.validate().is_err());
    }
    #[test]
    fn invalid_queries_are_refused_before_execution() {
        for query in [
            StorageQuery::default(),
            StorageQuery {
                limit: 101,
                ..Default::default()
            },
            StorageQuery {
                limit: 1,
                key: Some("a".into()),
                after: Some("b".into()),
            },
            StorageQuery {
                limit: 1,
                after: Some("".into()),
                key: None,
            },
        ] {
            assert!(query.validate().is_err());
        }
    }
}
