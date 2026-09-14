//! Typed access and conversion for protocol values, settings, and records.

use std::collections::HashMap;

use crate::omega::{ListValue, MapValue, Value, value};

/// A map of named values: settings, props, a unit's own state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values {
    entries: HashMap<String, Value>,
}

impl Values {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_map(entries: HashMap<String, Value>) -> Self {
        Self { entries }
    }

    /// One value, if it is there and it is of this type.
    pub fn get<T: FromValue>(&self, name: &str) -> Option<T> {
        T::from_value(self.entries.get(name)?)
    }

    /// One value, or what the reader decided it should be.
    pub fn get_or<T: FromValue>(&self, name: &str, fallback: T) -> T {
        self.get(name).unwrap_or(fallback)
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl IntoValue) {
        self.entries.insert(name.into(), value.into_value());
    }

    /// The same, chained: `Values::new().with("gap", 6)`.
    pub fn with(mut self, name: impl Into<String>, value: impl IntoValue) -> Self {
        self.set(name, value);
        self
    }

    /// Merge over `base`, overriding matching keys and preserving all others.
    pub fn over(mut self, base: &Values) -> Self {
        for (name, value) in &base.entries {
            self.entries
                .entry(name.clone())
                .or_insert_with(|| value.clone());
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn into_map(self) -> HashMap<String, Value> {
        self.entries
    }

    pub fn as_map(&self) -> &HashMap<String, Value> {
        &self.entries
    }
}

/// A type a `Value` can hold.
pub trait FromValue: Sized {
    fn from_value(value: &Value) -> Option<Self>;
}

/// A type that can become a `Value`.
pub trait IntoValue {
    fn into_value(self) -> Value;
}

macro_rules! scalar {
    ($type:ty, $variant:ident, $out:expr, $back:expr) => {
        impl FromValue for $type {
            fn from_value(value: &Value) -> Option<Self> {
                #[allow(clippy::redundant_closure_call)]
                match value.kind.as_ref()? {
                    value::Kind::$variant(held) => ($back)(held),
                    _ => None,
                }
            }
        }

        impl IntoValue for $type {
            fn into_value(self) -> Value {
                #[allow(clippy::redundant_closure_call)]
                Value {
                    kind: Some(value::Kind::$variant(($out)(self))),
                }
            }
        }
    };
}

scalar!(String, StringValue, |text| text, |held: &String| Some(
    held.clone()
));
scalar!(bool, BoolValue, |flag| flag, |held: &bool| Some(*held));
scalar!(f64, DoubleValue, |number| number, |held: &f64| Some(*held));
scalar!(i64, IntValue, |number| number, |held: &i64| Some(*held));
scalar!(u8, IntValue, i64::from, |held: &i64| u8::try_from(*held)
    .ok());
scalar!(u32, IntValue, i64::from, |held: &i64| u32::try_from(*held)
    .ok());
scalar!(i32, IntValue, i64::from, |held: &i64| i32::try_from(*held)
    .ok());
scalar!(u64, IntValue, |number: u64| number as i64, |held: &i64| {
    u64::try_from(*held).ok()
});

/// Encode or decode lists of values.
/// Decoding fails for the entire list if any element cannot be decoded.
impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Value {
        Value {
            kind: Some(value::Kind::List(ListValue {
                values: self.into_iter().map(IntoValue::into_value).collect(),
            })),
        }
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: &Value) -> Option<Self> {
        match value.kind.as_ref()? {
            value::Kind::List(list) => list.values.iter().map(T::from_value).collect(),
            _ => None,
        }
    }
}

impl IntoValue for &str {
    fn into_value(self) -> Value {
        self.to_string().into_value()
    }
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}

/// Bidirectional conversion between a struct and a values map.
/// Derived settings/record readers use defaults for missing fields.
pub trait Fields: Sized {
    fn read(values: &Values) -> Self;
    fn write(&self) -> Values;
}

/// Use a values map directly as raw settings or record fields.
impl Fields for Values {
    fn read(values: &Values) -> Self {
        values.clone()
    }

    fn write(&self) -> Values {
        self.clone()
    }
}

/// Conversion for types represented directly as a generic value map.
impl Fields for () {
    fn read(_values: &Values) -> Self {}

    fn write(&self) -> Values {
        Values::new()
    }
}

impl FromValue for Values {
    fn from_value(value: &Value) -> Option<Self> {
        match value.kind.as_ref()? {
            value::Kind::Map(map) => Some(Self::from_map(map.entries.clone())),
            _ => None,
        }
    }
}

impl IntoValue for Values {
    fn into_value(self) -> Value {
        Value {
            kind: Some(value::Kind::Map(MapValue {
                entries: self.into_map(),
            })),
        }
    }
}

/// Unit denotes the absence of a return value.
impl IntoValue for () {
    fn into_value(self) -> Value {
        Value::default()
    }
}

/// An optional value is encoded as an absent kind when it is `None`.
impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Value {
        self.map(IntoValue::into_value).unwrap_or_default()
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value) -> Option<Self> {
        match value.kind {
            None => Some(None),
            Some(_) => T::from_value(value).map(Some),
        }
    }
}

/// Power-profile commands use the schema's stable string names.
impl FromValue for crate::omega::PowerProfile {
    fn from_value(value: &Value) -> Option<Self> {
        match Self::from_str_name(&String::from_value(value)?)? {
            Self::Unspecified => None,
            profile => Some(profile),
        }
    }
}
impl IntoValue for crate::omega::PowerProfile {
    fn into_value(self) -> Value {
        self.as_str_name().into_value()
    }
}
