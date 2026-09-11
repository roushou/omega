//! The protocol: the generated ontology types and the wire version.

/// The newest wire version this build speaks. Sent in `Hello` and `Welcome`.
pub const PROTOCOL_VERSION: u32 = 2;

/// The oldest wire version this build still serves.
///
/// A peer offering a version outside `MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION`
/// is refused at the handshake. Raise this only for a change that breaks unit
/// binaries built against an older version.
pub const MIN_PROTOCOL_VERSION: u32 = 2;

/// The version two peers speak: the lower of [`PROTOCOL_VERSION`] and what
/// the peer offered.
pub const fn effective_version(peer: u32) -> u32 {
    if peer < PROTOCOL_VERSION {
        peer
    } else {
        PROTOCOL_VERSION
    }
}

/// Generated protobuf types.
///
/// prost-build consolidates every file sharing `package omega` into a single
/// `omega.rs`; pbjson-build emits the matching serde impls into
/// `omega.serde.rs`. Both are included here because they reference each
/// other's types by bare name.
///
/// The serde half is behind the `json` feature and is not generated
/// otherwise, so a unit compiles the ontology without a JSON encoding for
/// every type.
#[allow(clippy::all)] // generated code
pub mod omega {
    include!(concat!(env!("OUT_DIR"), "/omega.rs"));
    #[cfg(feature = "json")]
    include!(concat!(env!("OUT_DIR"), "/omega.serde.rs"));
}
