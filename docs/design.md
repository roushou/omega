# Open design questions

The [architecture](architecture.md) describes the implemented contracts, and the
[desktop platform design](desktop-platform.md) records composition decisions.
[Desktop capability surface](desktop-capabilities.md) is the reference for the
UI primitives and platform capabilities an author can build with.
This file tracks remaining limitations and future work.

[Command interoperability](command-interoperability.md) documents typed calls,
scoped discovery, and the launcher integration. Dynamic permission adoption and
transactional routines remain outside that contract.

## Persistence and recovery

- **Records:** replication survives plugin restarts while the daemon lives, but not
  a daemon restart. Shared [storage](storage.md) adds committed key-value operations with memory
  and JSON backends. Automatic schema migration and a transition from `Own`/`Watch`
  remain open; existing records retain their current semantics.
- **Activation:** accepting a build validates its inputs and prepares handover;
  it is not atomic process replacement or a health gate. A started schedule can
  fire before its target plugin completes a handshake. Readiness-gated activation
  would need an explicit definition of healthy and a policy for partial failure.
- **Filesystem recovery:** tests cover operation failures and process exits, not
  lost or reordered device writes. Interrupted stages remain until reclamation
  can establish that no live operation owns them.

## Runtime and diagnostics

- **Socket discovery:** without `XDG_RUNTIME_DIR` or an override, the fallback
  path includes the process ID. An explicit missing-runtime error would be clearer
  than a fallback that separate processes cannot share.
- **Memory and rendering:** byte and count limits bound logical payloads, not
  total heap usage. Instances cache views between dependency/model invalidations
  and deduplicate publication. Incremental wire rendering and large collections
  still require measured workloads and recovery semantics.
- **API exposure:** daemon internals expose more mutation-capable handles than
  ordinary callers need. Narrow them when a concrete consumer/test boundary is
  established; another shared-core crate would not itself resolve ownership.
- **Command lifetime:** caller timeout does not establish cancellation. Pending
  command futures retain a bounded slot until completion or disconnect. Explicit
  cancellation would need to distinguish queued work from external effects that
  have already started.

## Placement identity

Placed and unplaced surfaces share metadata with an optional placement ID. The
relationship between presentation kind and placement is enforced at runtime.
Revisit this representation when a feature needs that relationship enforced by
types; preserve the distinction between an indicator and its panel even when
they share a placement ID.

## Feature boundaries

`SetSetting` and `ToggleSetting` have no handler. Settings currently construct
plugins from the desired document; changing them restarts affected plugins. Runtime
setting changes need an owner and persistence semantics consistent with that model.
The broker [coverage test](../crates/omega-platform/tests/coverage.rs) lists the
unserved actions and separates broker responsibilities from daemon responsibilities.

Untrusted native plugins would require OS isolation. Manifest capabilities are
session authorization, not hostile-code confinement. A WASM or sandboxed tier
should follow an actual distribution requirement.

Preserve whole-state/view snapshots with bounded retention, deduplication, and
lag repair until a measured workload justifies a delta protocol. QML continues
to own layout; the protocol owns the node/property and interaction vocabulary.
External subsystem integrations stay behind platform services, and application
behavior remains in plugins.

## Application presentation

The launcher currently relies on host-selected output unless an output is given.
Automatic active-output selection and compositor activation-token acquisition need
an explicit host-to-platform contract. GIO handles D-Bus activation, but explicitly
supplied tokens on that path are refused until native launch-context token
forwarding is implemented. No path should silently claim guaranteed focus or
application startup from an admission acknowledgement.
