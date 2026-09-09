//! The protocol: the generated ontology types and the wire version.

/// The newest wire version this build speaks.
///
/// 2 renumbered `StateTopic.generic`. A unit binary built against 1 still
/// has the right manifest hash, so the handshake is the only thing that can
/// tell it apart from a current one — and reading its keyspace values as an
/// unknown field would drop them silently.
///
/// 3 made the manifest a schema message. The hash a peer presents is now
/// sha256 over `Manifest::canonical` rather than over canonical TOML, so a
/// v2 binary's hash is wrong for reasons no error message could explain.
pub const PROTOCOL_VERSION: u32 = 3;

/// The oldest wire version this build still serves.
///
/// Exact equality made every bump orphan every installed unit binary until
/// it was rebuilt — tolerable while the daemon and its units were always
/// built together, and not tolerable once a unit can be built by something
/// that is not this workspace. A floor says how far back compatibility is
/// promised, and [`effective_version`] says which version a given pair of
/// peers actually ended up speaking.
///
/// It equals [`PROTOCOL_VERSION`] here because 3 broke the manifest hash and
/// nothing older can connect. The next additive change raises the ceiling
/// and leaves this alone.
pub const MIN_PROTOCOL_VERSION: u32 = 3;

/// The version two peers speak, given what the other one offered.
///
/// The lower of the two: a peer never has to understand something newer than
/// it claimed, and neither has to guess.
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
/// `omega.serde.rs`. Both land in this one module because they reference
/// each other's types by bare name.
///
/// The serde half is behind `json`, and the build script does not generate
/// it otherwise. It exists for the observation socket, which a unit never
/// opens — so a plugin compiles the ontology's types without compiling a
/// JSON encoding for every one of them.
#[allow(clippy::all)] // generated code
pub mod omega {
    include!(concat!(env!("OUT_DIR"), "/omega.rs"));
    #[cfg(feature = "json")]
    include!(concat!(env!("OUT_DIR"), "/omega.serde.rs"));
}
