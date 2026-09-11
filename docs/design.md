# Open design questions

The [architecture](architecture.md) describes the implemented contracts. These
are remaining limitations and decision points, not a committed implementation plan.
Completed reviews and exploratory measurements are available in Git history.

## Persistence and recovery

- **Records:** replication survives unit restarts while the daemon lives, but not
  a daemon restart. Durable records need retention, schema migration and failure
  semantics before choosing a storage implementation.
- **Activation:** accepting a build validates its inputs and prepares handover;
  it is not atomic process replacement or a health gate. A started schedule can
  fire before its target unit completes a handshake. Readiness-gated activation
  would need an explicit definition of healthy and a policy for partial failure.
- **Filesystem recovery:** tests cover operation failures and process exits, not
  lost or reordered device writes. Interrupted stages remain until reclamation
  can establish that no live operation owns them.

## Runtime and diagnostics

- **Socket discovery:** without `XDG_RUNTIME_DIR` or an override, the fallback
  path includes the process ID. An explicit missing-runtime error would be clearer
  than a fallback that separate processes cannot share.
- **Memory and rendering:** byte and count limits bound logical payloads, not
  total heap usage. Every ready SDK instance still renders on state patches;
  deduplication avoids publication, not render work. Introduce dependency-based
  wakeups or caches only for a measured workload that needs them.
- **API exposure:** daemon internals expose more mutation-capable handles than
  ordinary callers need. Narrow them when a concrete consumer/test boundary is
  established; another shared-core crate would not itself resolve ownership.
- **Command lifetime:** caller timeout does not establish cancellation. Pending
  command futures retain a bounded slot until completion or disconnect. Explicit
  cancellation would need to distinguish queued work from external effects that
  have already started.

## Feature boundaries

`SetSetting` and `ToggleSetting` have no handler. Settings currently construct
units from the desired document; changing them restarts affected units. Runtime
setting changes need an owner and persistence semantics consistent with that model.
The broker [coverage test](../crates/omega-brokers/tests/coverage.rs) lists the
unserved actions and separates broker responsibilities from daemon responsibilities.

Untrusted native plugins would require OS isolation. Manifest capabilities are
session authorization, not hostile-code confinement. A WASM or sandboxed tier
should follow an actual distribution requirement.

Keep these existing choices unless a concrete requirement changes their tradeoff:

- External subsystem connections belong to brokers; third-party logic runs in units.
- Commands and views compose through typed records; progress can be represented
  there without a separate task/progress protocol.
- State values and view trees replicate whole, with deduplication and lag repair.
  Delta protocols would introduce ordering and resynchronization responsibilities.
- The shell owns drafts, focus, selection and panel visibility. Bindings carry
  semantic actions and values rather than raw keyboard or pointer events.
- Panels are placements of widget surfaces. They do not need a separate SDK trait.
- QML owns layout, while the protocol owns the node and property vocabulary.
