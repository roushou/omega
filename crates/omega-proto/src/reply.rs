use crate::omega::{Error, ErrorCode, Frame, Result as OpResult, frame, result};
use prost::Message;

impl Frame {
    /// A terminal reply. Oversized outcomes become a bounded refusal on the
    /// original stream, so a valid invoke still receives an answer.
    pub fn reply(stream_id: u64, outcome: result::Outcome) -> Self {
        let mut reply = Self {
            stream_id,
            body: Some(frame::Body::Result(OpResult {
                outcome: Some(outcome),
                done: true,
            })),
        };
        if reply.encoded_len() > crate::MAX_FRAME_LEN {
            reply.body = Some(frame::Body::Result(OpResult {
                done: true,
                outcome: Some(result::Outcome::Error(Error {
                    code: ErrorCode::PayloadTooLarge as i32,
                    message: "response exceeds frame limit".into(),
                })),
            }));
        }
        reply
    }
}
