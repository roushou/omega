# Omega protocol

The shared protocol between Omega's daemon, plugins, configuration tools, and
renderer. This crate provides message types, validated identifiers, manifests,
framing, transport, and handshake support.

Protobuf schemas define state topics, actions, events, desired configuration,
and view trees. Rust types are generated at build time; `protoc` is bundled, so
building this crate does not require a separate compiler installation.

## JSON support

The optional `json` feature enables the protobuf JSON mapping used by the
observation socket and document serialization. It is disabled by default. Native
plugins use the binary protocol and do not need the generated JSON support.

This is a protocol library for Omega components. Plugin authors should normally
use [omega-rs](https://crates.io/crates/omega-rs), which exposes domain handles and
typed commands over these messages.

[API reference](https://docs.rs/omega-proto) ·
[Schema contract](https://github.com/roushou/omega/blob/main/crates/omega-proto/schema/README.md) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.
