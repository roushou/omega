# Desktop capability surface

Reference for Omega's authoring surface against
[Omarchy](https://omarchy.org/) shell plugins. [Architecture](architecture.md)
describes the implemented runtime, [desktop-platform.md](desktop-platform.md)
records composition decisions, and [design.md](design.md) tracks open questions.

Parity means an author can build the same artifact from Rust. It does not mean
exposing QML, raw shell strings, or in-process unsandboxed code.

## Authoring surface

| Surface                 | Declared in                                      | Guarded by                                         |
| ----------------------- | ------------------------------------------------ | -------------------------------------------------- |
| UI nodes and properties | `crates/omega-proto/src/ui.rs`                   | renderer props test, `every_node` fixture          |
| System topics           | `crates/omega-proto/src/topic.rs`                | platform coverage test                             |
| Actions                 | `crates/omega-proto/schema/omega/action.proto`   | `ActionKind` cost, platform coverage test          |
| Capabilities            | `crates/omega-proto/schema/omega/plugin.proto`   | manifest grants                                    |
| Events                  | `crates/omega-proto/schema/omega/event.proto`    | derived from state in `omega-daemon/src/events.rs` |
| Presentations           | `crates/omega-proto/schema/omega/instance.proto` | `PresentationSpec` validation                      |

Those files are the source of truth for what exists; this doc does not repeat
their lists.

## Omarchy parity

| Omarchy plugin kind | Omega equivalent                                      |
| ------------------- | ----------------------------------------------------- |
| `bar-widget`        | widget surface in a bar placement                     |
| `panel`             | popup presentation anchored to a placement            |
| `overlay`           | overlay presentation                                  |
| `menu`              | overlay with keyboard-on-demand and outside dismissal |
| `osd`               | overlay with no keyboard and a `timeout_ms` auto-hide |
| `service`           | headless `Service` surface, session-scoped            |
| `command` module    | `Shell::capture` polled by a surface on an interval   |
| `bar` (full bar)    | out of scope                                          |
| `qml` module        | out of scope                                          |

## Deliberate boundaries

Out of scope:

- **Full-bar replacement.** The Omarchy bar engine owns layout, gestures, and
  native widgets; Omega owns widget content.
- **Authentication surfaces** (polkit agent, session-lock PAM). They need
  OS-level trust boundaries, not manifest capabilities.
- **Arbitrary QML.** Contradicts the typed, reconciled node protocol; the
  command widget is the escape hatch.
- **Component marketplace, durable application sessions, file explorer, and
  incremental wire rendering** — unchanged from
  [desktop-platform.md](desktop-platform.md).

Deferred until a consumer exists:

- **Nightlight and DNS provider** — host-specific service management, not
  queryable subsystems.
- **Reminders** — a daemon-owned timer subsystem alongside `Schedules`.
- **Weather, software updates, and Tailscale** — niche domains.
- **Theme, font, boot, and update events** — they originate in Omarchy's hook
  runner, not in state the daemon observes.
