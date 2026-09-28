# Omega schema

These Protobuf schemas define the messages shared by Omega's daemon, plugins,
configuration tools, and renderer. `omega-proto` generates Rust types from them
at build time. Edit the schemas, not generated Rust or JSON implementations.

## Organization

Schema paths below are relative to `schema/omega/`.

| Schema              | Responsibility                                           |
| ------------------- | -------------------------------------------------------- |
| `wire.proto`        | Frames, handshake, and bidirectional requests            |
| `value.proto`       | Generic values for settings and command payloads         |
| `state.proto`       | Topic envelope and replicated patches                    |
| `state/*.proto`     | State payloads grouped by domain                         |
| `action.proto`      | Actions and keybindings                                  |
| `event.proto`       | Events                                                   |
| `plugin.proto`      | Manifests, surfaces, and capabilities                    |
| `instance.proto`    | Instance identity, presentations, renderer attachment    |
| `ui.proto`          | Declarative view trees                                   |
| `document.proto`    | Desired desktop configuration                            |
| `preview.proto`     | Isolated development sessions, separate from live frames |
| `observation.proto` | Observation socket message selection and heartbeat       |

## Contract

Actions, events, and state topics have closed taxonomies. Extensible values and
custom events use explicit protocol fields. Add a state topic to its domain
schema and to the topic envelope; keep existing field tags and enum numbers
stable, and reserve removed fields.

Desired configuration and observed state have separate messages. The daemon
owns topic and view revisions. A topic with its payload unset explicitly reports
absence, such as a missing battery or an unavailable broker. An unpublished topic
is not equivalent to an absent one. Required readings gate surface startup;
optional readings retain subscriptions without delaying the first render.

`ViewTree.readiness` distinguishes waiting from a completed render, including an
empty render. Waiting trees have no root and carry distinct, valid system-topic
names in `pending_topics`. Ready and unspecified trees carry no pending topics.
Failed trees have no root or pending topics and carry a nonblank `render_error`
of at most 4096 UTF-8 bytes. Other readiness states carry no render error.
Failure replaces the previous tree and its interactions for that instance;
other instances in the plugin remain usable. Health includes the diagnostic.
An unspecified legacy tree with a root proves rendering occurred; an unspecified
empty tree is ambiguous. Readiness metadata participates in view deduplication.

Choose topic boundaries and update resolution around what should wake consumers.
For example, the clock topic reports minute-resolution time. Payloads should not
carry redundant constants or duplicate facts that can disagree.

Identifiers and payloads are validated at their boundaries. Wire strings do not
imply arbitrary accepted input, and generated message types alone do not establish
domain validity.

## Requests and failures

Every invocation receives an outcome or refusal on its own stream. Stream IDs
are allocated by parity: daemon even, peer odd. The observation socket uses the
same request and result vocabulary encoded as JSON.

The observation socket carries one protobuf JSON object per line: `Frame` for
requests/results, `StateTopic` for state updates, `InstanceSnapshot` for scoped
view updates (including destruction), and `Heartbeat` for liveness. There is no
additional envelope. Their discriminating keys are `invoke`/`result`, `topic`,
`instance`, and `heartbeat`. A view's optional `module` names a configured
placement; it is absent for transient instances. Requested and observed state
use protobuf enum names, with default scalar fields omitted. Consumers must
apply schema defaults rather than require every key.

Malformed request payloads and unknown JSON fields receive `INVALID_ARGUMENT` on
the original stream when the top-level `streamId` or `stream_id` is uniquely
present and a valid uint64. Both decimal strings and integer JSON numbers are
accepted. Duplicate IDs (including aliases), missing/invalid IDs, and invalid
JSON cannot be correlated and use stream zero. This recovery never admits the
malformed request for dispatch. Valid JSON refusals leave the connection usable;
oversized lines, invalid UTF-8, and exhausted transport capacity close it.
An operation result that cannot be encoded as JSON is replaced by an
`INVALID_ARGUMENT` refusal on the same stream. This does not undo completed
effects. An unencodable unsolicited publication terminates that observer's
connection with a logged encoding error.

Live requests are authorized before payload validation and routing. Validation in
`omega-proto::action` covers every action kind, including required fields, enum
values, numeric ranges, identifiers, and process/D-Bus string constraints.
Desired documents and scheduled actions use the same validation rules. Document
validation also checks scheduled plugin and command references against the build.
Backend availability and encoding restrictions remain handler responsibilities.

Failure codes distinguish malformed input (`INVALID_ARGUMENT`), unmet
prerequisites (`FAILED_PRECONDITION`), unavailable services (`UNAVAILABLE`),
aggregate admission limits (`RESOURCE_EXHAUSTED`), and oversized individual
payloads (`PAYLOAD_TOO_LARGE`). `DEADLINE_EXCEEDED` means the wait expired; it does
not establish whether an external action completed.

`GetDeployment` is an operator-only snapshot of generation acceptance, the last
reconciliation pass, shell application, current plugin phases, and plugin health.
Health projects declarations, desired placements, instance identity, readiness,
missing readings, and presentation state without exposing view trees. It is
independent of process phase and native renderer attachment. Shell results
carry their own generation because explicit application can precede activation.
A settled reconciliation pass is not a guarantee that all plugin processes are
running; their phases remain authoritative.

## Verification

From the repository root:

```sh
cargo test -p omega-proto
cargo test -p omega-platform --test coverage
```

Protocol tests cover wire shapes and validation. Broker coverage checks that
state and action declarations have implementations. Changes to UI properties
also require regenerating the renderer's readers and running its checks; see the
[renderer guide](https://github.com/roushou/omega/blob/main/crates/omega-renderer/shell/README.md).

## Media targeting

`MediaKey.player_id` selects the target. Absence means automatic
selection at execution time; a present value must parse as a `PlayerId`, and an
unavailable explicit target is refused. Empty is invalid. Daemons and plugins must
be upgraded together so an older reader cannot discard the target field.

`PlayerInfo.can_*` describes the endpoint's advertised transport abilities,
independent of a plugin's manifest grants. A method reply is an acknowledgement,
not a substitute for observing playback state.

## Bluetooth targeting

`BluetoothDevice.id` is adapter-qualified. The schema preserves
presence for `battery_percent`: absent is unknown, zero is empty. Bluetooth
connect/disconnect actions require the Bluetooth capability and a validated
device ID. A vanished endpoint is refused without selecting another adapter.
Rebuild plugins and upgrade the daemon together.

## Instances and renderer authority

UI declarations and command endpoints are separate manifest fields. RenderWidget,
RemoveWidget, and PublishView carry an explicit InstanceRef; removed module_id
fields are reserved. Instances are allocated by the daemon and expire with their
plugin session. Placement IDs locate desired configuration, never runtime authority.

AttachRenderer is owner-only on the observation socket. It narrows that connection
to a plugin's standalone presentations or one embedded/popup placement, requiring
explicit supported features. Reattachment revokes the previous scope holder.
The reply carries metadata; full views follow separately to bound individual frames.
InspectInstances is an owner diagnostic. Unattached observers receive no view trees.

Interact supplies identity, revision, node key, event, and optional value; binding
resolution uses the daemon's retained tree. Renderer operations cannot invoke
arbitrary commands or destroy instances. ChangePresentation separates requested
intent from ReportPresentation observations. Closed observations also record closed
intent so renderer recovery cannot reopen dismissed windows. Destroy expires the
identity and emits a tombstone; a repeated destroy is FAILED_PRECONDITION.

## Local surface behavior

A Bind targets either a declared command or a nonzero local ID, never both.
Local bindings cannot carry command arguments. SurfaceEvent is daemon-to-plugin
only: the daemon resolves the retained tree before forwarding ID, instance and
value. The plugin validates current registry ownership and typed input. IDs from
another render, instance or incarnation are not valid capabilities.

SurfaceLifecycle acknowledges presented/hidden/closed intent in the plugin's
serialized runtime. Close cancels local task delivery; hide retains it. The
command surface enum value is reserved; commands use CommandEndpoint exclusively.
Renderer attachment requires LOCAL_MESSAGES and CONTROLLED_INPUTS in addition to
instance and scoped-interaction features. Deploy daemon, plugins and renderer
coherently; the accepted version range is defined in `src/protocol.rs`
(relative to the crate root).

## Application services

`ApplicationsState` is a bounded full catalogue on the `applications` topic.
`Application.id` and `LaunchApp.desktop_id` are validated desktop-entry IDs.
`LaunchApp.uris` contains absolute URIs, not shell arguments; admission is an
Action result on the requesting stream. An optional activation token is never inferred from a
long-running daemon's inherited environment.

`OverlayPresentation.dismiss_on_outside` opts into outside-click dismissal.
Plugins may request Hide/Close only for their own current instance identities.
Lifecycle acknowledgement and the original request outcome use separate streams
on the same connection; the original request must not block frame reception.

## Development preview transport

`preview.proto` defines bounded JSON messages for private development sessions.
It is not accepted by daemon dispatch or the observation socket. The CLI checks
both connected peers against its spawned runner/renderer PIDs. Versioning is
independent of the production handshake. Case epochs identify a fresh model
lifetime; revisions identify its rendered bindings. Input from an old revision
is refused. Effect IDs resolve once, and snapshots expose operation kinds rather
than arguments. A reset invalidates pending effects and renderer-local drafts.

## Command contracts

`CommandEndpoint` carries input/output shapes and a description. Dependencies name
a provider-independent command ID with its canonical signature. `Bind` retains
that identity and signature; a local message has neither. `InvokePlugin` routes through
the same authority checks for SDK calls, retained bindings, and operator actions.
`ListCommands` returns a `CommandCatalogue` outcome on the request stream. Listing
is caller-scoped and grants no access. These contracts require protocol version 2.

`CommandHostStatus` projects provider phase, process facts, restart eligibility,
bounded startup errors, and invocation counts. Its recent failures contain only
command identity, invocation identity, timing, and outcome codes. Caller-scoped
catalogues include failures only for visible commands. `CommandHostPhase` is a
closed enum; an unspecified phase is unknown, not idle or healthy.

## UI vocabulary and behavior

[`NodeKind`](../src/ui/mod.rs) owns the extensible UI vocabulary on top of
`ViewNode`: property encodings, typed defaults, inclusive numeric bounds, allowed
children, and native event payloads. SDK view finalization also rejects vocabulary
that this version does not declare. Daemon publication validates known contracts
while preserving extensions from newer peers. A malformed publication returns `INVALID_ARGUMENT` and
does not replace the retained tree. Unknown properties and event names remain
extensible. Unknown node kinds are opaque, including their descendants.

Leaves accept no children; layouts and choices accept sequences; forms accept
fields with unique nonempty names; viewports accept at most one canvas. Compose
multiple viewport elements inside a layout. Integer properties use nonnegative
32-bit values in `int_value`; fractions and fraction lists must be finite.
Viewport zoom is bounded to 0.25–8 and slider/progress fractions to 0–1.

| Event family                                     | Control value                                                               |
| ------------------------------------------------ | --------------------------------------------------------------------------- |
| Button press, dialog actions, declared shortcuts | Absent                                                                      |
| Toggle, checkbox, disclosure changes             | Boolean                                                                     |
| List and choice selection, field submission      | String                                                                      |
| Slider changes                                   | Double between 0 and 1                                                      |
| Image wheel                                      | Finite double                                                               |
| Field and textarea edits                         | Map: string `text`, unsigned `revision` and `reset`                         |
| Form submission                                  | Map of strings                                                              |
| Viewport wheel, drag, pinch                      | Map of finite doubles: `zoom`, `offset_x`, `offset_y`, `x`, `y`, `dx`, `dy` |

Edit fields are required. Missing gesture fields default to zoom 1 and zero
coordinates/deltas; supplied fields must have valid types. Additional fields are ignored.
Integral gesture coordinates from older renderers are accepted, but canonical
encoders always emit doubles. Edit revisions are encoded as protobuf JSON integer
strings, including values above the signed 32-bit range. Fixed command arguments
keep their declared encoding; these rules govern the appended control value.

The daemon validates known event payloads against the retained node after
identity, freshness, availability, and binding checks. Preview/harness dispatch
uses the same resolver. The renderer generates event encoders from this vocabulary
and shares them between live and preview transports. Cross-language fixtures test
canonical protobuf JSON against QML encoding and SDK input decoding.

## Generic values and schema evolution

`Value` is a tagged union. An unset kind (`{}` in JSON) means an absent/unit
value, including `Option::None`; zero, false, and empty text are present values
and retain their oneof field. JSON null on a field is treated as unset. Lists
and maps retain their element tags: consumers must not guess types from numeric
appearance. Bytes use base64. Floating values remain distinct from integer
values even when their mathematical value is integral.

`int_value` is signed 64-bit; `uint_value` is unsigned 64-bit. Canonical JSON
encodes both as decimal strings, preserving values above JavaScript's exact
integer range. Keep IDs, revisions, and generic 64-bit integers as strings in
JavaScript; convert only when a narrower domain proves the value is safe.
SDK `u64` writes `uint_value` and reads it or a nonnegative legacy `int_value`.
Signed readers never reinterpret unsigned values. SDK `u8` and `u32`, including
UI integer properties and edit revisions, retain their `int_value` contract.
The `UNSIGNED` command shape accepts the same unsigned/legacy forms; `INTEGER`
continues to require signed encoding. No conversion wraps, saturates, or routes
64-bit integers through doubles.

Observation and preview JSON boundaries use `omega_proto::json::Json` and require
finite floating-point values recursively, including lists, maps, and colors.
Encoding or decoding non-finite values fails explicitly. Protobuf binary messages
can represent them, but transport representability does not establish domain
validity. Generated serde implementations alone do not enforce this finite-only
rule; raw `serde_json` serialization can turn non-finite numbers into null.
Command numeric shapes and known UI properties also enforce their domain bounds.

Keep existing tags and enum numbers stable; reserve removed names and numbers.
Prost skips unknown binary fields and does not retain them on re-encoding. An
unknown oneof alternative therefore becomes unset, which cannot be distinguished
from intentional absence. JSON readers reject unknown fields and unknown enum
names; observation/preview decoding also rejects enum numbers that the generated
serializer cannot represent. Duplicate populated fields and oneof alternatives
are errors. Explicit domain extension points (UI kind/property names and map
keys) are separate from unknown schema fields and retain their documented rules.
Do not enable global unknown-field dropping to make new operations appear valid.

Adding a field is not automatically compatible across binary and JSON consumers.
A change that requires understanding a new oneof, enum, or authority constraint
must negotiate support or raise the minimum protocol version before emitting it.
Protocol **3** adds full-range unsigned values and unsigned command shapes;
the accepted binary range is currently **3–3**, so rebuild daemon and plugins
together. Observation has no version handshake: deploy its schema and clients
together. Its view updates now use generated `InstanceSnapshot` JSON; consumers
must accept enum names and omitted defaults. Renderer attachment features govern
UI support, not generic schema evolution. Preview version **2** adopts the same
unsigned values and finite-only JSON rules; rebuild preview runners with the CLI
and renderer.
