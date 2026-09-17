# Pipelines and recovery

`omega-base::execution` separates declaration from execution. A pipeline declares
an ordered sequence of named, typed slots. Each slot supplies one operation;
the runner owns progress, attempt history, interruption, and stopping on failure.
`omega-host::recovery` independently owns durable filesystem changes.

## Declare, then execute

The initialization sequence lives in `omega-cli/src/initialize/pipeline.rs`.
`Steps::compose` is its authoritative ordering; `Steps::production` selects
implementations without executing them. Operations cannot choose or run the
next step. The CLI invokes the completed pipeline once.

```rust,ignore
let steps = Steps::production();
let pipeline = steps.compose(request.bare);
let run = pipeline.run(request, observer).await;
```

`Operation<Input>` declares an output and error type. `Step<Input, Output, Error>`
binds an implementation to a stable description. `.then(step)` requires its input
type to match the preceding output. Duplicate step IDs are refused during
composition. `Pipeline::steps()` returns the declaration before any effects.

Intermediate values are data. Named operations such as `DescribePlugins`,
`Validate`, and `Publish` perform the next action explicitly. A validated build
owns its unpublished generation and workspace lock; publishing consumes it and
releases the source lock before activation waits. Context unrelated to an
operation can pass through with `Carry` or `Step::carrying`.

All operations use an async contract; synchronous work returns without awaiting.
Values and implementations are owned and `'static`. Futures are caller-polled,
need not be Send, and spawn no tasks themselves. Internally, implementations and
composition links use boxed dispatch; values crossing steps retain their concrete
types. There is no string-keyed context, downcasting, or runtime graph scheduler.

## Outcomes and reporting

Observers receive `Running` followed by `Completed`, `Skipped`, `Failed`, or
`Interrupted` for each attempted step. A skipped operation still returns its
output. `Progress` emits messages and structured paths scoped to that step;
operations never obtain a `Ui` or the runner. Details are retained in its report
and delivered immediately, including recovery paths emitted before later failure.

`Run` contains the output or the original operation error, the failing step's
identity, and the attempt reports. Unreached steps have no attempt. A reached
unconfigured slot fails explicitly and never falls back to a production operation.

Dropping a polled run or unwinding marks its active attempt interrupted. An
unpolled run has made no attempt. External effects may continue after their future
is dropped, so interruption does not imply rollback. Observers must not panic.
There is no automatic retry or serialization of arbitrary operation values.

## Testing the production composition

Initialization tests call the same `Steps::compose` used by the CLI:

```rust,ignore
let mut steps = Steps::isolated();
steps.preflight.replace(Stub::returning(inspected_fixture));
steps.prepare_workspace.replace(PrepareWorkspace); // exercise real file planning
steps.adopt_shell.replace(Stub::failing(expected_error));

let run = steps.compose(false).run(request, &mut recorder).await;
assert_eq!(run.result.unwrap_err().step.id.0, "init.adopt");
```

Isolated steps begin unconfigured. Every real operation reached by a test is an
explicit choice. Replacements preserve slot identity and must have compatible
input, output, and error types. Fakes implement the same `Operation` contract;
they can capture inputs and produce domain fixtures without changing the plan.
`testing::Stub` supplies a fixed result, `Pass` returns its input, `Pending` stays
pending, and `Gate` waits for explicit release before invoking its operation.
A forgotten gate release fails loudly. `Recorder` captures outcome/detail events.

Pipeline tests cover failure at every desktop step, no successor execution,
workspace-only selection, and cancellation before publication. Integration tests
combine real filesystem recovery and generation publication with replaced
compiler and host operations. Separate CLI tests compile actual generated Rust
and reconcile it through an isolated daemon. Mocking a step verifies composition;
it does not establish the correctness of that step's external effects.

## Durable changes

A `Change` is a serializable operation with a versioned kind and four methods:

- `inspect` classifies the target as its before-state, after-state, unchanged
  (both states match), or a conflict.
- `apply` executes the operation durably, checking its preconditions again.
- `restore` compensates the operation durably, checking recovery preconditions again.
- `confirm` flushes an already observed state without replaying effects, checking
  that it still matches. Recovery uses this after an interrupted write.

Use this contract for effects with defined recovery semantics. Read-only checks,
service restarts, and arbitrary commands do not acquire a rollback method merely
because they run as pipeline steps.

`RecoveryStore` stores JSON records under `Layout::recovery_dir()` (normally
`~/.local/state/omega/recovery`). The directory is private to the current user.
Records are written with mode `0600` in a `0700` directory. They contain recovery
data and must be treated as private installation state. Receipt scans decode
metadata while skipping snapshot payloads. Symlink and non-file records are refused.
Each record has an independent `ChangeId`, a format version, a change kind, and
one of these states:

```text
Prepared → Applying → Applied
               ↓         ↓
            Restoring → Restored
```

Restoration can also abandon a prepared change whose before-state is still
present. The store writes and flushes each transition before the corresponding
effect and checks postconditions before recording completion. A record left in `Applying` might describe either an incomplete operation
or an operation that completed before its acknowledgment was saved.

`RecoveryStore::open::<C>` opens an existing record under an exclusive store
lock. `SavedChange::inspect` observes the target. `accept` acknowledges an
interrupted apply only when its after-state is present and its durability has been confirmed, without replaying it.
`restore` either restores the before-state, recognizes that it is already
restored, or refuses an external edit. A new change is refused while the store
contains unfinished records. Dropping a handle releases the lock without applying
or restoring anything. Completed records are retained; there is no automatic
retention or deletion policy. Unchanged installations verify current files without
creating records, backups, or the recovery directory.

Journal-write failures invalidate the live handle for further mutations because
the rename may already have published the next state. Drop it and reopen the
record to read the actual state. A prepared record cannot undo contents changed
by another writer before its effect began. A restored record cannot undo later
matching edits. A successful backend return is checked against the expected
state before the record is marked applied or restored.

```rust,ignore
let mut saved = store.open::<Replacement>(&id)?;
match saved.inspect()? {
    Observation::After => saved.restore()?,
    Observation::Before | Observation::Unchanged => saved.restore()?,
    Observation::Conflict => { /* report the conflict and preserve the files */ }
}
```

The CLI exposes `omega recovery list`, `inspect <id>`, `accept <id>`, and
`restore <id>` for filesystem replacements. Restore requires a stopped daemon and
holds the workspace mutation lock before opening the recovery store. Other
applications using the host API must coordinate active services and configuration
ownership before invoking it. Restoring a
service file does not reload systemd or stop a daemon; restoring renderer assets
does not restart the shell. These policies belong to their workflows.

## Filesystem replacement

`Replacement` captures the current target and a desired `Snapshot`. Snapshots
support regular files and directory trees, including nested symlinks without
following them. They preserve file contents and ordinary permission bits. Root
symlinks, special files, special permission bits, invalid child names, and file/directory type changes are
refused. Ownership, ACLs, extended attributes, and hard-link identity are outside
this contract; it is intended for Omega configuration and assets.

Files publish through `AtomicFile`; directory trees publish through `StageDir`.
Recovering a newly created target removes it. Parent directories may remain.
Directory removal first renames the target aside; interruption during cleanup
can leave a hidden displaced entry. Recovery determines completion from the
original target, not from the presence of cleanup leftovers.

Recovery snapshots include both before and after contents. They allow recovery
after an interrupted write and detect files added or edited afterward. Store
locking serializes participating writers; unrelated applications do not take
that lock. Callers must exclude concurrent external writers during publication:
a comparison followed by rename is not a filesystem compare-and-swap.

## Initialization

`omega-cli::initialize` owns initialization policy. One pipeline retains the
attempt history across preparation, compilation, installation, publication, and
verification. CLI argument types select layout, build profile, and bare/full mode.

1. Preflight checks unfinished recovery, Rust tools, host availability, foreign
   daemons, installation targets, workspace TOML, and shell ownership. Source
   mutation locks may create coordination directories; configuration files have
   not changed at this point.
2. Fresh shell imports establish ownership and preserve the original backup.
   Workspace files and optional local Cargo overrides are installed individually
   through durable replacements. Build outputs and unrelated workspace contents
   are never copied into backups.
3. Shared build operations compile Rust, query plugin manifests, and evaluate
   and validate the system document. Their output owns an unpublished generation
   and the workspace lock. Dropping it cannot activate that build.
4. Renderer and service files are installed. Systemd reload, enablement, and
   startup must succeed, and the daemon must answer with this CLI's version.
5. Publication makes the validated generation available to the daemon. The
   workspace lock is released before waiting for reconciliation and shell
   application of that exact generation.
6. The shell restarts, and live renderer attachments must match the embedded
   build. A workspace without Omega placements reports live QML as unverified.

`--bare` ends after workspace preparation without invoking host commands or
compiling. `--debug` selects the debug build profile for full initialization.
Repeated initialization preserves existing sources and checks unchanged files
without allocating more backups. Service activation and shell restart still run.

Failures stop the sequence and report the failed step, retained records, and
recovery commands. Compilation failure leaves source files available for editing;
service activation failure leaves installed files but does not publish the new
build. A failure after publication leaves the generation published and may require
`omega rollback`. No automatic compensation crosses filesystem, systemd, and
shell lifecycles. Interrupted records block further initialization until explicitly
accepted or restored. The shell ownership journal retains its existing separate
contract and first-adoption backup.

Initialization keeps a per-run change log alongside the pipeline reports.
Filesystem operations record verified `Created`, `Updated`, or `Unchanged`
outcomes and their recovery receipts; failed targets are marked unverified.
The summary uses these facts without parsing progress messages or scanning old
records. Existing Rust entry points are included as unchanged replacements so
their preservation is visible. Shell backup availability is observed by the
adoption step, including when adoption fails after creating the backup.

The initialization diagnostic retains the original error chain and nested source
labels. A recovery error naming a record takes priority over step-specific retry
advice. Publication success comes from the completed step report; a publication
failure is reported as potentially having taken effect. Rendering diagnostics and
summaries performs no recovery or rollback.

## Other consumers

- `omega build` shares compilation, manifest evaluation, document validation,
  and publication steps with initialization. Its optional activation wait uses
  the same exact-generation observation contract.
- Standalone service-file and copied renderer installation use the same durable
  replacement operation; their command owners retain activation policy.
- Renderer status, restart, and attachment verification live outside argument
  parsing and are shared by initialization and shell commands.

Linked renderer development retains its existing installation contract. There is
no general scheduler, automatic compensation, or serialization of Rust step
objects. Initialization is a sequence of individually durable effects, not one
all-or-nothing transaction.

## Internal ownership

Recovery separates the `Change` contract, versioned records, store access, pure
operation policy, and saved-change execution. Policy receives the persisted state
and observed facts; it performs no I/O. The saved-change executor writes intent,
runs effects, confirms postconditions, and commits outcomes. `Replacement::install`
uses that executor directly. There is no nested prepare/apply/verify pipeline in
filesystem recovery; the owning workflow reports one meaningful installation step.
