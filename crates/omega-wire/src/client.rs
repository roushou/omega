//! The peer's half of a connection.
//!
//! Everything that is the same whether the peer is a unit or the operator:
//! connect, say `Hello`, read the `Welcome`, allocate stream ids that cannot
//! collide with the daemon's, and answer keepalives. What differs — a unit's
//! state mirror, the CLI's one-shot request — is built on top.

use std::time::Duration;

use tokio::net::UnixStream;

use crate::omega::{Frame, Invoke, Ping, Pong, Welcome, frame, invoke};
use crate::stream::PeerStreams;
use crate::{CodecError, Handshake, HandshakeError, Refusal, Socket, Transport};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("cannot reach the daemon at {}: {source}", path.display())]
    Unreachable {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("transport error: {0}")]
    Transport(#[from] CodecError),
    #[error("handshake failed: {0}")]
    Handshake(#[from] HandshakeError),
    #[error("the daemon refused: {0}")]
    Refused(#[from] Refusal),
    #[error("the daemon did not answer in time")]
    Timeout,
    #[error("the daemon closed the connection")]
    Closed,
}

/// A connected peer.
#[derive(Debug)]
pub struct Client {
    transport: Transport<UnixStream>,
    streams: PeerStreams,
}

impl Client {
    /// How long to wait for the daemon's `Welcome`, and for an answer.
    pub const TIMEOUT: Duration = Duration::from_secs(5);

    /// Connect and complete the handshake.
    ///
    /// The token is what makes a peer a unit; an empty one says "I am not a
    /// unit", which the operator means literally.
    pub async fn connect(
        socket: &Socket,
        manifest_hash: &str,
        token: &str,
    ) -> Result<(Self, Welcome), ClientError> {
        let stream = socket
            .connect_stream()
            .await
            .map_err(|source| ClientError::Unreachable {
                path: socket.path().to_path_buf(),
                source,
            })?;

        Self::over(stream, manifest_hash, token).await
    }

    /// Complete the handshake over a stream someone else opened.
    ///
    /// The socket path is the daemon's address, not the protocol: given a
    /// connected stream there is nothing left to look up. A `UnixStream::pair`
    /// is a connection with no listener and no file, which is what lets a unit
    /// be tested against a daemon that is really a test.
    pub async fn over(
        stream: UnixStream,
        manifest_hash: &str,
        token: &str,
    ) -> Result<(Self, Welcome), ClientError> {
        let mut transport = Transport::new(stream);

        transport
            .send(Handshake::hello(manifest_hash, token))
            .await?;

        let frame = tokio::time::timeout(Handshake::TIMEOUT, transport.recv())
            .await
            .map_err(|_| ClientError::Timeout)??;

        if let Some(refusal) = frame.as_ref().and_then(Refusal::of) {
            return Err(ClientError::Refused(refusal));
        }

        let welcome = Handshake::expect_welcome(frame)?;
        Ok((
            Self {
                transport,
                streams: PeerStreams::new(),
            },
            welcome,
        ))
    }

    /// The id for this peer's next request. Odd, always: even ids are the
    /// daemon's, and an answer has to be unambiguous.
    pub fn allocate(&mut self) -> u64 {
        self.streams.allocate()
    }

    /// Send an op on a stream. `stream_id` 0 is fire-and-forget.
    pub async fn invoke(&mut self, stream_id: u64, op: invoke::Op) -> Result<(), ClientError> {
        self.send(Frame {
            stream_id,
            body: Some(frame::Body::Invoke(Invoke { op: Some(op) })),
        })
        .await
    }

    pub async fn send(&mut self, frame: Frame) -> Result<(), ClientError> {
        self.transport.send(frame).await?;
        Ok(())
    }

    /// The next frame, with the housekeeping every peer shares: keepalives
    /// answered, refusals raised.
    ///
    /// `Ok(None)` when the daemon closes the connection.
    pub async fn recv(&mut self) -> Result<Option<Frame>, ClientError> {
        loop {
            let Some(frame) = self.transport.recv().await? else {
                return Ok(None);
            };

            if let Some(refusal) = Refusal::of(&frame) {
                return Err(ClientError::Refused(refusal));
            }

            // A daemon checking whether this peer is still there gets its
            // answer here rather than from every caller's loop.
            if let Some(frame::Body::Ping(Ping { nonce })) = frame.body.as_ref() {
                let nonce = *nonce;
                self.send(Frame {
                    stream_id: frame.stream_id,
                    body: Some(frame::Body::Pong(Pong { nonce })),
                })
                .await?;
                continue;
            }
            if matches!(frame.body, Some(frame::Body::Pong(_))) {
                continue;
            }

            return Ok(Some(frame));
        }
    }

    /// Wait for the answer to one request, discarding frames that are not it.
    /// Callers that need to see those frames use [`recv`] and match
    /// themselves.
    ///
    /// [`recv`]: Self::recv
    pub async fn answer(
        &mut self,
        stream_id: u64,
    ) -> Result<crate::omega::result::Outcome, ClientError> {
        loop {
            let frame = tokio::time::timeout(Self::TIMEOUT, self.recv())
                .await
                .map_err(|_| ClientError::Timeout)??
                .ok_or(ClientError::Closed)?;

            let Some(frame::Body::Result(result)) = frame.body else {
                continue;
            };
            if frame.stream_id == stream_id {
                return result.outcome.ok_or(ClientError::Closed);
            }
        }
    }
}
