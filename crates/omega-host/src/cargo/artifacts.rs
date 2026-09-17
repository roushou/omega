use cargo_metadata::{Artifact, PackageId, TargetKind};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// Library-test executables emitted for the requested package. Dependency artifacts
/// and non-test outputs are excluded using Cargo's package ID and target metadata.
#[derive(Debug)]
pub struct TestArtifacts {
    package: PackageId,
    executables: BTreeSet<PathBuf>,
    diagnostics: String,
}

impl TestArtifacts {
    pub(super) fn new(package: PackageId) -> Self {
        Self {
            package,
            executables: BTreeSet::new(),
            diagnostics: String::new(),
        }
    }

    pub(super) fn record(&mut self, artifact: Artifact) {
        if artifact.package_id != self.package || !artifact.profile.test {
            return;
        }
        let library = artifact.target.kind.iter().any(|kind| {
            matches!(
                kind,
                TargetKind::Lib
                    | TargetKind::RLib
                    | TargetKind::DyLib
                    | TargetKind::CDyLib
                    | TargetKind::StaticLib
                    | TargetKind::ProcMacro
            )
        });
        if library && let Some(executable) = artifact.executable {
            self.executables.insert(executable.into_std_path_buf());
        }
    }

    /// Require exactly one executable; never silently choose the last artifact.
    pub fn library_test(&self) -> Result<&Path, ArtifactError> {
        match self.executables.len() {
            0 => Err(ArtifactError::Missing(self.package.clone())),
            1 => Ok(self.executables.first().expect("one executable").as_path()),
            _ => Err(ArtifactError::Ambiguous(self.package.clone())),
        }
    }

    /// Compiler diagnostics and non-JSON stdout, retaining the first 64 KiB plus
    /// a truncation marker. Cargo's own stderr is inherited during compilation.
    pub fn diagnostics(&self) -> &str {
        &self.diagnostics
    }

    pub(super) fn diagnostic(&mut self, text: &str) {
        const LIMIT: usize = 64 * 1024;
        if self.diagnostics.len() > LIMIT {
            return;
        }
        let remaining = LIMIT - self.diagnostics.len();
        if text.len() <= remaining {
            self.diagnostics.push_str(text);
        } else {
            let mut end = remaining;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            self.diagnostics.push_str(&text[..end]);
            self.diagnostics
                .push_str("\n[Cargo diagnostics truncated]\n");
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("Cargo emitted no library-test executable for {0}")]
    Missing(PackageId),
    #[error("Cargo emitted multiple library-test executables for {0}")]
    Ambiguous(PackageId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_bounded_without_splitting_utf8_and_mark_truncation_once() {
        let package = serde_json::from_str("\"example\"").unwrap();
        let mut artifacts = TestArtifacts::new(package);
        artifacts.diagnostic(&"x".repeat(64 * 1024 - 1));
        artifacts.diagnostic("é");
        let truncated = artifacts.diagnostics().to_owned();
        assert!(truncated.ends_with("\n[Cargo diagnostics truncated]\n"));
        artifacts.diagnostic("ignored");
        assert_eq!(artifacts.diagnostics(), truncated);
    }
}
