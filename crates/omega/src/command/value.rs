//! Strict command value codecs and their discoverable wire shapes.
use crate::{Error, Percent};
use omega_proto::omega::{CommandType, Value, command_type::Kind};
use omega_proto::{FromValue, IntoValue};

/// A command value with strict decoding and a transport-neutral description.
/// Derive `omega::Output` for a record returned by a command. Custom types may
/// implement this trait and the value conversions; decoding must reject invalid data.
/// Integer wire values are signed 64-bit; `u64` values above `i64::MAX` are rejected.
///
/// ```
/// use omega::{command::CommandValue, config::IntoValue};
/// #[derive(Debug, PartialEq, omega::Output)]
/// struct Summary { name: String, count: u32 }
/// let value = Summary { name: "Inbox".into(), count: 3 }.into_value();
/// let summary = Summary::decode_value(&value).unwrap();
/// assert_eq!(summary.count, 3);
/// assert!(Summary::decode_value(&"invalid".into_value()).is_err());
/// ```
pub trait CommandValue: IntoValue + FromValue + Send + 'static {
    fn shape() -> CommandType;
    fn decode_value(value: &Value) -> Result<Self, Error> {
        Self::shape()
            .accepts(value)
            .map_err(|e| Error::invalid(e.to_string()))?;
        Self::from_value(value).ok_or_else(|| Error::invalid("invalid command value"))
    }
}

macro_rules! scalar {
    ($ty:ty, $kind:ident) => {
        impl CommandValue for $ty {
            fn shape() -> CommandType {
                CommandType::of(Kind::$kind)
            }
        }
    };
}
scalar!((), Unit);
scalar!(bool, Boolean);
scalar!(String, Text);
scalar!(i64, Integer);
scalar!(f64, Number);
macro_rules! integer {
    ($($ty:ty),*) => { $(impl CommandValue for $ty {
        fn shape() -> CommandType { CommandType { minimum: Some(<$ty>::MIN as f64), maximum: Some((<$ty>::MAX as u64).min(i64::MAX as u64) as f64), ..CommandType::of(Kind::Integer) } }
    })* };
}
integer!(u8, u32, u64, i32);
macro_rules! identifier {
    ($($ty:ty),*) => { $(impl CommandValue for $ty {
        fn shape() -> CommandType { CommandType { identity: stringify!($ty).into(), ..CommandType::of(Kind::Text) } }
    })* };
}
identifier!(
    omega_proto::ApplicationId,
    omega_proto::PlayerId,
    omega_proto::BluetoothDeviceId,
    omega_proto::WorkspaceName
);
impl CommandValue for omega_proto::WorkspaceIndex {
    fn shape() -> CommandType {
        CommandType {
            minimum: Some(1.0),
            maximum: Some(i32::MAX as f64),
            ..CommandType::of(Kind::Integer)
        }
    }
}

impl CommandValue for Percent {
    fn shape() -> CommandType {
        CommandType {
            minimum: Some(0.0),
            maximum: Some(1.0),
            identity: "percent".into(),
            ..CommandType::of(Kind::Number)
        }
    }
}

impl CommandValue for omega_proto::omega::PowerProfile {
    fn shape() -> CommandType {
        CommandType {
            choices: vec![
                "POWER_PROFILE_SAVER".into(),
                "POWER_PROFILE_BALANCED".into(),
                "POWER_PROFILE_PERFORMANCE".into(),
            ],
            ..CommandType::of(Kind::Choice)
        }
    }
}

impl<T: CommandValue> CommandValue for Option<T> {
    fn shape() -> CommandType {
        CommandType {
            element: Some(Box::new(T::shape())),
            ..CommandType::of(Kind::Optional)
        }
    }
}

impl<T: CommandValue> CommandValue for Vec<T> {
    fn shape() -> CommandType {
        CommandType {
            element: Some(Box::new(T::shape())),
            ..CommandType::of(Kind::List)
        }
    }
}
