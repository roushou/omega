//! Shared mechanisms independent of Omega's hosts and application policy.
//!
//! [`execution`] declares typed pipelines and reports their outcomes. Workflow
//! owners choose ordering and implementations; operations own effects and recovery. This crate uses only the standard
//! library; filesystem persistence and service integration belong to consumers.

pub mod execution;
