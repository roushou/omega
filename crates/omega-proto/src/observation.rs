//! Observation socket addressing and newline-delimited JSON decoding.
//! State and views stream without a handshake. Requests use protocol Frames
//! and the daemon's shared dispatcher and authorization policy.

use crate::Socket;
use crate::omega::StateTopic;
use crate::omega::{Frame, Invoke, frame, invoke};

/// The daemon's observation endpoint and JSON frame encoding.
#[derive(Debug)]
pub struct Observation;

impl Observation {
    /// The socket's name under the runtime directory.
    pub const SOCKET_NAME: &'static str = "omega-shell.sock";

    /// The variable that overrides it, for a second daemon or a test.
    pub const SOCKET_ENV: &'static str = "OMEGA_SHELL_SOCKET";

    /// `$OMEGA_SHELL_SOCKET`, else `$XDG_RUNTIME_DIR/omega-shell.sock`.
    pub fn socket() -> Socket {
        Socket::resolve_named(Self::SOCKET_NAME, Self::SOCKET_ENV)
    }

    /// Decode a state-topic line, returning `None` for other observation messages.
    pub fn topic(line: &str) -> Option<StateTopic> {
        serde_json::from_str(line).ok()
    }

    /// Encode a protocol request with its correlation stream ID.
    pub fn request(stream_id: u64, op: invoke::Op) -> Frame {
        Frame {
            stream_id,
            body: Some(frame::Body::Invoke(Invoke { op: Some(op) })),
        }
    }

    /// One request, as the line to write.
    pub fn line(frame: &Frame) -> Result<String, serde_json::Error> {
        serde_json::to_string(frame)
    }

    /// The answer one line carries, or `None` when the line was a view or a
    /// topic arriving alongside it.
    pub fn answer(line: &str) -> Option<Frame> {
        let frame: Frame = serde_json::from_str(line).ok()?;
        matches!(frame.body, Some(frame::Body::Result(_))).then_some(frame)
    }
}
