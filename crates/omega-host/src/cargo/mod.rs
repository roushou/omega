//! Source-preserving Cargo documents and asynchronous Cargo invocation.
//!
//! [`Manifest`] owns a `Cargo.toml`; [`Config`] owns a `.cargo/config.toml`.
//! Accessors read that same document, and edits retain unrelated keys and comments.
//! Parsing and editing perform no I/O. Use [`crate::Layout::file`] for atomic persistence.
//! [`Cargo`] runs builds, metadata queries, and library-test compilation in an explicit
//! working directory. Callers choose profiles, output directories, and resolution policy.

mod artifacts;
mod command;
mod config;
mod dependency;
mod fields;
mod manifest;
mod package;
mod pattern;
mod request;
mod workspace;

pub use artifacts::{ArtifactError, TestArtifacts};
pub use cargo_metadata::{Metadata, Package as ResolvedPackage, PackageId};
pub use command::{Cargo, InvocationError};
pub use config::Config;
pub use dependency::{Dependencies, Dependency, DependencyDetail};
pub use fields::CargoError;
pub use manifest::{CargoSlot, Manifest};
pub use package::{Inherited, Package};
pub use pattern::{PathPattern, PatternError};
pub use request::{
    BuildRequest, MetadataRequest, PackageSpec, PackageSpecError, Resolution, Selection,
    TestBuildRequest,
};
pub use workspace::{MembersError, Workspace};
