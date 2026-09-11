# Omega host

Filesystem and configuration support for Omega's CLI and daemon. This crate
owns path resolution, atomic writes, staged build generations, and typed TOML
documents.

`Layout` resolves where Omega's files belong. `AtomicFile` writes and synchronizes
a replacement before exposing it to readers. Generation types manage staged
artifacts and rollback, while `TomlSchema` and `TomlFile` bind configuration
formats to their locations.

These APIs serve Omega's host processes. Plugin authors use
[omega-rs](https://crates.io/crates/omega-rs) and do not need host filesystem
machinery in their plugins.

[API reference](https://docs.rs/omega-host) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.
