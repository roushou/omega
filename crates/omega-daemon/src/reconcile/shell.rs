//! Explicit shell application uses the same validated generation and installer as activation.
use super::ValidatedBuild;
use omega_host::{Generations, Layout};

#[derive(Debug, thiserror::Error)]
pub enum ShellApplyError {
    #[error("nothing built; run omega build")]
    NothingBuilt,
    #[error("this generation declares no shell")]
    NotDeclared,
    #[error(transparent)]
    Build(#[from] crate::DaemonError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Install(#[from] omega_host::shell::InstallError),
    #[error(transparent)]
    Compile(#[from] omega_document::shell::ShellError),
}
#[derive(Debug)]
pub struct ShellApplication;
impl ShellApplication {
    pub fn apply(layout: &Layout, overwrite: bool) -> Result<(), ShellApplyError> {
        let generation = Generations::new(layout)
            .pin_current()?
            .ok_or(ShellApplyError::NothingBuilt)?;
        let build = ValidatedBuild::read(generation)?;
        if build.document.shell_json.is_empty() {
            return Err(ShellApplyError::NotDeclared);
        }
        build.apply_shell(layout, overwrite)
    }
}
