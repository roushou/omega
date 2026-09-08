//! The protocol: the generated ontology types and the wire version.

/// The wire protocol version. Both peers must agree; a `Hello` mismatch
/// closes the connection.
///
/// 2 renumbered `StateTopic.generic`. A unit binary built against 1 still
/// has the right manifest hash, so the handshake is the only thing that can
/// tell it apart from a current one — and reading its keyspace values as an
/// unknown field would drop them silently.
pub const PROTOCOL_VERSION: u32 = 2;

/// Generated protobuf types.
///
/// prost-build consolidates every file sharing `package omega` into a single
/// `omega.rs`; pbjson-build emits the matching serde impls into
/// `omega.serde.rs`. Both are included in this one module because they
/// reference each other's types by bare name.
#[allow(clippy::all)] // generated code
pub mod omega {
    include!(concat!(env!("OUT_DIR"), "/omega.rs"));
    include!(concat!(env!("OUT_DIR"), "/omega.serde.rs"));
}
