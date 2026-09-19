//! Executable host identity and bounded execution policy.

mod identity;
mod policy;

pub use identity::{HostId, InvocationId, ProcessId};
pub use policy::{ExecutionPolicy, HostPolicy, HostPolicyError, Lifetime, StartPolicy};
