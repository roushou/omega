//! Positional values delivered to a command.
use omega_proto::{FromValue, omega::Value};

/// What a command was called with.
#[derive(Debug, Clone, Default)]
pub struct Args {
    values: Vec<Value>,
}

impl Args {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    /// The argument at this position, if it is of this type.
    pub fn get<T: FromValue>(&self, index: usize) -> Option<T> {
        T::from_value(self.values.get(index)?)
    }

    pub(crate) fn into_values(self) -> Vec<Value> {
        self.values
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
