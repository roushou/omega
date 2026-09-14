//! Identity of a renderer bundle, reported when its QML attaches.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererFingerprint(String);

#[derive(Debug, thiserror::Error)]
#[error("renderer fingerprint must be 64 lowercase hexadecimal characters")]
pub struct RendererFingerprintError;

impl RendererFingerprint {
    pub fn parse(value: &str) -> Result<Self, RendererFingerprintError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(RendererFingerprintError);
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
