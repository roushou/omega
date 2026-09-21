# Desktop capability surface

Maintainer design reference for expanding Omega's UI primitives and platform
capabilities toward parity with Omarchy shell plugins. The
[architectural principles](principles.md) govern primitives, layering, and
review; [architecture.md](architecture.md) describes the implemented runtime;
[desktop-platform.md](desktop-platform.md) records composition decisions;
[design.md](design.md) tracks open questions.

The yardstick is what an Omarchy Quickshell plugin can author — its plugin
kinds (`bar-widget`, `panel`, `overlay`, `menu`, `service`, `bar`), its
first-party widget catalogue, and its capture/hook/reminder automation.
Parity means an author can build the same artifact from Rust; it does not mean
exposing QML, raw shell strings, or in-process unsandboxed code.

## Current surface

**Node kinds** (wire `ViewNode.type`): `text`, `icon`, `progress`, `button`,
`slider`, `toggle`, `form`, `field`, `list`, `stack`, `separator`, `spacer`,
`header`, `graph`, `group`, `grid`, `image`.

**Shared modifiers**: `color`, `bold`, `emphasis`, `tone`, `padding`,
`tooltip`, `disabled`, `busy`, `width`, `height`, `fill`, `id`, `key`,
`shortcuts`.

**Platform state** (25 `SystemTopic`s): applications, battery, network, audio,
backlight, mains, monitors, plugins, time, wifi, workspaces, window, media,
bluetooth, system, idle, peripherals, input, vpn, disk, power-profile,
throughput, thermals.

**Actions** (26 `ActionKind`s): launch, run-command, settings (unserved),
workspace/window controls, lock/sleep/hibernate/reboot/shutdown, screenshot
(broker, no SDK handle), media, volume, backlight, notify, invoke-plugin,
floating/fullscreen, power-profile, wifi, bluetooth.

**Capabilities** (12): state-read, state-write, spawn, system-control, media,
audio, backlight, notify, screenshot, clipboard, network, bluetooth. Screenshot
and clipboard are declared but not yet reachable from the SDK.

**Presentation** (`Presentation` oneof): embedded (bar placement), popup
(anchored panel), window, overlay. `SurfaceKind` declares only `Widget`.

**Events**: monitor, window, power, notification, schedule, keybind, custom.

The [coverage test](../crates/omega-platform/tests/coverage.rs) forces every
topic and action to be served or explicitly named unserved; the
[props test](../crates/omega-renderer/tests/props.rs) forces every node
property to be read by a delegate or named `NOT_DRAWN`; the
[every_node fixture](../crates/omega/tests/plugin.rs) pins wire shape and
vocabulary. Every addition below is expected to extend all three.

## Omarchy parity target

| Omarchy plugin kind | Omega equivalent |
| ------------------- | ---------------- |
| `bar-widget`        | widget surface in a bar placement |
| `panel`             | popup presentation anchored to a placement |
| `overlay`           | overlay presentation |
| `menu`              | new summonable menu surface/presentation |
| `service`           | new headless `Service` surface kind |
| `bar` (full-bar)    | out of scope (see §Out of scope) |
| `command` bar module | new polling reading (see §Phase 5) |
| `qml` bar module    | out of scope, deliberate boundary |

Widget catalogue → capabilities map:

| Omarchy widget/panel | Requires |
| -------------------- | -------- |
| audio (per-app mixer, output picker) | audio streams topic + stream/default-sink actions |
| microphone (mute, source volume) | input state in audio topic + input actions |
| network (DNS provider) | DNS action |
| power profiles | already present |
| bluetooth, monitor, clock, media, workspaces, tray | already present |
| weather, tailscale, agents, system-update | new niche domains (§Phase 2, tier 2) |
| indicators (DnD, nightlight, reminder, recording, stay-awake, dictation) | new topics/actions per indicator |
| notifications service | notifications topic (reply/click already events) |
| clipboard manager | clipboard topic + actions |
| reminders overlay | reminders topic + actions |
| lock screen | lock surface/presentation (operator-gated) |
| OSD | OSD surface/presentation |
| polkit agent | out of scope (authentication surface) |
| image picker / emojis / background | covered by overlay + image/reading work; no new protocol |

Automation parity: Omega's `Schedules` + `Keybinds` + `Reaction` already cover
Omarchy hooks and reminders in kind; the gap is event vocabulary (§Phase 4),
not a new mechanism.

---

## Phase 1 — UI primitive expansion

Implemented. Decisions that deviate from the initial list:

- **`segmented` folded into `Choice`/`group`.** `group` is already a row of
  segments (see `Group.qml`); a second kind would duplicate it. Use `Choice`.
- **`dialog` is an inline card.** Modal stacking above an instance belongs to
  presentation, not a node. `Dialog` draws title, body, confirm/cancel, and
  outside/Escape dismissal inline; a modal presentation is a separate follow-up.
- **`surface::Search<T>` deferred.** `ui::SearchSelect` (field + keyed list) and
  the content components landed; a reusable ranking helper has no second
  consumer yet.
- **Dropdown is an inline popover.** The renderer owns expansion within the
  instance's layout; no window-level popup is introduced. Typed filtering
  (`searchable`) is deferred until a consumer needs it.

### New node kinds

| kind | props | events | SDK builder |
| ---- | ----- | ------ | ----------- |
| `dropdown` | `selected` (string), `placeholder` (string) | `select` | `Dropdown<T: ChoiceValue>` |
| `checkbox` | `on` (bool), `label` (string) | `change` | `Checkbox` |
| `dialog` | `title`, `body`, `confirm`, `cancel` (strings) | `confirm`, `cancel`, `dismiss` | `Dialog` |
| `badge` | `count` (number), `hidden_when_zero` (bool) | — | `Badge` |
| `status` | `title`, `message` (strings), `icon` (string) | — | `EmptyState` |
| `keycap` | `label` (string) | — | `Keycap` |
| `disclosure` | `title` (string), `open` (bool) | `toggle` | `Disclosure` |
| `scroll` | (none; children) | — | `Scroll` |

`dropdown` and `segmented` take `option(value, label)` children keyed by the
value, like `Choice`. The renderer owns popover presentation for `dropdown`;
the SDK exposes no layout contract for it. `dropdown` children must be assigned
`selection_key` exactly as `list`/`group` are today.

### Field extension for numeric input

Extend `field` rather than add a second input pipeline. New props: `numeric`
(bool), `min` (double), `max` (double), `step` (double). The renderer constrains
entry and reports `TextEdit` values that fail range/step only when the field
loses focus; committed submits stay `String` and decode through `Input` so
`u32`/`Percent` commands keep working. `Field::numeric(min, max, step)` sets
the props.

```rust
let age = Field::new("Age")
    .numeric(0.0, 150.0, 1.0)
    .on_submit(SetAge);
```

### SDK ergonomics promoted from real configs

`~/dev/omx` hand-rolls `Detail`, `ItemRow`, `PanelHeader`, `LabelledControl`,
and search ranking per plugin. Promoted into `omega::ui::content` as ordinary
`Component`s (no new node kinds): `Detail`, `Labelled`, `PanelHeader`,
`ItemRow`, and a `SearchSelect` built from `Field::navigate` + `List`.

### Acceptance

- New node kinds appear in `every_node`, render in the headless smoke test, and
  their props are consumed by delegates (no new `NOT_DRAWN` entries).
- `Dropdown` selection keys survive key scoping (test mirrors the existing
  `list`/`group` selection-key test).
- Numeric `Field` respects min/max/step in `Field.qml` and refuses out-of-range
  commits.
- `Dialog` dismiss is covered in the headless QML test (bound and unbound).

---

## Phase 2 — Platform state and controls

Declare topics in `crates/omega-proto/src/topic.rs` and payloads in
`schema/omega/state/`; declare actions in `schema/omega/action.proto` with a
row in `ActionKind` and a capability cost. Each new action belongs to exactly
one broker; the coverage test names the broker's topics/actions.

**Landed:** capture, clipboard, audio-stream, and notification domains.
**Remaining:** nightlight, reminders, and DNS — each needs an owner other than a
stateless shell-tool broker.

### Landed — capture, clipboard, audio streams, notifications

| Topic | Payload | Notes |
| ----- | ------- | ----- |
| `clipboard` | `ClipboardState` | current plain text only; history is deferred |
| `audio-streams` | `AudioStreamsState` | per-app streams (index, app, volume, muted) |
| `notifications` | `NotificationsState` | notifications Omega raised and not yet closed |

`AudioState` gained input state (`input_volume`, `input_muted`,
`default_source`) on the existing `audio` topic.

| ActionKind | Capability cost | Broker |
| ---------- | --------------- | ------ |
| `CaptureText` | `Screenshot` | desktop (grim \| tesseract \| wl-copy) |
| `RecordScreen` | `Screenshot` | desktop (wf-recorder, SIGINT stop) |
| `WriteClipboard` | `Clipboard` | clipboard (wl-copy) |
| `ClearClipboard` | `Clipboard` | clipboard |
| `SetStreamVolume` / `SetStreamMute` | `Audio` | pipewire (pactl) |
| `SetDefaultSink` | `Audio` | pipewire |
| `SetInputMute` / `SetInputVolume` | `Audio` | pipewire |

`Screenshot` gained its SDK handle. `SetSetting`/`ToggleSetting` were **deleted
from the vocabulary**: runtime settings have no owner, and an unserved action
is a capability granted for something that never happens (see
[design.md](design.md)). Capture-text OCR and recording follow the existing
grim/slurp shell-tool broker style; the recording child is owned by the broker
and interrupted with SIGINT on stop and disconnect. The audio broker now reads
sinks, sources, and sink-inputs in one pass and watches sink/source/stream
subscription events.

SDK handles landed: `platform::capture::{Capture, Ocr, Recording}`,
`platform::clipboard::{Clipboard, Write}`,
`platform::audio::{Streams, Stream, StreamControl, SinkControl, SourceControl}`,
and `platform::notification::Notifications`. Capture text goes to the clipboard,
matching Omarchy's own text-capture behavior.

The notifications reading is the daemon's own in-flight list, not every
application's: a freedesktop notification client cannot enumerate the server's
history, and Omega is not the notification server. The broker records each
`Notify` id and removes it on `NotificationClosed`.

### Remaining tier-1

| Item | Why it is not a shell-tool broker |
| ---- | --------------------------------- |
| `nightlight` (`SetNightlight`, `NightlightState`, `CAPABILITY_NIGHTLIGHT`) | host-specific service management (hyprsunset config + process), not a queryable subsystem |
| `reminders` (`CreateReminder`/`DismissReminder`, `RemindersState`, `CAPABILITY_REMINDERS`) | daemon-owned timers alongside `Schedules`, with event derivation and state projection |
| `SetDnsProvider` | NetworkManager per-connection D-Bus state, not a single stateless call |

New capabilities still to add: `CAPABILITY_NIGHTLIGHT`, `CAPABILITY_REMINDERS`.

### Tier 2 (deferred until a consumer exists)

`weather`, `updates`, `tailscale` topics; `UpdateSystem`
(`CAPABILITY_PACKAGES`), and Tailscale controls.

### Acceptance

- Coverage test passes with the new topics/actions assigned to exactly one
  broker (or named daemon-owned).
- `omega::testing` fixtures gain `topic::clipboard` and `State::clipboard`;
  no live backend fallback.
- Capability costs are declared once in `ActionKind::cost`.
- `platform.rs`'s one-primitive-reading table lists the new `clipboard` topic.

---

## Phase 3 — Presentation and surface kinds

**Landed:** the OSD presentation (a timed overlay), the menu builder, and the
`Service` headless surface kind. **Remaining:** `Lock` (out of scope —
PAM/session-lock is an authentication surface).

### Reframed: menus and OSDs are overlay refinements

The initial plan split `Menu`/`Osd`/`Lock` into new `SurfaceKind`s. In
practice a menu is an overlay with `KEYBOARD_POLICY_ON_DEMAND` +
`dismiss_on_outside`, and an OSD is an overlay with `KEYBOARD_POLICY_NONE` + a
short auto-hide timeout. They are presentation refinements, not new surface
kinds. `OverlayPresentation` gained `timeout_ms`; the daemon arms an auto-hide
timer when a timed overlay starts or is re-presented, guarded by a per-instance
epoch so a re-Present cancels the stale timer.

### Landed

- `OverlayPresentation.timeout_ms` (proto), validated to `0..=86_400_000`.
- `omega_document::Presentations::menu()` and `::osd()` typed builders, plus
  `Overlay::timeout_ms()`.
- Daemon auto-hide: `Instance` carries an `epoch`; `Present` advances it and
  re-arms the timer; the timer re-checks the epoch and visibility before
  hiding. `create` arms the timer for timed overlays that start visible.
- `SurfaceKind::Service` (manifest): headless, session-scoped, no render, no
  placement, no presentation. `plugin!().service(Service)` / `service_as`
  register it; the runtime constructs services once at session start, runs
  `initialize`/`mounted`, and polls their tasks without ever publishing a view.
  Document validation already excludes non-`Widget` surfaces from placement, so
  a `Service` surface is unplaceable by construction.

### Remaining

- `Lock` presentation/surface: out of scope (authentication).

### Acceptance (landed)

- `Presentations::osd` produces a non-focusable overlay with a short timeout;
  `menu` produces an on-demand overlay that dismisses on outside.
- A timed overlay hides itself after its timeout, and the daemon test uses
  paused time to verify it.
- A `Service` surface declares `SurfaceKind::Service`, its startup hook runs,
  and it never publishes a view; document validation refuses to place it.

---

## Phase 4 — Event vocabulary

**Landed:** idle transitions, clipboard changes, and the battery-level payload.
**Deferred:** theme/font/boot/update events — they originate in the Omarchy
host's hook runner, not in any state the daemon observes, so declaring them
would name events that never fire. The plugin-emitted `EVENT_CUSTOM` already
covers host-adjacent signals today; a host→daemon event contract is a separate
follow-up.

### Landed

- `EVENT_IDLE_ENTERED` / `EVENT_IDLE_EXITED` — derived from `IdleState.idle`
  transitions in `Transitions`.
- `EVENT_CLIPBOARD_CHANGED` — derived from `ClipboardState.text` transitions.
- `PowerEvent.battery_percent` (fraction 0..1): battery crossings carry the
  level that crossed, matching Omarchy's `$1` hook argument. Mains events
  leave it unset.

`Transitions::of` now returns `(EventKind, Option<event::Detail>)` instead of a
bare kind, so derived events carry their payload. `Reaction::fire` remains the
only consumer; events are still derived from state transitions in one place and
never persisted.

### Acceptance (landed)

- A plugin declaring the new kinds receives them through the existing event
  path (daemon test).
- Battery crossings carry the crossing level; idle and clipboard fire once per
  crossing, not per poll.

---

## Phase 5 — Polling reading (command bar module parity)

**Deferred: no concrete consumer, and one design point is still open.** The
mechanism is clear — a manifest-declared command + interval, run by the daemon
(not a per-plugin subprocess), projected into a plugin keyspace that a `Poll<T>`
reading watches. What remains open is the decode contract: a command's plain
text or JSON output must map to `T` before the daemon can publish it, and
picking the wrong default (string-only vs. JSON) would make the first real
consumer fight the primitive.

Sketch, for when a consumer exists:

- `Poll<T: PluginState + FromValue>` is a `Reads` field over
  `plugin.<name>.poll.<T::KEY>`, with `const COMMAND: &str` and
  `const INTERVAL: Duration` from the type.
- The manifest declares polls (`address`, `command`, `interval_seconds`) so the
  daemon runs them on the existing schedule machinery and publishes decoded
  output with bounded `omega-host::process` capture.
- Decode failures mark the reading invalid; they never silently drop. A test
  fixture injects synthetic poll values without spawning.

`onClick` needs nothing new: it is a command binding on the surrounding view.

### Acceptance (deferred)

- `Poll<T>` participates in required-reading gating and `has_reading()`.
- Output limits and timeouts use `omega-host::process`; cancellation targets
  the direct child only.
- A test fixture supplies synthetic `Poll` values without spawning.

---

## Sequencing

1. **Phase 1** — UI primitives. Done.
2. **Phase 2, tier 1** — capture, clipboard, audio streams/input, notifications
   topics/actions. Done; nightlight, reminders, and DNS deferred with reasons.
3. **Phase 3** — OSD/menu (overlay refinements) and `Service` surface kind. Done;
   `Lock` out of scope.
4. **Phase 4** — event vocabulary. Done for idle/clipboard/battery-payload;
   theme/font/boot/update deferred (host hook runner, no daemon observation).
5. **Phase 5** — polling reading. Deferred (see above).
6. **Phase 2, tier 2** — weather, updates, tailscale. Deferred until a consumer.

Each landed phase keeps the five CI checks green.

## Out of scope

- Full-bar replacement (`bar` plugin kind): the Omarchy bar engine owns layout,
  gestures, and native widgets; Omega owns widget content. Revisit only if a
  measured author workload demands it.
- Polkit agent, session-lock PAM, and other authentication surfaces: they need
  OS-level trust boundaries, not manifest capabilities.
- Arbitrary QML (`type: "qml"`): contradicts the typed, reconciled node
  protocol. The polling reading is the intended escape hatch.
- Component marketplace, durable application sessions, file explorer, and
  incremental wire rendering: unchanged from [desktop-platform.md](desktop-platform.md).
