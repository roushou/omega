# Omega schema

These Protobuf schemas define the messages shared by Omega's daemon, plugins,
configuration tools, and renderer. `omega-proto` generates Rust types from them
at build time. Edit the schemas, not generated Rust or JSON implementations.

## Organization

Schema paths below are relative to `schema/omega/`.

| Schema           | Responsibility                                           |
| ---------------- | -------------------------------------------------------- |
| `wire.proto`     | Frames, handshake, and bidirectional requests            |
| `value.proto`    | Generic values for settings and command payloads         |
| `state.proto`    | Topic envelope and replicated patches                    |
| `state/*.proto`  | State payloads grouped by domain                         |
| `action.proto`   | Actions and keybindings                                  |
| `event.proto`    | Events                                                   |
| `plugin.proto`   | Manifests, surfaces, and capabilities                    |
| `instance.proto` | Instance identity, presentations, renderer attachment    |
| `ui.proto`       | Declarative view trees                                   |
| `document.proto` | Desired desktop configuration                            |
| `preview.proto`  | Isolated development sessions, separate from live frames |

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
an exact plugin and command with its canonical signature. `Bind` retains that
owner and signature; a local message has neither. `InvokePlugin` routes through
the same authority checks for SDK calls, retained bindings, and operator actions.
`ListCommands` returns a `CommandCatalogue` outcome on the request stream. Listing
is caller-scoped and grants no access. These contracts require protocol version 2.
