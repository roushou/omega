# Architecture

Maintainer reference for implemented subsystem boundaries and runtime contracts.
For plugin development, start with the [authoring guide](authoring.md).
The [architectural principles](principles.md) define rules for implementation and review.
The [desktop platform decisions](desktop-platform.md) summarize composition rules;
[open design questions](design.md) track limitations.

## Two planes

The **configuration plane** is a Rust program that computes a state document
with no side effects. The **runtime plane** is units running as supervised
processes, converged toward that document.

The daemon runs published binaries and documents without invoking Cargo.
A failed build leaves the last accepted generation active.

## Crate boundaries

| Crate                | Responsibility                                                             |
| -------------------- | -------------------------------------------------------------------------- |
| `omega-base`         | Shared mechanisms: typed pipeline execution and observation                |
| `omega-proto`        | Schema, identifiers, wire vocabulary, codecs and socket contracts          |
| `omega-keyboard`     | Logical keyboard events, chords and conflict-checked keymaps               |
| `omega-host`         | Layout, durable files, generations, workspace documents and discovery      |
| `omega-document`     | Desired-state builders and shared validation                               |
| `omega-derive`       | Field-based SDK derives                                                    |
| `omega` (`omega-rs`) | Unit-author SDK; depends on proto, derive and keyboard, not host or daemon |
| `omega-platform`     | External subsystem connections, readings and actions                       |
| `omega-daemon`       | Sessions, supervision, state, action routing and convergence               |
| `omega-renderer`     | Host-independent QML controls and generated readers                        |
| `omega-omarchy`      | Omarchy authoring, compilation, validation, transport and installation     |
| `omega-preview`      | Development case registration and isolated surface sessions                |
| `omega-cli`          | Build and deployment orchestration, environment resolution and terminal UI |

The SDK declares dependencies by fields: readings and composites read, records
hold unit memory, and effects request changes. `Reads` excludes effects from
widgets. Derives generate through `omega::internal`; the field declarations form
the manifest extracted from the compiled unit. Construction settings arrive at
handshake, with placement settings layered over unit settings for widget instances.
The protocol's optional `json` feature is used by observation/document consumers;
a standalone unit does not compile the generated JSON implementations.

Configuration entry points can return `omega_document::Result<()>`. Its error
enum wraps document, extension, validation and I/O failures; config authors can also
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

## Value conversions

Textual identifiers, topic addresses, cadences, and Cargo documents implement
`FromStr`. String identifiers also implement `TryFrom<String>` to validate and
retain owned storage, and `TryFrom<&str>` for borrowed conversion. Serde and
protocol-value decoding use the same validation. Identifier grammars and length
limits belong to their defining types.

Validated presentations and instance references use `TryFrom` from their wire
types. Effect and local binding identities use `TryFrom<u64>` and infallible
`From<NonZeroU64>`. Systemd state conversions accept unknown spellings and preserve
them in `Other`. Glyph and node-kind lookups return `Option` through `from_name`,
preserving unknown-name fallback behavior.

Format-specific decoding stays explicit: `DocumentFile::decode` reads document
JSON, and systemd status uses `from_show_output`. Renderer attachments use
`from_request` to validate feature requirements and initialize revocation state.
Platform parsers that combine inputs or produce collections retain named methods.

## Reading validation and render failures

The SDK mirror validates each accepted system-topic payload against its topic.
Application, media-player, and Bluetooth-device collections are converted to
owned, typed models once per accepted revision. `Reading<T>` distinguishes pending,
explicit absence, invalid data, and valid data, including empty collections.
Invalid readings replace older data atomically, expose a topic/revision diagnostic,
and recover on a newer valid or absent reading. Production and fixture contexts
use the same mirror; accessors never parse identifiers during rendering.

A surface render collects local bindings before committing them with its validated
tree. Capacity and identity exhaustion reject the entire render. Failed renders
revoke the preceding bindings and publish a rootless failed tree with a bounded
diagnostic, clearing stale UI. Failures are cached until dependency or model
invalidation; publication acknowledgements do not retry rendering. The daemon
projects per-instance failures into health without changing process lifecycle.

Unit tokens and instance identities require OS randomness. Failure is returned to
the caller before installing identities. Development adoption prepares its token
before stopping the current process; supervised spawn failures follow the ordinary
failure-reporting and backoff path.

## Workspace and build ownership

`omega-host::cargo` owns `Manifest` (`Cargo.toml`) and `Config`
(`.cargo/config.toml`). Each retains one source document, including comments and
unknown fields. Package and workspace views borrow from that document; dependency
accessors return typed values. Parsing checks TOML syntax, and accessors validate
the fields they read. Fallible edits leave the document unchanged on failure.
`TomlSchema` supplies each document's codec and location, so `TomlFile` preserves
the source representation through reads and atomic writes.

`omega-host::package::PackageName` validates names used by both plugin and library
scaffolding and supplies their Rust identifiers. CLI callers use that type directly.
`cargo::PathPattern` handles member matching and directory expansion.
`omega-host::workspace` owns source roles and `Plugins::discover`. CLI scaffolding selects editions, dependency recipes, member
patterns, and release settings. Initialization, plugin creation, and checkout
linking edit the host Cargo documents; workspace locks and recoverable file
publication remain in the CLI's workspace operations.

Settled filesystem watching lives in `omega-host::fs` behind the optional `watch`
feature, enabled by CLI and daemon. Document-only consumers do not enable native
watching. The daemon’s `watch` module selects the generation paths to observe;
the shared watcher owns event filtering and settlement. It coalesces create,
modify and remove notifications into wakeups; callers re-read state rather than
receiving typed filesystem events.

`omega-host::cargo::Cargo` owns asynchronous Cargo invocation. Construction requires
a working directory and defaults to `cargo` on PATH, with an explicit executable
override. Each request owns package selection and dependency-resolution policy.
Build requests default to the dev profile; library tests use Cargo's test profile.
Target directories are overridden only when requested. Environment and local Cargo
configuration are inherited. CLI `Build::request` selects Omega's target directory and
the caller's build profile; previews retain the workspace's Cargo configuration.
Status queries request offline, locked metadata.

Metadata and compiler messages are decoded with `cargo_metadata`. Metadata capture
is limited to 64 MiB stdout and 64 KiB stderr. Test compilation streams JSON lines
with an 8 MiB per-line limit, retaining at most 64 KiB of diagnostics plus a truncation
marker. Malformed known messages fail; unknown message reasons are ignored. Library
test selection uses the requested package ID and refuses missing or ambiguous
executables. Preview rebuilds resolve that ID again to account for manifest edits.

Cargo operations have no built-in deadline. Dropping an operation kills its direct
Cargo child, without promising descendant termination or rollback of filesystem
effects. Workspace locking and caller deadlines remain application policy.

The CLI’s `build` module owns compilation policy, manifest extraction, document
evaluation, staged publication, and generation-specific activation waits. Its
`Check` operation validates without publishing. Build/check command entry points
parse options and supply an explicit layout and profile. Checkout diagnostics
belong to checkout operations, not to the link command parser.

## Subprocess execution

`omega-host::process::Process` accepts a Tokio command and owns short-lived process
execution. It closes stdin and either inherits output or captures stdout and stderr
concurrently under explicit byte limits. A limit violation is an error, never
silent truncation. Exit status and raw bytes remain available to callers, which own
arguments, decoding, and the meaning of a nonzero exit.

There is no default timeout. A caller-selected timeout covers waiting and pipe
draining after spawn. Execution failures and timeouts kill and reap the direct child
before returning, reporting cleanup failures separately. Dropping the future requests
a kill with Tokio's best-effort reaping. Neither case supervises descendants or rolls
back effects; inherited pipes may outlive the direct child and require a timeout.

Cargo builds and metadata, systemd commands, initialization probes, plugin manifest
queries, system-document evaluation, shell restarts and rescans, and preview environment
probes use this executor. Cargo's compiler-message reader retains its streaming
child lifecycle and parsing in
`cargo`; long-lived development and preview processes retain their own supervision.

The CLI allows 10 seconds for plugin manifests and 30 seconds for system documents,
with 8 MiB stdout and 64 KiB stderr each. Initialization probes allow 10 seconds;
shell restart allows 45 seconds. Both capture at most 64 KiB per stream. Cargo and
systemd limits remain with their respective clients. Shell rescan and each preview
environment probe allow 10 seconds and 64 KiB per stream. Rescan failure is reported
as a warning after file removal; it does not undo the removal. Preview rejects
nonzero exits, empty responses, and invalid UTF-8 before publishing capture artifacts.

## Systemd service ownership

`omega-host::systemd` owns systemd integration. `ServiceUnit` renders a simple
service definition from a literal `ExecStart` and explicit dependencies, restart
policy, and stop deadline. Command words are quoted, environment expansion is
disabled, and percent specifiers are escaped. `UnitName` rejects paths, patterns,
and option-like names.

`Manager` owns the user/system scope, systemctl executable, and per-command
30-second deadline. It invokes systemctl directly, limits each output stream to
64 KiB, and preserves failed exit status and stderr. Dropping or timing out an
operation kills the local systemctl process; an already submitted systemd job
can still finish. Callers inspect state before retrying an uncertain operation.
`Status` reads load, activity, enablement, fragment path, and reload state from
one `systemctl show` call. Inactive or missing services are data; transport errors
and malformed responses are errors. Unknown future state names are retained.

`Service` binds a manager, unit name, and absolute unit-file path. Its file
inspection distinguishes absence from read errors. Installation returns a
`Replacement` for the existing recovery machinery. Removal synchronizes the
parent directory. Neither operation reloads systemd or changes service state.
Installed file contents, loaded manager state, and application readiness remain
separate observations.

The CLI's `DaemonService` defines Omega's graphical-session binding and shutdown
timeout. `ServiceManager` resolves the user unit directory once, including
`OMEGA_SERVICE_DIR`. Daemon commands and initialization construct the same host
handles. The CLI selects activation order, checks foreground daemon conflicts,
publishes file replacements, and verifies the daemon protocol after activation.

## The plugin API

`omega::platform` groups SDK handles by external service domain. `audio` owns
output and media readings and controls; `power` owns battery, mains, profile,
peripheral readings, and composite power status. `network`, `bluetooth`,
`desktop`, `system`, `session`, `time`, `notification`, `process`, and
`applications` own their corresponding service contracts. Each domain defines
its handles beside their accessors and controls. Native implementations stay in
`omega-platform`; the SDK depends on neither it nor its native libraries.

The root exposes `Surface`, `Command`, `Reaction`, `Plugin`,
`View`, derives, errors, and shared values such as `Percent`. `surface` owns UI
entry points, typed references, local messages, tasks, and lifecycle contracts.
`command` owns public endpoints, input decoding, and typed command references;
`reaction` owns event-triggered behavior. `ui` owns component composition and
interaction bindings. `plugin` owns registration and Omega supervision readings,
separate from the machine resource readings in `platform::system`.

The private `MountedSurface` execution contract lives with the production surface
instance runtime. Registration constructs implementations; runtime publication and
the harness consume the same contract. Surface execution does not depend on the
registration module. Derive-facing contracts continue through `omega::internal`.

`record` separates the shared record contract from its read-only and writing
handles. `effect` owns generic completion, receipts, and queue admission; concrete
controls belong to their service domains. Private `wiring` generates field
construction and capability declarations without centrally defining domain types.
Runtime context and the replicated mirror live under `runtime`; registration
storage lives under `plugin`. Derives access a narrow `internal` export boundary.

`Reads` excludes effects from render declarations regardless of module location.
Topic-coverage tests require exactly one primitive handle per system topic;
composites declare multiple dependencies without adding another primitive reading.

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

Reload replaces changed plugin processes. Process supervision also handles crash recovery.

## Protocol

Protocol messages use Protobuf schemas and length-prefixed frames over Unix
sockets. Peer credentials establish process identity. The observation socket
encodes the same requests and outcomes as newline-delimited protobuf JSON.
Multiplexing, request correlation, flow control, and version negotiation are
implemented above the encoding layer. `crates/omega-proto/schema/` defines the
shared messages; generated Rust types are used by all native participants.

The current protocol version and minimum accepted version are both 7, defined in
`omega-proto/src/protocol.rs`. Rebuild plugins and update the daemon together
when that compatibility boundary changes. Action and adoption execution is bounded per control or
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

The identity also carries the defining package for document actions.
`Actions::invoke(focus::Tick)` requires a command with `Input = ()`;
`Actions::invoke_with(audio::SetVolume, Percent::whole(50))` encodes its typed input.
Schedules and keybindings consume the same action. Dynamic targets use
`invoke_named` or `invoke_named_with`, with target and input validation at runtime.

Scalar inputs consume exactly one argument; `()` consumes none. `derive(Input)`
decodes a strict map. Missing, mistyped and unknown fields are refused before
`call` runs, rather than defaulting as configuration and records do. `Args` remains
an explicit raw input for commands implementing an external argument grammar.
`Called::of` accepts typed inputs; `Called::raw` exercises malformed wire input.

`omega run` sends arguments as text; scalar `Input` implementations parse them
according to the command's declared type. Numbers and booleans use Rust's text
syntax. `Percent` accepts `40%` or `0.4`, rejecting out-of-range and non-finite
values; power profiles accept `saver` (`power-saver`), `balanced`, and
`performance`, as well as their wire names. Strings are preserved verbatim.
This conversion is confined to command inputs: `FromValue`, configuration,
records, and derived input maps retain their strict value types.

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

Unit supervision operations require the operator. Presentation changes also
accept scoped renderers; plugins may only hide or close their own instances.

Grants are read from the daemon's copy of the manifest, never from a frame.
Capabilities cannot be self-declared at runtime: a unit's manifest is
extracted at build time from the binary itself.

`authorization` owns role and manifest-grant values. Session admission owns peer
credentials and operator authentication; the dispatch policy table consumes these
facts to authorize operations. The sibling `attachment` module owns renderer
scopes, negotiated features, and revocable instance permits. `UnitTable` owns
renderer claims and coordinates revocation with instance mutations under its
existing locks. Attachment metadata parses protocol identity directly and does
not depend on the unit table or session dispatch.

Native units run as the owner’s uid. Manifest grants constrain authenticated unit
sessions; they do not sandbox hostile code, prevent another operator connection,
or restrict that uid's filesystem access. A manifest hash verifies declaration
agreement, not binary authenticity. The dispatch [policy table](../crates/omega-daemon/src/session/dispatch/policy.rs)
is the operation-level authorization contract.

## Units

A unit declares UI surfaces and command endpoints separately in its manifest.
A widget surface renders: the daemon creates each instance with construction
settings and pulls its first tree; the unit pushes subsequent trees. A command
endpoint is invoked by `omega run`, a retained UI binding, or another unit with
`CAPABILITY_SPAWN`. Commands are not UI instances.

`omega-proto::CommandAnswer` defines terminal command and interaction answers:
an acknowledgement or a value, including an empty value. Daemon action routing,
retained-binding interactions, and CLI command decoding share this boundary.
Other reply kinds, including streaming output, are refused as `INVALID_ARGUMENT`;
peer refusals retain their declared code and message. An acknowledgement retains
the operation's semantics: detached shell actions acknowledge admission, not exit.

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

Workspace controls use validated `WorkspaceIndex` and `WorkspaceName` values.
The SDK effect has no reading dependency and queues the existing `SwitchWorkspace`
action; `ActionKind` remains the authority for its capability cost. Focus is
observed through `Workspaces`, never inferred from a dispatch acknowledgement.

The Hyprland adapter selects legacy or Lua dispatch syntax from `j/status` before
each action. An older compositor's explicit `unknown request` response selects
legacy mode; unknown providers and malformed replies fail. The pure dispatcher
encodes arguments as data, and the transport sends an action once without fallback
replay. Monitor-focus changes invalidate monitor, workspace, and window readings.
Workspace focus is joined by compositor ID; malformed focus replies fail the read,
while an empty object explicitly means no focused workspace.

A session registration is a lease. Replacing it cancels the previous session;
its guard cannot disconnect the replacement. Adoption cleanup also checks the
issued token. Views and installed instance specifications end with the lease.
Runtime identity is `(InstanceId, IncarnationId)`, allocated by the daemon.
Surface IDs name declarations; placement IDs name desired configuration. Neither
is an interaction capability. There are no automatically created unplaced widgets.
`PresentationProvider` reconciles bar indicators, anchored popups, and configured
standalone presentations through the same instance registry under `UnitTable`.

Document validation limits bar placement IDs to 128 bytes, matching embedded
presentation creation and configured standalone placement IDs. Omarchy-projected
bars use the same validation. General `ModuleId` parsing remains unbounded; the
length rule applies when an identifier declares a presentation placement. Oversized
bar IDs fail build/check validation before publication, rather than convergence.

### Independent presentations

`Presentations::window` and `Presentations::overlay` in `omega-document` accept
typed widget references. They compose with `Document::presentation`; settings
layer over plugin settings at construction. `omega present <unit> <surface>` opens
a transient singleton; `--new` creates an independent instance, and `--config`
accepts an ordinary JSON object. Changing an existing singleton's construction
settings or presentation is refused; configured changes destroy and recreate it.

Create allocates identity and initializes the widget before exposing it. Present,
hide, and close retain the instance; destroy removes its authority and tree before
asking the unit to release it. Requested and observed visibility are separate.
Native dismissal records closed intent, so reconciliation and renderer recovery
keep it closed. A plugin restart expires all its identities; configured instances
are reconstructed and transient instances are lost. An uncertain creation/removal
terminates the affected plugin session rather than keeping untracked instances.

The private `units::presentation_state` value owns request, report, and disconnect
decisions. Intent can only be visible, hidden, or closed; observation additionally
allows unknown. Closed reports set both facts to closed, other reports update only
observation, and disconnect preserves intent. Explicit reopen can therefore leave
visible intent with a last observation of closed until the renderer reports again.
Instance construction chooses the initial values. UnitTable retains lifecycle
serialization, publication-before-commit for request/report transitions, and plugin
acknowledgement/cancellation guards; the value owns no locks, tasks, or session state.

One supervised Quickshell host per plugin renders its windows and overlays from
embedded shared assets. Hosts restart with bounded backoff and reattach to live
instances. Normal windows identify as `org.omega.<unit>`; initial size and focus
remain subject to compositor policy. Overlays declare keyboard policy and an
optional output; a missing named output keeps the overlay hidden. Closed instances
keep their host warm until destroyed. Startup failures are logged to
`~/.cache/omega/logs/<unit>.renderer.log`; accepted presentation intent does not
promise a visible native window.

The owner bootstraps a renderer on the observation socket with `AttachRenderer`.
Its connection then has renderer authority only: one plugin's windows/overlays,
or one embedded/popup placement. Features must include instance identity, scoped
interactions, and each consumed presentation kind. Replacing the same attachment
revokes its predecessor. State topics remain openly observable; private view trees
require attachment. The attach answer contains bounded metadata, followed by
individual scoped view updates. Lag repairs use an atomic snapshot/history boundary;
destroyed or expired instances produce tombstones. Disconnect invalidates observed
visibility without changing requested intent.

Omarchy bar replicas share one connection and presentation transaction per placement.
The adapter's `PanelSession` selects one native popup owner: clicking a replica
opens the panel on that output, and clicking another replica transfers ownership
without hiding the instance. Only the owner may dismiss it. Removing that owner
requests a hide; a remote present without an owner selects the first registered
replica. Observations describe the shared presentation, not each inactive replica.

Interactions carry identity, retained revision, node key, event name, and optional
control value. The daemon resolves the binding and arguments from its retained tree;
it rejects stale revisions, hidden instances, disabled/busy ancestry, duplicate
keys, undeclared commands, and instances outside the attachment. Per-instance QML
sessions isolate busy state, failures, and form completion even when windows share
one transport. Pending interactions are never automatically retried after disconnect.

`omega-proto::Interaction::resolve` owns tree-level eligibility for daemon
interactions, `SurfaceHarness`, and surface previews. It returns a nonzero local
binding or borrowed command/arguments, without executing behavior. Duplicate target
keys are rejected before availability or event lookup, independent of traversal
order; unrelated duplicate keys do not affect the selected interaction. Disabled
or busy ancestors block their descendants, and local bindings cannot also carry
command names or arguments. Daemon scope, revision, visibility, and command-grant
checks remain outside this resolver. The SDK instance runtime retains ownership
of local captures and rejects stale or foreign binding IDs.

Limits include 256 instances per plugin, 4096 globally, 128 KiB construction settings
per instance and 8 MiB in aggregate, 64 observation connections, bounded request
queues, and five-second observation writes. The hub also bounds retained views.
The SDK caps pending view publications and retries current trees after acknowledgments.
All participants must negotiate a supported protocol version. Renderer attachment
also requires local-message and controlled-input features. Commands are separate manifest
endpoints throughout; wire manifests cannot advertise them as UI surfaces.

### Surface instances

Every UI declaration implements `Surface` and registers with `Plugin::surface`.
`derive(Surface)` wires read-only dependencies and construction settings and produces
its typed `SurfaceRef`. The implementation declares `Model`, `Message`, and
`Effects` associated types. Use `()` for unused model/effects and `Infallible` for
no local messages, with an exhaustive empty match in the required `update` method.
Only behavior receives effects; render receives an immutable model and event builder.
All declarations use the same instance constructor, binding registry, and task scheduler.
`SurfaceHarness` and surface previews support every surface. `Drawn::of` is a fallible
one-shot helper using the same initialization and readiness checks.

Each instance owns its model, current binding registry, tasks, and cached view.
Local event IDs are process-unique and never reused across renders. The renderer
submits node/event identity against a retained view; the daemon resolves either a
public command or a local ID, and the plugin checks that ID against the current
instance registry before decoding input. Captured values never cross the socket.
A superseded binding is refused rather than interpreted against newer captures.
There is one current registry, bounded to 4096 entries. A binding-generation change
is a semantic view change even if visible labels are identical.

Updates and lifecycle hooks are serialized on the plugin runtime. Declared topic
changes invalidate affected instances; acknowledgments do not render again. Startup
waits for required readings. `Optional<R>` retains subscriptions/capabilities but
removes that field's readiness gate, exposing `is_pending()` alongside the handle's
existing absence/accessor methods. Model initialization happens once before gating.

Managed asynchronous and blocking work has instance ownership, a replaceable key,
a task ID used as its generation, and bounded admission. Late completions must still
own their key. Close aborts delivery and forgets pending messages/bindings before
the closed hook. Hide retains work. A blocking worker cannot be forcibly unwound;
its permit stays inside the worker until exit, even after close or replacement.
External effects retain their independent receipt/accounting semantics. Dropping a
local task cannot undo or replay an admitted action.

Controlled text records committed edit and explicit reset revisions. Rust ignores
edits from old resets; QML ignores values older than its current draft and defers
composition. Edit traffic is coalesced while a request is pending without disabling
the editor. Initial focus and field-to-list navigation are scoped to an instance;
component key scopes also qualify navigation targets. Lists preserve selected
identity across reorder and clear a controlled selection whose key disappeared.

`SurfaceHarness` reuses production model/binding/task execution with isolated effect
queues and explicit completion outcomes. The stateful search example uses fixture
catalogue data and does not launch desktop applications.

## Reconciliation

Domain providers compute plans before applying changes. Presentation preparation
compiles the Omarchy payload once and resolves declarations against stored
manifests. The convergence caller captures `UnitTable::installed_presentations`,
then passes desired and installed maps to the pure `PresentationProvider::plan`.
The snapshot copies configs, presentation specifications, and retained anchor
addresses while holding the unit lock; transient and starting instances are
excluded. Requested/observed visibility is not a construction input, so planning
does not reopen dismissed windows. Apply still resolves live anchors and sessions.

Environment preparation validates and renders the document before the caller
reads the installed file. Its pure plan compares those captured bytes with the
prepared contents; apply writes the retained contents through `AtomicFile`. An
absent or empty file already satisfies an empty declaration. Other read failures
abort planning before any provider applies changes.

Unit planning takes the build's unit-name set and a captured set of supervised or
adopted names. Schedule planning takes captured timer declarations. Both validate
the desired document and compare only explicit inputs. The caller captures each
provider's facts separately; these snapshots are not an atomic system-wide view.
Unit application retains the handover lock and live ownership checks. `Schedules`
retains task ownership, shutdown checks, and unchanged timers; planning does not
claim that a future application will succeed.
Convergence runs
in one task, one pass at a time, with changes keyed by entity ID. The daemon owns the convergence task, cancels
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

| domain          | converges                    |
| --------------- | ---------------------------- |
| `units`         | which units run              |
| `environment`   | the session environment file |
| `schedules`     | persistent timers            |
| `presentations` | configured surface instances |

Topics separate facts with different update rates. Brokers compute transfer rates
from successive counters and handle counter resets before publication.

Schedules use `every <n><s|m|h|d>` with an immediate first tick. Cron is unsupported.
Each tick emits `EVENT_SCHEDULE_FIRED` and dispatches the optional action.
Document schedules are trusted configuration, independent of peer capabilities.

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
  Cargo.toml Cargo.lock   workspace; members are system/ and registered plugins/
  system/                 → document.json, one entry point, no side effects
  plugins/                independent runnable packages
  crates/              reusable Rust libraries, never supervised
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

Scaffolding separates content generation (`scaffold/`), workspace mutation
(`workspace/`), and local checkout overrides (`checkout/`). `ConfigWorkspace`
holds `.cargo/omega.lock` while commands inspect and change source files.
Prepared operations validate names, membership, and dependency conflicts before
publishing. Existing Cargo documents retain comments and unrelated options;
initialization adds missing ignore entries without replacing the user's rules.

New plugin directories are staged under `.cargo/staging`, outside `plugins/*`
membership globs, and published with a no-replace rename. File
edits retain original bytes, reject stale preparation, and attempt rollback on
failure without overwriting subsequent edits. This is not a crash-atomic
multi-file transaction. Incomplete rollback reports the affected paths. A retry
can complete missing files and matching manifest references; existing plugin
source is never replaced. Automatic checkout linking applies only to a newly
created workspace without an existing Cargo config; subsequent source changes
use `omega link`.

Build generations are immutable through Omega's APIs. Publication flushes a
private stage before replacing `current`. Acceptance is a separate durable record
written after validation and handover preparation; it does not certify unit health
or completed convergence. Startup tries `current`, then accepted and previous
builds if the candidate is unusable, leaving a rejected pointer intact for diagnosis.

The convergence worker projects live activation and reconciliation progress into
`Deployment`. The operator-only `GetDeployment` request returns that snapshot
with phases read from `UnitTable`; it does not infer acceptance from build files
or duplicate process state. `omega status` compares this snapshot
with the locally published generation. A rejected candidate remains visible while
the accepted build continues to converge. Shell application records its own
generation and result because explicit application can precede activation.
Ordinary `omega status` uses this operator snapshot; `--json` writes its protocol
JSON to stdout, including generation identities and plugin health. Human-readable
status goes to stderr; `omega status <plugin>` adds instance details and log paths.

Plugin health is an on-demand projection from `UnitTable`: manifest surfaces,
the latest validated placement plan, session-owned instances, and retained view
readiness. Lifecycle and health are sampled under the same unit-table lock.
Missing configured instances remain waiting across session loss; unplaced means
no desired placement or live instance. A running plugin without declared surfaces
is a healthy background plugin. A failed render makes plugin health failed and
exposes the instance diagnostic. Otherwise, one waiting instance makes the plugin
waiting, even if another instance has rendered. This does not change the lifecycle topic
or feed diagnostic reads back into plugin invalidation.

The SDK attaches startup readiness to each view. Waiting trees name required
system topics not yet received; explicit topic absence satisfies the gate.
Readiness changes participate in publication deduplication, so waiting can become
ready even when both trees have no root. The daemon validates metadata and projects
it without returning private view contents. Legacy nonempty trees prove a render
occurred; legacy empty trees remain unknown. Ready describes rendering only;
renderer attachment and requested/observed visibility remain separate facts.

`omega build --wait` captures the
identity from its own reserved stage before publication and waits for that exact
build's acceptance, settled reconciliation, and successful shell application (or
no shell declaration). Rejection, shell failure, and superseding publication fail
the wait. A timeout bounds observation, not daemon work; the build stays published.
These results describe the current daemon lifetime and the last pass or application,
not a persistent health history or proof that the shell file has no external edits.

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

Each surface instance tracks readiness, dependency/model invalidation, and its
cached view. Required system topics must have reported before the first render;
an explicit absence is a report, and unwritten records use defaults.
`Optional<R>` keeps the subscription while removing its startup gate and exposes
`is_pending()` separately from the reading’s availability.

An instance awaiting required topics answers with an empty tree and pushes its
view once ready. Dependency changes, local messages, and task/lifecycle updates
invalidate the instance; unrelated state patches and publication acknowledgments
do not rerender it. Identical trees are not republished. The daemon retains its
own deduplication boundary for all clients. Closing clears bindings and cancels
task delivery; destroying discards the instance and its cache.

Positional keys identify fixed layout positions. Explicit local keys inside
components are escaped and qualified by their parent path; moving component
instances need stable caller-supplied keys. Incremental wire updates require a
measured workload and a snapshot recovery contract.

Observation connections retain instance identity, surface and presentation metadata,
visibility and revision for deletion repair after lag, with the retained tree root
cleared. They release initial snapshots after sending them. Views and state topics
serialize by reference into a line owned by the pending write. The hub owns
authoritative trees; connections keep metadata rather than a second full-tree cache.
Each connection keeps its existing write
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

Plugins publish declarative view trees. Shared QML controls render them in both
Omarchy placements and standalone Quickshell hosts. Hosts supply theme and assets.

The CLI embeds renderer assets. `omega shell install` installs the assets carried
by that binary; replacing the binary alone does not refresh installed or loaded
QML. Installation restarts Omarchy to clear its component cache, then checks live
renderer attachments. Each attachment reports a SHA-256 fingerprint embedded in
its QML connection, covering the complete host and core asset bundle. The daemon
exposes active attachment scopes, fingerprints, and required live placements
through deployment status;
replaced and disconnected attachment leases no longer appear. Empty fingerprints
identify legacy or directly linked sources and are unverified. `--no-restart`
installs files without claiming activation. Node props come from the protocol's `NodeKind` table. Generated `Props.js`
readers, SDK emission tests and the shell's explicit undrawn-prop list keep the
vocabulary aligned. See the [renderer contract](../crates/omega-renderer/shell/README.md)
for interaction, host integration and node implementation.

## Design constraints

- Keep typed APIs concise for common desktop operations.
- Reconcile only changed entities; retain unaffected processes and instances.
- Preserve the last accepted generation when compilation or validation fails.
- Delegate external service behavior to its platform integration.
- Share UI primitives across Omarchy and standalone hosts.

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

The SDK examples include audio controls, media playback, Wi-Fi and a focus timer. They are ordinary
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
connected or failed state independently. The implementation uses the first
wireless adapter, visible networks and open/personal authentication or saved
profiles. Newly supplied credentials create a volatile NetworkManager profile,
not a persistent saved network. Hidden networks, enterprise authentication and
adapter selection require explicit additional APIs.

`Choice<T>` separates a string-backed value from its presentation. `ChoiceValue`
keys must decode through the same type's `Input` implementation; the command
binding accepts that type, while the renderer continues submitting a stable key.
Power-profile inputs reject unknown names and the unspecified sentinel before
executing a command. `disabled_if(bool)` sets conditional availability directly
on any styled node; `disabled()` is its unconditional shorthand.

`audio::Media` observes players; `audio::MediaControl` requests playback effects.
`PlayerId` validates a well-known MPRIS bus-name suffix and travels unchanged
through readings, command inputs, bindings, and records. `active()` resolves at
execution time; `player(&id)` refuses an absent endpoint without falling back.
An id identifies an endpoint, not a process lifetime: a restarted application
may reuse it. The broker resolves the current unique bus owner before calling,
so disappearance after resolution cannot activate or redirect to a new process.
The broker exposes MPRIS control abilities and refuses unsupported operations.
A successful method reply acknowledges the request; the subsequent reading is
the authority on playback state. Transport methods use a typed proxy within the
broker, and effects retain the shared capability and completion contract.

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

`derive(Surface)` supplies a `SurfaceRef<T>` whose identity is the defining crate
and the kebab-case type name; `#[omega(name = "...")]` pins the surface name.
Registration (`.surface(Indicator)`) and placement
(`PluginWidget::new("audio", audio::Indicator).panel(audio::Panel)`) consume the
same reference. Commands cannot be passed as widgets. Registration checks the
owning unit, and panel attachment checks that both widgets belong to the same
unit. Manifest validation still proves that the selected widgets are registered.
Explicit named APIs support imported and dynamic configuration.

The host-specific shell declaration is carried opaquely in `StateDocument`;
`omega-omarchy` owns its interpretation and compilation. Plugin SDK consumers
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

Bluetooth controls use `BluetoothControl` and a `bluetooth::DeviceId` taken from
a reading. The identity includes the adapter; connect and disconnect never
fall back to another device. Only paired or connected devices are exposed.
`can_connect()` requires pairing, an unblocked device, and a powered adapter.
Battery absence stays distinct from 0%. See `examples/bluetooth.rs` for an
indicator and panel with typed commands. Pairing remains in Bluetooth settings.

### Composable views

`Surface::render` returns `View`, an owned subtree that containers also accept.
`View::empty()` contributes no node or layout gap. `Ui` remains an alias for
source compatibility. Key assignment and wire encoding happen at publication,
so converting a helper to `View` does not finalize its position in a larger tree.

`ui::Component` describes reusable presentation from ordinary values and typed
`Bind<I>` inputs. Components have no registration, subscriptions, effects or
independent lifecycle; their parent widget owns those responsibilities. Containers
accept components directly, including borrowed instances. Shared modifiers apply
to the component's root and return a `View`, without introducing a layout wrapper.

Each component instance scopes its internal keys by their full parent path. Moving instances need
stable caller-supplied keys. Local key segments escape `~` and `/`; generated
positions use a reserved segment so they cannot collide with explicit local keys.
Calling `Component::render` directly bypasses its boundary; compose the component
itself or convert it into `View`. `Drawn::of_view` exercises the same conversion.

List and choice values remain independent of scoped node identities. When scoping
changes those identities, the SDK carries the original value in `selection_key`.
The renderer distinguishes an absent property from an explicitly empty value and
falls back to node keys for views without that property.

### Workspace and renderer boundaries

Source members have validated roles: `system/`, direct children of `plugins/`, or
of `crates/`. Library crates build with the workspace but are never queried for
unit manifests. Builds and source mutations share the workspace lock. Published
generations use `units/` paths for runtime binaries.

`omega-document` owns core desired state and the `DocumentExtension` composition
contract. `omega-omarchy` owns shell authoring, compilation, validation of its
projected instances, transport adapter, and installation. Its validator removes
the handled shell payload before invoking core validation, which rejects an
unhandled payload. The shell payload remains host-specific; core validation does not interpret it.

`omega-renderer` embeds shared QML controls, standalone hosts, and preview assets
and depends on `omega-proto`.
Both the Omarchy adapter and the isolated fixture harness use that core. Theme,
assets, interaction session, and allocated dimensions enter at `ViewNode`;
nested controls retain bindings to them. The fixture harness captures interactions
locally. It does not attach to a live plugin or provide Rust fixture discovery.

## Application catalogue and activation

`omega-platform` owns Linux services; plugins compile no GIO dependency.
Its applications worker owns one GLib main context and all native GObjects on a
separate thread. GIO applies XDG precedence, visibility, localization, desktop
field codes, terminal routing, and D-Bus activation. A bounded request channel
carries owned values; application startup never waits for process exit.

The `applications` topic is a complete catalogue capped at 4096 entries and
512 KiB. Native invalidations refresh it. Search, ranking, selection, and query
text remain local to each surface instance. The SDK exposes `Applications`,
`ApplicationId`, and the `Launcher` effect under `omega::platform::applications`.
Missing readings differ from a successfully empty catalogue. Activation performs
a fresh lookup and requires Spawn; success means admission, not eventual startup.
Unknown outcomes are not replayed. URI arguments remain literal. Optional tokens
are passed to spawned environments; token-bearing D-Bus activation is currently
refused explicitly, and the renderer does not acquire compositor tokens.

`surface::Presentation` is an instance-scoped behavior dependency. A plugin may
hide or close its own current instance, but may not open arbitrary presentations
or control another plugin. Lifecycle requests run outside the connection reader
so the same connection can acknowledge them. A per-instance gate orders lifecycle
delivery; cancellation before acknowledgement ends that plugin session rather
than retaining unacknowledged intent.

The launcher example composes Field, List, Image, and Text with typed local
bindings and managed tasks. Input navigation waits for the latest controlled edit
to reach the rendered list before admitting activation. Hosts resolve theme icons;
the shared renderer core receives the resolver. Overlays optionally dismiss on
outside clicks and accept an explicit output. The default output is host-selected,
not a promise to follow the active monitor.

## Development previews

`omega-preview` is a dev dependency, used from an explicit Rust library test.
`Cases` owns named fresh factories for components or production `SurfaceHarness`
instances. The same factories support initial-render checks, structural assertions,
and behavioral tests. Preview registrations do not change a plugin manifest.

The CLI builds the selected library test and connects it to a private, bounded
preview transport defined in `preview.proto`. Both runner and renderer peers are
checked against their spawned process IDs. These sockets are development channels,
not daemon observation sockets; no production session, grants, or backends exist.
Captured effects retain their bounded SDK receipts until explicitly resolved or
reset. Case epochs separate model lifetimes and clear QML-local drafts/focus.

The development host uses the shared ViewNode/Theme/Assets/control runtime. It
keeps the last good tree visibly stale during failed rebuilds. Screenshot mode
uses a fixed software environment, asset readiness, and motion suppression.
Decoded-pixel comparison is gated by environment metadata; baseline updates are
explicit. Native icon providers require fixed local fixture images in software
captures. The [preview guide](previews.md) records usage and environmental limits.

## Keyboard primitives and routing

`omega-keyboard` is a dependency-free crate for logical keyboard identities,
exact-modifier chords and composable `Keymap<A>` values. It owns matching only;
focus, native input and routing belong to hosts. The SDK adapts actions into
existing `Bind<()>` values and serializes scoped shortcut declarations on view
nodes. No keyboard event transport or additional surface lifecycle is introduced.

QML routes unconsumed keys from the focused control through ancestor view nodes.
The selected binding uses the existing instance/revision interaction checks.
A shared generated conformance corpus verifies Rust and QML matching. Typed
Component-scoped `.id()` references resolve to exact node keys before publication.
Duplicate IDs, missing references, and non-list navigation targets fail view validation.
See [keyboard APIs and boundaries](keyboard.md).

## Declarative installation pipelines

`omega-cli::initialize::pipeline` declares initialization in one place, then runs
that composition through `omega-base::execution`. Steps bind typed inputs/outputs
to explicit operations and stable identities. Test implementations replace slots
without reconstructing the sequence; isolated slots have no production fallback.
Progress and failures come from one runner. Intermediate artifacts never execute
the next action implicitly.

Initialization and `omega build` share compilation, validation, and publication
operations. A validated build owns the source lock and unpublished generation.
Initialization verifies the daemon before publication, then waits for the exact
generation's shell application and verifies renderer attachment after restart.

Filesystem recovery stays in `omega-host`, which has no execution dependency.
`SavedChange` owns durable intent and postcondition checks; `Replacement::install`
is a single domain operation, including the unchanged-target case. Shell ownership
and service activation retain their own contracts. See [pipelines and recovery](workflows.md)
for lifecycle rules, test replacement examples, and recovery boundaries.
