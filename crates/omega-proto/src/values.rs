//! Narrowing the generic data model.
//!
//! `Value` is the escape hatch the ontology leaves open — a unit's own
//! keyspace, a widget's props, an instance's settings. It is a oneof, and
//! matching one at a call site is how a typo becomes a silent default.
//!
//! This is the narrowing the schema asks for: a map with typed accessors, and
//! [`Fields`] for the structs that *are* such a map. Two sides need it and
//! neither is the daemon — a plugin reads its settings, the config plane
//! writes them — so it lives here, with the type they are both made of.

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

    /// These values laid over `base`: everything both scopes said, with this
    /// one winning key by key.
    ///
    /// A widget instance's settings over its unit's, so configuring a plugin
    /// once is not repeated at every placement — and naming one key in a
    /// placement does not silently reset the rest to their defaults, which is
    /// what replacing rather than layering would do.
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

/// A list of anything that is itself a value.
///
/// Without this a unit can hold a scalar in its keyspace and nothing else,
/// which closes the escape hatch composition rests on: a workspace unit could
/// not publish its workspaces, and a media unit could not publish a queue.
///
/// All or nothing on the way back. A list with one element the reader cannot
/// make sense of is a list it does not understand, and quietly dropping the
/// element would hand back a shorter list that looks complete.
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

/// A struct that is a map of values.
///
/// Derived at both ends of every boundary a map crosses: a plugin's settings
/// are written by the config plane and read by the plugin, and a plugin's own
/// state is written by it and read by anyone it lets. Deriving both directions
/// from the same fields is what keeps the two ends spelling things the same
/// way.
///
/// Reading is total: a field the map does not carry takes its default, so
/// adding a field never breaks a writer that predates it.
pub trait Fields: Sized {
    fn read(values: &Values) -> Self;
    fn write(&self) -> Values;
}

/// A map is already its own fields.
///
/// For settings whose shape is not a type — a document configuring a unit
/// whose settings type it cannot name, and every test that would otherwise
/// write a `Fields` impl to say two keys.
impl Fields for Values {
    fn read(values: &Values) -> Self {
        values.clone()
    }

    fn write(&self) -> Values {
        self.clone()
    }
}

/// A type that is a whole map, for a value that has no fields to speak of.
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
