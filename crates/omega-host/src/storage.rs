//! Versioned storage envelopes and exclusive access to a state root.
use crate::{AtomicFile, Layout};
use omega_proto::omega::{
    StorageDescriptor, StorageEntry, StoragePage, StorageQuery, StorageRevision,
};
use omega_proto::storage::{StorageId, StorageKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io,
    os::fd::AsRawFd,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct EntryKey(String);
impl std::str::FromStr for EntryKey {
    type Err = omega_proto::storage::StorageError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for EntryKey {
    type Error = omega_proto::storage::StorageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        StorageKey::validate(&value)?;
        Ok(Self(value))
    }
}

impl<'de> Deserialize<'de> for EntryKey {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(de)?).map_err(serde::de::Error::custom)
    }
}

impl EntryKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub value: serde_json::Value,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: StorageId,
    pub codec: String,
    pub schema_version: u32,
    pub epoch: String,
    pub revision: u64,
    #[serde(deserialize_with = "Envelope::decode_entries")]
    pub entries: BTreeMap<EntryKey, Entry>,
}

impl Envelope {
    fn decode_entries<'de, D: serde::Deserializer<'de>>(
        de: D,
    ) -> Result<BTreeMap<EntryKey, Entry>, D::Error> {
        struct Entries;
        impl<'de> serde::de::Visitor<'de> for Entries {
            type Value = BTreeMap<EntryKey, Entry>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique storage entry keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut entries = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<EntryKey, Entry>()? {
                    if entries.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate storage key"));
                    }
                }
                Ok(entries)
            }
        }
        de.deserialize_map(Entries)
    }

    pub fn empty(descriptor: &StorageDescriptor) -> io::Result<Self> {
        use std::io::Read;
        let mut random = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        Ok(Self {
            id: descriptor.validate().map_err(io::Error::other)?,
            codec: descriptor.codec.clone(),
            schema_version: descriptor.schema_version,
            epoch: random.iter().map(|b| format!("{b:02x}")).collect(),
            revision: 0,
            entries: BTreeMap::new(),
        })
    }

    pub fn revision(&self) -> StorageRevision {
        StorageRevision {
            epoch: self.epoch.clone(),
            revision: self.revision,
        }
    }

    pub fn validate(&self, descriptor: &StorageDescriptor) -> io::Result<()> {
        if self.id != descriptor.validate().map_err(io::Error::other)?
            || self.codec != descriptor.codec
            || self.schema_version != descriptor.schema_version
            || self.epoch.len() != 32
            || !self.epoch.bytes().all(|b| b.is_ascii_hexdigit())
            || self.entries.len() > descriptor.max_entries as usize
        {
            return Err(io::Error::other(
                "incompatible or oversized storage envelope",
            ));
        }
        for entry in self.entries.values() {
            if entry.revision == 0
                || entry.revision > self.revision
                || serde_json::to_vec(&entry.value)?.len() > descriptor.max_value_bytes as usize
            {
                return Err(io::Error::other("invalid storage entry revision or size"));
            }
        }
        if serde_json::to_vec(self)?.len() > descriptor.max_total_bytes as usize {
            return Err(io::Error::other("storage capacity exceeded"));
        }
        Ok(())
    }

    pub fn page(&self, query: &StorageQuery) -> StoragePage {
        let mut entries = self
            .entries
            .iter()
            .filter(|(key, _)| query.accepts(key.as_str()));
        let selected = entries
            .by_ref()
            .take(query.limit as usize)
            .map(|(key, entry)| StorageEntry {
                key: key.0.clone(),
                json: entry.value.to_string().into_bytes(),
                revision: entry.revision,
            })
            .collect();
        StoragePage {
            revision: Some(self.revision()),
            entries: selected,
            total: self.entries.len() as u64,
            truncated: entries.next().is_some(),
        }
    }
}

#[derive(Debug)]
pub struct Lease {
    _file: File,
}

impl Lease {
    pub fn acquire(layout: &Layout) -> io::Result<Self> {
        crate::Directory::create_all(&layout.state)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout.storage_lock())?;
        // SAFETY: flock receives a live descriptor and no pointers.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { _file: file })
    }
}

#[derive(Debug, Clone)]
pub struct JsonStore {
    file: AtomicFile,
}

impl JsonStore {
    pub fn new(layout: &Layout, id: &StorageId) -> Self {
        Self {
            file: AtomicFile::at(layout.storage_file(id)),
        }
    }

    /// Read an existing envelope for offline inspection; never create or repair it.
    pub fn inspect(&self, id: &StorageId) -> io::Result<Envelope> {
        use std::io::Read;
        let mut bytes = Vec::new();
        File::open(self.file.path())?
            .take(u64::from(StorageDescriptor::MAX_TOTAL_BYTES) + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > StorageDescriptor::MAX_TOTAL_BYTES as usize {
            return Err(io::Error::other("storage envelope exceeds limit"));
        }
        let value: Envelope = serde_json::from_slice(&bytes)?;
        value.validate(&StorageDescriptor {
            id: id.to_string(),
            persistent: true,
            schema_version: 1,
            codec: StorageDescriptor::CODEC.into(),
            max_entries: StorageDescriptor::MAX_ENTRIES,
            max_value_bytes: StorageDescriptor::MAX_VALUE_BYTES,
            max_total_bytes: StorageDescriptor::MAX_TOTAL_BYTES,
            writable: false,
        })?;
        Ok(value)
    }

    pub fn load(&self, descriptor: &StorageDescriptor) -> io::Result<Envelope> {
        use std::io::Read;
        match File::open(self.file.path()) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                std::io::Read::by_ref(&mut file)
                    .take(u64::from(descriptor.max_total_bytes) + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > descriptor.max_total_bytes as usize {
                    return Err(io::Error::other("storage envelope exceeds limit"));
                }
                let value: Envelope = serde_json::from_slice(&bytes)?;
                value.validate(descriptor)?;
                file.sync_all()?;
                if let Some(parent) = self.file.path().parent() {
                    crate::Directory::sync(parent)?;
                }
                Ok(value)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let value = Envelope::empty(descriptor)?;
                value.validate(descriptor)?;
                self.write(&value).map_err(crate::fs::WriteError::into_io)?;
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }

    /// Post-publication errors require suspending access and reconciling the file.
    pub fn write(&self, envelope: &Envelope) -> Result<(), crate::fs::WriteError> {
        use std::os::unix::fs::PermissionsExt;
        self.file.publish(
            &serde_json::to_vec(envelope).map_err(io::Error::other)?,
            std::fs::Permissions::from_mode(0o600),
        )
    }
}
