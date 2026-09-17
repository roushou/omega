//! The protocol: the generated ontology types and the wire version.

/// The newest wire version this build speaks. Sent in `Hello` and `Welcome`.
pub const PROTOCOL_VERSION: u32 = 8;

/// The oldest wire version this build still serves.
///
/// A peer offering a version outside `MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION`
/// is refused at the handshake. Raise this only for a change that breaks plugin
/// binaries built against an older version.
pub const MIN_PROTOCOL_VERSION: u32 = 8;

/// The version two peers speak: the lower of [`PROTOCOL_VERSION`] and what
/// the peer offered.
pub const fn effective_version(peer: u32) -> u32 {
    if peer < PROTOCOL_VERSION {
        peer
    } else {
        PROTOCOL_VERSION
    }
}

/// Generated protobuf types and optional protobuf JSON implementations.
/// Both generated files share this module to resolve bare type references.
#[allow(clippy::all)] // generated code
pub mod omega {
    include!(concat!(env!("OUT_DIR"), "/omega.rs"));
    #[cfg(feature = "json")]
    include!(concat!(env!("OUT_DIR"), "/omega.serde.rs"));
}
