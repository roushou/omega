use super::{Manager, ManagerError, ServiceUnit, Status, UnitName};
use crate::{
    fs::Directory,
    recovery::{Replacement, Snapshot},
};
use std::path::{Path, PathBuf};

/// A named service, its manager, and the caller-selected unit-file path.
/// Construction does no I/O. Manager operations never install or remove files;
/// file operations never reload, enable, or start the service.
///
/// ```
/// use omega_host::{Layout, systemd::{Manager, Scope, Service, UnitName}};
/// let name = "worker.service".parse::<UnitName>()?;
/// let path = Layout::service_file(std::path::Path::new("/example/systemd/user"), &name);
/// let service = Service::new(Manager::new(Scope::User), name, path)?;
/// assert_eq!(service.name().as_str(), "worker.service");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct Service {
    manager: Manager,
    name: UnitName,
    path: PathBuf,
}

impl Service {
    pub fn new(
        manager: Manager,
        name: UnitName,
        path: impl Into<PathBuf>,
    ) -> Result<Self, ServiceError> {
        let path = path.into();
        if !name.is_service()
            || !path.is_absolute()
            || path.file_name() != Some(std::ffi::OsStr::new(name.as_str()))
        {
            return Err(ServiceError::Location { name, path });
        }
        Ok(Self {
            manager,
            name,
            path,
        })
    }

    pub fn name(&self) -> &UnitName {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn manager(&self) -> &Manager {
        &self.manager
    }

    pub async fn status(&self) -> Result<Status, ManagerError> {
        self.manager.status(&self.name).await
    }

    pub async fn start(&self) -> Result<(), ManagerError> {
        self.manager.command(&self.name, &["start"]).await
    }

    pub async fn stop(&self) -> Result<(), ManagerError> {
        self.manager.command(&self.name, &["stop"]).await
    }

    pub async fn restart(&self) -> Result<(), ManagerError> {
        self.manager.command(&self.name, &["restart"]).await
    }

    /// Enable the service, optionally starting it. Success does not establish application readiness.
    pub async fn enable(&self, now: bool) -> Result<(), ManagerError> {
        self.manager
            .command(
                &self.name,
                if now {
                    &["enable", "--now"]
                } else {
                    &["enable"]
                },
            )
            .await
    }

    pub async fn disable(&self, now: bool) -> Result<(), ManagerError> {
        self.manager
            .command(
                &self.name,
                if now {
                    &["disable", "--now"]
                } else {
                    &["disable"]
                },
            )
            .await
    }

    /// Prepare a recoverable replacement. The caller publishes it through its
    /// recovery store, then explicitly reloads and activates the manager.
    pub fn prepare_install(&self, definition: &ServiceUnit) -> Result<Replacement, ServiceError> {
        Replacement::prepare(
            &self.path,
            Snapshot::file(definition.to_string().into_bytes()),
        )
        .map_err(|source| ServiceError::Prepare {
            path: self.path.clone(),
            source,
        })
    }

    /// Compare the file on disk with a definition. Only ENOENT means missing;
    /// unreadable files are errors. This does not inspect loaded systemd state.
    pub fn installed(&self, definition: &ServiceUnit) -> Result<Installed, ServiceError> {
        let found = match std::fs::read_to_string(&self.path) {
            Ok(found) => found,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Installed::Missing);
            }
            Err(source) => {
                return Err(ServiceError::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        Ok(if found == definition.to_string() {
            Installed::Current
        } else {
            Installed::Stale {
                exec_start: ServiceUnit::declared_command(&found),
            }
        })
    }

    /// Remove the unit file and synchronize its parent directory. The caller must
    /// stop/disable first. A sync failure means removal happened but durability is uncertain.
    pub fn remove(&self) -> Result<bool, ServiceError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(source) => {
                return Err(ServiceError::Remove {
                    path: self.path.clone(),
                    source,
                });
            }
        }
        Directory::sync(self.path.parent().expect("absolute file path")).map_err(|source| {
            ServiceError::Remove {
                path: self.path.clone(),
                source,
            }
        })?;
        Ok(true)
    }
}

/// Unit-file state, independent of manager state and application readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    Missing,
    Current,
    /// Different source; the command is a declaration for display, not a running executable.
    Stale {
        exec_start: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("{name} needs an absolute unit-file path ending in the same .service name: {}", path.display())]
    Location { name: UnitName, path: PathBuf },
    #[error("cannot prepare service file {}: {source}", path.display())]
    Prepare {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot inspect service file {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot durably remove service file {}: {source}", path.display())]
    Remove {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TempPath,
        systemd::{ExecStart, Scope},
    };

    #[test]
    fn service_handles_validate_identity_and_distinguish_missing_from_unreadable() {
        let root = TempPath::sibling(Path::new("/tmp/omega-service-file"), "test");
        let name = "example.service".parse::<UnitName>().unwrap();
        let manager = Manager::new(Scope::User);
        assert!(Service::new(manager.clone(), name.clone(), root.join("other.service")).is_err());
        assert!(Service::new(manager.clone(), name.clone(), "example.service").is_err());
        assert!(
            Service::new(
                manager.clone(),
                "example.target".parse::<UnitName>().unwrap(),
                root.join("example.target")
            )
            .is_err()
        );
        let service = Service::new(manager, name, root.join("example.service")).unwrap();
        let unit = ServiceUnit::new(ExecStart::new("/usr/bin/true").unwrap());
        assert_eq!(service.installed(&unit).unwrap(), Installed::Missing);
        std::fs::create_dir_all(service.path()).unwrap();
        assert!(matches!(
            service.installed(&unit),
            Err(ServiceError::Read { .. })
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
