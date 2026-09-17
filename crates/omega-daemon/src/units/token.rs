//! The secret a spawned unit proves itself with.

/// A one-time secret handed to exactly one spawned process.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitToken(String);

impl UnitToken {
    /// Generate a 128-bit token from OS randomness.
    /// Returns an error when the OS cannot supply randomness; no fallback is used.
    pub fn mint() -> Result<Self, TokenError> {
        Self::generate(getrandom::fill)
    }

    fn generate(
        fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
    ) -> Result<Self, TokenError> {
        let mut bytes = [0u8; 16];
        fill(&mut bytes).map_err(TokenError)?;
        Ok(Self(bytes.iter().map(|b| format!("{b:02x}")).collect()))
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

/// OS randomness was unavailable; no token was issued.
#[derive(Debug, thiserror::Error)]
#[error("cannot generate a secure unit identity: {0}")]
pub struct TokenError(getrandom::Error);

impl From<getrandom::Error> for TokenError {
    fn from(error: getrandom::Error) -> Self {
        Self(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn randomness_failure_never_produces_a_token() {
        assert!(UnitToken::generate(|_| Err(getrandom::Error::UNSUPPORTED)).is_err());
        let token = UnitToken::generate(|bytes| {
            bytes.fill(0xab);
            Ok(())
        })
        .unwrap();
        assert_eq!(token.as_str(), "ab".repeat(16));
    }
}
