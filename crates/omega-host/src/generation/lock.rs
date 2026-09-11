use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::sync::Arc;

/// The kernel releases the lock when its final file descriptor closes.
#[derive(Debug)]
pub(super) struct FileLock {
    _file: File,
}

impl FileLock {
    pub(super) fn exclusive(path: &Path) -> io::Result<Self> {
        let file = Self::open(path)?;
        Self::acquire(&file, libc::LOCK_EX)?;
        Ok(Self { _file: file })
    }

    pub(super) fn shared(path: &Path) -> io::Result<Self> {
        let file = Self::open(path)?;
        Self::acquire(&file, libc::LOCK_SH)?;
        Ok(Self { _file: file })
    }

    pub(super) fn try_exclusive(path: &Path) -> io::Result<Option<Self>> {
        let file = Self::open(path)?;
        match Self::acquire(&file, libc::LOCK_EX | libc::LOCK_NB) {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn protect_child(self: &Arc<Self>, command: &mut std::process::Command) {
        let lease = self.clone();
        // pre_exec runs after fork. fcntl is async-signal-safe, and descriptor
        // flags are private to the child; the parent's descriptor stays CLOEXEC.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(lease._file.as_raw_fd(), libc::F_SETFD, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    fn open(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
    }

    fn acquire(file: &File, operation: libc::c_int) -> io::Result<()> {
        loop {
            // The descriptor stays open for this call; flock does not retain pointers.
            if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}
