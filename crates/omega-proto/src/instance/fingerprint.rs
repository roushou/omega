//! Identity of a renderer bundle, reported when its QML attaches.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererFingerprint(String);

#[derive(Debug, thiserror::Error)]
#[error("renderer fingerprint must be 64 lowercase hexadecimal characters")]
pub struct RendererFingerprintError;

impl std::str::FromStr for RendererFingerprint {
    type Err = RendererFingerprintError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for RendererFingerprint {
    type Error = RendererFingerprintError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for RendererFingerprint {
    type Error = RendererFingerprintError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(RendererFingerprintError);
        }
        Ok(Self(value))
    }
}

impl RendererFingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
