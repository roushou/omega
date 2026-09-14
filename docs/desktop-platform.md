# Desktop platform decisions

Maintainer design reference for composition and ownership boundaries. [Architecture](architecture.md) describes
current runtime contracts, the [completed plan](archive/desktop-platform-plan.md) records
milestone validation, and [open questions](design.md) tracks remaining work.

## Purpose

Omega is a foundation for users to build their own desktop. It supplies an
opinionated workspace, runtime, deployment system, and development workflow.
Users compose visual primitives and implement application behavior in Rust.

The same foundations support bar indicators, panels, independent overlays,
normal windows, and isolated previews. The application launcher exercises them
with real platform services; a full file explorer remains a future workload.

## Workspace composition

```text
~/.config/omega/
  Cargo.toml
  system/                    desktop composition
  plugins/                   independently runnable features
    power/
    launcher/
  libraries/                 reusable Rust crates
    desktop-ui/
```

`system/` selects settings, presentations, placements, and desktop automation.
Plugins expose surfaces, commands, and reactions. Libraries hold reusable UI,
logic, or domain contracts without acquiring an implicit process lifecycle.
Private components remain ordinary Rust modules; file names do not register UI.

System code depends on plugins and libraries. Plugins depend on libraries and
may consume another plugin’s typed published records. Reusable libraries should
not depend on concrete runnable plugins: extract shared contracts into a library
when consumers need them. Cargo resolves dependencies; local checkout overrides
belong in the ignored `.cargo/config.toml` written by `omega link`.

Scaffolding creates workspace membership and requested dependencies. It prints
placement suggestions because deciding where a surface appears belongs to the
configuration author. Legacy source migration preserves package identities and
last-good built generations, and refuses ambiguous layouts.

## Content, state, and behavior

| Concept      | Responsibility                                                   |
| ------------ | ---------------------------------------------------------------- |
| Component    | Builds a subtree from explicit inputs and typed bindings         |
| Surface      | Declares a UI entry point and its reading dependencies           |
| Instance     | Owns a model, construction settings, bindings, and managed tasks |
| Presentation | Describes where an instance appears and its visibility           |
| Placement    | Gives a configured presentation a stable identity                |
| Command      | Exposes a typed public plugin endpoint                           |
| Reaction     | Handles an external transition                                   |

`Surface` keeps read-only authoring short. `StatefulSurface` adds a local model,
messages, lifecycle hooks, and a separate effect declaration. The render object
receives readings; behavior receives effects. Derives enforce that separation
through SDK wiring, rather than trusting every render method to avoid effects.

```text
UI message -> update model -> render View -> renderer
                  |
                  +-> managed task -> completion message -> update model
reading change -----------------------> invalidate dependent instance
```

Updates are serialized per instance. Async work returns messages and cannot
retain a mutable model borrow across an await. Replaceable tasks carry generations
so an old result cannot overwrite newer work. Closing cancels task delivery;
hiding retains work; destroying releases the instance. External-effect admission
is separate from task cancellation: dropping a future cannot undo a launch.

`Bind<T>` lets a component accept either a local message binding or a public
command without depending on the caller’s implementation. Captures stay in Rust.
Only current, instance-authorized bindings can dispatch; stable node identity
does not authorize a callback from an obsolete render.

Private queries, selection, and transient errors belong to instance models.
Intentionally shared facts use replicated records. Construction settings are
layered inputs, not mutable application state. Required readings gate startup;
`Optional<R>` explicitly permits a loading view before the first report.

## SDK module ownership

`omega::platform::<domain>` owns each external service’s SDK types, readings,
composites, and controls. `surface`, `command`, and `reaction` own distinct authoring
contracts; `ui` owns composable view nodes and interaction bindings. `plugin` owns
registration and Omega supervision, while `record` owns shared plugin state.

Shared field wiring and runtime state are private implementation layers. Generic
effect completion belongs to `effect`; platform modules depend on that mechanism,
not on each other’s control implementations. Root re-exports keep common authoring
contracts short without duplicating domain namespaces.

## Renderer and host boundaries

QML owns layout and immediate input mechanics: caret, IME composition, drag,
hover, and physical focus. Rust owns application selection and semantic edits.
Controlled text uses edit/reset revisions to prevent a delayed view from
replacing newer typing. Lists preserve keyed delegates and support typed
selection and activation.

The shared renderer has no Omarchy imports. Hosts supply theme, assets,
constraints, and interaction sessions. Omarchy owns the embedded/popup adapter;
a supervised Quickshell host provides independent windows and overlays.
Components compose shared primitives without acquiring a host dependency.

The daemon owns instance identity, incarnation, authority, routing, and desired
presentation state. Hosts report observed state and dismissal. Scoped renderer
attachments authorize particular views and interactions. Host restart can recover
an instance while its plugin lives; plugin restart invalidates old bindings and
views. Transient application sessions are not durable across daemon restarts.

Full view snapshots remain bounded and deduplicated, with snapshot repair after
lag. Incremental rendering, directory paging, and thumbnail pipelines need
measured workloads before introducing another synchronization contract.

## Platform and development boundaries

`omega-platform` owns external Linux connections. The application backend uses
GIO on its own GLib context; native dependencies stay outside the SDK. The bounded
application catalogue is shared state, while ranking and selection stay in the
launcher. Activation takes typed application IDs and literal URIs. Success means
admission, not guaranteed focus or successful application startup.

`omega-preview` is an explicit development dependency. Cases register in ordinary
Rust tests, including alongside private components. They use the production
surface harness and renderer with captured effects and synthetic fixtures, without
a daemon or live service fallback. This is effect isolation, not an OS sandbox;
fixture code must control its own I/O, clocks, and randomness.

Structural, behavioral, QML, visual, and native integration tests cover different
contracts. Pixel comparisons require matching raster environments and fixed
assets; they do not prove compositor focus, IME behavior, or popup positioning.
See the [preview guide](previews.md) for registration and capture usage.

## Out of scope

A full file explorer, durable application sessions, a component marketplace,
arbitrary user QML nodes, and incremental wire rendering are separate projects.
The launcher, multiple-instance window example, existing desktop plugins, and
component previews provide concrete consumers for the current boundaries.
