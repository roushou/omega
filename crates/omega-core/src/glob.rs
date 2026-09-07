//! Wildcard matching for the one place omega needs it: expanding Cargo's
//! `workspace.members` patterns into directories.

use std::path::{Path, PathBuf};

use crate::error::PatternError;

/// A `*`/`?` pattern for a single path segment. Never crosses `/`.
#[derive(Debug, Clone, Copy)]
pub struct Glob<'a>(&'a str);

impl<'a> Glob<'a> {
    pub fn new(pattern: &'a str) -> Self {
        Self(pattern)
    }

    pub fn is_wildcard(&self) -> bool {
        self.0.contains(['*', '?'])
    }

    pub fn matches(&self, text: &str) -> bool {
        let p: Vec<char> = self.0.chars().collect();
        let t: Vec<char> = text.chars().collect();
        let (m, n) = (p.len(), t.len());

        // dp[i][j]: the first i pattern chars match the first j text chars.
        let mut dp = vec![vec![false; n + 1]; m + 1];
        dp[0][0] = true;
        for i in 1..=m {
            if p[i - 1] == '*' {
                dp[i][0] = dp[i - 1][0];
            }
        }
        for i in 1..=m {
            for j in 1..=n {
                dp[i][j] = if p[i - 1] == '*' {
                    dp[i - 1][j] || dp[i][j - 1]
                } else if p[i - 1] == '?' || p[i - 1] == t[j - 1] {
                    dp[i - 1][j - 1]
                } else {
                    false
                };
            }
        }
        dp[m][n]
    }
}

/// A `/`-separated pattern of [`Glob`] segments, expanded against a root into
/// the directories it names.
#[derive(Debug, Clone, Copy)]
pub struct PathPattern<'a>(&'a str);

impl<'a> PathPattern<'a> {
    pub fn new(pattern: &'a str) -> Self {
        Self(pattern)
    }

    pub fn expand(&self, root: &Path) -> Result<Vec<PathBuf>, PatternError> {
        let mut matches = vec![PathBuf::new()];

        for segment in self.0.split('/') {
            let glob = Glob::new(segment);
            let mut next = Vec::new();

            for base in &matches {
                if !glob.is_wildcard() {
                    next.push(base.join(segment));
                    continue;
                }
                next.extend(self.children(root, base, glob)?);
            }
            matches = next;
        }

        Ok(matches.into_iter().map(|rel| root.join(rel)).collect())
    }

    /// The subdirectories of `root/base` whose name matches `glob`.
    fn children(
        &self,
        root: &Path,
        base: &Path,
        glob: Glob<'_>,
    ) -> Result<Vec<PathBuf>, PatternError> {
        let dir = root.join(base);
        let entries = std::fs::read_dir(&dir).map_err(|source| PatternError::Expand {
            pattern: self.0.to_string(),
            dir: dir.clone(),
            source,
        })?;

        let mut matched = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| PatternError::Expand {
                pattern: self.0.to_string(),
                dir: dir.clone(),
                source,
            })?;
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name();
            if glob.matches(&name.to_string_lossy()) {
                matched.push(base.join(name));
            }
        }
        matched.sort();
        Ok(matched)
    }
}
