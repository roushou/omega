use anyhow::Context;
use omega_host::{
    cargo::{Cargo, Metadata, MetadataRequest, PackageId, TestBuildRequest},
    fs::{Changes, Recursion},
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct Build {
    cargo: Cargo,
    package: String,
    paths: Vec<PathBuf>,
}

impl Build {
    pub(super) async fn new(cargo: Cargo, name: &str) -> anyhow::Result<Self> {
        let metadata = cargo.metadata(MetadataRequest::new()).await?;
        Self::select(&metadata, name)?;

        let mut paths = vec![metadata.workspace_root.into_std_path_buf()];
        for package in metadata.packages {
            if package.source.is_none() {
                let parent = package
                    .manifest_path
                    .parent()
                    .context("package manifest has no parent")?
                    .as_std_path()
                    .to_path_buf();
                if !paths.iter().any(|root| parent.starts_with(root)) {
                    paths.push(parent);
                }
            }
        }

        Ok(Self {
            cargo,
            package: name.to_owned(),
            paths,
        })
    }

    fn select(metadata: &Metadata, name: &str) -> anyhow::Result<PackageId> {
        let mut selected = metadata
            .workspace_packages()
            .into_iter()
            .filter(|package| package.name.as_str() == name);
        let package = selected
            .next()
            .with_context(|| format!("unknown preview workspace package {name}"))?
            .id
            .clone();
        anyhow::ensure!(
            selected.next().is_none(),
            "ambiguous preview workspace package {name}"
        );

        Ok(package)
    }

    pub(super) fn watch(&self) -> anyhow::Result<Changes> {
        Ok(Changes::watch(
            &self.paths.iter().map(|p| p.as_path()).collect::<Vec<_>>(),
            Recursion::Recursive,
        )?)
    }

    pub(super) async fn compile(&self) -> anyhow::Result<PathBuf> {
        // A manifest edit may change the package ID between rebuilds.
        let metadata = self.cargo.metadata(MetadataRequest::new()).await?;
        let package = Self::select(&metadata, &self.package)?;
        let artifacts = self
            .cargo
            .compile_tests(TestBuildRequest::library(package))
            .await
            .context("preview build failed; save to retry")?;
        Ok(artifacts
            .library_test()
            .context(
                "package needs a library test target; register previews::preview in its library",
            )?
            .to_path_buf())
    }
}
