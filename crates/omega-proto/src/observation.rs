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
    /// Decode a Frame strictly. On rejection, retain a unique, valid top-level
    /// `streamId` (or `stream_id`) so the caller can send a correlated refusal.
    /// Invalid JSON, duplicate IDs, and missing or invalid IDs use stream zero.
    pub fn decode_request(line: &str) -> Result<Frame, RequestError> {
        crate::json::Json::decode(line).map_err(|source| RequestError {
            stream_id: serde_json::from_str::<Correlation>(line)
                .map_or(0, |correlation| correlation.0),
            source,
        })
    }

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
        crate::json::Json::decode(line).ok()
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
        crate::json::Json::encode(frame)
    }

    /// The answer one line carries, or `None` when the line was a view or a
    /// topic arriving alongside it.
    pub fn answer(line: &str) -> Option<Frame> {
        let frame: Frame = crate::json::Json::decode(line).ok()?;
        matches!(frame.body, Some(frame::Body::Result(_))).then_some(frame)
    }
}

/// A JSON decoding failure and the stream that can receive its refusal.
#[derive(Debug, thiserror::Error)]
#[error("not a request: {source}")]
pub struct RequestError {
    stream_id: u64,
    #[source]
    source: serde_json::Error,
}

impl RequestError {
    /// The recoverable request ID, or zero when correlation is impossible.
    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }
}

// Scan only for correlation after strict decoding fails. Do not decode through
// serde_json::Value: it collapses duplicate object keys and could select a wrong ID.
struct Correlation(u64);

impl<'de> serde::Deserialize<'de> for Correlation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(Self(0))
    }
}

impl<'de> serde::de::Visitor<'de> for Correlation {
    type Value = Self;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an object with a unique stream ID")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self, A::Error> {
        let mut stream_id = None;
        while let Some(key) = map.next_key::<String>()? {
            if matches!(key.as_str(), "streamId" | "stream_id") {
                if stream_id.is_some() {
                    return Err(serde::de::Error::duplicate_field("streamId"));
                }
                stream_id = Some(
                    map.next_value::<pbjson::private::NumberDeserialize<u64>>()?
                        .0,
                );
            } else {
                map.next_value::<serde::de::IgnoredAny>()?;
            }
        }
        Ok(Self(stream_id.unwrap_or(0)))
    }
}
