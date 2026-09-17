use omega_proto::PluginName;

/// A workspace package name usable as a Rust crate identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageName {
    plugin: PluginName,
    rust_ident: String,
}

impl std::str::FromStr for PackageName {
    type Err = PackageNameError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let plugin = PluginName::try_from(input)?;
        let rust_ident = input.replace('-', "_");
        // Include reserved keywords across supported editions; templates use plain paths.
        const RESERVED: &[&str] = &[
            "as",
            "async",
            "await",
            "break",
            "const",
            "continue",
            "crate",
            "dyn",
            "else",
            "enum",
            "extern",
            "false",
            "fn",
            "for",
            "if",
            "impl",
            "in",
            "let",
            "loop",
            "match",
            "mod",
            "move",
            "mut",
            "pub",
            "ref",
            "return",
            "self",
            "static",
            "struct",
            "super",
            "trait",
            "true",
            "type",
            "unsafe",
            "use",
            "where",
            "while",
            "abstract",
            "become",
            "box",
            "do",
            "final",
            "gen",
            "macro",
            "override",
            "priv",
            "try",
            "typeof",
            "unsized",
            "virtual",
            "yield",
            "std",
            "core",
            "alloc",
            "test",
            "proc_macro",
            "system",
            "omega",
        ];
        if RESERVED.contains(&rust_ident.as_str()) {
            return Err(PackageNameError::Reserved(input.to_owned()));
        }
        Ok(Self { plugin, rust_ident })
    }
}

impl PackageName {
    pub fn plugin(&self) -> &PluginName {
        &self.plugin
    }
    pub fn package(&self) -> &str {
        self.plugin.as_str()
    }
    pub fn rust_ident(&self) -> &str {
        &self.rust_ident
    }
}

/// An invalid config package name.
#[derive(Debug, thiserror::Error)]
pub enum PackageNameError {
    #[error(transparent)]
    Identifier(#[from] omega_proto::IdentError),
    #[error("{0:?} cannot be used as a config crate name; choose a different name")]
    Reserved(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_must_work_as_rust_crates() {
        for name in ["type", "gen", "self", "system", "omega", "proc-macro"] {
            assert!(name.parse::<PackageName>().is_err(), "{name}");
        }
        assert_eq!(
            "audio-output".parse::<PackageName>().unwrap().rust_ident(),
            "audio_output"
        );
    }
}
