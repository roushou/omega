//! Cargo source documents with unknown fields preserved across edits.
mod config;
mod dependency;
mod manifest;
mod package;
mod profile;
pub use config::CargoConfig;
pub use dependency::{
    Dependencies, Dependency, DependencyDetail, DependencySource, DependencySpec,
};
pub use manifest::{CargoManifest, CargoSlot, Workspace};
pub use package::{Edition, Package, WorkspacePackage};
pub use profile::{Profile, ReleaseProfile};
