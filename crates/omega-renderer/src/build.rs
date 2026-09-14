//! Content identity embedded in the QML connection, never read from disk at attachment time.
use crate::Asset;
use omega_proto::instance::RendererFingerprint;
use sha2::{Digest, Sha256};
use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct Build(RendererFingerprint);
impl Build {
    const MARKER: &'static str = "readonly property string buildFingerprint: \"\"";

    pub fn of<'a>(version: &str, assets: impl IntoIterator<Item = &'a Asset>) -> Self {
        let mut assets: Vec<_> = assets.into_iter().collect();
        assets.sort_by_key(|asset| asset.name);
        let mut hash = Sha256::new();
        for bytes in std::iter::once(version.as_bytes()).chain(
            assets
                .iter()
                .flat_map(|asset| [asset.name.as_bytes(), asset.contents.as_bytes()]),
        ) {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        Self(
            RendererFingerprint::parse(&format!("{:x}", hash.finalize()))
                .expect("SHA-256 hex digest"),
        )
    }
    pub fn fingerprint(&self) -> &str {
        self.0.as_str()
    }

    pub fn contents<'a>(&self, asset: &'a Asset) -> Cow<'a, str> {
        if asset.name == "core/RendererConnection.qml" {
            assert_eq!(
                asset.contents.matches(Self::MARKER).count(),
                1,
                "renderer connection requires one fingerprint marker"
            );
            Cow::Owned(asset.contents.replace(
                Self::MARKER,
                &format!(
                    "readonly property string buildFingerprint: \"{}\"",
                    self.fingerprint()
                ),
            ))
        } else {
            Cow::Borrowed(asset.contents)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_covers_version_names_and_contents_independent_of_order() {
        let a = Asset {
            name: "a",
            contents: "one",
        };
        let b = Asset {
            name: "b",
            contents: "two",
        };
        let base = Build::of("1", [&a, &b]);
        assert_eq!(base.fingerprint(), Build::of("1", [&b, &a]).fingerprint());
        assert_ne!(base.fingerprint(), Build::of("2", [&a, &b]).fingerprint());
        assert_ne!(
            base.fingerprint(),
            Build::of(
                "1",
                [
                    &a,
                    &Asset {
                        contents: "changed",
                        ..b
                    }
                ]
            )
            .fingerprint()
        );
        assert_ne!(
            base.fingerprint(),
            Build::of(
                "1",
                [
                    &a,
                    &Asset {
                        name: "renamed",
                        ..b
                    }
                ]
            )
            .fingerprint()
        );
    }
    #[test]
    fn loaded_connection_reports_the_installed_bundle_not_a_disk_lookup() {
        let build = crate::Desktop::build();
        let connection = crate::Core::FILES
            .iter()
            .find(|a| a.name == "core/RendererConnection.qml")
            .unwrap();
        assert!(build.contents(connection).contains(build.fingerprint()));
        assert!(!connection.contents.contains(build.fingerprint()));
    }
}
