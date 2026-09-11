//! Typed command arguments, decoded before command execution.
use crate::{Args, Error, Percent};
use omega_proto::omega::Value;
use omega_proto::{FromValue, IntoValue};

/// A command's input at the invocation boundary.
///
/// Scalar inputs consume exactly one argument; `()` consumes none. Derive
/// [`omega::Input`](crate::Input) for a strict map or [`omega::Form`](crate::Form)
/// for a map whose text fields also describe a form.
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
scalar_input!(String, bool, f64, i64, u64, Percent);

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
