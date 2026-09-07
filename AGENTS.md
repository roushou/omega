# AGENTS.md

Guidance for agents and contributors.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --check
```

All four must pass before a change is complete; CI runs the same four on every
push. `omega-proto` generates its types from its own `schema/`, with a
bundled `protoc` — nothing has to be installed for it.

`cargo test -- --ignored` additionally runs the end-to-end cycle through the
real binaries (`omega-cli/tests/e2e.rs`), which compiles a scaffolded config,
runs that config's own tests, and is too slow for every run.

## Rules

- `crates/omega-proto/schema/` is the single source of truth; that crate's
  `build.rs` generates from it. Never hand-edit generated files.
- Fail loud over silent drop (no `filter_map` over a closed enum).
- Paths come from `Layout`; writes are atomic.
- No free functions: every operation hangs off a struct, enum, or trait.
- The control socket is a trust boundary with three kinds of peer: a **unit**
  (holding the spawn token, granted what its manifest declares), the
  **operator** (the daemon's own uid — lifecycle only, no capabilities), and
  everyone else (refused). Identity is (SO_PEERCRED pid, spawn token) resolved
  by the supervisor; grants are read from the daemon's copy of the manifest,
  never from a frame. Each op declares the role it is served to. Every op declares what it requires in the
  `POLICY` table in `session/dispatch.rs` — an op with no row is refused, so
  authorization cannot be forgotten at a call site.
- Refuse out loud: a peer that is turned away gets a `Refusal` frame with a
  closed `ErrorCode`, never a silent drop or a bare EOF. Every `Invoke` is
  answered with a `Result` on its own `stream_id` — an outcome or a refusal.
- The manifest is the ceiling, `Subscribe` is the selection: a unit is woken
  for the topics it declared and may narrow that at runtime, never widen it.
  A unit writes `unit.<its own name>.<key>` and nothing else.
- Identifiers are newtypes with one shared rule (`UnitName`, `SurfaceId`,
  `ModuleId`), parsed at the edge that receives them. A map keyed by `String`
  is a map that has not decided what it holds.
- Which `ErrorCode` a domain error becomes is declared once per error type in
  `refusal.rs`, not chosen at each call site. A unit's own refusal passes
  through unflattened: the daemon was the messenger.
- Stream ids are split by parity in `omega-proto` — the daemon's even, a
  peer's odd — because both ends allocate on one connection and an answer has
  to be unambiguous.
- Topic and event names are validated where they enter — `Topic::parse`,
  `EventKind::from_str_name` — so a typo fails the build, not silently
  subscribes to nothing.
- Watch, do not poll: `Changes` reports settled filesystem events. Reading a
  file is an event too, so only Create/Modify/Remove count — a rebuild reads
  the whole config, and treating that as a change makes a watcher rebuild
  because it built. A directory replaced by a rename destroys a watch on
  itself, so watch its parent as well.
- Long-lived tasks select on `Shutdown`: units are asked to exit (SIGTERM,
  then a deadline), never only killed. Writes are durable, not just atomic —
  `AtomicFile` fsyncs the file and its directory.
- A view is what a unit is _currently_ saying, so its views end with its
  session (`Hub::forget_unit`, from `UnitTable::disconnected`). Keeping them
  froze the bar: `BarProvider::rendered` asks the hub which instances have a
  view and treats that as "the unit knows about this instance" — but that
  knowledge lived in the unit's process. A view outliving the process meant a
  restarted unit was never told about its instances again, and the shell drew
  its last frame forever. Observers are told with an empty tree, because a
  shell that is not told cannot know to stop drawing.
- `module` is an address, not a filter. Empty means the surface's single
  instance — what a plugin publishes to until a document instantiates it —
  and a surface the document also placed in a bar therefore has _two_ views
  in the hub. A reader that matches modules loosely latches onto whichever
  spoke last.
- The shell draws a tree, it does not flatten one: `ViewNode.qml` puts one
  item on screen per node and recurses, and a node kind it does not know draws
  nothing — so a tree from a newer plugin degrades to the parts this shell
  understands rather than failing whole. What the two halves agree on is
  pinned in Rust (`every_node_kind_carries_the_props_the_renderer_reads`),
  because QML cannot be compiled against the SDK's types and a prop renamed on
  one side is a widget that silently draws nothing. Protobuf JSON writes a
  64-bit integer as a _string_: `Props.js` parses, and a reader that forgets
  lays out a NaN.
- The observation socket takes requests as well as giving answers, and they
  are the protocol's own: a JSON line is a `Frame` carrying an `Invoke`,
  dispatched through the same `Dispatcher` and the same `POLICY` table a
  framed one is. A shell cannot encode protobuf, and that is a reason for a
  second _encoding_, never a second taxonomy of actions spelled as strings.
  Reading it is open; asking anything of it is the operator's, checked by the
  same uid rule the control socket uses.
- Introspection goes through the state plane, not a side channel: the
  supervisor publishes the `units` topic, and `omega status` reads it from the
  read-only observation socket. A unit's own output goes to
  `~/.cache/omega/logs/<unit>.log` — in the cache, because a build replaces
  the state dir and the log explaining the last crash has to outlive it.
- One table knows the units: manifest, lifecycle, token, supervision handles,
  session. Nothing keeps a parallel record, and the `units` topic is a
  projection of it. A unit's phase changes by applying a `Transition`, never
  by composing a status somewhere and hoping it matches.
- A spawned process is `starting`, not `running`: a unit is running when it
  has completed the handshake and the daemon has vouched for it.
- Convergence runs in one task, one pass at a time: triggers are requests
  that merge into the next pass, never calls. A bar instance can wait seconds
  on a wedged unit, and nothing else in the daemon may wait with it.
- Desired state is a document, never a script. `system/` computes it with no
  side effects; providers `plan` purely and only then `apply`, so a change can
  be shown before it happens. Convergence is per entity id, which is what
  keeps the blast radius of an edit to the thing it names.
- A built unit the document never mentions runs: putting a crate in the
  workspace is already a declaration. The document exists to override it.
- A unit's settings are _construction_, not state: a plugin's fields are built
  out of them, so they arrive once, in the `Welcome`, and changing them means
  running the unit again. `ConfigProvider` records what the document says and
  cycles whatever that changed, and it runs _before_ `UnitProvider` because a
  unit must not be spawned before the settings its own handshake will carry
  are on file. A unit whose settings were _removed_ has changed as much as one
  whose settings were edited — it has to go back to its defaults — so the plan
  compares every built unit, not only the configured ones.
- Settings have two scopes and they layer, they do not replace: `Units::
  configured` gives a unit its settings, `Modules::widget` gives one placement
  its own, and a widget instance is built from the second laid over the first
  (`Values::over`). Naming one key where a widget is placed must not silently
  reset the rest to their defaults. A command or a reaction is never placed
  anywhere, so its unit's settings are the only ones it can ever have — which
  is why delivering them is what makes `#[omega(config)]` mean anything
  outside a bar.
- Events are transitions of state, derived in one place, not announced by
  each source: "the AC was unplugged" and `battery.charging == false` are the
  same fact and cannot be allowed to disagree. Events are not stored — a unit
  that was not listening missed it.
- An action's capability is declared per action kind in `action::ActionKind`,
  and checked before the daemon considers whether it can perform it: a missing
  handler must never read as a grant.
- A unit serves the commands its manifest declares and no others; the daemon
  checks that before asking, so a typo is answered with what does exist. A
  command runs with the unit's own capabilities — invoking one is not a way to
  borrow powers the caller was never granted, which is why a _unit_ needs
  `CAPABILITY_SPAWN` to invoke another and the operator needs only to be the
  owner.
- A view is addressed by (unit, surface, module): the document can
  instantiate one surface more than once, so an instance is what gets
  rendered and published. The daemon pulls the first render of an instance
  (`RenderWidget`, to hand the unit its configuration); the unit pushes every
  one after it.
- A plugin declares what it needs by holding it. A `Battery` field is the
  topic and the capability to read it; a `Notify` field is the capability to
  raise one. `#[derive(Widget)]` adds the fields up, and that sum _is_ the
  manifest — `omega build` compiles a plugin and asks it (`--omega-manifest`,
  the same way it asks the config plane what the machine should be). There is
  no `manifest.toml`, no `include_str!`, no `SURFACE` const, and no second
  place to get any of it wrong. The cost is that `omega check` compiles.
- A plugin is a library as well as a program. `system/` depends on the plugins
  it configures — they are crates in one workspace — so a plugin's settings
  are its own type and a misspelled one is a build error in the config rather
  than a default silently taken on the machine. `src/main.rs` is the library
  and a call.
- A struct that crosses a boundary as a map of values derives both directions
  (`Fields`, in `omega-proto` beside the `Value` it is made of): the config
  plane writes settings and the plugin reads them; a plugin writes its state
  and another reads it. A boundary where each side spells the field names for
  itself is a boundary where they can disagree. Reading is total — a field the
  map does not carry takes the type's `Default`, so adding one never breaks a
  writer that predates it.
- Plugins compose through state, not calls. Every plugin owns the keyspace
  `unit.<name>.<key>`; `Own<T>` writes it and `Watch<T>` reads anyone's, and
  the address comes from the type — `#[derive(Topic)]` takes the unit from the
  crate that defines the type and the key from the type's name, so
  `Watch<lamp::Power>` is `unit.lamp.power` with no string a rename can leave
  behind. Writing is an effect (`Own` is a `Does`); reading is not.
- A widget renders and may hold only state; a command and a reaction may hold
  effects. Rendering happens on every change the machine reports and identical
  trees are dropped, so an effect there would fire on every percent the
  battery moves. The `Reads` bound says so at compile time, with the message
  that explains where the field belongs instead.
- The typed path has to be shorter than the untyped one, or the SDK is
  decoration. A reading is a `Percent`, not an `f64` needing `* 100.0`; a view
  is `Text::new(charge)`, not a node wrapped in a tree; keys are the path to a
  node and only written for a list whose items move. One value of a plugin is
  built per instance, so its settings are fields and there is no "which
  instance am I" to ask.
- A config's manifest names the published crates, always. It is a git
  repository that has to build on every machine it is cloned onto, and an
  absolute path into somebody's home directory does not travel. Building
  against a checkout is therefore not a second manifest but a local override —
  cargo's `[patch]`, in `.cargo/config.toml`, which the scaffold tells git to
  ignore. `omega link [path]` writes it (from `$OMEGA_SOURCE`, else the tree a
  development build was compiled from) and `omega link --published` removes
  it; `omega init` links a first config when it can see a checkout, which is
  every machine until the crates are published. Linking also sets the
  workspace's version requirement to the checkout's, because a patch cargo
  cannot match is a patch cargo silently ignores.
- Cargo is run _in_ the config, not merely on it: `.cargo/config.toml` is
  discovered by walking up from the current directory, not from
  `--manifest-path`, so a build invoked from anywhere else silently loses the
  patch.
- Every TOML document is a `TomlSchema`, declared once with its kind and
  location. Address one with `Layout::file::<S>(key)`; never join a path or
  name a file at a call site. Invariants beyond parsing go in `Validated`,
  so a rule has one implementation and every reader gets it.
- Declare data once and derive the rest: a generated crate's dependencies
  come from `Scaffold::UNIT_DEPENDENCIES` or `SYSTEM_DEPENDENCIES`, never a
  second list — and there are two because they are two audiences. A unit is
  written against the SDK and speaks the protocol; the config plane computes
  a document and exits. One list for both made every unit declare an
  authoring API it never calls.
- Crates are named for what they are, in the vocabulary the system already
  uses: `omega` is what a unit is written against (not "sdk", which
  names an audience rather than a thing), and `omega-document` holds the
  desired-state document (not "config", which in this repo already means the
  config dir and `units.toml`). `omega-proto` is what both halves speak: the
  wire format, the manifest, the identifiers, and where a file goes. It
  depends on no other omega crate.
- A unit author's build is a design constraint. `omega` depends on
  `omega-proto` and `omega-derive` and nothing else, and the machinery only a
  host runs — filesystem watching, directory staging, glob expansion,
  workspace discovery — lives in `omega-daemon`'s `host` module rather than
  under the protocol, so a plugin that draws a battery compiles no inotify
  watcher. The CLI reaches that machinery through the daemon it already
  depends on. When something shared is tempting to put in `omega-proto`, ask
  what it costs a unit first.
- A unit must be runnable and testable by whoever is writing it, or it is
  written once and never changed. Both are protocol problems, and both are
  solved by handing the unit a connection instead of making it find one.
  `omega dev <unit>` asks the daemon for the unit's identity (`AdoptUnit`,
  operator-only): the supervised process is stopped, this process takes its
  place with a real token and the manifest's real grants, and the adoption
  lasts exactly as long as the asking connection — closing the terminal gives
  the unit back. An adopted unit is _held_, so the reconciler does not start
  the built binary underneath it, and it reports its own phase, because the
  lifecycle it would otherwise report belongs to a process that has left.
  `omega::testing` is the same idea without a daemon: `Rendered::of` for a
  widget, which is a pure function, and `TestDaemon` over a `UnixStream::pair`
  for everything that needs the protocol.
- The profile belongs to whoever is waiting for the build. `Profile::Debug` is
  the inner loop and `Profile::Release` is what a machine runs; `Layout`
  derives the output paths from it, and the daemon does not care which it was
  handed. Nothing hardcodes `--release`.
- The CLI's output is a product surface, not a side effect. Every line goes
  through `ui::Ui`: decoration to stderr and answers to stdout (so `omega run
  … | jq` gets a value), verbs from the closed `Step` enum on cargo's
  twelve-column rail (cargo's own lines scroll past in the same column of the
  same terminal), paths shortened against `$HOME`, one accent per line, and
  colour left to `anstream` — write the styling unconditionally and let the
  stream strip it. A `println!` outside `ui/` is a second format nobody chose.
  Every line puts something in the rail: twelve columns of leading space with
  no verb in them is whitespace that means nothing, and a reader sees it as a
  mistake. `Ui::detail` is for what continues a sentence — an error's causes —
  and a list of separate things gets a verb per line instead.
- Deadlines are measured on tokio's clock (`tokio::time::Instant`), never the
  system's, so `#[tokio::test(start_paused = true)]` can assert what a
  keepalive timeout or a backoff actually paces instead of sleeping through it.
  A test that sleeps to observe a timer is testing the machine's load.
- `omega init` sets a _machine_ up and takes no argument; `omega new <name>`
  writes a _plugin_. They were one command, and founding a workspace as a side
  effect of scaffolding your first unit is backwards — it left no way to have a
  config with no plugins, and made `omega init battery` read as initialising
  something called battery. `new` rather than `add` because `omarchy plugin
  add <git-url>` already means _install somebody else's_, and omega will want
  that word for the same thing.
- `omega new` adds the plugin to `system/Cargo.toml` but **prints** the line
  that puts it in a bar rather than writing it. A path between two crates of
  one workspace is bookkeeping; which bar a widget belongs in and what it is
  configured with is a decision, and the config plane is the author's own Rust
  program, not a generated file. The hint lives in `Scaffold::placement_hint`
  beside the template whose names it uses, held together by a test, and is
  fully qualified so it compiles wherever it is pasted — a hint that needs a
  second hint about an import is a hint that failed.
- One noun for the background process: `omega daemon` runs it and `omega
  daemon install` makes it a service. The unit file is genuinely not the
  daemon — `ServiceManager` is the seam for whatever keeps user processes
  running — but that is an implementation seam, and a second CLI word for one
  thing serves the implementation rather than the person looking for how to
  keep it running. The bare form still runs, because running is what it has
  always meant and what gets typed most.
- The daemon's systemd unit is _generated_, not carried, because it names the
  binary's own path — which is also the failure worth diagnosing: two omegas
  on one machine, the service running the other one, looking exactly like a
  desktop that came back after a reboot. `omega daemon status` compares the
  installed `ExecStart` against `current_exe`. The unit is `PartOf` and
  `WantedBy` `graphical-session.target` on both sides, because the daemon
  binds sockets in `$XDG_RUNTIME_DIR` and draws through a shell that belongs
  to one session; `TimeoutStopSec` must stay above the daemon's own five-second
  grace per unit, or systemd kills it in the middle of stopping them.
- `Service` takes the unit file's _path_; only `ServiceManager` knows where
  that is. The coupling matters because a `Service` that asked a manager for
  its own path could only be tested through a process-wide variable, which
  parallel tests in one binary share and clobber.
- The QML renderer and the daemon are one protocol in two halves, so the
  renderer travels _inside_ the binary (`include_str!` in `omega-cli/src/
  shell.rs`) and `omega shell install` writes what the binary carries. The one
  it installs is by construction the one its daemon speaks to; a stale copy is
  unreachable rather than merely discouraged. The manifest's `version` is
  pinned to the crate's by a test rather than patched at install time, so the
  file that ships is the file that is written and `--link` serves the same
  version a copy does. A second test asserts the carried file list _is_ the
  directory on disk, because a file added to the plugin and left out of the
  list installs as a plugin that loads and draws nothing. Installing swaps a
  staged directory in, which is what removes a file an older version shipped;
  copying on top would leave it to be loaded forever.
- The daemon is given its two socket paths (`Daemon::builder`) rather than
  resolving them from the environment: resolving is what confines a daemon to
  one per process, and naming them is what lets the run loop — reload,
  arrivals, convergence, shutdown ordering — be tested in-process against a
  temp dir. Sessions in tests are served over a `UnixStream::pair`, which
  needs no listener and no socket file: `SO_PEERCRED` reports this process on
  both ends, which is exactly what a unit or the operator presents.

## Docs

- `docs/architecture.md` — design and rationale
- `crates/omega-proto/schema/README.md` — schema rules
- `crates/omega-cli/shell/README.md` — the renderer and the shell socket
