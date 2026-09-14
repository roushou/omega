# Desktop platform implementation plan

Status: complete — phases 1–5 implemented and dogfooded. This is the delivery
record: milestone scopes and checkpoints below retain their original planning
language, and each delivery record describes validation at that milestone.
Temporary adapters mentioned in early phases were removed by later phases.
See [architecture](architecture.md) for current contracts, [design decisions](desktop-platform.md)
for their rationale, and [open questions](design.md) for remaining work.

## Delivery rules

Each phase is a working vertical slice, not a sequence of placeholder traits or
crate moves presented as features. Work within a phase can use several commits,
but acceptance applies to the complete slice. Do not stop after each file move
or request approval for routine implementation choices already covered here.

Preserve a usable desktop between phases. A schema/SDK/host change is completed
across all affected consumers before deployment. Do not introduce indefinitely
maintained dual protocols or parallel lifecycle authorities. Where a transition
needs an adapter, specify its removal point and test both sides until removal.

Before each phase, inspect current source and config changes, check applicable
instructions, and establish the baseline. Preserve unrelated user work. The user
configuration is a separate repository; keep implementation and config commits
separate when committing is requested. Building a milestone does not implicitly
request publication or a release.

For source changes, run all required checks:

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --check
crates/omega-renderer/shell/lint.sh
```

Also run the offscreen QML suite when changing renderer behavior and the ignored
CLI e2e tests when changing scaffolding/build discovery. Test the migrated config
workspace, including Clippy. Documentation-only edits need link and diff checks,
not a full build merely to claim progress.

## Phase 1 — Workspace roles and reusable renderer foundations

**Outcome:** users can create reusable libraries, and the same extracted control
renderer works inside Omarchy and in an isolated normal-window fixture harness.
The harness proves the boundary; it is not yet the user-facing preview product.

### Complete together

- Introduce validated workspace roles for `system/`, `plugins/`, and `libraries/`.
  Make build discovery, manifest extraction, scaffolding, checks, and watch roots
  agree that library membership does not imply execution.
- Provide an inspected migration from `units/` to `plugins/`. Preserve package
  identities, settings, local link overrides, and old published generations.
  Refuse collisions or ambiguous mixed layouts before writing.
- Add a concise library-scaffolding path; finalize its command spelling alongside
  plugin scaffolding. Both manage Cargo dependencies without deciding placement.
- Extract the shared QML renderer from Omarchy imports. Introduce explicit theme,
  constraints, assets, and interaction-session inputs; keep production appearance
  consistent through an Omarchy adapter.
- Extract Omarchy-specific authoring/compilation/installation into
  `omega-omarchy` and keep core `omega-document` host independent. Update generated
  dependencies, packaging, version checks, and shell adoption round trips.
- Supply a standalone Qt/Quickshell fixture window using the shared renderer and
  a production-quality default theme. Keep it disconnected from real commands.
- Move one genuinely shared presentation into a config library to exercise the
  workflow. Do not extract every private component for symmetry.

### Acceptance

- A scaffolded library builds and is consumed by a plugin and `system/` without
  being inspected as a runnable unit or appearing in daemon supervision.
- Migration succeeds on a copy of the user's workspace; collisions fail without
  partial destructive changes. All eight existing plugins still build and run.
- Core configuration compiles without an Omarchy integration dependency.
- The same renderer nodes are used in Omarchy and the fixture window. Verify
  keyboard controls, resizing, theme changes, and stable keys in both contexts.
- Existing shell adoption/build e2e tests pass with the extracted integration.

### Review checkpoint

Show the resulting workspace and the two hosts rendering the same component.
Record any measurable appearance or dependency differences. The renderer must
not hide Omarchy imports behind a supposedly generic singleton with the same
hard dependency.

### Delivery record — 2026-09-14

Implemented on `feat/desktop-platform`:

- `omega new <name> --lib --into <member>` creates libraries and explicit Cargo
  dependencies. Discovery validates roles; builds include every workspace member;
  development watches also react to shared-library changes.
- `omega migrate --check` inspects the source rename. `omega migrate` journals
  manifest edits, renames without replacing destinations, and recovers interrupted
  work without overwriting external edits. Published generations remain intact.
- `omega-omarchy` owns shell authoring, compilation, integration validation,
  configuration application, transport, and renderer installation. Configuration
  composes `Document::with(Shell)`. Core document validation refuses an unhandled
  integration payload; the existing protocol field is retained for phase 2.
- `omega-renderer` depends only on `omega-proto`. Both hosts use its embedded core
  with theme, asset resolver, interaction session, and allocated dimensions.
- Run `quickshell -p crates/omega-renderer/shell` for the isolated fixture window.
  Normal/loading/long-label/disabled cases capture commands locally. This is not
  the Rust component-preview product planned in phase 5.
- The user's eight plugins moved to `plugins/`. Audio and brightness consume
  `libraries/desktop-ui::LevelControl`, passing values and typed bindings.

Validation: all five required checks; three real CLI e2e tests; 48 offscreen QML
checks; config tests and Clippy; migration/build on a separate copy before live
migration. The normal-window harness loaded under the real Quickshell process.
Quickshell requires the shared core to be inside its configuration root, so the
harness entry point is `shell/shell.qml`.

The installed CLI and renderer match, all eight plugins run with zero restarts,
and local Cargo overrides resolve every Omega dependency to this checkout. The
Omarchy adapter delegates its theme functions to the existing host; the shared
percentage control adds an explicit 8-unit gap inside audio/brightness panels.
The live shell logged transient socket-unavailable warnings while the daemon was
stopped for deployment, with no renderer loading errors after installation.

## Phase 2 — Explicit instances and independent presentations

**Outcome:** a registered read-only surface can open as an overlay or normal
window, independently of the bar, under production lifecycle and authorization.

### Complete together

- Introduce surface, instance, incarnation, placement, and presentation types in
  the protocol and SDK. Separate command endpoints from UI declarations.
- Replace automatic anonymous widget instances and `module_id`-driven lifecycle
  with explicit instance creation. Translate bar placements into this model.
- Establish one daemon instance registry and a presentation state machine.
  Define create/present/hide/close/destroy and actual visibility reporting.
- Add authenticated renderer attachment and instance-scoped view delivery and
  interactions. Specify external Omarchy bootstrap and owner diagnostics.
- Supervise a standalone host per plugin and support read-only overlay and normal
  window presentations, plus the existing embedded/popup adapter.
- Add typed configuration builders and an operator CLI for presenting a declared
  entry point. Fail explicitly for unsupported host features.
- Negotiate required host/protocol features. Bound instances, attachments, view
  storage, and pending lifecycle requests; repair lag using fresh snapshots.
- Migrate existing typed widget references and configuration coherently. A short
  compatibility adapter may keep read-only widget authoring operational until
  the final `Surface` trait shape is established in phase 3.

### Acceptance

- Open a surface without any bar placement or Omarchy process requirement.
- Open two normal-window instances with different construction settings.
- Repeated singleton presentation creates only one instance. User dismissal does
  not cause reconciliation to reopen it.
- Restart a UI host and reattach while the plugin remains alive; restart the
  plugin and prove old incarnation events are rejected and stale views cleared.
- Exercise unauthorized attachment, unknown instance, duplicate destroy, lag,
  disconnect, and unsupported-feature outcomes through the real protocol.
- Existing bar indicators/panels use the same explicit instance registry.

### Review checkpoint

Demonstrate independent windows, singleton reopening, and host recovery. Record
startup/resource measurements and the exact focus behavior on the target desktop.
A focus request must not be advertised as an unconditional compositor guarantee.

### Delivery record — 2026-09-14

Protocol v5 establishes explicit instance/incarnation identity, separate command
endpoints, one UnitTable instance registry, and scoped renderer attachments.
Bars and standalone presentations use the same lifecycle and retained binding
validation. Read-only Surface authoring remains the deliberate phase-3 adapter;
there is no parallel runtime protocol.

Typed `Presentations::window` / `Presentations::overlay` builders compose with
`Document::presentation`. `omega present <unit> <surface>` supports singleton
reopening, `--new`, JSON construction settings, and overlays. A supervised
Quickshell host per plugin consumes embedded renderer assets. Per-instance
interaction sessions isolate errors, busy controls, and form completion.

Real protocol tests cover independent settings, singleton reuse, dismissal,
reconciliation, stale revisions, expired identities, duplicate destruction,
feature negotiation, scope refusal and attachment replacement. The observation
suite covers scoped delivery, disconnect and lag repair. Native dogfooding used
an isolated config and daemon: two differently configured windows appeared without
Omarchy placements; after closing one and restarting the renderer, only the other
returned (2.33 s in the final run). A warm keyboard-exclusive overlay appeared in
63 ms and used its own layer namespace. These are single-run checks, not latency
benchmarks. Hyprland tiled the normal windows, overriding their requested initial
sizes; focus and geometry remain compositor decisions.

Validation: all five required repository checks, 51 offscreen QML checks, three
real CLI e2e tests, and the existing config workspace's tests and Clippy passed.
One power-plugin assertion now checks command endpoints separately from surfaces;
plugin authoring required no changes. The matching local binary, rebuilt config,
and renderer are installed. All eight plugins run with zero restarts; the registry
holds their sixteen explicit instances, with all eight indicators observed visible
and panels initially hidden. The isolated test daemon was stopped after validation.

The normal-window fixture remains disconnected from production commands. Local
models, async tasks, controlled inputs, and the public Surface API are phase 3.

## Phase 3 — Stateful surfaces, local messages, and structured work

**Outcome:** build an interactive search interface with independent local models,
fixture data, typed bindings, and deterministic async behavior.

### Complete together

- Finalize the simple and stateful public Surface APIs using compile-tested
  examples. Keep read-only authoring short; separate render access from effects.
- Implement instance-owned models, serialized updates, lifecycle messages,
  dependency invalidation, and bounded asynchronous task execution.
- Generalize `Bind<T>` to local message targets and public commands. Keep Rust
  messages/captures local, enforce binding provenance, and define revision and
  in-flight event behavior. Bound obsolete registry retention.
- Implement task replacement, result-generation checks, destruction cleanup,
  and separately tracked external-effect receipts.
- Add semantic field edits, IME-safe controlled values/reset revisions, controlled
  selection, focus scopes, and cross-control navigation/activation.
- Define pending versus absent readings and opt-in startup dependencies so an
  application can display loading UI without changing all existing widget gates.
- Establish injectable service/time seams and a surface runtime test harness.
  Use fixture search results and simulated activation, not real desktop launching.
- Remove phase-2 transitional instance/authoring paths once migrated and tested.

### Acceptance

- Two instances can type and select independently without replicated query state.
- Delayed render updates cannot overwrite newer edits. IME editing and keyboard
  navigation remain usable while results change.
- An older search result cannot replace a newer result, including after task
  cancellation or instance recreation. Closing prevents further result delivery.
- Public commands remain available through CLI, but local message handlers are
  not advertised as command endpoints.
- Invalid, obsolete, and wrong-instance bindings cannot dispatch behavior.
- Reordering results preserves item identity; removing a selected item follows a
  documented fallback; pending/refused activation preserves usable focus.
- Author examples and doctests prove effects cannot be held by the render-side
  declaration through normal SDK wiring.

### Review checkpoint

Review the actual code needed for a brightness surface and a stateful search
surface. The abstraction succeeds only if authors avoid manual transport,
callback bookkeeping, and mutex coordination. Adjust API spelling now rather
than cementing conversational pseudocode.

### Delivery record — phase 3, 2026-09-14

- `Surface` / `derive(Surface)` and `Plugin::surface` are the read-only path.
  `StatefulSurface` / `Plugin::stateful` opt into a model, messages, and a separate
  `derive(Effects)` behavior dependency set. Typed `SurfaceRef`s work for both.
  The Widget authoring API and command-as-surface manifest adapter are removed.
- `Events::on` creates typed local bindings with process-unique IDs. Captures
  stay in the plugin; only the current render's registry is retained. Protocol v6
  routes daemon-resolved `SurfaceEvent`s with incarnation checks. Old captures,
  foreign-instance bindings and ambiguous command/local targets are refused.
- Instances process updates serially, invalidate on declared dependencies, and
  cache rendering between invalidations. `Optional<Reading>` explicitly opts out
  of startup gating and exposes pending separately from reported absence.
- `Task::perform`, `replace`, `blocking`, and `batch` own work under each instance.
  Admission is bounded to 16 tasks per update, 64 per instance and 256 per plugin.
  Replaced generations cannot deliver; native close cancels delivery and clears
  bindings. Started blocking work retains its permit until it actually exits.
  Hiding retains work; presented/hidden/closed hooks can adjust the model.
- `TextValue` / `TextEdit` and `Field::controlled` / `on_change` track edit/reset
  revisions. The renderer keeps typing immediate, coalesces pending edits, defers
  IME composition, and ignores older controlled values. Field navigation targets
  lists in the same component/instance scope; selection uses stable domain keys.
- `SurfaceHarness` uses the production model/binding/task implementation with
  injected readings, behavior dependencies, Tokio time, and explicit effect
  outcomes. No fixture operation automatically reaches the live desktop.
- `crates/omega/examples/stateful_search.rs` is a standalone fixture catalogue,
  with independent query, selection, loading/error state, and keyboard activation.
  Native validation typed different queries in two windows and activated a fixture
  result in just one. Hyprland input injection required explicit settled focus.

Validation covers Rust unit/integration/doctests (including effect exclusion),
managed worker admission through close, real protocol local routing and refusal,
three real CLI e2e tests, and 56 offscreen QML checks. The QML suite includes
newer drafts versus older views, coalesced pending edits, programmatic resets,
and injected IME composition transitions. Native testing exercised two different
queries and keyboard activation through the real daemon/renderer, then dismissal
and renderer restart with model/query retention. An actual IME engine was not
injected in the native test.

The eight-plugin config builds and its tests/Clippy pass after the mechanical
`Widget` → `Surface`, `WidgetRef` → `SurfaceRef`, `.widget` → `.surface` migration.
The independently staged fixture daemon is stopped after native validation.

All five required repository checks pass. The matching local binary, rebuilt
config and renderer are installed. All eight live plugins run with zero restarts;
all eight indicators report visible, and a live audio-panel open/close check
confirmed the acknowledged lifecycle through Omarchy. The config migration changes
only the eight plugin source files.

One parallel rerun failed the unchanged generation-cleanup test (zero reclaimed
entries where one was expected). The isolated test and a subsequent complete
suite both passed. No generation implementation or test changes were included.

## Phase 4 — Application services and the production launcher

**Outcome:** an Omega launcher replaces the fixture source with real applications
and supports real activation, an independent overlay, and a desktop shortcut.

### Complete together

- Evolve backend organization into `omega-platform`, supporting catalogue
  subscriptions and request/response operations without forcing queries into
  globally replicated state. Update all workspace/link/package references.
- Replace ad hoc desktop-entry shell execution. Evaluate GIO as the concrete
  backend and isolate its execution/dependencies from SDK consumers.
- Implement typed application IDs, catalogue invalidation, visibility/localization
  rules, icon resolution, activation context, and admission/failure semantics.
- Build `plugins/launcher` from shared primitives and local state. Put reusable
  search logic/components in libraries only where they have a meaningful API.
- Implement bounded ranking/results, loading/empty/error states, initial focus,
  navigation, activation, and explicit reopen/reset behavior.
- Wire a shortcut through an explicit compositor integration or existing user
  shortcut mechanism to the presentation action. Do not require broad desktop
  keybind convergence merely to open the launcher; record which route is used.
- Exercise successful activation and failure without silently changing the user's
  existing launcher binding before the new path works.

### Acceptance

- Search and launch real applications by name, including entries with spaces,
  terminal applications, and D-Bus activation where supported.
- Hidden entries and precedence behave correctly; catalogue changes are observed.
- No shell interpolation of desktop IDs or supplied arguments. Launch completion
  does not wait for the application to exit and does not block unrelated services.
- Activation failure stays visible and retryable. Successful admission dismisses
  the overlay once; reconnect does not repeat a launch.
- Icons, focus, Escape/outside dismissal, monitor selection, and reopening work on
  the user's actual desktop. Test the content in a normal window as well.
- Verify the isolated fixture version remains runnable with the same surface.

### Review checkpoint

Dogfood the launcher before expanding scope. Report cold/warm open behavior,
query responsiveness, task/view bounds, and known compositor limitations. Fix
observed architectural weaknesses inside this milestone rather than declaring
success because a window appears.

### Delivery record — phase 4, 2026-09-14

- Renamed the native service crate to `omega-platform`. GIO owns application
  discovery, invalidation, desktop-entry interpretation, and asynchronous
  activation on a dedicated main-context thread. Native dependencies remain
  outside plugin builds; CI and release build dependencies include GLib headers.
- Added typed application IDs, an optional catalogue reading, an activation effect,
  theme icons, typed list bindings, and instance-scoped presentation effects.
  Catalogue bounds are 4096 entries/512 KiB; queries are instance-local, capped at
  256 characters/16 terms with at most 20 displayed results. A generic remote
  query framework is not required for this bounded catalogue.
- The launcher example and `~/.config/omega/plugins/launcher` use the same surface
  implementation. Injected catalogue/effect fixtures exercise failure, retry,
  duplicate submission, instance isolation, catalogue removal, and reopen reset.
  These run through `SurfaceHarness`; interactive fixture selection is phase 5.
- Native testing exposed a self-dismissal acknowledgement deadlock. Lifecycle
  operations now leave the receive loop free, serialize per instance, and stop a
  session if delivery ends without acknowledgement. A duplex-connection regression
  test covers this path, alongside ownership checks.
- The local binary/config/renderer were rebuilt and deployed in order, retaining
  `/tmp/omega-phase4-rollback`. All nine plugins run with zero restarts. The existing
  eight indicators and audio panel retain their lifecycle behavior.
- The launcher uses a personal Hyprland binding, Super+Shift+Alt+Space, without
  replacing existing bindings. Native tests cover real Foot activation, icons,
  initial focus, keyboard navigation, Escape, query reset, explicit eDP-1 output,
  and two independent normal windows. An isolated daemon measured about 1.3 s
  cold and 21 ms warm opening; these are local observations, not latency guarantees.
- Required workspace checks, 58 offscreen QML tests, three CLI e2e tests, and the
  migrated config tests/Clippy passed. The private-D-Bus backend fixture covers
  precedence, hidden entries, localization, names/paths with spaces, terminal
  routing, literal URI arguments, invalidation, and D-Bus success/refusal.

Limits: the shortcut is registered and its CLI command is verified; synthetic
keyboard events did not trigger the compositor binding. Outside-click dismissal
is implemented but has no automated native pointer test. Only one physical output was available. Default output selection
remains host-selected. No compositor activation token is acquired automatically;
explicit tokens work for process activation and are refused for D-Bus activation.
The existing Qt portal registration warning remains. Launch success means
admission; a process that subsequently fails cannot be reported as a refused launch.

## Phase 5 — Preview, live development, and regression tooling

**Outcome:** plugin and library authors can inspect named cases visually, edit
code, simulate interactions, and reuse cases for automated tests.

### Complete together

- Provide explicit development-target registration of component and surface cases,
  including private components. Keep registrations out of production manifests.
- Add `omega preview` for plugin and library packages: case selection, viewport,
  theme, reset, event inspection, and simulated service outcomes.
- Reuse production renderer/runtime paths and the phase-3 test seams. Never
  automatically fall through to production service handles.
- Watch local dependency sources, compile incrementally, and keep the last good
  view visibly stale on failure. Make build diagnostics actionable.
- Provide deterministic screenshot capture and comparison with explicit baseline
  review, fixed environment metadata, asset readiness, and animation settling.
- Reuse the same cases for structural, behavioral, and visual checks. Protect
  secrets from event logs and screenshot fixtures unless deliberately synthetic.

### Acceptance

- Preview a private component and a shared library without a dummy production
  plugin, bar installation, daemon adoption, or changes to desktop state.
- Simulate launch success/refusal/delay and confirm expected UI transitions.
- Editing a shared library refreshes its selected consumer. A compile failure
  preserves the last good preview and recovery works on the next successful build.
- A visual regression yields a reproducible comparison artifact; baseline updates
  are deliberate. Runtime behavior tests still catch wrong activation targets.
- Existing components and the launcher ship useful cases demonstrating the DX.

### Review checkpoint

Use the preview workflow for an actual component change and inspect a deliberate
visual failure. Document environmental limits of pixel comparisons separately
from host integration tests.

### Delivery record — phase 5, 2026-09-14

- `omega-preview` is an explicit dev dependency. A normal library test registers
  `Cases`, including private components and stateful surfaces. Ordinary tests
  validate initial renders; `Cases::draw` and shared `SurfaceHarness` factories
  reuse the same fixtures for structural and behavioral checks. Registrations do
  not enter production manifests or plugin binaries.
- `omega preview <package>` builds the named test target and starts an isolated
  renderer with case selection, viewport/theme controls, reset, and an inspector.
  Effects wait for explicit success/refusal; leaving them pending exercises delays.
  Simulated presentation effects apply the production lifecycle without closing
  the development window. Reset/rebuild epochs discard renderer-local drafts.
- A private development protocol uses schema-generated JSON, bounded messages,
  private session directories, and peer PID checks. It has no daemon connection or
  live backend fallback. Event history stores kinds, not input/operation values.
- Source edits in the workspace and existing local path dependencies rebuild
  incrementally. A broken shared-library edit retained the last successful tree
  with a stale warning; a corrected edit recovered in the same window.
- Offscreen capture fixes viewport/theme/scale/font/backend, waits for image loads,
  and disables built-in motion. Baselines require explicit updates and carry
  environment metadata, including loaded Qt/font-library hashes. Repeated captures
  matched exactly; a deliberate shared-label edit reported 754 changed pixels and
  produced a difference PNG. Fixed SVG fixtures avoid native icon-provider textures
  that do not render correctly under the software backend.
- The user config has launcher cases (applications/loading/empty) and shared
  `desktop-ui` cases. The CLI, renderer, and config are deployed together with a
  rollback backup under `/tmp/omega-phase5-rollback`.
- Validation includes all required workspace checks, 61 QML checks, the three
  real CLI build tests, config tests/Clippy, capture/comparison, and native rebuild
  failure/recovery. Existing bar and launcher behavior receive a live smoke check.

Limits: adding a new external path dependency requires restarting the preview to
refresh its watch roots. Capture requires fixed local images instead of native
theme icon providers. Fixture-owned clocks/randomness and arbitrary Rust I/O
must be controlled by the fixture; this is effect isolation, not an OS sandbox.
Pixel checks are separate from compositor/input/portal integration checks.
See [the preview guide](previews.md) for the concrete API and workflow.

## Documentation and rollout at every milestone

Update `architecture.md` only for behavior actually shipped. Update relevant SDK
doctests, templates, CLI help, schema contracts, renderer docs, and repository
instructions when their invariants change. Mark phases complete only with their
acceptance evidence; amend the design when implementation establishes a better
boundary, explaining the current constraint rather than keeping stale alternatives.

For local deployment, preserve the established order:

1. Build/test and install the matching local CLI/daemon.
2. Stop the running daemon before publishing rebuilt config artifacts.
3. Build the migrated config using its local Cargo overrides.
4. Install matching renderer/integration assets when changed.
5. Start/install the daemon and verify plugin, host, presentation, and shell state.

Maintain a recoverable last-good generation and migration backup. A failed
candidate must not leave an ambiguous mixed config layout or incompatible active
renderer. Keep scope and source changes concrete before requesting any necessary
external publication or destructive-operation approval.

## Completion boundary

The program is complete when existing desktop plugins, the independent launcher,
a multiple-instance normal-window example, and library/component previews share
the intended contracts and pass their lifecycle, interaction, and regression checks.
A full file explorer, durable application sessions, incremental wire rendering,
and a general component marketplace are subsequent projects.
