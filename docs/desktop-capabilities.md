# Desktop capability surface

Reference for Omega's authoring surface against
[Omarchy](https://omarchy.org/) shell plugins. [Architecture](architecture.md)
describes the implemented runtime, [desktop-platform.md](desktop-platform.md)
records composition decisions, and [design.md](design.md) tracks open questions.

Parity means an author can build the same artifact from Rust. It does not mean
exposing QML, raw shell strings, or in-process unsandboxed code.

## Delivery status

The capability expansion is shipped in 0.5.0; 0.5.1 adds renderer compatibility
and CLI fixes. See the [release notes](../CHANGELOG.md). This sequence is separate
from the completed [desktop platform foundations](desktop-platform.md#delivery-status).

| Phase                           | Status                          | Delivered scope                                                                                                                                           |
| ------------------------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 — UI primitives               | Complete                        | Dropdown, checkbox, inline dialog, display controls, and numeric fields; typed search, panel helpers, textarea, and image/viewport gestures also shipped. |
| 2 — Platform state and controls | Complete for the selected scope | Capture/OCR/recording, plain-text clipboard, audio streams/input/output sinks, and Omega-owned notification state. Other domains remain deferred below.   |
| 3 — Presentations and services  | Complete                        | Menus and timed OSDs as overlay refinements; headless, session-scoped services. Authentication surfaces remain out of scope.                              |
| 4 — Events                      | Complete for observed state     | Idle transitions, clipboard changes, and battery-level payloads. Host-originated events remain deferred below.                                            |
| 5 — Command polling             | Re-scoped and shipped           | `Shell::capture` supplies bounded command output; a surface owns its polling interval, result decoding, and model updates.                                |

Phase 5 does not provide a `Poll<T>` reading or manifest-declared polling jobs.
[`Shell::capture`](../crates/omega/src/platform/process.rs) is an effect requiring
the spawn capability. It returns text with trailing whitespace trimmed and reports
nonzero exits as errors. Surface behavior runs the effect and stores the result in
its model; rendering reads that model.

## Next work

No further numbered phase is scheduled. Start the next capability change from a
concrete consumer and its missing contract. For command polling, exercise the
shipped capture effect in a real widget before introducing shared polling or
decoding APIs. The deferred domains below need a consumer and an explicit owner
before implementation. Cross-cutting runtime limitations remain in
[open design questions](design.md).

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
