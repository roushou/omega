//! Typed command arguments, decoded before command execution.
use crate::{Args, Error, Percent};
use omega_proto::omega::Value;
use omega_proto::{FromValue, IntoValue};

/// A command's input at the invocation boundary.
///
/// Scalar inputs consume exactly one argument; `()` consumes none. Derive
/// [`omega::Input`](crate::Input) for a strict map or [`omega::Form`](crate::Form)
/// for a map whose text fields also describe a form.
///
/// Scalar commands also accept text from `omega run`, parsed according to their
/// input type. `Percent` accepts `40%` or a fraction such as `0.4`; power profiles
/// accept `saver`, `balanced`, or `performance`. String inputs remain unchanged.
///
/// ```
/// use omega::{Args, Input, Percent, config::IntoValue};
/// let level = Percent::decode(Args::new(vec!["40%".into_value()])).unwrap();
/// assert_eq!(level, Percent::whole(40));
/// assert_eq!(String::decode(Args::new(vec!["40%".into_value()])).unwrap(), "40%");
/// ```
pub trait Input: Sized + Send + 'static {
    fn decode(args: Args) -> Result<Self, Error>;
    fn encode(self) -> Vec<Value>;
}

impl Input for () {
    fn decode(args: Args) -> Result<Self, Error> {
        if args.is_empty() {
            Ok(())
        } else {
            Err(Error::invalid("expected no arguments"))
        }
    }
    fn encode(self) -> Vec<Value> {
        Vec::new()
    }
}

impl Input for Args {
    fn decode(args: Args) -> Result<Self, Error> {
        Ok(args)
    }
    fn encode(self) -> Vec<Value> {
        self.into_values()
    }
}

macro_rules! scalar_input {
    ($($ty:ty),* $(,)?) => { $(
        impl Input for $ty {
            fn decode(args: Args) -> Result<Self, Error> {
                if args.len() != 1 { return Err(Error::invalid("expected one argument")); }
                args.get(0).ok_or_else(|| Error::invalid(concat!("expected ", stringify!($ty))))
            }
            fn encode(self) -> Vec<Value> { vec![self.into_value()] }
        }
    )* };
}
scalar_input!(
    String,
    omega_proto::ApplicationId,
    omega_proto::PlayerId,
    omega_proto::BluetoothDeviceId,
    Option<omega_proto::PlayerId>
);

struct ScalarInput;
impl ScalarInput {
    fn decode<T: FromValue>(
        args: Args,
        expected: &str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> Result<T, Error> {
        if args.len() != 1 {
            return Err(Error::invalid(format!("expected one argument: {expected}")));
        }
        args.get::<T>(0)
            .or_else(|| args.get::<String>(0).and_then(|text| parse(&text)))
            .ok_or_else(|| Error::invalid(format!("expected {expected}")))
    }

    fn percent(text: &str) -> Option<Percent> {
        let fraction = match text.strip_suffix('%') {
            Some(percent) => percent.parse::<f64>().ok()? / 100.0,
            None => text.parse::<f64>().ok()?,
        };
        Percent::from_value(&fraction.into_value())
    }

    fn profile(text: &str) -> Option<omega_proto::omega::PowerProfile> {
        use omega_proto::omega::PowerProfile;
        match text {
            "saver" | "power-saver" => Some(PowerProfile::Saver),
            "balanced" => Some(PowerProfile::Balanced),
            "performance" => Some(PowerProfile::Performance),
            _ => None,
        }
    }
}

macro_rules! text_input {
    ($ty:ty, $expected:literal, $parse:expr) => {
        impl Input for $ty {
            fn decode(args: Args) -> Result<Self, Error> {
                ScalarInput::decode(args, $expected, $parse)
            }
            fn encode(self) -> Vec<Value> {
                vec![self.into_value()]
            }
        }
    };
}
text_input!(bool, "true or false", |text| text.parse().ok());
text_input!(i64, "a signed 64-bit integer", |text| text.parse().ok());
text_input!(u64, "an unsigned 64-bit integer", |text| text.parse().ok());
text_input!(f64, "a number", |text| text.parse().ok());
text_input!(
    Percent,
    "a percentage from 0% to 100% or a fraction from 0 to 1",
    ScalarInput::percent
);
text_input!(
    omega_proto::omega::PowerProfile,
    "a power profile: saver, balanced, or performance",
    ScalarInput::profile
);

impl FromValue for Percent {
    fn from_value(value: &Value) -> Option<Self> {
        let fraction = f64::from_value(value)?;
        if fraction.is_finite() && (0.0..=1.0).contains(&fraction) {
            Some(Self::of(fraction))
        } else {
            None
        }
    }
}
impl IntoValue for Percent {
    fn into_value(self) -> Value {
        self.fraction().into_value()
    }
}
