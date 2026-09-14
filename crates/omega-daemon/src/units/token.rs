//! The secret a spawned unit proves itself with.

/// A one-time secret handed to exactly one spawned process.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitToken(String);

impl UnitToken {
    /// Generate a 128-bit token from OS randomness.
    pub fn mint() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the OS must provide randomness");
        Self(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UnitToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
