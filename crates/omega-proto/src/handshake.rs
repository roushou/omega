//! The handshake, shared by both peers.
//!
//! The unit speaks [`Hello`] first; the daemon answers [`Welcome`]. Both sides
//! validate the protocol version and extract the expected frame through this
//! type, so the protocol boundary has exactly one implementation.

use std::time::Duration;

use crate::omega::{Frame, Hello, Welcome, frame};
use crate::protocol::{MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, effective_version};

#[derive(Debug)]
pub struct Handshake;

impl Handshake {
    /// How long a peer may take to send its opening frame.
    pub const TIMEOUT: Duration = Duration::from_secs(5);

    /// The environment variable carrying a spawned unit's one-time token.
    pub const TOKEN_ENV: &'static str = "OMEGA_UNIT_TOKEN";

    /// The unit's opening frame. The token is the one the daemon put in the
    /// unit's environment; a peer without one is not a unit.
    pub fn hello(manifest_hash: &str, token: &str) -> Frame {
        Frame {
            stream_id: 0,
            body: Some(frame::Body::Hello(Hello {
                protocol_version: PROTOCOL_VERSION,
                manifest_hash: manifest_hash.into(),
                token: token.into(),
            })),
        }
    }

    /// The token this process was spawned with, empty when it was not spawned
    /// by a daemon.
    pub fn token_from_env() -> String {
        std::env::var(Self::TOKEN_ENV).unwrap_or_default()
    }

    /// Validate a peer's claimed protocol version, and answer with the one
    /// the two of them will actually speak.
    ///
    /// A peer inside the supported window is accepted at the lower of the two
    /// versions. Outside it, the connection ends here: a peer that is too old
    /// would be served frames it cannot parse, and one that is too new would
    /// be answered by a daemon that cannot parse its.
    pub fn negotiate(peer: u32) -> Result<u32, HandshakeError> {
        if !(MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&peer) {
            return Err(HandshakeError::VersionMismatch {
                peer,
                oldest: MIN_PROTOCOL_VERSION,
                newest: PROTOCOL_VERSION,
            });
        }
        Ok(effective_version(peer))
    }

    /// Extract and validate a [`Hello`] from a received frame.
    pub fn expect_hello(frame: Option<Frame>) -> Result<Hello, HandshakeError> {
        match frame {
            Some(Frame {
                body: Some(frame::Body::Hello(hello)),
                ..
            }) => {
                Self::negotiate(hello.protocol_version)?;
                Ok(hello)
            }
            Some(other) => Err(HandshakeError::Unexpected {
                expected: "Hello",
                got: Box::new(other.body),
            }),
            None => Err(HandshakeError::ClosedBefore("Hello")),
        }
    }

    /// Extract and validate a [`Welcome`] from a received frame.
    pub fn expect_welcome(frame: Option<Frame>) -> Result<Welcome, HandshakeError> {
        match frame {
            Some(Frame {
                body: Some(frame::Body::Welcome(welcome)),
                ..
            }) => {
                Self::negotiate(welcome.protocol_version)?;
                Ok(welcome)
            }
            Some(other) => Err(HandshakeError::Unexpected {
                expected: "Welcome",
                got: Box::new(other.body),
            }),
            None => Err(HandshakeError::ClosedBefore("Welcome")),
        }
    }
}

/// Why a connection could not be established.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("connection closed before {0}")]
    ClosedBefore(&'static str),
    #[error("expected {expected}, got {got:?}")]
    Unexpected {
        expected: &'static str,
        got: Box<Option<frame::Body>>,
    },
    #[error("protocol mismatch: peer speaks v{peer}, we serve v{oldest}..=v{newest}")]
    VersionMismatch { peer: u32, oldest: u32, newest: u32 },
}
