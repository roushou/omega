//! Shared Hello/Welcome exchange and protocol-version validation.

use std::time::Duration;

use crate::omega::{Frame, Hello, Welcome, frame};
use crate::protocol::{MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, effective_version};

#[derive(Debug)]
pub struct Handshake;

impl Handshake {
    /// How long a peer may take to send its opening frame.
    pub const TIMEOUT: Duration = Duration::from_secs(5);

    /// The environment variable carrying a spawned plugin's one-time token.
    pub const TOKEN_ENV: &'static str = "OMEGA_PLUGIN_TOKEN";

    /// The plugin's opening frame. The token is the one the daemon put in the
    /// plugin's environment; a peer without one is not a plugin.
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

    /// Negotiate the lower supported protocol version or refuse an incompatible peer.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_supported_protocol_range_is_accepted() {
        for version in [0, MIN_PROTOCOL_VERSION - 1, PROTOCOL_VERSION + 1, u32::MAX] {
            assert!(matches!(
                Handshake::negotiate(version),
                Err(HandshakeError::VersionMismatch { .. })
            ));
        }
        for version in MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION {
            assert_eq!(Handshake::negotiate(version).unwrap(), version);
        }
    }
}
