# shell

The Quickshell side of Omega: a first-party plugin that renders the view trees
units publish to the daemon.

## How it works

The daemon owns a second Unix socket, `omega-shell.sock`. It streams
everything the daemon holds as newline-delimited JSON, and it takes requests
back on the same connection.

Three shapes of line come out:

- a view — `{"unit": ..., "surface": ..., "view": ...}`, plus `"module"`
  when the state document instantiated that surface
- a state topic — `{"topic": "battery", "revision": "3", "battery": {...}}`
- an answer — `{"streamId": 1, "result": {...}}`, to something asked

A consumer reads the ones it cares about and ignores the rest; the bar plugin
takes views, `omega status` takes the `units` topic.

One shape goes in, and it is the daemon's own `Frame` written as JSON rather
than a second vocabulary:

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

That is what pressing a button is: the operator asking a unit to run one of
its own commands — the same request `omega run` makes, authorized through the
same policy table, and refused out loud on the stream that carried it.
Reading the socket needs nothing; asking anything of it requires being the
daemon's own user, exactly as on the control socket.

```
unit (Rust) ──protobuf──▶ daemon ──JSON lines──▶ Quickshell Socket
                               └── omega-shell.sock
```

QML can't decode protobuf, so the daemon speaks JSON on the shell socket and
the plugin reads it directly with Quickshell's `Socket` — no bridge process.

Four things about this that cost a day to learn, written down so they do not
again:

- QML refuses a component that names itself — _"ViewNode is instantiated
  recursively"_ — so a child node is loaded by **url**, resolved when it is
  reached rather than when the file is compiled.
- `WidgetButton` sizes itself from its label. A widget that draws its own
  content has no label, so the slot must be given a `fixedWidth` from the
  content or the bar lays out seventeen pixels of nothing.
- Views and state topics share the stream. A line is a view only if it _has_
  a view, and a filter that forgets to check that will treat every state
  change as a view of nothing.
- A socket whose peer goes away does not reliably report itself closed, so
  the reconnect watches for **silence** rather than for `connected`. A widget
  that trusts `connected` freezes on the last view it ever saw, which is
  worse than showing nothing: it looks like a working widget reporting a
  stale number.

`ViewNode.qml` draws a tree: one item per node, recursing through stacks, with
five kinds — `text`, `icon`, `progress`, `button`, `stack`. A kind it does not
know draws nothing, so a tree from a newer plugin degrades to the parts this
shell understands rather than failing whole. `Props.js` reads a node's props,
which are protobuf `Value`s: the trap is `intValue`, which protobuf JSON
writes as a _string_.

A view is addressed by three things. `unit` and `surface` say what it is:
surface ids are chosen by unit authors and are only unique within a unit, and
the daemon fills in the unit it authenticated — a unit cannot publish into
another's surface. `module` says _which instance_: the state document can put
the same widget in a bar twice with different configuration, and each instance
is its own view. A line with no `module` is the surface's single instance.

Each view line carries the daemon-assigned monotonic `revision` for its
surface (`view.revision`); the reconciler uses it to drop stale and duplicate
frames.

- `plugins/omega.view/` is a `bar-widget` plugin that connects to
  `omega-shell.sock` and renders the text nodes of the view tree into the bar.

## Run it

```bash
omega build            # compile ~/.config/omega → ~/.local/state/omega
omega daemon           # run the daemon in the foreground (binds both sockets)
omega daemon install   # ...or as part of the session, so it survives a reboot
```

The control socket serves only units the daemon spawned. This socket is
read-only and unauthenticated — nothing can be asked of it, only observed —
and lives in `$XDG_RUNTIME_DIR`, which is private to the user. To point a
debug client at the _control_ socket, start the daemon with
`--allow-debug-clients`.

The daemon binds the shell socket at `$XDG_RUNTIME_DIR/omega-shell.sock`
(override with `OMEGA_SHELL_SOCKET`); the plugin defaults to the same path.

## Install the plugin

```bash
omega shell install                       # then: omarchy plugin enable omega.view --section right
omega shell status                        # what is installed, and whether it matches this omega
omega shell uninstall
```

The renderer is **carried inside the `omega` binary** (`include_str!`) rather
than copied out of a checkout, because it and the daemon are one protocol in
two halves: `Props.js` decoding `intValue` as a string is the same fact as the
Rust test that pins `"intValue": "6"`. Embedding makes them inseparable — the
renderer a binary installs is by construction the one its daemon speaks to,
and a stale copy stops being a mistake to avoid and becomes a state that
cannot be reached. A hand-copied plugin one day behind was the first of five
stacked reasons the bar once drew nothing.

Two things follow. `omega shell install` writes through a staging directory
and swaps it in, so the shell never sees the plugin half-written and a file an
older version shipped cannot survive to be loaded forever. And `omega check`
warns when what is installed differs from what this binary carries, which is
the only symptom that failure otherwise has.

Installing does not enable: where a widget sits in the bar is the person's
layout, so the command prints `omarchy plugin enable` rather than editing
`shell.json`.

### Working on the QML

```bash
omega shell install --link            # symlink this checkout instead of copying
omega shell install                   # ...and back to the embedded copy
```

The shell **finds** a linked plugin — its scan globs `*/` and follows the
link — but does not **watch** one: `inotifywait -r` does not descend through a
symlink, so saving a file in the checkout fires no event. Edits need
`omarchy restart shell`, and the widget already on screen keeps running the
code it was built with until then either way.
