# AGENTS.md

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --check
crates/omega-renderer/shell/lint.sh   # QML static checks
```

All five must pass; CI runs the same five. `protoc` is bundled — nothing to
install. `cargo test -- --ignored` additionally runs `omega-cli/tests/e2e.rs`,
which compiles a scaffolded config through the real binaries.

## Architectural rules

Apply [the architectural principles](docs/principles.md) to new code, refactors,
and reviews. These rules also apply to AI coding tools working in this repository.

- Build coherent primitives with explicit semantics and minimal dependencies.
  Keep host integration and application policy in the layers that own them.
- Separate pure decisions and state transitions from effect execution. Pass facts
  into decisions explicitly; keep environment lookup and native APIs at boundaries.
- Give each fact and policy one authoritative owner. Define identity, scope,
  lifetime, ordering, cancellation, and failure where concerns compose.
- Dependencies point toward contracts and logic, not concrete executors or hosts.
  Use modules first; add traits, generics, or crates for a concrete boundary.
- Provide concise, opinionated SDK defaults without requiring authors to assemble
  internal layers. Keep underlying capabilities usable through explicit contracts.
- Review complete behavior paths as well as modules. Test decisions in isolation
  and verify their composition at integration boundaries. Report concrete coupling
  or broken invariants before proposing structural changes.
- Distinguish implemented contracts from intended architecture. State inspection
  scope and trace behavior before proposing abstractions.

## Style

- No free functions: every operation hangs off a struct, enum, or trait.
- Fail loud over silent drop. No `filter_map` over a closed enum.
- Parse identifiers into newtypes at boundaries (`UnitName`, `SurfaceId`, `ModuleId`).
  Maps use domain-specific key types.
- Paths come from `Layout`; writes go through `AtomicFile`, which fsyncs the
  file and its directory.
- Public API docs address plugin authors: state behavior first, then relevant defaults,
  units, errors, lifecycle rules, and a small compilable example. Keep design rationale
  in architecture docs. Internal comments state non-obvious constraints only. Remove
  narratives, historical explanations, rejected alternatives, and code restatements.
  Public API examples in `crates/omega` use doctests.
- Error types live with the operations that raise them. Reserve `error.rs` for a
  crate-wide error type; group operation-specific errors with their owning module.
- Use directories for modules containing separately readable concepts, such as
  dispatch and its policy table. File length alone does not determine module boundaries.

## Protocol

- `crates/omega-proto/schema/` is the source of truth; `build.rs` generates
  from it. Never hand-edit generated files.
- Stream ids split by parity — daemon even, peer odd. Both ends allocate on
  one connection.
- Every `Invoke` is answered on its own `stream_id`: an outcome or a
  `Refusal` with a closed `ErrorCode`. Never a silent drop or bare EOF.
- Which `ErrorCode` a domain error becomes is declared once per error type in
  `refusal.rs`. A unit's own refusal passes through unflattened.
- Topic and event names are validated where they enter (`Address::parse`,
  `EventKind::from_str_name`).
- The observation socket uses the same Frame/Invoke schema and policy as the control
  socket, encoded as JSON. State reading is open; private views require a scoped
  renderer attachment. Bootstrap and unit supervision operations require the operator.
  Presentation changes also accept scoped renderers; plugins may only hide or close
  their own instances.

## Trust

- Authenticate units through SO_PEERCRED pid and spawn token. Resolve identity and
  grants from supervision records and the daemon manifest, never peer claims. The
  daemon uid identifies the operator; each operation declares which roles it accepts.
  Observation renderers use scoped attachments.
- Every operation declares its authorization requirement in
  `session/dispatch/policy.rs`. Missing policy rows are refused.
- The manifest is the ceiling; `Subscribe` narrows it and never widens. A unit
  writes `unit.<its own name>.<key>` and nothing else.
- Declare action capabilities once in `action::ActionKind`. Check authorization
  before handler availability.
- A unit serves only registered commands with its own capabilities. Cross-unit
  invocation requires CAPABILITY_SPAWN for units; the operator is authorized by uid.

## Daemon

- `UnitTable` owns manifests, lifecycle, tokens, supervision handles, and sessions.
  Apply lifecycle changes through Transition; the units topic projects that state.
- A spawned process is starting until its handshake succeeds, then running.
- Run convergence in one independent task. Merge triggers into the next pass and
  execute one pass at a time; plugin waits must not block the connection accept loop.
- Desired state is a document, never a script. `system/` computes it with no
  side effects; providers `plan` purely, then `apply`. Convergence is per
  entity id.
- A built plugin the document never mentions runs. Membership under `plugins/`
  declares execution; libraries never do. The document supplies overrides.
- Instances and views end with the plugin session. `PresentationProvider` projects
  configured placements into `UnitTable` instances; transient presentations use the
  same registry. Runtime identity is `(InstanceId, IncarnationId)`, not a placement.
  Expiration emits tombstones so renderers cannot keep drawing stale instances.
- Renderer attachments narrow an owner connection to one plugin's standalone views
  or one embedded/popup placement. Replacing a scope revokes its old attachment.
  Interactions resolve retained bindings by instance, revision, node key and event.
- Requested and observed presentation state are distinct. Native close records
  closed intent; reconciliation and host restart must not reopen a dismissed view.
- Derive events from state transitions in one place. Events are not persisted.
- `Schedules` owns timers across reconciliation passes. Unchanged schedules retain
  their timers. The first tick is immediate.
- Cadence syntax is `every <n><s|m|h|d>`. Reject unsupported cron expressions.
- Long-lived tasks select on `Shutdown`. Units are asked to exit (SIGTERM,
  then a deadline), never only killed.
- `Changes` coalesces Create/Modify/Remove notifications into settled wakeups,
  excluding reads. Watch parent directories to detect watched directories replaced
  by rename.
- Use tokio::time::Instant for deadlines and paused Tokio time for pacing tests.
- Pass both socket paths into `Daemon::builder`; do not resolve them from process
  environment inside the daemon. Session tests use UnixStream::pair.
- Write unit logs to `~/.cache/omega/logs/<unit>.log` so generation replacement
  preserves crash diagnostics.

- A field is a **reading** (one topic, no interpretation), a **composite**
  (several topics, interpretation once), a **record** (this plugin’s own memory),
  or an **effect**. `platform/<domain>/` owns service handles, accessors,
  composites, and controls. Private `wiring` supplies common construction and
  capability mechanics; it does not centrally define domain handles.
- Topics determine render invalidation. Facts with independent update rates use
  separate topics; composites combine them for consumers.
- The SDK root exposes surface/command/reaction contracts, derives, `View`, and
  shared reading values. `platform` groups external service domains; `plugin`
  owns Omega registration and supervision. `surface` owns UI declarations and
  references; `command` owns endpoints, input, and command references; `reaction`
  owns event behavior. `ui`, `config`, `record`, `effect`, and `testing` expose
  their respective APIs. Runtime context/mirror and wiring stay private.
- Derive expansions reference `::omega::internal::`, independent of public re-exports.
- Compile `omega new` templates through `crates/omega/tests/scaffold.rs`.

## Units and settings

- Settings construct plugin fields and arrive in Welcome. Unit setting changes
  restart the plugin. Build activation installs settings before unit convergence;
  compare settings across builds, including units whose settings were removed.
- Settings layer, they do not replace: `Units::configured` sets a unit's,
  `Modules::widget` sets one placement's, and an instance is the second over
  the first (`Values::over`). A command or reaction is never placed, so its
  unit's settings are the only ones it can have.
- A placement is desired configuration; an instance owns runtime identity, settings,
  and presentation state. There are no automatic unplaced widget instances.
- The daemon creates an instance and pulls its first render with construction
  settings; the unit pushes later trees with that instance's identity.

## SDK

- Derive manifests from declared fields and registrations. `omega build` and
  `omega check` compile plugins and query their binaries with --omega-manifest.
- Plugins expose a library target so system configuration can use typed settings,
  commands, and surface references.
- Share records through Own<T> and Watch<T>. UnitState derives the record address
  from the defining crate and type. Own writes require Does; Watch requires Reads.
- Render declarations hold only readings. Commands, reactions, and stateful
  behavior may hold effects. The `Reads` bound enforces the render-side rule;
  dependency/model invalidation controls rendering, and identical trees are dropped.
- Derive Config to implement Fields for typed map boundaries. Missing fields use
  Default for forward compatibility with older writers.
- Prefer short typed APIs and measurement values such as Percent over raw scalars.
- `omega dev <unit>` uses operator-only AdoptUnit to replace the supervised process
  for the connection lifetime. Hold supervision during adoption and retain manifest
  grants. `omega::testing` provides isolated fixtures without a daemon.

## Renderer

- Embed renderer sources in omega-renderer. `omega shell install` writes the
  installing binary's sources; reinstall after updating the binary. Installation
  restarts Omarchy and verifies live attachment fingerprints. Installed files alone
  do not prove which QML the running shell uses. Linked and legacy renderers are
  unverified; never infer their build identity from files on disk.
- `ViewNode.qml` puts one item on screen per node and recurses. An unknown
  node kind draws nothing, so a tree from a newer plugin degrades to the parts
  this shell understands.
- NodeKind in omega-proto defines renderer properties. Generate checked-in Props.js
  through omega_renderer::Props; OMEGA_REGENERATE=1 updates it. Accessors are scoped
  by node kind and property. Preserve the distinction between protobuf JSON int64
  strings and fractional numbers.
- Tests verify SDK property vocabulary, generated accessor use, and explicit
  NOT_DRAWN exceptions. Keep wire-shape assertions and vocabulary coverage based
  on the shared every_node fixture.
- Test renderer manifest version against the crate version and the embedded file
  list against disk. Install by replacing a staged directory to remove obsolete files.

## Crates

```
omega-proto    wire format, manifest, identifiers — what crosses a socket
omega-keyboard logical keyboard events, chords, and conflict-checked keymaps
omega-host     files, generations, Cargo workspace documents, discovery, watching
omega          the SDK — what a unit is written against
omega-document the desired-state document
omega-derive   proc-macros
omega-platform  the subsystems the daemon brokers: state out, actions in
omega-daemon   runtime authority, supervision, routing, and convergence
omega-renderer host-independent QML controls and generated readers
omega-omarchy  Omarchy authoring, compilation, transport, and installation
omega-preview  development cases and isolated surface sessions
omega-cli      the binary
```

- Name crates by responsibility. omega is the authoring SDK; omega-document owns
  desired configuration.
- Keep host dependencies out of plugin builds. omega may depend on shared protocol
  and derive crates and omega-keyboard, but not host infrastructure. Put wire
  contracts in omega-proto and filesystem/build machinery in omega-host.
- Keep protobuf JSON implementations behind omega-proto's json feature. Host clients
  enable it; plugin authors must not need it.
- Hash Manifest::canonical protobuf bytes: repeated fields sorted and deduplicated,
  then encoded in tag order.

## CLI

- `cli/` owns arguments and command entry points. `build/` owns compilation,
  validation, generation planning/publication, and activation waits. Build helpers
  must not depend on command parser types or resolve their own workspace environment.
- `omega-host::workspace` owns Cargo schemas, member patterns, roles, and plugin
  discovery. `omega-host::fs` owns settled watching under its optional `watch`
  feature. The daemon owns runtime generation selection, not source-workspace tools.

- Route CLI output through ui::Ui: decoration to stderr, machine-readable answers
  to stdout. Use Step verbs, twelve-column alignment, home-relative paths, anstream
  color handling, and one accent per line. Ui::detail continues an item; separate
  items get separate verbs.
- `omega init` initializes the config workspace. `omega new <name>` scaffolds a
  plugin; --lib scaffolds a reusable library.
- `omega new` wires Cargo dependencies and prints a placement hint without editing
  layout. Scaffold::placement_hint uses fully qualified exported symbols.
- `omega daemon` runs the daemon; `omega daemon install` installs its service.
- Generate the systemd unit with current_exe. Daemon status compares that path with
  installed ExecStart. TimeoutStopSec must exceed the five-second plugin stop grace.
- Pass a unit-file path into Service. ServiceManager alone resolves its location.
- Let the caller select build profile; do not hardcode --release.

## Config workspace

- Config manifests use registry dependencies. Store local checkout patches in
  gitignored .cargo/config.toml through omega link; --published removes them. Match
  version requirements to the checkout so Cargo can select the patch.
- Run Cargo with the config as its working directory so it discovers local patches.
- Every TOML document is a `TomlSchema` declared once with its kind and
  location. Address one with `Layout::file::<S>(key)`; never join a path at a
  call site. Invariants beyond parsing go in `Validated`.
- Generated dependencies come from `Scaffold::UNIT_DEPENDENCIES`,
  `SYSTEM_DEPENDENCIES`, and optional `PREVIEW_DEPENDENCIES`. Keep production
  plugin, configuration-plane, and development dependencies separate.

## Docs

- `docs/authoring.md` — plugin and component authoring
- `docs/previews.md` — preview setup and visual testing
- `docs/architecture.md` — runtime contracts and maintainer reference
- `docs/design.md` — open design questions and feature boundaries
- `crates/omega-proto/schema/README.md` — schema rules
- `crates/omega-renderer/shell/README.md` — the renderer and the shell socket

## Desktop workspace foundations

- Config members are `system/`, `plugins/<name>/`, or `libraries/<name>/`.
  Only plugins are queried for manifests and supervised. Libraries never imply
  execution. Runtime generation paths remain `units/`.
- `omega migrate` journals source edits before moving `units/` to `plugins/`.
  Recovery preserves external edits and refuses ambiguous mixed layouts. Source
  mutations and builds hold the same workspace lock.
- `Document::with` composes a `DocumentExtension`. Core document validation never
  interprets Omarchy payloads: `omega-omarchy` handles and projects them first.
- QML controls live in `omega-renderer/shell/core`, without `qs.*` imports.
  Omarchy installs its adapter and the embedded core together. Theme, assets, and
  session propagate as bindings to preserve dynamic host updates. The fixture
  harness has no live connection or process-spawning capability.

## Stateful surfaces

- Every UI declaration implements Surface with Model, Message, and Effects associated
  types. Use () for unused model/effects and Infallible for no local messages; update
  remains required and handles Infallible with an empty match. Register all surfaces
  with .surface. Derive(Surface) requires Reads; effects reach behavior, never render.
- SurfaceHarness and previews use the production instance runtime for all surfaces.
  Drawn::of is fallible and must respect initialization and required-reading gates.
- Runtime instances serialize messages and own the current render's local binding
  registry. IDs are unique across the process; captures never cross the protocol.
  Stale or foreign-instance bindings are refused, never decoded with new captures.
- Production, previews, and harness interaction APIs must agree on ambiguous node
  keys, inherited disabled/busy state, and binding validity.
- Cache views between dependency/model invalidations. Publication acknowledgments
  must not rebuild local bindings and trigger an endless publication loop.
- Task keys replace generations immediately. Close cancels delivery; hide retains
  work. A started blocking worker keeps its admission permit until actual exit.
- Controlled edits and resets carry independent revisions. Newer local text must
  survive older views, and incomplete IME composition must not emit committed edits.
- `SurfaceHarness` uses the production scheduler with isolated effects. Fixture
  completions are explicit; never fall through to a real backend.

## Applications

- GIO objects stay on the applications worker's GLib main-context thread. The SDK
  depends on no native application service libraries.
- The catalogue is bounded shared state; queries and selection are instance-local.
  Activation accepts typed desktop IDs and literal URIs, never shell fragments.
- Activation success is admission. Unknown outcomes must not be replayed.
- A surface may close/hide its own current instance. Lifecycle requests must leave
  the connection reader free to receive the plugin's acknowledgement.

## Previews

- `omega-preview` is development tooling: register cases in explicit library tests,
  never production manifests. Preview sessions use `SurfaceHarness` and captured
  effects, with no fallback to live services.
- Preview epochs reset both Rust model state and renderer-local drafts. View
  revisions still gate interactions within an epoch.
- Captures compare decoded pixels only under matching recorded raster environments.
  Baseline updates are explicit. Software capture uses fixed local fixture images,
  not native theme-icon providers.
