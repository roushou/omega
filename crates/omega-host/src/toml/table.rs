use std::collections::BTreeMap;
use std::collections::btree_map::{IntoIter, Iter};

use serde::{Deserialize, Serialize};

/// Named TOML entries sorted by key for deterministic serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Table<V>(BTreeMap<String, V>);

impl<V> Table<V> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, name: impl Into<String>, value: V) -> Option<V> {
        self.0.insert(name.into(), value)
    }

    pub fn remove(&mut self, name: &str) -> Option<V> {
        self.0.remove(name)
    }

    pub fn get(&self, name: &str) -> Option<&V> {
        self.0.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, String, V> {
        self.0.iter()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Take every entry of `other`, overwriting on conflict.
    pub fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

impl<V> Default for Table<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V, K: Into<String>> FromIterator<(K, V)> for Table<V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl<V, K: Into<String>> Extend<(K, V)> for Table<V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        self.0.extend(iter.into_iter().map(|(k, v)| (k.into(), v)));
    }
}

impl<V> IntoIterator for Table<V> {
    type Item = (String, V);
    type IntoIter = IntoIter<String, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a Table<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = Iter<'a, String, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
