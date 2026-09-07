//! The handshake, shared by both peers.
//!
//! The unit speaks [`Hello`] first; the daemon answers [`Welcome`]. Both sides
//! validate the protocol version and extract the expected frame through this
//! type, so the protocol boundary has exactly one implementation.

use std::time::Duration;

use crate::PROTOCOL_VERSION;
use crate::error::HandshakeError;
use crate::omega::{Frame, Hello, Welcome, frame};

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

    /// Validate a peer's claimed protocol version.
    pub fn check_protocol(peer: u32) -> Result<(), HandshakeError> {
        if peer != PROTOCOL_VERSION {
            return Err(HandshakeError::VersionMismatch {
                peer,
                ours: PROTOCOL_VERSION,
            });
        }
        Ok(())
    }

    /// Extract and validate a [`Hello`] from a received frame.
    pub fn expect_hello(frame: Option<Frame>) -> Result<Hello, HandshakeError> {
        match frame {
            Some(Frame {
                body: Some(frame::Body::Hello(hello)),
                ..
            }) => {
                Self::check_protocol(hello.protocol_version)?;
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
                Self::check_protocol(welcome.protocol_version)?;
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
