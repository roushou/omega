# schema

The single source of truth. Generated Rust types are shared by the daemon and
every user crate — `Keybind` in the daemon is literally `Keybind` in your
config.

## Layout

- `wire.proto` — the frame envelope, handshake, and bidirectional RPC.
- `value.proto` — the generic data model (the open escape hatch).
- `state.proto` — the topic envelope and the replicated patches. The oneof
  in it _is_ the ontology: what a topic can carry is one list, in one place.
- `state/*.proto` — the payloads, one file per domain. Adding a topic is a
  message in the domain it belongs to and one arm in the envelope.
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
5. **Absence is a value, not a silence.** A `StateTopic` published with its
   oneof unset is the daemon saying there is nothing to report — no battery,
   no adapter, the broker is down. Saying nothing instead is indistinguishable
   from never having been asked, which is what a unit waits on before its
   first render.
6. **A field whose value is a constant is a field two readers will disagree
   about.** `BacklightState.max_percent` was always 100; it is `reserved`.
7. **A topic's resolution is part of its design.** `TimeState` is truncated to
   the minute, timestamp included, because last-value-wins only coalesces a
   value that is actually the same — a field moving every second would wake
   every clock on the bar sixty times an hour to redraw two digits.

## The minimal slice

Three messages carry the whole first vertical slice: `Frame`, `StatePatch`,
`ViewTree`. Inside them: `BatteryState`, one `Act`, and one `WidgetModule`.

Resource failures retain distinct codes: `UNAVAILABLE` means a required service
or session is absent, `RESOURCE_EXHAUSTED` means aggregate/count admission failed,
`PAYLOAD_TOO_LARGE` means one payload exceeds its limit, and `DEADLINE_EXCEEDED`
means the wait expired without proving whether an external effect completed.
`FAILED_PRECONDITION` remains for unmet protocol or domain prerequisites.
Existing enum numbers must not be reassigned.

Action payload validation lives in `omega-proto::action`, separate from capability
policy and subsystem availability. Every action kind has an exhaustive validation
arm. Required oneofs, nonzero action enums, positive workspace indexes, finite
volume changes, absolute volume in 0..=1, absolute backlight in 0..=100, selected
boolean flags, identifiers, required text and NUL-free process/D-Bus strings are
checked before execution. Missing window selectors (including empty selector
messages) retain focused-window semantics. Signed finite volume deltas and signed
backlight deltas remain relative changes; no arbitrary delta range is imposed.

Live requests authorize first, then validate and route. Invalid payloads map to
`INVALID_ARGUMENT` independently of handler presence. Desired documents and timer
admission apply the same rules; event-only schedules remain valid. Document
validation additionally checks scheduled unit/command references against the build.
Backend-specific selector/encoding limitations and runtime availability remain
handler responsibilities. Validation does not define escaping or shell quoting.
