# Shared storage

Omega provides typed key-value storage through `omega::storage`. Stores can live
in memory for a daemon session or persist as JSON across restarts. This feature
is implemented on the current branch and requires protocol version 2; it is not
available in the published 0.3.9 binaries.

Declare a store in a normal Rust library, then use its type in plugins. There is
no storage-specific derive and no storage registration or consumer list in the
system document. Registered plugin fields determine access.

## Declare a store

For example, put these types in `crates/productivity/src/tasks.rs`:

```rust
use omega::storage::{Backend, Storage, StoragePolicy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    pub completed: bool,
}

pub struct Tasks;

impl Storage for Tasks {
    type Key = String;
    type Value = Task;

    const ID: &'static str = "productivity.tasks";
    const POLICY: StoragePolicy = StoragePolicy::Persistent {
        backend: Backend::Json,
        schema_version: 1,
    };
}
```

`ID` identifies the resource within the daemon's state directory. Renaming a
plugin, crate, or Rust type does not rename its data. IDs contain validated,
dot-separated identifier segments. Two declarations with the same ID must agree
on backend, codec, schema version, and limits; incompatible declarations fail
`omega check`, build validation, and activation.

Use `StoragePolicy::Memory` for daemon-session memory. Removing the last consumer
retains the store; reattaching does not reset it. Independent daemon state roots
have independent stores.

Keys implement `Display` and `FromStr`; the blanket `StorageKey` implementation
requires canonical round-tripping. Validated domain identifiers work as well as
`String`. Keys contain 1–256 UTF-8 bytes without control characters and sort by
encoded text. Keys never become filenames.

Values use Serde's JSON representation. Domain validation belongs in their
serialization/deserialization implementations. Use JSON-compatible types; JSON
is not a lossless representation of arbitrary Rust values such as non-finite
floating-point numbers. Backend selection does not change the `json-v1` codec.

## Read and write

Declare `Store<Tasks>` in a command, reaction, or surface's effects. It supports:

```rust
let inserted = store.insert(id.clone(), task).await?;

if let Some(entry) = store.get(&id).await? {
    let mut task = entry.value;
    task.completed = true;

    let replaced = store.replace(id.clone(), entry.revision, task).await?;
    store.remove(id, replaced).await?;
}
```

- `get(&key)` returns an entry or `None`.
- `list(Query::new().limit(50))` returns a bounded page.
- `insert(key, value)` creates an entry and refuses an existing key.
- `replace(key, expected, value)` and `remove(key, expected)` require the entry's
  current epoch/revision token. A stale token fails with a conflict.
- Replacing a value with an equal JSON value preserves its entry revision.
- `Store<Tasks, ReadOnly>` exposes reads without mutation methods.

A revision contains a random store epoch and a monotonic counter. Entry revisions
track the last mutation of that entry. Deleting and reinserting a key cannot
reuse its old token. Use the token returned by `get`, `insert`, or `replace` for
subsequent conditional writes; a page revision describes the whole store.

These operations perform I/O and cannot be render dependencies. `Store` does not
implement `Reads`, including its read-only form.

Successful writes are committed, including file and directory sync for JSON.
Omega never retries a mutation automatically. A connection failure or timeout
may leave its outcome unknown: read by key before deciding what to do next.
Retain insertion keys across that reconciliation.

## Subscribe from a surface

A subscription associates a query with a storage type:

```rust
use omega::storage::{Query, Subscription};

pub struct FirstTasks;

impl Subscription for FirstTasks {
    type Storage = Tasks;

    fn query(&self) -> Query<Tasks> {
        Query::new().limit(20)
    }
}
```

Queries support a single key or key-ordered enumeration, optionally starting
strictly after a key. The default limit is 50; the maximum is 100. `total` counts
all entries in the store and `truncated` indicates more matching entries.

The surface owns `Subscribed<FirstTasks>` and starts it during initialization:

```rust
use omega::storage::{Snapshot, Subscribed};

#[derive(omega::Surface)]
pub struct Agenda {
    tasks: Subscribed<FirstTasks>,
}

// Inside impl Surface for Agenda:
fn initialize(&mut self, _: &mut Self::Model) -> omega::Result<()> {
    self.tasks.start(FirstTasks)
}
```

`initialize` runs once before `mounted` and the first render. Starting observation
validates and queues the query without waiting for I/O. An unstarted subscription
is an initialization error. The query stays fixed for the instance's lifetime.

Render matches `tasks.snapshot()`:

- `Snapshot::Loading`: the first answer has not arrived.
- `Snapshot::Ready(page)`: committed entries, possibly empty.
- `Snapshot::Failed(message)`: admission, access, storage, or decoding failure.

Updates invalidate the owning surface. Subscription state does not use the
required-system-reading gate: loading and errors remain drawable. A write's
acknowledgment does not wait for every consumer to render its result.

Hiding retains observation. Closing or destroying the instance unsubscribes;
disconnecting the plugin drops all of its subscriptions. Cleanup capacity is
reserved when observation starts, so a full effect queue cannot prevent
unsubscription. Late updates for disposed subscriptions are ignored.

## Access and limits

The existing derives collect descriptors from `Store` and `Subscribed` fields,
including a surface's effects. Imports alone declare nothing. Read and write
access are combined per authenticated plugin and checked against its manifest.
A read-only declaration cannot write through a forged request.

The operator can inspect values. Renderer attachments cannot inspect arbitrary
storage. Stored values are not included in the public state mirror. These are
Omega protocol guarantees; native Rust plugins are not an operating-system
sandbox, and store IDs are not secrets.

`Storage::LIMITS` can lower these defaults:

| Budget                    | Default / maximum       |
| ------------------------- | ----------------------- |
| Entries per store         | 10,000                  |
| Encoded JSON per value    | 16 KiB                  |
| Complete encoded envelope | 8 MiB                   |
| Key size                  | 256 UTF-8 bytes         |
| Entries per page          | Default 50, maximum 100 |

The daemon retains at most 64 stores and admits at most 32 simultaneous storage
operations. A plugin session has at most 32 subscriptions; they also consume SDK
effect admission capacity for their cleanup. Exhaustion is an explicit error,
not an eviction. Slow subscribers do not hold a store's commit lock while waiting
to send their snapshots. Intermediate revisions may be coalesced.

## JSON files and recovery

Persistent envelopes live at `~/.local/state/omega/storage/<id>.json`, or under
`OMEGA_STATE_DIR` when overridden. Files use mode `0600` and contain:

```json
{
  "id": "productivity.tasks",
  "codec": "json-v1",
  "schema_version": 1,
  "epoch": "d70383c1604041d9b28dc40a8ae7e2f1",
  "revision": 1,
  "entries": {
    "task-1": {
      "value": { "title": "Buy coffee", "completed": false },
      "revision": 1
    }
  }
}
```

The daemon serializes writes per store. It plans a candidate without changing
committed state, publishes through `AtomicFile`, then updates memory and notifies
subscribers. Reads wait for a pending commit to finish.

A failure before rename leaves the previous state available. If rename succeeded
but directory sync failed, the outcome is unknown: the store stops serving reads
and writes, and subscribers receive an error. Restart to reopen, validate, and
sync the file before using it again. Corrupt data, duplicate entry keys,
incompatible envelopes, and unsupported versions are refused without replacing
the file with an empty store.

A state-root lease prevents another daemon or offline inspector from concurrently
using these files. Admitted work retains the lease and its capacity until it
finishes, even if its caller disconnects. Shutdown closes admission and waits up
to three seconds for pending storage operations; a timeout reports uncertainty.

Only version-1 persistent schemas are currently supported. There is no automatic
migration, import, or reset command. For manual recovery, stop the daemon, back up
the envelope, and inspect it before making changes. Existing plugin logs and the
systemd journal remain separate from storage.

## Inspect storage

Commands print JSON to stdout:

```sh
omega storage list
omega storage show productivity.tasks --limit 50
omega storage show productivity.tasks --after task-50
omega storage export productivity.tasks > tasks.json

# Inspect persistent files while the daemon is stopped:
omega storage --offline list
omega storage --offline export productivity.tasks > tasks.json
```

Online export reads bounded pages and verifies a constant epoch/revision. If the
store changes during export, the command fails before writing any answer; retry
for a consistent snapshot. Offline commands explicitly acquire the storage lease
and never create missing store files. Memory stores are unavailable offline.

## Test without a daemon

`SurfaceHarness` captures subscription requests and storage effects. Complete
captures explicitly, then deliver typed snapshots with `Stored<Tasks>`:

```rust
use omega::storage::Revision;
use omega::testing::Stored;

harness.take_effect().unwrap().complete(Ok(None))?;

let snapshot = Stored::<Tasks>::new(Revision {
    epoch: "a".repeat(32),
    revision: 1,
})?
.entry("task-1".into(), task, 1)?;

harness.storage(&snapshot)?;
```

The fixture applies the surface's declared query. It does not write files, open
sockets, or implicitly complete effects. Production runtime tests verify pushed
updates and refusal invalidation; daemon tests exercise actual shared mutations,
revision conflicts, persistence, and authenticated socket sessions.

## Ownership and remaining scope

| Layer                             | Responsibility                                                   |
| --------------------------------- | ---------------------------------------------------------------- |
| `omega::storage`                  | Authoring contracts, typed operations, subscription snapshots    |
| `omega-proto::storage` and schema | Identifiers, descriptors, requests, revisions, errors            |
| `omega-daemon::storage`           | Resource lifetime, admission, mutation planning, commit ordering |
| `omega-host::storage`             | Envelopes, paths, exclusive lease, durable files                 |
| CLI                               | Inspection and export                                            |

No new published crate or host dependency in the SDK is needed. SQLite,
automatic migrations, cross-store transactions, dynamic query replacement,
credential storage, and large blobs remain outside this implementation.
`Own`, `Watch`, and existing plugin-record behavior remain available; consumers
need an explicit migration before those APIs can be removed.
