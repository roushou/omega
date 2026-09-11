# Architecture

## Two planes

The **configuration plane** is a Rust program that computes a state document
with no side effects. The **runtime plane** is units running as supervised
processes, converged toward that document.

The split resolves the bootstrap paradox: the committed document boots a
machine with no toolchain. It also means a config that does not compile cannot
take the desktop down — the last good document keeps running. Cargo is a
build-time concern on the developer's machine, never a boot-time requirement.

## Crate boundaries

| Crate                | Responsibility                                                             |
| -------------------- | -------------------------------------------------------------------------- |
| `omega-proto`        | Schema, identifiers, wire vocabulary, codecs and socket contracts          |
| `omega-host`         | Layout, durable files, TOML documents and generation ownership             |
| `omega-document`     | Desired-state builders and shared validation                               |
| `omega-derive`       | Field-based SDK derives                                                    |
| `omega` (`omega-rs`) | Unit-author SDK; depends on proto and derive, not host or daemon           |
| `omega-brokers`      | External subsystem connections, readings and actions                       |
| `omega-daemon`       | Sessions, supervision, state, action routing and convergence               |
| `omega-renderer`     | Embedded QML assets and their installation                                 |
| `omega-cli`          | Build and deployment orchestration, environment resolution and terminal UI |

The SDK declares dependencies by fields: readings and composites read, records
hold unit memory, and effects request changes. `Reads` excludes effects from
widgets. Derives generate through `omega::internal`; the field declarations form
the manifest extracted from the compiled unit. Construction settings arrive at
handshake, with placement settings layered over unit settings for widget instances.
The protocol's optional `json` feature is used by observation/document consumers;
a standalone unit does not compile the generated JSON implementations.

Configuration entry points can return `omega_document::Result<()>`. Its error
enum wraps document, shell, validation and I/O failures; config authors can also
use their own error library. Generated workspaces require neither `anyhow` nor
`miette` as a direct dependency. Build publication and daemon adoption use the
same document validator. `omega check` evaluates and validates without
staging or publishing a generation. A configuration may contain only native
shell widgets, with no Omega plugins. Fresh workspaces start with `system`;
`omega new` registers each added plugin in Cargo's workspace membership.
Validation retains underlying error sources, available widget surfaces and duplicate placement locations for callers to inspect.

Terminal diagnostics belong to `omega-cli::ui::Ui`. Its `miette` adapter renders
shell-import JSON snippets and suggestions for duplicate placements or invalid
widget surfaces to stderr, preserving outer operation context. Other failures
retain the ordinary error-chain display. Internal orchestration still uses
`anyhow`; neither the SDK nor the document API depends on `miette`. Errors from
the separate configuration process arrive as stderr text, not typed diagnostics.

## The plugin API

Public modules follow the domain an author works with. `audio` contains both
`Audio` and `Volume`; `power` contains battery and mains readings, their `Power`
interpretation, and profile control. `network` groups connectivity, Wi-Fi and
traffic; `desktop` groups displays, brightness, workspaces and keyboard state.
`bluetooth`, `time`, `system`, `session`, `notification` and `process` expose the
remaining domains. These are export boundaries over shared topic and capability
machinery, not additional runtime layers.

The root contains surface contracts, derives, errors and shared values such as
`Percent`. `ui` builds views, `config` describes construction settings, `record`
holds plugin memory, and `testing` builds fixtures. `effect` defines the operation
returned by a control, its receipt and failure semantics. Device controls belong
to their domains. Reading/composite implementation modules and wiring traits are
private; macro expansion accesses the required contracts through `internal`.

Module placement does not grant capabilities. A domain may expose both read and
control handles, but the `Reads` bound still prevents a widget holding a control.
Public examples and doctests use domain paths; there are no parallel public
reading/effect handle paths to keep in sync.

## The daemon

The daemon authorizes sessions, supervises units and converges the document.
`UnitTable` owns process and session facts; `Hub` owns replicated state and views;
`Schedules` owns timers across convergence passes. Brokers own external subsystem
connections, including NetworkManager, PipeWire, UPower and Hyprland.

Units communicate with the daemon through frames and run as separate native
processes. Their code is not loaded into the daemon.

## State

The daemon owns the replicated state. Units keep a revisioned mirror, so a
reading is a local memory read rather than a round-trip. `Own<T>` holds a local
record initialized from that mirror; its updates are serialized and published
in order. Admission is reserved before changing local memory; a rejected update
closure never runs. Admitted writes remain locally optimistic if publication is
later refused, and their receipt reports that failure. `Watch<T>` observes only
replicated values. Record replication is in-memory and survives a unit restart,
but not a daemon restart.

Replication is last-value-wins on the _value_: a source that polls an
unchanged reading produces no revision and wakes nobody. A unit receives only
the topics its manifest declares, narrowed at runtime by `Subscribe`. A
subscriber that falls behind takes a snapshot together with a fresh subscription.
The SDK rejects older revisions, including values older than a removal. View
observers receive tombstones for instances absent from their replacement snapshot.

Units also own state. Every unit has the keyspace `unit.<name>.<key>`,
writable by that unit alone with `CAPABILITY_STATE_WRITE` and readable by any
unit that declares that keyspace. Units compose through replicated records;
typed `Watch<T>` consumers depend on the crate defining `T`.

Hot reload is therefore kill-and-restart, and crash recovery is the same code
path. Liveness is a property of the process model, not the language.

## Protocol

protobuf as the IDL, no gRPC. Length-prefixed frames over a Unix socket,
`SO_PEERCRED` for unforgeable identity, canonical JSON for the observation
socket and debug output.

protobuf gives messages, not a protocol. Multiplexed streams, bounded queues,
last-value-wins coalescing, request ids, and handshake version negotiation are
designed on top. Encoding is swappable; ontology is permanent — so the schema
is where the care goes. It models a desktop, not Hyprland.

`crates/omega-proto/schema/` is the single source of truth. The generated Rust
types are shared by the daemon, the CLI, and every user crate: `Keybind` in
the daemon and `Keybind` in a config are the same type, with no translation
layer where a lie can live.

Protocol v2 requires `RemoveWidget` support. Rebuild v1 units before connecting
them to a v2 daemon. Action and adoption execution is bounded per control or
observation connection and runs concurrently with frame reception, so a unit can receive a command while its
request is outstanding. Deadlines bound queue admission and response waits; a
timeout does not prove an external action was not performed. Both encodings use
one operation queue with at most 16 tasks and 8 MiB of request payloads.
Subscription changes remain ordered in the connection loop. Request task panics
are logged and answered on their original stream.

Each listener runs at most 64 connection tasks; excess peers wait in the kernel
backlog until capacity is available. The daemon owns both task sets across
accept-loop cancellation, triggers shutdown, and aborts and joins connections
before completing unit shutdown. Dropping a standalone observation server also
cancels its children. Connection task panics are reported when reaped.

Observation requests have a four-MiB encoded-line limit. Partial bytes belong to
the connection, including split UTF-8 sequences, so cancellation by a broadcast
or heartbeat loses no input. Overlong or invalid UTF-8 input closes that
connection. Valid requests still use the shared dispatcher and policy table.
Each observation write, including its newline and flush, has a five-second
budget; a stalled reader cannot retain its task indefinitely while output waits.

The binary `Duplex` transport and the observation writer continue reading while
a write waits. Each retains at most 32 complete incoming frames/lines and 8 MiB.
The application consumes buffered input in order when the write finishes.
Exhaustion terminates the connection explicitly; reads never build an unbounded
backlog. Binary `send` still completes only after the frame is written. A failed
or cancelled write poisons that connection so a partial frame cannot be resumed
as a new one. Session shutdown cancels the entire connection future, including
pending writes.

Protobuf frames have the same four-MiB body limit in both encoding and decoding.
Encoding checks size before modifying the output buffer. Terminal replies use
`Frame::reply`: an oversized outcome becomes a bounded `PayloadTooLarge`
refusal on the original stream. Daemon responses, SDK answers and refusals share
that constructor. A refused widget render does not install an instance or cache
the rejected tree. JSON and protobuf limits apply to their encoded sizes, so
escaping can make a JSON request reach its limit sooner.

## Commands and effects

Commands derive `Command` to declare their fields and implement the trait with
associated `Input` and `Output` types and an async `call(input)` returning
`Result<Output, omega::Error>`.
Outputs implement `IntoValue`; `()` becomes terminal success without a value.
The runtime converts values and errors to closed protocol outcomes. Authors use
ordinary return values and `?`; daemon refusals preserve their code.

The derive gives a command one identity, shared by `.command::<SetVolume>()`
and `.on_change(SetVolume)`. Named-field commands expose a `CommandRef` constant
in Rust's value namespace; the reference contains no command instance or effects.
Controls require matching inputs (`Percent`, `bool`, `String` or `()`), and
`SetVolume.with(value)` binds a complete input for a button. Duplicate registered
command names are refused. A reference does not register the command: invoking an
unregistered command remains a runtime refusal.

Scalar inputs consume exactly one argument; `()` consumes none. `derive(Input)`
decodes a strict map. Missing, mistyped and unknown fields are refused before
`call` runs, rather than defaulting as configuration and records do. `Args` remains
an explicit raw input for commands implementing an external argument grammar.
`Called::of` accepts typed inputs; `Called::raw` exercises malformed wire input.

Effect handles and record writes return an awaitable `Effect`. Admission happens
when the method is called, including local record updates; awaiting observes
terminal success or failure. `Effect::receipt` exposes admission and a unique
completion receipt for manual polling or explicit detachment. Dropping an
admitted effect does not cancel it. An unobserved completion failure ends the
unit. Completion is the daemon's operation result; launching a program confirms
spawn, not process exit.

A runtime admits at most 64 queued or in-flight effects. The five-second deadline
starts at enqueue. Expired queued effects never execute; a sent effect that times
out retains its slot until a terminal reply or disconnect, because its external
execution may still be running. Effects also carry the encoded-byte budget
described below. Timeout, unavailable runtime, capacity exhaustion and oversized
payloads have distinct wire error codes.

Command futures have a separate 64-entry bound, checked before invoking user
code. They run concurrently while the connection loop receives frames, renders
widgets and completes effects. A caller's timeout does not cancel command work;
a command occupies its slot until it finishes or the connection ends. Disconnect
aborts command tasks and resolves outstanding effects. Commands share their
constructed instance, so mutable application state requires synchronization.
Reactions and rendering remain synchronous. `testing::Called` awaits commands
and completes captured effects, exposing their typed result to tests.

## Trust

Three kinds of peer on the control socket:

| peer        | identity                        | may do                               |
| ----------- | ------------------------------- | ------------------------------------ |
| unit        | spawn token + `SO_PEERCRED` pid | what its manifest declares           |
| operator    | the daemon's own uid            | lifecycle, actions and subscriptions |
| anyone else | —                               | refused                              |

Grants are read from the daemon's copy of the manifest, never from a frame.
Capabilities cannot be self-declared at runtime: a unit's manifest is
extracted at build time from the binary itself.

Native units run as the owner’s uid. Manifest grants constrain authenticated unit
sessions; they do not sandbox hostile code, prevent another operator connection,
or restrict that uid's filesystem access. A manifest hash verifies declaration
agreement, not binary authenticity. The dispatch [policy table](../crates/omega-daemon/src/session/dispatch/policy.rs)
is the operation-level authorization contract.

## Units

One unit type, many surfaces — not widget/plugin/app/script/service. A unit
declares which surfaces it exposes, and a surface is either rendered or
called:

- a **widget** surface renders — pulled once per instance so the unit learns
  its configuration, pushed thereafter;
- a **command** surface is invoked, by `omega run` or by another unit holding
  `CAPABILITY_SPAWN`.

Both directions of the protocol are used, with stream ids split by parity so
each side answers only what it asked.

Events are derived from state transitions in the daemon, not announced by each
source, so reality and its announcements cannot disagree. They are delivered
only to units whose manifests declare them, and are not stored.

Actions are authorized per kind — `RunCommand` costs `CAPABILITY_SPAWN`,
`Lock` costs `CAPABILITY_SYSTEM_CONTROL` — with the check ahead of the
implementation, so an action the daemon cannot yet perform is never a grant.

Action payload rules live in `omega-proto::action`, with an exhaustive arm per
kind and a field-specific `ActionError`. Live actions authorize before validation,
then validate before routing or side effects; the daemon maps validation failures
to `INVALID_ARGUMENT` in `refusal.rs`. This rejects missing changes/targets,
invalid enums, non-finite volume changes, out-of-range absolute levels, false
selected toggle/focus flags, invalid unit/command identifiers and malformed text.
Defaults with meaning remain valid: omitted window selectors mean focused,
relative changes remain signed, and empty notification text is allowed.

Desired-document validation uses the same payload rules and resolves scheduled
unit commands against the build's manifests. Timer admission validates before
removing an existing timer, so malformed replacements preserve running schedules.
Event-only schedules remain valid. Availability and backend-specific limitations
are checked at execution; Hyprland refuses unsafe dispatcher delimiters and
non-focused monitor moves rather than ignoring their selectors. Payload validation
does not replace process argument encoding or shell quoting.

A session registration is a lease. Replacing it cancels the previous session;
its guard cannot disconnect the replacement. Adoption cleanup also checks the
issued token. Views and installed instance specifications end with the lease.
An instance is addressed by `(unit, surface, module)`; the empty module is the
anonymous instance, not a wildcard. Placements coexist with it.

## Reconciliation

The four domain providers `plan` purely against the document and only then `apply`, so a
change can be shown before it happens. Convergence runs in one task, one pass
at a time, and is keyed per entity id — which is what keeps the blast radius
of an edit to the thing it names. The daemon owns the convergence task, cancels
an in-progress pass on shutdown and joins it before draining schedules. Dropping
the owner also cancels convergence.

`omega build` copies binaries into a private generation before asking those
artifacts for their manifests. The completed directory is synced before one
atomic write publishes its name in `current`. A reload resolves that name once;
`ValidatedBuild` validates its descriptor, executable files, manifests and document
before anything running is changed. Ordinary convergence reads no build inputs.

`DocumentValidation` is shared by the CLI and daemon. Unknown units, duplicate
identifiers, ambiguous widget surfaces, invalid cadences and invalid environment
variables are rejected. Domains without providers are rejected explicitly.
Environment values are encoded as literal shell data, including quotes and
newlines; the reconciler compares the complete projection rather than parsing
shell source back into a second representation.

Activation compares manifests, construction settings and executable contents.
Only changed or removed units are stopped. Their sessions are revoked before
manifests and settings are installed together. Handshakes and development
handovers share the activation boundary. A changed unit held by `omega dev`
blocks activation until that lease ends. Identical units keep running, including
through a build that changes only the environment or widget placements.
Supervision restarts the exact executable it was given; it does not poll binary
modification times.

The worker validates all four plans before applying units, environment, schedules,
then instances. Their order is explicit; there is no dynamic provider registry or
string-based executable diff. Unit changes contain parsed names and start/stop
operations, environment changes contain validated rendered contents, schedule
changes contain full declarations or removal ids, and widget changes contain
typed addresses and settings. Apply never looks up its payload in the document.

A failed application stops the pass and retries after two seconds with fresh
plans. A unit still held after a stop request is pending, including a disabled
development unit until its adoption ends. Unchanged timers are left alone on
retry. Environment read errors are failures, not missing files. There is no
separate settings provider: construction settings belong to build activation.

| domain        | converges                        |
| ------------- | -------------------------------- |
| `units`       | which units run                  |
| `environment` | the session environment file     |
| `schedules`   | persistent timers                |
| `bars`        | surface instances placed in bars |

Two topics can come from one subsystem when they move at different rates:
`network` carries the SSID and the signal, `throughput` carries bytes per
second, because a widget drawing the network's name should not be woken twice a
second by bytes it is not showing. A broker that reports a rate differences two
samples itself — a counter is meaningless alone, and every widget that did its
own subtraction would have its own idea of what to do when an interface
disappears.

A schedule is the one thing the runtime plane does that nobody asked for. Its
cadence is `every <n><s|m|h|d>` — one grammar, in `omega-proto`, with an immediate first tick and no cron support,
so the config
plane that writes it and the daemon that reads it cannot drift. When one
fires, the daemon publishes `EVENT_SCHEDULE_FIRED` and performs the action the
document gave it, if it gave one; a schedule with no action is a cadence a
unit reacts to. Nothing here is capability-checked, because a schedule is the
machine's own document speaking rather than a unit asking.

Schedule startup rejects periods outside the monotonic clock range.
Schedule mutations are serialized. Replacing or removing a timer joins it before
releasing its registry entry, so cancellation during a replacement retains the
old task's owner. Unchanged declarations retain their cadence. Shutdown closes
admission and joins every timer; it interrupts a timer even while that timer awaits
an action result. Provider failures remain pending convergence work.

A broker skips queued actions whose reply receiver has already closed before
execution starts. Cancellation does not undo an external action already started.

Brokers own their external connections. Connect and read calls have ten-second
deadlines; action deadlines are five seconds from admission, including queue time.
Idle event waits have no deadline. A failed or timed-out operation disconnects the
broker, retracts its topics and backs off before reconnecting and taking a fresh
reading. Shutdown also cancels the outstanding operation and disconnects. Timeout
does not establish whether an external action took effect, and actions are never
automatically replayed. Missing handlers are `Unimplemented`; unavailable services, capacity exhaustion,
oversized payloads and expired deadlines are `Unavailable`, `ResourceExhausted`,
`PayloadTooLarge` and `DeadlineExceeded`. Their numeric assignments belong to the schema.

Broker requests share an 8 MiB encoded-payload budget across the daemon, alongside
the eight queued requests per broker. Daemon-to-unit requests share another 8 MiB
budget across units, with sixteen queued and sixteen sent requests per session.
Admission refuses excess work without waiting. Broker permits cover execution;
unit-request permits and sent slots remain until a terminal result or disconnect,
even after the caller times out or cancels. Intermediate streaming results do not
complete the waiting caller. These are logical-work bounds, not aggregate memory
limits. A responsive unit that never finishes its requests remains saturated until
it answers or disconnects; cancelling callers cannot admit more work underneath it.

Each SDK effects queue has an 8 MiB encoded-payload budget in addition to its
64-request bound. Payloads must fit within the frame limit, with envelope space
reserved. Sending moves the payload out of the queue; the pending map keeps only
the reply sender, deadline and admission permits. Both permits survive wire timeout
until the terminal reply or disconnect.
Record updates reserve the maximum legal payload before invoking the update
callback and release unused bytes once its result is known. Byte exhaustion does
not run the callback; an oversized result runs the callback but leaves the local
record unchanged. This conservative reservation can refuse a small record update
when less than one maximum-sized payload remains.

## Layout

```
~/.config/omega/          source only
  Cargo.toml Cargo.lock   workspace; members are system/ and registered units/
  system/                 → document.json, one entry point, no side effects
  units/                  independent unit packages
  target/                 cargo's, at cargo's default path; gitignored

~/.local/state/omega/
  current                 atomically published generation name
  generations.toml        accepted and previous generation names
  .generations.lock       publication, reference and cleanup transaction lock
  generations/<id>/       retained immutable build output
    document.json  units.toml
    .lease  .ready         process lease and managed-generation marker
    units/<name>/         the binary and its unit.pb
  environment             mutable runtime projection

~/.cache/omega/logs/      <unit>.log
```

`Layout` resolves paths; typed TOML schemas identify documents. `AtomicFile`
publishes files through exclusive temporary creation, file sync, rename and
parent-directory sync. `Directory` establishes and flushes the ancestor chain.
A flush failure is an error even if the new contents are already visible.

Build generations are immutable through Omega's APIs. Publication flushes a
private stage before replacing `current`. Acceptance is a separate durable record
written after validation and handover preparation; it does not certify unit health
or completed convergence. Startup tries `current`, then accepted and previous
builds if the candidate is unusable, leaving a rejected pointer intact for diagnosis.

Publication, acceptance, rollback, lease acquisition and cleanup share a store
transaction lock. Validated builds and supervised executables hold generation
leases. Children inherit the lease across exec, protecting surviving descendants
even if the daemon crashes. `omega clean --generations` reclaims only managed,
unreferenced generations without a live lease; malformed references abort cleanup.
Unmarked or unfinished directories are retained.

`omega rollback [id]` validates and republishes a retained generation under the
store lock. Without an ID it restores acceptance if the published candidate differs,
or the previous acceptance otherwise. The daemon activates it through normal
convergence. Legacy flat builds require `omega build`; they are not executed.

Renderer installation uses `StageDir` with Linux atomic rename: exchange for an
existing entry, no-replace for a first installation. Symlinks are replaced as
entries without changing their targets. The displaced entry is removed only after
publication is flushed. Unsupported rename or flush operations fail explicitly;
an interrupted installation can leave a displaced stage behind. Filesystem tests
cover injected operation failures and process exits, not storage-level power loss.

Logs live in the cache and outlive generation replacement. `omega clean` removes
Cargo output without removing those logs.

## UI

The SDK keeps readiness and the last sent tree with each configured widget
instance. Every declared system topic must have been reported before its widget
renders; an explicit absence is a report, and unwritten records use defaults.
Other surfaces do not share this readiness gate. A daemon-requested instance
awaiting its topics answers with an empty tree, retains its settings and pushes
its view once ready. Pull responses and pushed views share the same cache, so an
unchanged tree generates no further socket traffic. Reconfiguration and removal
replace or discard that instance's cache. Each ready instance still renders on
state patches; the cache avoids repeated serialization and publication, not render
work. The daemon also retains its own deduplication boundary for all clients.

Positional key assignment retains its formatted string and borrows parent keys;
explicit keys remain unchanged. Further render scheduling or serialization caches
need a concrete workload demonstrating that their complexity is worthwhile.

Observation connections retain only address/revision pairs for deletion repair
after lag, and release initial snapshots after sending them. Views and state
topics serialize by reference into a line owned by the pending write. The hub
owns authoritative trees; an observer does not need an additional full-tree cache
to remember which surfaces it has drawn. Each connection keeps its existing write
deadline and bounded input draining while other observers progress independently.

After assigning a revision, the hub wraps a view publication in `Arc`. Registry,
history, snapshots and receiver deliveries share that immutable publication;
history delivery increments a reference count under its mutex instead of copying
the tree. Replacing a view cannot mutate an earlier snapshot. Eviction removes
history ownership, while an active reader can retain the old value until its
operation completes. Existing logical byte/count limits and lag recovery still
apply; sharing is not an exact process-heap bound. State patches and events keep
their existing representation.

Each observation view batch and state batch must finish within five seconds;
progress on an individual line does not renew the batch deadline. Startup sends
one view batch followed by one state batch. View lag repair sends deletion and
current-view batches separately. A deadline failure ends the connection, including
any partial JSON line. View batches consume their snapshots, releasing each
completed publication before waiting on the next. Individual writes keep their
five-second deadline and bounded request draining. The private connection accepts
an async stream so in-memory backpressure tests can use Tokio's paused clock.

Units publish declarative view trees; the shell renders them. A first-party
Quickshell plugin instantiates QML per node, so units inherit Omarchy theming
without knowing about it. Normal Wayland toplevels need none of this.

The CLI embeds renderer assets. `omega shell install` installs the assets carried
by that binary; replacing the binary alone does not refresh installed or loaded
QML. Node props come from the protocol's `NodeKind` table. Generated `Props.js`
readers, SDK emission tests and the shell's explicit undrawn-prop list keep the
vocabulary aligned. See the [renderer contract](../crates/omega-renderer/shell/README.md)
for interaction, host integration and node implementation.

## Design rules

1. **The typed path must be shorter than the bash path** for the fifty most
   common operations. If `run("brightnessctl set 5%+")` is one line and the
   SDK is fifteen, the SDK is decoration on a pile of shell scripts.
2. **Blast radius of one.** Any edit touches exactly one thing; a keybind
   change does not restart the bar.
3. **Scales down.** A fresh install is a few lines in one file.
4. **A config that does not compile never takes the desktop down.**

Non-goals: not Nix; not a reimplementation of NetworkManager, PipeWire, UPower
or Hyprland; not a full OS. Omega is a programmable layer on top of Omarchy,
reusing its shell and compositor.

## Retained state and history

The state store retains at most 1 MiB of encoded unit records and 1 MiB of
system topics. The separate reserves prevent unit records from consuming the
space used for broker readings and lifecycle status. At most 4096 unit topic
addresses are retained, including unavailable values. Topics use validated
addresses as keys. Patches are validated and budgeted as one transaction before
changing any values, revisions, broadcasts or derived events. Smaller replacements
release capacity. Retained records are not evicted to make room for newer writes.

A full state snapshot fits below the binary frame limit. View retention has an
independent 8 MiB and 4096-entry bound. Individual views and events must fit below
the frame limit with envelope space reserved. Failed publications return typed
errors; a rejected first render cannot install an instance. Internal producers
report rejected publications in diagnostics.

Views use one monotonic sequence for the hub. Removing a surface releases its
entry while broadcasting a newer empty tree. Republishing that address receives
a still newer revision; deleted addresses require no retained revision map.

Each state, view and event history retains at most 64 publications and 8 MiB.
Byte or count eviction reports the number missed to a slow receiver. State and
view receivers recover with an atomic snapshot and subscription; event receivers
report lag without replaying historical events. Waiting for history is
cancellation-safe. The log owns the sequence; its watch channel only signals changes.
Closing publication still lets existing receivers drain retained entries after any
lag notification. These are encoded-payload and entry-count bounds, not an exact
bound on allocator overhead, decoder allocations or concurrent serialization
copies.

## Critical task failures

Broker drivers, unit supervisors, convergence and schedule timers run under the
daemon's explicit shutdown signal. A panic records the task's identity and panic
message, initiates shutdown immediately and remains the daemon's terminal error
after cleanup. A later ordinary stop cannot erase it. Retryable external failures
remain owned by brokers and supervisors; they do not trigger this panic policy.
Task cancellation is not a failure. Unit supervision releases its token and
control record when its future ends, including cancellation and unwinding.

## Endpoint ownership

Socket binding refuses non-socket entries and reclaims only sockets whose
connection fails with ConnectionRefused. Probing is nonblocking, a full backlog
is treated as live, and connecting has a deadline. A bound listener owns cleanup; dropping
a partially built daemon or standalone observation server releases its endpoint.
Cleanup compares the current entry's device and inode with the bound entry and
leaves replacements untouched. This is not protection against hostile same-user
filesystem races.

Socket discovery uses explicit overrides or `XDG_RUNTIME_DIR`. Without either,
the current fallback includes the process ID and is not a shared endpoint across
processes. Deployments outside the desktop session must supply socket paths.

## Verification

Run the repository checks from its root:

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
crates/omega-renderer/shell/lint.sh
```

`cargo test -- --ignored` includes the real scaffold/CLI build flow and process
integration tests. `cargo test -p omega-rs` checks the SDK without workspace feature
unification. CI also checks the declared Rust minimum, dependency use and policy.
Socket-pair tests and paused Tokio clocks exercise request ordering, deadlines and
shutdown; generation tests cover publication and recovery; schema and renderer
coverage tests keep declarations aligned with their consumers.

[Open design questions](design.md) records current limitations and the decisions
needed before adding capabilities.

## Dogfooding plugins

The SDK examples include audio controls, Wi-Fi and a focus timer. They are ordinary
units with indicator/panel surfaces and explicit commands. The focus timer's `tick`
command needs a one-second document schedule; Wi-Fi signal history uses `sample`.

`derive(Form)` declares string fields, persistent labels, placeholders, help text
and secret presentation in one input type. `Form::new(Connect)` takes its fields
from `Connect::Input`; `submit_label` names the button. Forms currently support
text inputs, including secret text, rather than arbitrary Rust field types.
The whole form is submitted as one strictly decoded map. Drafts remain local to the
shell and secret fields clear after successful submission. The shell correlates
control requests by stream, disables the submitting control while pending and
shows refusals. Timeout or disconnect reports an uncertain outcome, not cancellation.
Renderer behavior tests run with `crates/omega-renderer/shell/test.sh` (also in CI).

`WifiControl` requires network capability. Connection requests acknowledge
NetworkManager activation admission; the Wi-Fi reading reports connecting,
connected or failed state independently. The initial implementation uses the first
wireless adapter, visible networks and open/personal authentication or saved
profiles. Newly supplied credentials create a volatile NetworkManager profile,
not a persistent saved network. Hidden networks, enterprise authentication and
adapter selection require explicit additional APIs.

The focus timer uses Linux boot time through rustix's safe API: suspend counts,
wall-clock adjustments do not. Tick scheduling remains in the daemon and uses
Tokio's monotonic deadlines. The timer record survives unit replacement while the
daemon lives; daemon restart resets it. Completion is recorded before notification,
so notification is at most once and may be lost if the unit crashes between them.

`Section` and `Metric` compose existing stack/text nodes; they introduce no new
protocol kinds. `Emphasis` (primary, secondary, muted) describes importance;
`Tone` (neutral, warning, error, success) describes meaning. Tone takes precedence
when choosing semantic colors, and neither property changes interactivity.
The Rust API uses `Choice`, `fill_width`, `padding` and `muted`; established wire
identifiers such as `group`, `fill` and `pad` remain unchanged.

## Generated Omarchy configuration

The configuration plane's `Shell` owns the entire Omarchy `shell.json` document.
One ordered layout contains native widgets and Omega plugin placements. Each
plugin placement generates both its shell entry and its render-instance
declaration; unit, surface, placement identity, panel, and settings cannot drift
between independently authored layouts.

The host-specific shell declaration is carried opaquely in `StateDocument`;
`omega-document` owns its interpretation and compilation. Plugin SDK consumers
do not acquire Omarchy configuration dependencies. Compilation is pure and
validates the supported version-1 JSON format. Extension fields cannot overwrite
typed fields. The daemon validates widget references against the generation's
manifests and checks staged `shell.json` against the compiled declaration.

Shell output is staged with the existing build generation. Application uses
`ShellInstallation` and `AtomicFile`, with an advisory lock between Omega writers.
Its receipt stores the last installed semantic configuration and an in-progress
target, allowing recovery after interruption between target and receipt writes.
The first adoption keeps a backup. Paths come from `Layout`; `OMEGA_SHELL_CONFIG`
selects an isolated target for tests or alternate installations.

Generation activation applies the shell once; ordinary unit convergence does
not rewrite it. A shell conflict does not prevent valid plugins from starting.
`omega shell diff` compares live and generated configuration, and the
operator-only `ApplyShell` request applies a validated published generation.
`--overwrite` acknowledges external changes, but does not implicitly adopt an
unmanaged file. Rollback uses the same generation and application path.

Omarchy can persist inline widget settings itself. These writes become external
changes: Omega neither translates them back into Rust nor silently removes them.
Concurrent writes by programs that ignore Omega's lock cannot be fully
serialized; the installer rechecks the file immediately before replacement.
Installation across shell files, generation acceptance, and process lifecycles is
not an all-or-nothing transaction. Diagnostics distinguish accepted builds from
shell application failures.

Adoption emits a Rust module for review and never rewrites arbitrary Rust source.
Fresh non-bare initialization imports an existing shell configuration before
taking ownership. Unknown settings are preserved through explicit extensions;
ambiguous Omega entries and unsupported layout shapes fail rather than disappear.
