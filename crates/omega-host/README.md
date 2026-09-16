# Omega host

Filesystem and configuration support for Omega's CLI and daemon. This crate
owns path resolution, atomic writes, staged build generations, and typed TOML
documents and source workspace discovery.

`Layout` resolves where Omega's files belong. `AtomicFile` writes and synchronizes
a replacement before exposing it to readers. Generation types manage staged
artifacts and rollback, while `TomlSchema` and `TomlFile` bind configuration
formats to their locations.

`workspace` owns Cargo schemas, source roles, member-pattern expansion, and
runnable plugin discovery. `fs::Changes` provides settled filesystem watching when
the optional `watch` feature is enabled; ordinary document consumers do not need it.

These APIs serve Omega's host processes. Plugin authors use
[omega-rs](https://crates.io/crates/omega-rs) and do not need host filesystem
machinery in their plugins.

[API reference](https://docs.rs/omega-host) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.

## Workflows and recovery

Pipeline execution lives in `omega-base::execution`.
`recovery::RecoveryStore` records recoverable changes before
effects and supports explicit inspection and restoration after interruption.
`recovery::Replacement` applies those contracts to configuration files and asset
trees. See [pipelines and recovery](../../docs/workflows.md) for semantics and examples.
