# AGENTS.md

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --check
crates/omega-renderer/shell/lint.sh   # the QML, which no compiler sees
```

All five must pass; CI runs the same five. `protoc` is bundled — nothing to
install. `cargo test -- --ignored` additionally runs `omega-cli/tests/e2e.rs`,
which compiles a scaffolded config through the real binaries.

## Style

- No free functions: every operation hangs off a struct, enum, or trait.
- Fail loud over silent drop. No `filter_map` over a closed enum.
- Identifiers are newtypes parsed at the edge (`UnitName`, `SurfaceId`,
  `ModuleId`). A map keyed by `String` has not decided what it holds.
- Paths come from `Layout`; writes go through `AtomicFile`, which fsyncs the
  file and its directory.
- Comments state the constraint, not the story. Public API in `crates/omega`
  gets rustdoc with doctests; internal code gets a line saying why, where why
  is not obvious. Delete anything that restates the code.
- An error type lives with the operation that raises it: `CodecError` in
  `codec.rs`, `ShellError` in `shell.rs`. A file named `error.rs` earns the
  name only for a crate's own top-level error, and a `<thing>_error.rs` is a
  module that should have been a directory.
- A module is a directory when it holds concepts that are read separately —
  the policy table apart from the dispatcher, a schema's payloads apart from
  its envelope. Length alone is not a reason: one type of four hundred lines
  is one file.

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
- The observation socket carries the same `Frame`/`Invoke`/`POLICY` as the
  control socket, JSON-encoded because a shell cannot encode protobuf. A
  second encoding, never a second taxonomy. Reading is open; asking is the
  operator's.

## Trust

- Three peer kinds: **unit** (spawn token, granted what its manifest
  declares), **operator** (the daemon's own uid, lifecycle only), everyone
  else (refused). Identity is (SO_PEERCRED pid, spawn token), resolved by the
  supervisor. Grants come from the daemon's copy of the manifest, never a
  frame.
- Every op declares its requirement in the `POLICY` table in
  `session/dispatch/policy.rs` — its own file because it is the contract, and
  reading what a peer may do should not mean scrolling past the code that
  does it. An op with no row is refused, so authorization cannot be forgotten
  at a call site.
- The manifest is the ceiling; `Subscribe` narrows it and never widens. A unit
  writes `unit.<its own name>.<key>` and nothing else.
- Action capabilities are declared per kind in `action::ActionKind` and
  checked before the daemon considers whether it _can_ act — a missing
  handler must not read as a grant.
- A unit serves only the commands its manifest declares. A command runs with
  the unit's own capabilities, so invoking one is not a way to borrow powers:
  a _unit_ needs `CAPABILITY_SPAWN` to invoke another; the operator needs only
  to be the owner.

## Daemon

- One `UnitTable` knows the units: manifest, lifecycle, token, supervision
  handles, session. The `units` topic is a projection of it. Phase changes by
  applying a `Transition`, never by composing a status elsewhere.
- A spawned process is `starting`. It is `running` when the handshake
  completed and the daemon vouched for it.
- Convergence runs in one task, one pass at a time. Triggers merge into the
  next pass, never call into it — a bar instance can wait seconds on a wedged
  unit and nothing else may wait with it.
- Desired state is a document, never a script. `system/` computes it with no
  side effects; providers `plan` purely, then `apply`. Convergence is per
  entity id.
- A built unit the document never mentions runs. Putting a crate in the
  workspace is the declaration; the document exists to override it.
- Views end with the session (`Hub::forget_unit` from
  `UnitTable::disconnected`). `BarProvider::rendered` reads "the unit knows
  about this instance" from the hub, and that knowledge lived in the unit's
  process — a view outliving it means a restarted unit is never told about its
  instances again. Observers get an empty tree, because a shell that is not
  told cannot stop drawing.
- Events are transitions of state, derived in one place: "AC unplugged" and
  `battery.charging == false` are one fact and may not disagree. Events are
  not stored.
- Long-lived tasks select on `Shutdown`. Units are asked to exit (SIGTERM,
  then a deadline), never only killed.
- `Changes` reports _settled_ filesystem events, Create/Modify/Remove only —
  reading a file is an event, and a rebuild reads the whole config, so
  counting reads makes a watcher rebuild because it built. A directory
  replaced by rename destroys a watch on itself: watch the parent too.
- Deadlines use `tokio::time::Instant`, never the system clock, so
  `#[tokio::test(start_paused = true)]` can assert pacing. A test that sleeps
  to observe a timer tests the machine's load.
- `Daemon::builder` is _given_ its two socket paths. Resolving from the
  environment confines a daemon to one per process and makes the run loop
  untestable. Test sessions use `UnixStream::pair` — no listener, no socket
  file, and `SO_PEERCRED` reports this process on both ends.
- A unit's output goes to `~/.cache/omega/logs/<unit>.log` — the cache,
  because a build replaces the state dir and the log explaining the last
  crash has to outlive it.

## Units and settings

- Settings are _construction_, not state: a plugin's fields are built from
  them, so they arrive once in the `Welcome` and changing them re-runs the
  unit. `ConfigProvider` runs _before_ `UnitProvider` — a unit must not spawn
  before the settings its handshake carries are on file. It compares every
  built unit, not only configured ones, because removing settings changes a
  unit as much as editing them.
- Settings layer, they do not replace: `Units::configured` sets a unit's,
  `Modules::widget` sets one placement's, and an instance is the second over
  the first (`Values::over`). A command or reaction is never placed, so its
  unit's settings are the only ones it can have.
- A view is addressed by (unit, surface, module). `module` is an address, not
  a filter: empty means the surface's single instance, so a surface also
  placed in a bar has _two_ views in the hub. Matching modules loosely latches
  onto whichever spoke last.
- The daemon pulls an instance's first render (`RenderWidget`, to hand the
  unit its configuration); the unit pushes every one after.

## SDK

- A plugin declares what it needs by holding it. A `Battery` field is the
  topic and the capability; `#[derive(Widget)]` sums the fields and that sum
  _is_ the manifest — `omega build` asks the binary (`--omega-manifest`). No
  `manifest.toml`, no `include_str!`, no `SURFACE` const. The cost is that
  `omega check` compiles.
- A plugin is a library as well as a program, so `system/` depends on the
  plugins it configures and a misspelled setting is a build error in the
  config rather than a default silently taken.
- Plugins compose through state, not calls. `Own<T>` writes `unit.<name>.
  <key>`, `Watch<T>` reads anyone's, and `#[derive(UnitState)]` takes the unit
  from the defining crate and the key from the type — `Watch<lamp::Power>`,
  not a string a rename breaks. Writing is a `Does`; reading is not.
- A widget may hold only state; commands and reactions may hold effects.
  Rendering runs on every reported change and identical trees are dropped, so
  an effect there fires on every percent. The `Reads` bound says so at compile
  time.
- A struct crossing a boundary as a map derives `Fields` both directions.
  Reading is total — a missing field takes `Default`, so adding one never
  breaks a writer that predates it.
- The typed path must be shorter than the untyped one or the SDK is
  decoration. A reading is a `Percent`, not an `f64` needing `* 100.0`.
- `omega dev <unit>` asks for the unit's identity (`AdoptUnit`,
  operator-only): the supervised process stops, this one takes its place with
  a real token and the manifest's real grants, and the adoption lasts as long
  as the connection. An adopted unit is _held_ so the reconciler does not
  start the built binary underneath it, and it reports its own phase.
  `omega::testing` is the same idea without a daemon.

## Renderer

- The renderer travels _inside_ the binary (`include_str!` in
  `omega-renderer`); `omega shell install` writes what the binary
  carries, so a stale copy is unreachable rather than discouraged.
- `ViewNode.qml` puts one item on screen per node and recurses. An unknown
  node kind draws nothing, so a tree from a newer plugin degrades to the parts
  this shell understands.
- The prop vocabulary is `omega-proto`'s `NodeKind` table — the third of the
  shape `SystemTopic` and `ActionKind` have, and in that crate rather than the
  SDK because the renderer reads it and the daemon depends on the renderer.
  `Props.js` is _generated_ from it (`omega_renderer::Props`, checked in, with
  a test that regenerates and compares — `OMEGA_REGENERATE=1` writes it), so a
  shell calls `Props.textText(node)` and never spells a prop name itself. One
  reader per (kind, prop) because one name means two things: `value` is a
  fraction on a slider and a string on a field. Protobuf JSON writes int64 as
  a _string_, which is why `Number` and `Fraction` are separate kinds — a
  reader that forgets lays out a NaN.
- Three tests hold the vocabulary and its readers together: the SDK emits only
  props it declares, the shell calls only accessors it generates, and a prop
  nothing draws is a line in `NOT_DRAWN` with a reason. The wire shapes are
  still pinned separately (`every_node_kind_carries_the_props_the_renderer_reads`);
  both read one tree, `every_node()`, so neither is more exhaustive than the other.
- The manifest's `version` is pinned to the crate's by a test rather than
  patched at install time. A second test asserts the carried file list _is_
  the directory on disk. Installing swaps a staged directory in, which is what
  removes a file an older version shipped.

## Crates

```
omega-proto    wire format, manifest, identifiers, layout, TOML, staging
omega          the SDK — what a unit is written against
omega-document the desired-state document
omega-derive   proc-macros
omega-brokers  the subsystems the daemon brokers: state out, actions in
omega-daemon   the daemon, plus host/ (watching, globs, discovery)
omega-renderer the QML it ships, and how that installs
omega-cli      the binary
```

- Named for what they are: `omega` not "sdk" (an audience, not a thing),
  `omega-document` not "config" (which already means the config dir and
  `units.toml`).
- A unit author's build is a design constraint. `omega` depends on
  `omega-proto` and `omega-derive` and nothing else; host machinery lives in
  `omega-daemon::host`, which the CLI reaches through the daemon it already
  depends on. Before putting something shared in `omega-proto`, ask what it
  costs a unit.

## CLI

- Every line goes through `ui::Ui`: decoration to stderr, answers to stdout
  (so `omega run … | jq` gets a value), verbs from the closed `Step` enum on
  cargo's twelve-column rail, paths shortened against `$HOME`, one accent per
  line, colour left to `anstream`. A `println!` outside `ui/` is a second
  format nobody chose. Every line puts something in the rail. `Ui::detail`
  continues a sentence; a list of separate things gets a verb per line.
- `omega init` sets up a _machine_ and takes no argument; `omega new <name>`
  writes a _plugin_. `new` rather than `add` because `omarchy plugin add
  <git-url>` already means install somebody else's.
- `omega new` adds the crate to `system/Cargo.toml` but **prints** the
  placement line rather than writing it: a path between two crates is
  bookkeeping, which bar a widget belongs in is a decision. The hint lives in
  `Scaffold::placement_hint` and is fully qualified so it compiles wherever
  pasted.
- One noun for the background process: `omega daemon` runs it, `omega daemon
  install` makes it a service. The bare form still runs.
- The systemd unit is _generated_, because it names the binary's own path —
  which is the failure worth diagnosing: two omegas on one machine, the
  service running the other. `omega daemon status` compares the installed
  `ExecStart` against `current_exe`. `TimeoutStopSec` must stay above the
  daemon's five-second grace per unit.
- `Service` takes the unit file's _path_; only `ServiceManager` knows where
  that is. A `Service` that asked a manager for its own path could only be
  tested through a process-wide variable, which parallel tests clobber.
- The profile belongs to whoever waits for the build. Nothing hardcodes
  `--release`.

## Config workspace

- A config's manifest names the published crates, always: it is a git
  repository that must build on every machine it is cloned onto. Building
  against a checkout is a local override — cargo's `[patch]` in
  `.cargo/config.toml`, which the scaffold gitignores. `omega link [path]`
  writes it, `omega link --published` removes it. Linking also sets the
  version requirement to the checkout's, because a patch cargo cannot match
  is a patch cargo ignores.
- Cargo runs _in_ the config, not merely on it: `.cargo/config.toml` is found
  by walking up from the current directory, not from `--manifest-path`.
- Every TOML document is a `TomlSchema` declared once with its kind and
  location. Address one with `Layout::file::<S>(key)`; never join a path at a
  call site. Invariants beyond parsing go in `Validated`.
- Generated dependencies come from `Scaffold::UNIT_DEPENDENCIES` or
  `SYSTEM_DEPENDENCIES`, never a second list. Two lists because two
  audiences: a unit speaks the protocol, the config plane computes a document
  and exits.

## Docs

- `docs/architecture.md` — design and rationale
- `docs/design.md` — the abstractions being built next, and the roads not taken
- `crates/omega-proto/schema/README.md` — schema rules
- `crates/omega-renderer/shell/README.md` — the renderer and the shell socket
