# Omega shell renderer

The `omega.view` plugin renders Omega widgets and panels inside Omarchy's
Quickshell shell. Plugin authors build views with the Rust SDK; this directory
contains the shared QML core, fixture harness, and interaction tests.
The Omarchy adapter lives in `../../omega-omarchy/shell/`.

## Installation and development

The CLI carries an embedded copy of the renderer:

```sh
omega shell install
omega shell status
```

Installation prints the command for enabling the plugin in the shell. `omega init`
also enables it during initial setup. Reinstall after rebuilding the CLI to use
updated embedded assets. Installation stages and replaces the renderer directory,
removing obsolete files from earlier installations.

For QML development, link a checkout and restart the shell after edits:

```sh
omega shell install --link /path/to/omega
omarchy restart shell
```

Use `omega shell install` to return to the embedded copy. Linked files are
discoverable by the shell, but its recursive file watcher does not follow the
symlink.

## Connection contract

`core/RendererConnection.qml` connects to the observation socket and uses the
same `Frame` requests as the binary protocol, encoded as newline-delimited JSON.
The Omarchy adapter supplies a placement scope; `desktop/shell.qml` supplies a
plugin scope for independent windows and overlays. The owner bootstraps with
`AttachRenderer`, declaring supported features. The connection then has only its
renderer scope, rather than general operator authority.

State topics can be observed without attachment; view trees cannot. Attachment
returns instance metadata and then scoped view updates. Runtime identity is an
instance ID plus incarnation, not a surface or bar module name. The daemon owns
view revisions and expires identities when the plugin session ends. Replacing an
attachment revokes its predecessor. Snapshot repair clears destroyed instances.

Controls send `Interact` with identity, revision, node key, event name, and optional
value. The daemon resolves the command and fixed arguments from its retained tree.
`InstanceSession.qml` isolates pending state, errors, and form completion between
instances sharing a transport. Disconnect clears views and reports unknown outcomes;
commands are never retried automatically.

The daemon stages embedded core/desktop assets under its renderer cache and
supervises one standalone host per plugin. `omega present <unit> <surface>` opens
a singleton normal window; `--new`, `--overlay`, and `--config '{"label":"Example"}'`
select independent instances, overlays, and construction settings. Normal window
size/focus follow compositor policy. Overlays support explicit keyboard policy and
output selection through typed document builders or `omega present --output`.
`--dismiss-on-outside` opts into a full-output input area around centered overlay
content. Output selection defaults to the host choice. Overlay Escape and native window
dismissal record closed intent, which survives host restart and reconciliation. Plugin restart
expires instances and reconstructs only configured placements.

## Rendering and interaction

`BarWidget.qml` hosts the bar content and optional panel. Omarchy's `KeyboardPanel`
owns the panel window, focus, dismissal, and positioning. An interactive child
handles its own press; the surrounding slot opens the panel only for unhandled
presses. Bar width must account for the rendered content because the host button
has no text label to size itself from.

`ViewNode.qml` dispatches each node to a delegate under `nodes/`. Recursive children
are loaded by URL to avoid QML's recursive-component restriction. Unknown node
kinds draw nothing. Delegates read the current model and shared presentation from
`host`, so updates propagate without recreating the whole tree.

Layouts reconcile children by key, preserving interaction state across updates
and reordering. Allocated width constrains text wrapping; panels scroll when their
content exceeds the available height. Stacks inside columns span their available
width, and the wire `fill` property requests space along a stack's layout axis.

Draft text, slider drags, and pending requests live in the renderer. Selection
may be renderer-local or controlled by the surface model. Fields retain drafts
across unrelated updates; controlled text uses edit/reset revisions as described
below. Lists activate with typed selection values, falling back to row keys. Forms submit named
text fields as one map, retain labels and help while editing, and clear secret
fields after successful submission. Refused submissions retain drafts for retry.
Disabled or busy containers disable their descendants; pending commands prevent
repeat activation while retaining keyboard focus.

Tab moves between controls. Enter and Space activate buttons, toggles, and
choices; arrow keys adjust sliders or navigate lists, and Home/End set slider
limits. Enter submits a form, and Escape closes its panel. Focused controls have
a visible indicator.

Shared `emphasis` and `tone` properties express visual importance and feedback
independently. They do not grant interactivity or change routing. The shared default theme is host independent; the Omarchy adapter maps tokens
to the host's `Color` singleton.

## Generated readers and checks

`omega-proto::NodeKind` defines the property vocabulary. `Props.js` is generated
from it and handles protobuf JSON values, including integers encoded as strings.
`Icons.js` maps the protocol's icon names to glyphs. Do not edit either generated
file by hand.

From the repository root:

```sh
OMEGA_REGENERATE=1 cargo test -p omega-renderer
crates/omega-renderer/shell/lint.sh
crates/omega-renderer/shell/test.sh
```

Rust tests compare generated readers and embedded assets with the checked-in
files. QML lint checks the renderer sources; the offscreen interaction suite
checks view updates, input state, forms, and control behavior.

## Shared renderer inputs

`core/ViewNode.qml` takes `model`, `theme`, `assets`, `session`, and the allocated
Item width/height. Theme and session bindings propagate to children; a component
keeps its identity when its theme or siblings change. `Theme.qml` provides default
tokens; `OmarchyTheme.qml` maps them to Omarchy's current `Color` and `Style`.
`Assets.qml` accepts local files and image data URIs without fetching remote URLs.

The isolated normal-window harness uses this same core:

```sh
quickshell -p crates/omega-renderer/shell
```

Its normal/loading/long-label/disabled scenarios capture interactions locally.
It never opens the daemon socket. Rust-authored cases use `omega preview`; see
[the preview guide](../../../docs/previews.md).

## Stateful editing

The renderer supports typed local messages and declared commands through the
same instance-scoped interaction route. `Field::controlled(&TextValue)` and
`on_change` use committed TextEdit values with edit/reset revisions. Typing stays
local while pending edits coalesce; delayed values cannot overwrite a newer draft.
Programmatic resets do not emit user edits, and composition defers both edits and
incoming resets until the input method commits.

`Field::autofocus()` requests initial focus. `Field::navigate("results")` delegates
Up/Down and Enter to a keyed list without transferring text focus. Navigation is
qualified by component and instance scope. `List::selected` / `on_select` support
model-owned stable selection; disabled entries are skipped during navigation.

Application icons use `Image::icon`: hosts inject local theme lookup into
`Assets.iconResolver`; ordinary image paths remain local assets. No network
lookup is attempted. Field-to-list navigation waits for edits and composition
to settle into the rendered result revision before activating a row.

## Development preview host

`preview/` is the isolated host used by `omega preview`, with the same core
controls and a private development transport. `Viewport.qml` recreates its root
when a case epoch changes, so reset/rebuild cannot retain another model's drafts.
The inspector resolves captured effects explicitly. Captures use fixed software
rendering and local fixture assets; see [the guide](../../../docs/previews.md).
