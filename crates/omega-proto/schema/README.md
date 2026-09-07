# schema

The single source of truth. Generated Rust types are shared by the daemon and
every user crate — `Keybind` in the daemon is literally `Keybind` in your
config.

## Layout

- `wire.proto` — the frame envelope, handshake, and bidirectional RPC.
- `value.proto` — the generic data model (the open escape hatch).
- `state.proto` — typed system state topics and the replicated patches.
- `action.proto` — the closed action taxonomy and keybinds.
- `event.proto` — the closed event taxonomy.
- `unit.proto` — manifests, surfaces, capabilities.
- `ui.proto` — the declarative view tree.
- `document.proto` — the state document (desired state of the machine).

## Design rules

1. **Closed taxonomies, open escape hatches.** Actions and events are enums.
   `RunCommand`, `Value`, `CustomEvent`, and string ids are the explicit,
   capability-gated or SDK-narrowed escape hatches.
2. **Desired ≠ actual.** `document.proto` is intent; `state.proto` is reality.
   Never one type for both — that's how you get a Hyprland-shaped schema.
3. **Strings are validated at the edge.** Key symbols, topic names, and unit
   ids are strings on the wire; the SDK wraps them in closed enums and typed
   accessors so config still fails at compile time.
4. **Revisions are daemon-owned.** `StateTopic.revision` and `ViewTree.revision`
   are assigned by the daemon (monotonic per topic/surface); producers publish
   values, never revisions.

## The minimal slice

Three messages carry the whole first vertical slice: `Frame`, `StatePatch`,
`ViewTree`. Inside them: `BatteryState`, one `Act`, and one `WidgetModule`.
