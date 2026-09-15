//! Terminal answers for command invocation and surface interactions.

use crate::Refusal;
use crate::omega::{Empty, ErrorCode, Value, result};

/// A command succeeded with an acknowledgement or a value.
///
/// An acknowledgement has the invoked operation's completion semantics: for a
/// detached action it means admission, not process exit. An empty `Value` remains
/// a value. Streaming output is not a terminal answer for these invocation paths.
///
/// ```
/// use omega_proto::{CommandAnswer, omega::{Empty, result}};
/// let answer = CommandAnswer::try_from(result::Outcome::Ok(Empty {}))?;
/// assert_eq!(answer, CommandAnswer::Acknowledged);
/// # Ok::<(), omega_proto::Refusal>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum CommandAnswer {
    Acknowledged,
    Value(Value),
}

impl CommandAnswer {
    /// Encode the answer without changing its value or acknowledgement semantics.
    pub fn into_outcome(self) -> result::Outcome {
        match self {
            Self::Acknowledged => result::Outcome::Ok(Empty {}),
            Self::Value(value) => result::Outcome::Value(value),
        }
    }
}

impl TryFrom<result::Outcome> for CommandAnswer {
    type Error = Refusal;

    /// Preserve peer refusals; reject other reply kinds with `InvalidArgument`.
    /// Unknown refusal codes become `Unspecified`, as at the frame boundary.
    fn try_from(outcome: result::Outcome) -> Result<Self, Self::Error> {
        match outcome {
            result::Outcome::Ok(_) => Ok(Self::Acknowledged),
            result::Outcome::Value(value) => Ok(Self::Value(value)),
            result::Outcome::Error(error) => Err(Refusal::new(
                ErrorCode::try_from(error.code).unwrap_or(ErrorCode::Unspecified),
                error.message,
            )),
            result::Outcome::State(_)
            | result::Outcome::View(_)
            | result::Outcome::Output(_)
            | result::Outcome::Deployment(_)
            | result::Outcome::Instances(_) => {
                Err(Refusal::invalid("command returned an unexpected outcome"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::omega::{Error, Frame, frame};
    use prost::Message;

    #[test]
    fn answers_preserve_empty_values_and_wire_shape() {
        for answer in [
            CommandAnswer::Acknowledged,
            CommandAnswer::Value(Value::default()),
            CommandAnswer::Value(crate::IntoValue::into_value("answer")),
        ] {
            let frame = Frame::reply(17, answer.clone().into_outcome());
            let decoded = Frame::decode(frame.encode_to_vec().as_slice()).unwrap();
            assert_eq!(decoded.stream_id, 17);
            let Some(frame::Body::Result(result)) = decoded.body else {
                panic!("expected a result");
            };
            assert!(result.done);
            assert_eq!(CommandAnswer::try_from(result.outcome.unwrap()), Ok(answer));
        }
    }

    #[test]
    fn other_successful_reply_kinds_are_not_command_answers() {
        for outcome in [
            result::Outcome::State(Default::default()),
            result::Outcome::View(Default::default()),
            result::Outcome::Output(Default::default()),
            result::Outcome::Deployment(Default::default()),
            result::Outcome::Instances(Default::default()),
        ] {
            assert_eq!(
                CommandAnswer::try_from(outcome).unwrap_err().code,
                ErrorCode::InvalidArgument
            );
        }
    }

    #[test]
    fn refusals_preserve_every_declared_code_and_message() {
        for code in ErrorCode::Unspecified as i32..=ErrorCode::DeadlineExceeded as i32 {
            let code = ErrorCode::try_from(code).unwrap();
            assert_eq!(
                CommandAnswer::try_from(result::Outcome::Error(Error {
                    code: code as i32,
                    message: "peer explanation".into(),
                })),
                Err(Refusal::new(code, "peer explanation"))
            );
        }
        assert_eq!(
            CommandAnswer::try_from(result::Outcome::Error(Error {
                code: i32::MAX,
                message: "future code".into(),
            })),
            Err(Refusal::new(ErrorCode::Unspecified, "future code"))
        );
    }
}
