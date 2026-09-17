use super::{Change, Observation};
use crate::{AtomicFile, Directory, StageDir};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// One child name, validated on both capture and deserialization. Snapshot
/// entries cannot escape their containing directory or introduce nested paths.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
struct EntryName(String);

impl TryFrom<String> for EntryName {
    type Error = io::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value == "."
            || value == ".."
            || value.contains('/')
            || value.contains('\0')
        {
            return Err(io::Error::other("invalid snapshot entry name"));
        }
        Ok(Self(value))
    }
}

impl<'de> Deserialize<'de> for EntryName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Exact file contents, permission bits, and directory entries. Symlinks inside
/// directories are recorded without following them. Special files are refused.
/// Ownership, ACLs, extended attributes, and hard-link identity are not captured;
/// use this contract for Omega-owned configuration/assets, not arbitrary trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot(Node);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Node {
    Missing,
    File {
        bytes: Vec<u8>,
        mode: u32,
    },
    Directory {
        entries: BTreeMap<EntryName, Snapshot>,
        mode: u32,
    },
    Link {
        target: PathBuf,
    },
}

impl Snapshot {
    pub fn file(bytes: impl Into<Vec<u8>>) -> Self {
        Self(Node::File {
            bytes: bytes.into(),
            mode: 0o644,
        })
    }

    /// File contents with explicit ordinary Unix permissions. Special bits
    /// are rejected when preparing the replacement.
    pub fn file_with_permissions(
        bytes: impl Into<Vec<u8>>,
        permissions: std::fs::Permissions,
    ) -> Self {
        Self(Node::File {
            bytes: bytes.into(),
            mode: permissions.mode() & 0o7777,
        })
    }

    /// Build an asset tree without touching disk. Paths must be relative child
    /// paths; duplicates and file/directory collisions are refused.
    pub fn directory<I, P, B>(files: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = (P, B)>,
        P: AsRef<Path>,
        B: Into<Vec<u8>>,
    {
        let mut root = Self::empty_directory();
        for (path, bytes) in files {
            let names = path
                .as_ref()
                .components()
                .map(|part| match part {
                    std::path::Component::Normal(name) => name
                        .to_str()
                        .ok_or_else(|| io::Error::other("non-UTF-8 asset name"))
                        .and_then(|name| EntryName::try_from(name.to_owned())),
                    _ => Err(io::Error::other("asset paths must be relative child paths")),
                })
                .collect::<io::Result<Vec<_>>>()?;
            root.insert(&names, bytes.into())?;
        }
        Ok(root)
    }

    fn empty_directory() -> Self {
        Self(Node::Directory {
            entries: BTreeMap::new(),
            mode: 0o755,
        })
    }

    fn insert(&mut self, names: &[EntryName], bytes: Vec<u8>) -> io::Result<()> {
        let Some((name, rest)) = names.split_first() else {
            return Err(io::Error::other("empty asset path"));
        };
        let Node::Directory { entries, .. } = &mut self.0 else {
            return Err(io::Error::other("asset path collides with a file"));
        };
        if rest.is_empty() {
            if entries.contains_key(name) {
                return Err(io::Error::other("duplicate asset path"));
            }
            entries.insert(name.clone(), Self::file(bytes));
            Ok(())
        } else {
            entries
                .entry(name.clone())
                .or_insert_with(Self::empty_directory)
                .insert(rest, bytes)
        }
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self(Node::Missing));
            }
            Err(error) => return Err(error),
        };
        let mode = metadata.permissions().mode() & 0o7777;
        if metadata.is_symlink() {
            return Ok(Self(Node::Link {
                target: std::fs::read_link(path)?,
            }));
        }
        if mode > 0o777 {
            return Err(io::Error::other("snapshot refuses special permission bits"));
        }
        if metadata.is_file() {
            return Ok(Self(Node::File {
                bytes: std::fs::read(path)?,
                mode,
            }));
        }
        if !metadata.is_dir() {
            return Err(io::Error::other(format!(
                "{} is not a regular file or directory",
                path.display()
            )));
        }
        let mut entries = BTreeMap::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let name = EntryName::try_from(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| io::Error::other("non-UTF-8 snapshot entry"))?,
            )?;
            entries.insert(name, Self::read(&entry.path())?);
        }
        Ok(Self(Node::Directory { entries, mode }))
    }

    fn validate(&self, root: bool) -> io::Result<()> {
        match &self.0 {
            Node::Missing if !root => Err(io::Error::other("missing child in snapshot")),
            Node::Link { .. } if root => Err(io::Error::other(
                "replacement refuses a symlink destination",
            )),
            Node::File { mode, .. } | Node::Directory { mode, .. } if *mode > 0o777 => {
                Err(io::Error::other("invalid snapshot permissions"))
            }
            Node::Directory { entries, .. } => {
                for entry in entries.values() {
                    entry.validate(false)?;
                }
                Ok(())
            }
            Node::Missing | Node::File { .. } | Node::Link { .. } => Ok(()),
        }
    }

    fn write_new(&self, path: &Path) -> io::Result<()> {
        match &self.0 {
            Node::Missing => Err(io::Error::other("cannot materialize a missing snapshot")),
            Node::File { bytes, mode } => AtomicFile::at(path)
                .write_with_permissions(bytes, std::fs::Permissions::from_mode(*mode)),
            Node::Link { target } => {
                std::os::unix::fs::symlink(target, path)?;
                Directory::sync(
                    path.parent()
                        .ok_or_else(|| io::Error::other("link has no parent"))?,
                )
            }
            Node::Directory { entries, mode } => {
                Directory::create_all(path)?;
                for (name, entry) in entries {
                    entry.write_new(&path.join(&name.0))?;
                }
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(*mode))?;
                Directory::sync(path)
            }
        }
    }

    fn sync(&self, path: &Path) -> io::Result<()> {
        match &self.0 {
            Node::Missing | Node::Link { .. } => Ok(()),
            Node::File { .. } => std::fs::File::open(path)?.sync_all(),
            Node::Directory { entries, .. } => {
                for (name, entry) in entries {
                    entry.sync(&path.join(&name.0))?;
                }
                Directory::sync(path)
            }
        }
    }

    fn publish(&self, path: &Path) -> io::Result<()> {
        match &self.0 {
            Node::Missing => {
                let displaced = crate::TempPath::sibling(path, "removed");
                Directory::rename_new(path, &displaced)?;
                if std::fs::symlink_metadata(&displaced)?.is_dir() {
                    std::fs::remove_dir_all(&displaced)?;
                } else {
                    std::fs::remove_file(&displaced)?;
                }
                Directory::sync(
                    path.parent()
                        .ok_or_else(|| io::Error::other("target has no parent"))?,
                )
            }
            Node::Directory { .. } => {
                let stage = StageDir::new(path)?;
                self.write_new(stage.path())?;
                stage.commit()
            }
            Node::File { .. } => self.write_new(path),
            Node::Link { .. } => Err(io::Error::other(
                "replacement refuses a symlink destination",
            )),
        }
    }
}

/// A compare-before-write replacement of one configuration file or asset tree.
/// Before and after snapshots live in the durable record. Recovery preserves
/// external changes. Parent directories may be created and are not removed.
/// Callers must exclude concurrent non-Omega writers during publication: filesystem
/// comparison followed by rename is not a compare-and-swap transaction.
#[derive(Debug, Serialize, Deserialize)]
pub struct Replacement {
    target: PathBuf,
    before: Snapshot,
    after: Snapshot,
}

impl Replacement {
    pub fn prepare(target: &Path, after: Snapshot) -> io::Result<Self> {
        let target = std::path::absolute(target)?;
        let change = Self {
            before: Snapshot::read(&target)?,
            target,
            after,
        };
        change.validate()?;
        Ok(change)
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn changed(&self) -> bool {
        self.before != self.after
    }

    pub(super) fn creates_target(&self) -> bool {
        matches!(self.before.0, Node::Missing)
    }

    fn validate(&self) -> io::Result<()> {
        if !self.target.is_absolute()
            || self.target.file_name().is_none()
            || self
                .target
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(io::Error::other(
                "replacement needs an absolute target without parent traversal",
            ));
        }
        self.before.validate(true)?;
        self.after.validate(true)?;
        if matches!(
            (&self.before.0, &self.after.0),
            (Node::File { .. }, Node::Directory { .. })
                | (Node::Directory { .. }, Node::File { .. })
        ) {
            return Err(io::Error::other(
                "replacement cannot change a file into a directory or vice versa",
            ));
        }
        Ok(())
    }

    fn replace(&self, expected: &Snapshot, desired: &Snapshot) -> io::Result<()> {
        self.validate()?;
        if &Snapshot::read(&self.target)? != expected {
            return Err(io::Error::other("destination changed after preparation"));
        }
        if expected == desired {
            return Ok(());
        }
        desired.publish(&self.target)
    }
}

impl Change for Replacement {
    const KIND: &'static str = "filesystem-replacement-v1";
    type Error = io::Error;

    fn inspect(&self) -> io::Result<Observation> {
        self.validate()?;
        let current = Snapshot::read(&self.target)?;
        Ok(match (current == self.before, current == self.after) {
            (true, true) => Observation::Unchanged,
            (true, false) => Observation::Before,
            (false, true) => Observation::After,
            (false, false) => Observation::Conflict,
        })
    }

    fn apply(&self) -> io::Result<()> {
        self.replace(&self.before, &self.after)
    }

    fn restore(&self) -> io::Result<()> {
        self.replace(&self.after, &self.before)
    }

    fn confirm(&self, observed: Observation) -> io::Result<()> {
        if observed == Observation::Conflict || self.inspect()? != observed {
            return Err(io::Error::other(
                "destination changed before durability confirmation",
            ));
        }
        let expected = match observed {
            Observation::Before | Observation::Unchanged => &self.before,
            Observation::After => &self.after,
            Observation::Conflict => unreachable!("conflicts were refused"),
        };
        expected.sync(&self.target)?;
        for parent in self.target.ancestors().skip(1) {
            if parent.try_exists()? {
                Directory::sync(parent)?;
            }
        }
        if self.inspect()? != observed {
            return Err(io::Error::other(
                "destination changed during durability confirmation",
            ));
        }
        Ok(())
    }
}
