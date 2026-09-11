//! The observation stream: where to find it, and how to read a state line.
//!
//! The daemon streams everything it holds — rendered views and state topics —
//! as newline-delimited JSON on a second socket. Reading needs no handshake;
//! requests require the daemon owner's identity and the operator policy.
//!
//! Its address lives here because both readers need it and neither is the
//! daemon: `omega status` reads this stream, and so does the shell that draws
//! the bar.
//!
//! The state half of the stream is [`StateTopic`] — a protocol type, so a
//! reader parses it rather than indexing JSON by hand. The view half is not
//! here: a view is addressed by identifiers the protocol does not own.
//!
//! It also goes the other way. A shell that draws a button has to press it
//! and cannot encode protobuf, so a request is the *same* [`Frame`] carrying
//! the same [`Invoke`], written as JSON. The daemon authorizes a JSON request
//! through the same table it authorizes a framed one through.

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

    /// The state topic one line carries, or `None` when the line was
    /// something else.
    ///
    /// A reader that wants topics skips the lines that are not, which is what
    /// "everything the daemon holds, on one stream" costs its readers.
    pub fn topic(line: &str) -> Option<StateTopic> {
        serde_json::from_str(line).ok()
    }

    /// A request, on a stream of its own so its answer is unambiguous.
    ///
    /// The op is the protocol's own — `Act`, `RestartUnit`, and the rest —
    /// because a shell asking for something and a unit asking for it are the
    /// same request from the daemon's side, and only one of them should have
    /// a taxonomy.
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
