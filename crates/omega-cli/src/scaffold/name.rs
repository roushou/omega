use anyhow::{Result, bail};
use omega_proto::UnitName;

/// A protocol identity that can also be emitted as a Rust crate identifier.
#[derive(Debug)]
pub struct PluginName {
    unit: UnitName,
    rust_ident: String,
}

impl PluginName {
    pub fn parse(input: &str) -> Result<Self> {
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
            bail!("{input:?} cannot be used as a plugin crate name; choose a different name");
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
