# Omega shell renderer

The `omega.view` plugin renders Omega widgets and panels inside Omarchy's
Quickshell shell. Plugin authors build views with the Rust SDK; this directory
contains the QML implementation and its interaction tests.

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

`Connection.qml` connects to `$XDG_RUNTIME_DIR/omega-shell.sock` by default; its
`socketPath` property can select another address. The daemon's observation socket
streams state topics, views, and request results as newline-delimited JSON.
Consumers must distinguish these messages before interpreting their payloads.

A control sends the same `Frame` request used by the binary protocol:

```json
{
  "streamId": 1,
  "invoke": {
    "act": {
      "action": {
        "invokeUnit": { "unit": "lamp", "command": "toggle", "args": [] }
      }
    }
  }
}
```

Reading the observation stream does not require a unit handshake. Requests require
the daemon's own user and pass through the operator policy. Each invocation gets
an outcome or refusal on its request stream. The control socket separately serves
spawned units and authenticated operators; there is no debug-client bypass.

Views are addressed by `(unit, surface, module)`. The module selects an exact
placement; an empty module identifies the surface's unplaced instance. The daemon
authenticates the publishing unit and assigns view revisions. `Connection` routes
results to pending requests and reconnects after prolonged silence as well as
reported socket disconnection. It subscribes to unit lifecycle state to distinguish
starting, restarting, stopped, and failed plugins from intentionally empty views.
Disconnected views are cleared; pending commands report an unknown outcome and
are never retried automatically.

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

Draft text, slider drags, selection, and pending requests live in the renderer.
Fields retain drafts across unrelated updates; a changed explicit `value` lets
the plugin replace them. Lists use row keys for activation. Forms submit named
text fields as one map, retain labels and help while editing, and clear secret
fields after successful submission. Refused submissions retain drafts for retry.
Disabled or busy containers disable their descendants; pending commands prevent
repeat activation while retaining keyboard focus.

Tab moves between controls. Enter and Space activate buttons, toggles, and
choices; arrow keys adjust sliders or navigate lists, and Home/End set slider
limits. Enter submits a form, and Escape closes its panel. Focused controls have
a visible indicator.

Shared `emphasis` and `tone` properties express visual importance and feedback
independently. They do not grant interactivity or change routing. Theme colors
resolve through Omarchy's `Color` singleton in QML.

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
