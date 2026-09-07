//! The protocol: the generated ontology types and the wire version.

/// The wire protocol version. Both peers must agree; a `Hello` mismatch
/// closes the connection.
pub const PROTOCOL_VERSION: u32 = 1;

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
