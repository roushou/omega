//! The daemon's "no", as a frame.
//!
//! Refusals are part of the protocol, not a dropped connection: a unit learns
//! why it was turned away and can report it, rather than seeing an EOF.

use crate::omega::{Error, ErrorCode, Frame, Result as OpResult, frame, result};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}: {message}", .code.as_str_name())]
pub struct Refusal {
    pub code: ErrorCode,
    pub message: String,
}

impl Refusal {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The peer is not a unit this daemon spawned.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthenticated, message)
    }

    /// The op requires something the unit was not granted.
    pub fn denied(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::PermissionDenied, message)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn unimplemented(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unimplemented, message)
    }

    /// The peer is known, but the connection cannot proceed.
    pub fn precondition(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::FailedPrecondition, message)
    }

    /// The refusal as a terminal `Result` frame on `stream_id`.
    pub fn frame(&self, stream_id: u64) -> Frame {
        Frame {
            stream_id,
            body: Some(frame::Body::Result(OpResult {
                outcome: Some(result::Outcome::Error(Error {
                    code: self.code as i32,
                    message: self.message.clone(),
                })),
                done: true,
            })),
        }
    }

    /// The refusal a frame carries, if it carries one.
    pub fn of(frame: &Frame) -> Option<Self> {
        let frame::Body::Result(OpResult {
            outcome: Some(result::Outcome::Error(error)),
            ..
        }) = frame.body.as_ref()?
        else {
            return None;
        };
        Some(Self {
            code: ErrorCode::try_from(error.code).unwrap_or(ErrorCode::Unspecified),
            message: error.message.clone(),
        })
    }
}
