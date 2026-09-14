//! Positional values delivered to a command.
use omega_proto::{FromValue, omega::Value};

/// Raw positional command arguments.
#[derive(Debug, Clone, Default)]
pub struct Args {
    values: Vec<Value>,
}

impl Args {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    /// Return the argument at `index`, or `None` if it is missing or cannot be decoded as `T`.
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
