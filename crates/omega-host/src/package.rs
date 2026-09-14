use omega_proto::UnitName;

/// A workspace package name usable as a Rust crate identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageName {
    unit: UnitName,
    rust_ident: String,
}

impl PackageName {
    pub fn parse(input: &str) -> Result<Self, PackageNameError> {
        let unit = UnitName::parse(input)?;
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
        Ok(Self { unit, rust_ident })
    }

    pub fn unit(&self) -> &UnitName {
        &self.unit
    }
    pub fn package(&self) -> &str {
        self.unit.as_str()
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
