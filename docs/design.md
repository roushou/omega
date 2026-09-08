# Design

Where the next layer of Omega is going, and the shapes it is built out of.
[`architecture.md`](architecture.md) describes what exists. This describes
what is being added, why each abstraction has the shape it has, and — the
half that does not survive in code — what was considered and left out.

## The missing noun

Seven handles are public in the SDK. Between them they ask the daemon for
eight things, and one of the eight is served:

| handle          | asks for                           | served by        |
| --------------- | ---------------------------------- | ---------------- |
| `Battery`       | the `battery` topic                | the sysfs source |
| `Network`       | the `network` topic                | —                |
| `Audio`         | the `audio` topic                  | —                |
| `Shell::run`    | `RunCommand`                       | `action.rs`      |
| `Shell::launch` | `LaunchApp`                        | —                |
| `Notify`        | `Notify`                           | —                |
| `Session`       | `Lock` `Sleep` `Reboot` `Shutdown` | —                |
| `Volume`        | `SetVolume`                        | —                |

A unit holding `Network` never draws at all: the runtime holds the first
render until every declared topic has a value, and nothing publishes that
one. A unit holding `Volume` calls it and the daemon logs `UNIMPLEMENTED`.

Nothing catches this, because reading a subsystem and writing to it are
modelled as unrelated things. Left alone, `sources/` grows a PipeWire reader
and `action.rs` grows a PipeWire writer: two components, one subsystem, two
connections, two discovery paths, and no shared answer to whether PipeWire is
running at all.

That is a missing noun, not a backlog of missing features.

## Brokers

A **broker** owns a subsystem in both directions. It is the only thing that
holds a connection to NetworkManager, and it both projects that connection as
topics and serves the actions that write to it.

```rust
pub trait Broker: Send + 'static {
    fn name(&self) -> &'static str;

    /// What it covers. Static, so coverage is known before it runs.
    fn topics(&self) -> &'static [SystemTopic];
    fn actions(&self) -> &'static [ActionKind];

    /// State as it changes. A polled broker is an interval mapped over a
    /// read; a D-Bus broker is its signal stream. One shape, one driver.
    fn stream(&mut self) -> impl Stream<Item = Result<StatePatch, Self::Error>> + Send;

    async fn act(&mut self, action: &Action) -> Result<(), Self::Error>;
}
```

Four things follow.

**`SetVolume` has a home.** One PipeWire connection reports the volume and
sets it. The alternative is two components that each discover, connect, and
reconnect to the same daemon.

**Coverage is a test, not a habit.** The union of every registered broker's
`topics()` and `actions()` is compared against `SystemTopic::ALL` and
`ActionKind::ALL`. What is uncovered is listed explicitly with a reason, or
the test fails. This is the check the table above is missing.

**One broker per subsystem, not per topic.** UPower projects `battery` _and_
`peripherals`; Hyprland projects `display`, `workspaces` and `window`.
`StatePatch` already carries `repeated StateTopic`, so the fan-out is free —
and there is no connection-sharing layer to build, because a broker _is_ the
connection.

**The authorization order is structural.** Capability check, then route to
the broker claiming that kind, then `UNIMPLEMENTED` if none does. A missing
handler cannot read as a grant, because the grant is decided before the route
is looked up.

Streams rather than a `poll` method because UPower, NetworkManager, BlueZ and
MPRIS are signal-driven; polling them on a two-second interval is both wasteful
and late. A polled broker is `IntervalStream` mapped over a read, so both
cadences are one shape and the daemon has one driver.

`RunCommand` and `LaunchApp` are a `Process` broker. `InvokeUnit` stays
daemon-internal: routing between units is not brokering.

## Views and placement

A panel is not a surface kind.

What separates a bar widget from a panel is size, visibility, summoning and
focus. Every one of those is _where it is drawn_, not _what it is_ — and
placement is already a document concept. So a panel is another placement of
the same thing: a second of the unit's view surfaces, drawn as a popout
anchored to the first.

```rust
let wifi = Modules::plain_widget("wifi", wifi::UNIT);
Modules::panel(Modules::surface(wifi, "indicator"), "details")
```

One unit, two view surfaces, addressed by `surface_id` the way the protocol
already addresses them. `RenderWidget { surface_id, module_id, config }` is
unchanged, and the reconciler renders the two separately because they are
separate views of one placement.

**Anchored, not free-standing.** An earlier draft had a panel that could
float, summoned from nowhere, with anchoring as an optional field. The host
shell does not have that primitive: its popup is a bar item that pops out,
owning focus and dismissal against the bar it hangs from. A surface summoned
from nowhere is a different kind of window — a menu, an overlay — and would be
a different placement rather than this one with a field left empty.

**Whether it is open is the shell's.** Opening a panel is what the user is
doing, so no unit is told and none has to be asked; there is no action for it
and nothing the daemon has to push to the shell. What is _in_ the panel is the
unit's, and a unit that renders nothing for that surface has a panel with
nothing in it rather than a panel that will not open.

The protocol cost of all of this is zero. What it cost the document is two
strings on a placement, because a unit with two view surfaces can no longer be
addressed by name alone.

`SURFACE_KIND_WIDGET` should read `VIEW`, since a view no longer implies a
bar. Cosmetic, and only worth doing while the protocol is young.

## Interaction

> The shell owns what the user is doing. The daemon owns what is true. A unit
> owns what it derives.

A half-typed passphrase is not a fact about the machine; the connected SSID
is. Scroll offset, expanded row, selected index and caret position are all
_doing_, all the shell's, and none of them are on the wire.

Three consequences.

**Events are semantic.** `on_submit(value)`, `on_change(value)`,
`on_activate(id)` — never `on_key` or `on_mouse_move`. This is the rule that
keeps the protocol from drifting into a remote display server. One message
carries all of them:

```protobuf
message Bind {
  string command = 1;
  repeated Value args = 2;
}
```

The shell appends the event's own value as the last argument, so
`Slider::on_change` reaches the unit's command with `args ++ [0.42]`. Today a
press sends `args: []`, which is why a Wi-Fi list would need one registered
command per network. This one message is the whole difference between a
display and a control.

**Inputs are uncontrolled by default.** A field holds its own buffer and
emits on submit; a toggle flips optimistically and emits. The alternative puts
a Unix socket round trip in the path of every keystroke. The escape hatch is
HTML's: a `value` prop, when present, wins on every render. Leave room for it;
build the uncontrolled path.

**Keyboard navigation is a property of the `list` node.** Selection is doing,
so arrows move it in the shell and the unit hears only `on_activate(id)`. The
feature that most separates a real panel from a dropdown costs no wire surface
and is written once rather than once per panel.

## Presence

A desktop has no battery. An adapter is unplugged. NetworkManager dies. Two
wire forms, no schema addition — proto3's unset `oneof` already means "nothing
to report":

| the daemon has        | wire                   | a unit sees                |
| --------------------- | ---------------------- | -------------------------- |
| never said            | never published        | not known; hold the render |
| said there is nothing | published, value unset | no reading                 |
| said a reading        | published with value   | a reading                  |

The render gate is "every declared topic is _known_", and a topic is known
once the daemon has spoken about it — including to say there is nothing to
report. Without that a battery widget on a desktop waits forever, which is
what shipped.

**A unit branches on having a reading, not on why it has none.** No battery
in the machine and a broker that just died are one thing to a widget: draw
the no-reading branch. They are two things to an operator, so the difference
lives where an operator looks — the driver already holds each broker's name,
its backoff attempt, and its last error, and that is what `omega status`
reports.

An earlier draft of this section had a third unit-facing state, `Unknown`,
which a failing broker retracted its topics to. Implementing it showed that
inverts the failure: `Unknown` means _hold the render_, which is right
exactly once, before anything is known. Retracting a live topic to it does
not degrade a widget that was drawing 80% — it blocks the widget and takes it
off the bar. The states are not symmetric, and "no reading right now" is the
honest report for every reason there is no reading.

The lesson is one `BarWidget.qml` already carries for the socket: a peer that
goes away leaves `connected` reading true, and a widget that trusts it
freezes on a reading nobody is taking. Absence has to be said out loud. It
just must not be said by going quiet.

## Time

There is no time topic, so a clock is currently inexpressible. The naive fix
wakes every unit every second forever.

Because the derive builds the manifest from field _types_, granularity has to
live in the type — which is where it belongs anyway:

```rust
#[derive(omega::View)]
struct Clock {
    now: Time<Minutes>,   // the manifest says minutes; the daemon ticks accordingly
}
```

The cost is visible at the declaration. A bar of `Time<Minutes>` widgets
costs one wake a minute, not five a second.

Schedules are a separate thing: document-level actions on a cron, a
reconciler domain, not a topic. `Schedule` is in `document.proto` today with
nothing converging it.

## Long actions

"Connect to this network" has a lifecycle: connecting, then failed with a
reason, and both have to appear in the view.

This needs no new abstraction. A unit's command and its view are separate
structs in one process that compose _through state_, which is what `Own<T>`
and the `unit.<name>.<key>` keyspace already are. The command writes
`Own<Connecting>`; the view reads it and re-renders, over a local socket.

A task system with ids and progress frames would be a second composition
mechanism next to the one that already works.

## Roads not taken

Each of these was designed far enough to cost something before being dropped.

**Keyed or delta state replication.** Six topics are collections — Wi-Fi
networks, Bluetooth devices, MPRIS players, workspaces, tray items,
notifications — and the first instinct is a keyed set with add/remove/update
deltas. Forty access points is four kilobytes every five seconds on a Unix
socket. Last-value-wins over a `repeated` field is correct, and deltas buy
nothing while costing resync, ordering, and a second replication path.

One replication strategy, three _message_ shapes: scalar, list, and fixed
window. The window is how sparklines work — the daemon holds the ring buffer
and publishes sixty floats, so units stay stateless and a graph survives a
reload.

**Incremental view diffing on the wire.** Whole trees, dropped when
identical. A forty-row panel at 2 Hz is not a bandwidth problem. This does
make keyed lists non-negotiable in the _renderer_: `assign_keys` already runs
in Rust and `Props.children` ignores it, so a list reordering by signal
strength rebuilds every delegate and drops focus and scroll.

**Raw input events.** Semantic events only. The line that stops this becoming
X11.

**Third-party brokers.** The daemon is the trust boundary; no user code runs
in its address space. Third-party data is a unit writing its own keyspace.
This is also what keeps the broker list finite — weather, calendar, mail,
package updates, Docker and git status are units, not brokers.

**Generating the QML prop readers.** The pinning test already exists and
works. A build-time generator is more machinery than the drift justifies.

**A layout engine.** Quickshell has one.

## Structure

```
omega-proto ──┬── omega-derive ──┐
              ├───────────────────┴── omega
              ├── omega-document
              ├── omega-brokers ...................... new
              └── omega-daemon ── omega-renderer ..... new
                        └──────── omega-cli
```

**`omega-brokers`** — one file per subsystem, which is also the list of what
serves each action:

```
broker.rs           the trait; Brokers::all()
upower.rs           battery, peripherals
network_manager.rs  network, vpn
bluez.rs            bluetooth
pipewire.rs         audio          + SetVolume, MediaKey
logind.rs           session, idle  + Lock, Sleep, Hibernate, Reboot, Shutdown
hyprland.rs         display, workspaces, window + workspace and window actions
mpris.rs            media          + MediaKey
backlight.rs        backlight      + SetBacklight
notify.rs           notifications  + Notify
procfs.rs           system (cpu, memory, disk, net, thermal)
clock.rs            time
process.rs                         + RunCommand, LaunchApp
dbus.rs             connection helpers
```

It depends on `omega-proto` and its subsystem clients and nothing else. The
daemon is the crate a reviewer has to read end to end; four thousand lines of
D-Bus marshalling do not belong in it.

**`omega-daemon`** — `sources/` and `source.rs` are replaced by `broker.rs`:
one driver that pumps streams into the `Hub`, routes actions after the
capability check, supervises with backoff, and retracts topics on failure.
`reconcile/panels.rs` joins `bars.rs`. The current `add_source` drops its
`JoinHandle` and never selects on `Shutdown`; the driver is where that is
fixed.

**`omega`** — `source/` disappears, because a unit does not know brokers
exist. It has readings.

```
state/     mod.rs (Own, Watch, Topic, reads!) and one file per topic handle
ui/        mod.rs node.rs style.rs bind.rs
           text.rs layout.rs control.rs display.rs list.rs
effect/    unchanged
surface.rs View, Command, Reaction
```

One file per node _family_, not per node: twenty files of fifty lines is
worse than five of two hundred and fifty. The `styled!` macro stays rather
than becoming a `Style` trait — inherent methods need no import at the call
site, and the typed path has to be the short one.

**`omega-renderer`** — the QML tree, `Renderer`, install and link.
`include_str!` cannot reach outside a crate directory, so the only question is
which crate; the tree going from four files to twenty-five answers it. It also
ends a three-way collision: `omega/src/ui/` is the view tree,
`omega-cli/src/ui/` is the terminal UI, `omega-cli/src/shell.rs` is the
Quickshell renderer, and `omega-daemon/src/shell.rs` is the shell socket.

```
shell/plugins/omega.view/
  manifest.json      kinds: ["bar-widget", "panel"]
  BarWidget.qml      Panel.qml        two hosts
  Connection.qml     socket, filter, reconnect — shared
  ViewNode.qml       dispatch only: type to delegate url
  nodes/*.qml        one per node kind
  Props.js  Icons.js
```

`Connection.qml` is what makes the second host cheap: everything below the
layout in `BarWidget.qml` is protocol, and a panel needs all of it.

Theme names resolve through `qs.Commons.Color` rather than the One Dark
literals hardcoded in `ViewNode.colorOf` — in `ViewNode` itself, not a
`Theme.js`, because a `.pragma library` cannot reach a QML singleton.

Keyed reconciliation belongs with the first stateful node rather than here. A
`Repeater` over a plain array has no keying to turn on — keeping delegates
across a reorder means reconciling a `ListModel` by key — and there is nothing
to keep until a child holds something: a slider's drag, a toggle's optimistic
flip. Text does not care what rebuilds it.

**Schema** — `state.proto` keeps the envelope and the `oneof`, because the
oneof _is_ the ontology: what a topic can carry is one list, and splitting
that would split the thing the schema exists to fix. Payloads move to
`state/*.proto`, one file per domain, under the same `package omega` — prost
consolidates by package, so every generated type still lands in one Rust
module and no consumer changes.

Files exist where there are payloads to put in them: `power`, `network`,
`media`, `display`, `units` today, and `session`, `system` and `time` when
the topics that belong in them land. An empty file named after a plan is a
plan two people will disagree about.

The oneof's typed arms occupy 10..19 with `generic` at 20, which does not fit
twenty topics: renumbering `generic` out of the way is a one-line edit now and
a migration later.

## Order

1. The `topics!` and `actions!` tables in `omega-proto`, and the coverage
   test. First, because its value is preventing the drift the later steps
   would cause — and it fails immediately on the holes that exist today.
2. Renumber the `StateTopic` oneof.
3. Extract `omega-brokers`. Define `Broker`, move the battery source into
   `upower.rs`, write the driver with retraction, backoff and `Shutdown`.
   Then `pipewire.rs`, which is the first broker to prove the shape by making
   `SetVolume` real.
4. `Bind` and arguments on the wire. Everything interactive waits on it.
5. Extract `omega-renderer`. `Connection.qml`, `nodes/`, theme.
6. Split `omega/src/ui/`; add the control nodes, and the keyed
   reconciliation a stateful child is what needs.
7. Split `state.proto`. Topics start landing.
8. A panel surface on a bar placement, rendered by the second host in
   `BarWidget.qml`, and the controls that were waiting for focus and scroll:
   `field`, `select`, `list`.

Steps 1 and 2 are hours and pay down debt. 3 and 5 are the two real
refactors, both behaviour-preserving and checkable against the existing
suite. Nothing after step 4 changes the protocol.

## The test of all of it

Rebuild `omarchy.network` as an Omega unit. It is 1,970 lines of QML and 380
of JavaScript. If it does not come out at a few hundred lines of Rust, the
typed path is not shorter than the untyped one and the SDK is decoration.

If `SetVolume`, keyboard navigation, or the connect-and-fail lifecycle each
needed a special case to get there, one of the abstractions above is wrong.

### What happened

`crates/omega/examples/wifi.rs` — **218 lines, 153 of them not comment or
blank**, for the indicator, the panel, the network picker, and three
commands. Against 1,970 lines of QML and 380 of JavaScript.

Nothing needed a special case: the panel is a second view surface, the picker
is a keyed list that owns its own cursor, the row bindings carry their own
arguments, the passphrase field appends what was typed, and the shell owns
every piece of interaction state. The abstractions held.

### What it could not express, at first

The first pass could not show a list of networks to pick from. Two holes, and
the second was the one that mattered:

1. **`NetworkState` describes the connection the machine has**, not the ones
   it could have. A scan result has nowhere to live. It wants a topic of its
   own rather than another field — forty access points whose signal jitters
   would bump the `network` revision and wake every indicator that only
   wanted the SSID.
2. **A unit cannot work around that in its own keyspace.** `Value` carries
   `ListValue` and `MapValue`, but `IntoValue`/`FromValue` are implemented for
   scalars and `Values` only. So a unit cannot hold a list of anything.

The second is the real defect. The first is one topic that has not been
written yet; the second meant _no_ unit could publish a collection, which
quietly closed the escape hatch the whole composition story rests on — "a
plugin can publish a fact and another can draw it" only held for facts that
are one scalar deep.

**Closed.** `Vec<T>` is a value where `T` is, and `#[derive(Config)]` makes a
struct a value in its own right, so a struct can be an element of a list or a
field of another struct. Reading a list is all-or-nothing — an element nobody
can read makes the list unreadable rather than making it shorter, because a
list that looks complete and is not is the worse answer. Reading a _field_
stays total, so a malformed list takes the type's default and one bad
document does not fail the rest.

The first is closed too: `wifi` is a topic of its own, filled by the
NetworkManager broker, which folds a network's several radios into one row and
drops the hidden ones. Both fell out of writing the thing rather than out of
designing it, which is the whole argument for writing it.

### What it cost to write

Two papercuts, both fixed:

- `Column::new()` returns a `Stack`, so a helper that builds one is written
  `fn join() -> Stack`. `Column` names a direction, not a type, and the
  compiler error says something else — so the rustdoc on both directions now
  says it instead.
- `Answer` had `value`, `done` and `refused` but no `From<&str>`, so the
  obvious `Answer::from("connecting")` did not compile. It does now.
