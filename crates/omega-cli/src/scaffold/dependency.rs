use omega_host::cargo::Dependency;

/// Where a generated dependency comes from. The declaration; [`Dependency`]
/// is the resolved result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySource {
    /// A crates.io requirement.
    Registry(&'static str),
    /// One of Omega's crates, using the scaffold's selected release version.
    OmegaCrate,
}

/// One line of a generator's dependency table.
#[derive(Debug, Clone, Copy)]
pub struct DependencySpec {
    pub name: &'static str,
    /// What the registry calls it, when that is not `name`.
    pub package: Option<&'static str>,
    pub source: DependencySource,
    pub features: &'static [&'static str],
}

impl DependencySpec {
    pub const fn registry(name: &'static str, version: &'static str) -> Self {
        Self {
            name,
            package: None,
            source: DependencySource::Registry(version),
            features: &[],
        }
    }

    pub const fn omega(name: &'static str) -> Self {
        Self {
            name,
            package: None,
            source: DependencySource::OmegaCrate,
            features: &[],
        }
    }

    /// Called one thing in a manifest, published as another.
    pub const fn published_as(mut self, package: &'static str) -> Self {
        self.package = Some(package);
        self
    }

    /// What the registry — and so a `[patch]` table — calls this crate.
    pub const fn package(&self) -> &'static str {
        match self.package {
            Some(package) => package,
            None => self.name,
        }
    }

    pub const fn with_features(mut self, features: &'static [&'static str]) -> Self {
        self.features = features;
        self
    }

    /// Construct a workspace-inherited dependency entry.
    pub fn inherited(&self) -> (&'static str, Dependency) {
        (self.name, Dependency::inherited())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_host::cargo::Dependencies;

    #[test]
    fn a_member_crate_inherits_exactly_the_declared_dependencies() {
        const SPECS: &[DependencySpec] = &[
            DependencySpec::registry("tokio", "1").with_features(&["macros"]),
            DependencySpec::omega("omega"),
        ];

        let inherited = Dependencies::from_iter(SPECS.iter().map(DependencySpec::inherited));

        assert_eq!(
            inherited.names().collect::<Vec<_>>(),
            vec!["omega", "tokio"]
        );
        assert!(
            inherited
                .iter()
                .all(|(_, dependency)| dependency.is_inherited())
        );
    }
}
