//! Socket discovery, binding and endpoint ownership.

use crate::Transport;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use tokio::net::{UnixListener, UnixStream};

/// The daemon's control socket: the protocol endpoint both peers agree on.
#[derive(Clone, Debug)]
pub struct Socket {
    path: PathBuf,
}

impl Socket {
    /// The control socket: `$OMEGA_SOCKET`, else `$XDG_RUNTIME_DIR/omega.sock`.
    pub fn resolve() -> Self {
        Self::resolve_named("omega.sock", "OMEGA_SOCKET")
    }

    /// A named socket under the runtime dir (e.g. the shell socket).
    pub fn resolve_named(name: &str, env: &str) -> Self {
        let path = std::env::var(env)
            .map(PathBuf::from)
            .or_else(|_| std::env::var("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join(name)))
            .unwrap_or_else(|_| {
                std::env::temp_dir().join(format!("{name}.{}", std::process::id()))
            });
        Self { path }
    }

    /// An explicit path (tests, unusual deployments).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bind a listener, creating parent directories.
    ///
    /// A socket file left by a crashed process is cleared, but one that is
    /// still being served is not: unlinking it would silently steal the
    /// endpoint from a running daemon, and every unit connecting afterwards
    /// would reach the wrong one. The difference is whether anybody answers.
    pub fn bind(&self) -> io::Result<BoundSocket> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match std::fs::symlink_metadata(&self.path) {
            Ok(metadata) => {
                if !metadata.file_type().is_socket() {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "socket path contains a non-socket entry",
                    ));
                }
                match self.probe() {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            format!("{} is already served", self.path.display()),
                        ));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "socket listener backlog is full",
                        ));
                    }
                    Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        std::fs::remove_file(&self.path)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        let listener = UnixListener::bind(&self.path)?;
        let metadata = std::fs::symlink_metadata(&self.path)?;
        Ok(BoundSocket {
            listener,
            path: self.path.clone(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    /// Whether something is listening on this path right now.
    pub fn is_live(&self) -> bool {
        match self.probe() {
            Ok(()) => true,
            Err(error) => error.kind() == io::ErrorKind::WouldBlock,
        }
    }

    // A full Unix listen backlog is live, and must not block startup or status.
    fn probe(&self) -> io::Result<()> {
        let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
        socket.set_nonblocking(true)?;
        socket.connect(&socket2::SockAddr::unix(&self.path)?)
    }

    /// Connect a raw byte stream (no framing).
    pub async fn connect_stream(&self) -> io::Result<UnixStream> {
        tokio::time::timeout(crate::Handshake::TIMEOUT, UnixStream::connect(&self.path))
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "socket connect deadline exceeded")
            })?
    }

    /// Connect a protobuf-framed transport.
    pub async fn connect(&self) -> io::Result<Transport<UnixStream>> {
        Ok(Transport::new(self.connect_stream().await?))
    }
}

/// Owns the listening socket and its filesystem entry.
/// Cleanup leaves an entry replaced by another owner untouched.
#[derive(Debug)]
pub struct BoundSocket {
    listener: UnixListener,
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl BoundSocket {
    pub async fn accept(&self) -> io::Result<(UnixStream, tokio::net::unix::SocketAddr)> {
        self.listener.accept().await
    }
}

impl Drop for BoundSocket {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path)
            && metadata.file_type().is_socket()
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
