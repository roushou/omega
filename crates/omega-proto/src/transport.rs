//! The transport: a framed byte stream carrying [`Frame`]s, plus the daemon's
//! control [`Socket`].

use std::io;
use std::path::{Path, PathBuf};

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::Framed;

use crate::codec::FrameCodec;
use crate::error::CodecError;
use crate::omega::Frame;

/// A framed transport over an async byte stream.
#[derive(Debug)]
pub struct Transport<S> {
    inner: Framed<S, FrameCodec>,
}

impl<S> Transport<S> {
    pub fn new(io: S) -> Self {
        Self {
            inner: Framed::new(io, FrameCodec),
        }
    }

    pub fn into_inner(self) -> S {
        self.inner.into_inner()
    }
}

impl<S> Transport<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Split into independent read and write halves, so a caller can `select!`
    /// over reads while writing from another branch.
    pub fn split(self) -> (ReadHalf<S>, WriteHalf<S>) {
        let (write, read) = self.inner.split();
        (ReadHalf { inner: read }, WriteHalf { inner: write })
    }
}

impl<S> Transport<S>
where
    S: AsyncWrite + Unpin,
{
    pub async fn send(&mut self, frame: Frame) -> Result<(), CodecError> {
        self.inner.send(frame).await
    }
}

impl<S> Transport<S>
where
    S: AsyncRead + Unpin,
{
    /// Receive one frame. `Ok(None)` is a clean EOF.
    pub async fn recv(&mut self) -> Result<Option<Frame>, CodecError> {
        match self.inner.next().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

/// The read half of a split [`Transport`].
#[derive(Debug)]
pub struct ReadHalf<S> {
    inner: SplitStream<Framed<S, FrameCodec>>,
}

impl<S> ReadHalf<S>
where
    S: AsyncRead + Unpin,
{
    pub async fn recv(&mut self) -> Result<Option<Frame>, CodecError> {
        match self.inner.next().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

/// The write half of a split [`Transport`].
#[derive(Debug)]
pub struct WriteHalf<S> {
    inner: SplitSink<Framed<S, FrameCodec>, Frame>,
}

impl<S> WriteHalf<S>
where
    S: AsyncWrite + Unpin,
{
    pub async fn send(&mut self, frame: Frame) -> Result<(), CodecError> {
        self.inner.send(frame).await
    }
}

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
    pub fn bind(&self) -> io::Result<UnixListener> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if self.path.exists() {
            if self.is_live() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("{} is already served", self.path.display()),
                ));
            }
            std::fs::remove_file(&self.path)?;
        }

        UnixListener::bind(&self.path)
    }

    /// Whether something is listening on this path right now.
    pub fn is_live(&self) -> bool {
        std::os::unix::net::UnixStream::connect(&self.path).is_ok()
    }

    /// Connect a raw byte stream (no framing).
    pub async fn connect_stream(&self) -> io::Result<UnixStream> {
        UnixStream::connect(&self.path).await
    }

    /// Connect a protobuf-framed transport.
    pub async fn connect(&self) -> io::Result<Transport<UnixStream>> {
        Ok(Transport::new(self.connect_stream().await?))
    }
}
