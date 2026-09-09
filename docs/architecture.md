# Architecture

## Two planes

The **configuration plane** is a Rust program that computes a state document
with no side effects. The **runtime plane** is units running as supervised
processes, converged toward that document.

The split resolves the bootstrap paradox: the committed document boots a
machine with no toolchain. It also means a config that does not compile cannot
take the desktop down — the last good document keeps running. cargo is a
build-time concern on the developer's machine, never a boot-time requirement.

## The daemon

One Rust daemon is the trust boundary, state owner, supervisor, and
reconciler. It brokers over NetworkManager, PipeWire, UPower and Hyprland and
never reimplements them. No user code runs in its address space.

Units only ever send frames. No native modules, no FFI. That boundary is what
makes the runtime swappable and keeps the language from becoming a cage.

## State

The daemon owns all state; units are stateless. State is replicated into
units, so a reading is a local memory read rather than a round-trip.

Replication is last-value-wins on the _value_: a source that polls an
unchanged reading produces no revision and wakes nobody. A unit receives only
the topics its manifest declares, narrowed at runtime by `Subscribe`. A
subscriber that falls behind is resynchronized with a full snapshot rather
than left silently stale.

Units also own state. Every unit has the keyspace `unit.<name>.<key>`,
writable by that unit alone with `CAPABILITY_STATE_WRITE` and readable by any
unit that names it. This is how units compose without knowing each other at
build time.

Hot reload is therefore kill-and-restart, and crash recovery is the same code
path. Liveness is a property of the process model, not the language.

## Protocol

protobuf as the IDL, no gRPC. Length-prefixed frames over a Unix socket,
`SO_PEERCRED` for unforgeable identity, canonical JSON for the observation
socket and debug output.

protobuf gives messages, not a protocol. Multiplexed streams, bounded queues,
last-value-wins coalescing, request ids, and handshake version negotiation are
designed on top. Encoding is swappable; ontology is permanent — so the schema
is where the care goes. It models a desktop, not Hyprland.

`crates/omega-proto/schema/` is the single source of truth. The generated Rust
types are shared by the daemon, the CLI, and every user crate: `Keybind` in
the daemon and `Keybind` in a config are the same type, with no translation
layer where a lie can live.

## Trust

Three kinds of peer on the control socket:

| peer        | identity                        | may do                          |
| ----------- | ------------------------------- | ------------------------------- |
| unit        | spawn token + `SO_PEERCRED` pid | what its manifest declares      |
| operator    | the daemon's own uid            | lifecycle only, no capabilities |
| anyone else | —                               | refused                         |

Grants are read from the daemon's copy of the manifest, never from a frame.
Capabilities cannot be self-declared at runtime: a unit's manifest is
extracted at build time from the binary itself.

## Units

One unit type, many surfaces — not widget/plugin/app/script/service. A unit
declares which surfaces it exposes, and a surface is either rendered or
called:

- a **widget** surface renders — pulled once per instance so the unit learns
  its configuration, pushed thereafter;
- a **command** surface is invoked, by `omega run` or by another unit holding
  `CAPABILITY_SPAWN`.

Both directions of the protocol are used, with stream ids split by parity so
each side answers only what it asked.

Events are derived from state transitions in the daemon, not announced by each
source, so reality and its announcements cannot disagree. They are delivered
only to units whose manifests declare them, and are not stored.

Actions are authorized per kind — `RunCommand` costs `CAPABILITY_SPAWN`,
`Lock` costs `CAPABILITY_SYSTEM_CONTROL` — with the check ahead of the
implementation, so an action the daemon cannot yet perform is never a grant.

## Reconciliation

Providers `plan` purely against the document and only then `apply`, so a
change can be shown before it happens. Convergence runs in one task, one pass
at a time, and is keyed per entity id — which is what keeps the blast radius
of an edit to the thing it names.

Four domains today:

| domain        | converges                                                                         |
| ------------- | --------------------------------------------------------------------------------- |
| `config`      | unit settings; runs first, so a unit never spawns before its settings are on file |
| `units`       | which units run                                                                   |
| `bars`        | surface instances placed in bars                                                  |
| `environment` | the session environment                                                           |

## Layout

```
~/.config/omega/          source only
  Cargo.toml Cargo.lock   workspace; members are system/ and units/*
  system/                 → document.json, one entry point, no side effects
  units/                  independent unit packages
  target/                 cargo's, at cargo's default path; gitignored

~/.local/state/omega/     build output
  document.json  units.toml
  units/<name>/           the binary and its unit.toml

~/.cache/omega/logs/      <unit>.log
```

Logs live in the cache rather than the state dir because a build replaces the
state dir, and the log explaining the last crash has to outlive it.

`omega build` compiles the workspace, runs `system/` to emit the document,
validates that every unit it names was built, and stages the result.

## UI

Units publish declarative view trees; the shell renders them. A first-party
Quickshell plugin instantiates QML per node, so units inherit Omarchy theming
without knowing about it. Normal Wayland toplevels need none of this.

The renderer ships inside the `omega` binary, so the renderer installed is by
construction the one the daemon speaks to.

## Design rules

1. **The typed path must be shorter than the bash path** for the fifty most
   common operations. If `run("brightnessctl set 5%+")` is one line and the
   SDK is fifteen, the SDK is decoration on a pile of shell scripts.
2. **Blast radius of one.** Any edit touches exactly one thing; a keybind
   change does not restart the bar.
3. **Scales down.** A fresh install is a few lines in one file.
4. **A config that does not compile never takes the desktop down.**

Non-goals: not Nix; not a reimplementation of NetworkManager, PipeWire, UPower
or Hyprland; not a full OS. Omega is a programmable layer on top of Omarchy,
reusing its shell and compositor.

## Not built

- `policy/` and `hosts/` directories. Per-host composition is expressible
  today by branching on `Host::name()` inside `system/`.
- A declarative TOML tier for units. Every unit is a compiled Rust crate.
- A WASM tier. Only worth it if a registry ever needs untrusted distribution.
- `SetSetting` and `ToggleSetting`, the only two actions nothing serves. They
  act on the state document's own settings, which no subsystem owns, so they
  wait on something to own them rather than on a broker.

Every other topic and action is brokered. That is a test rather than a claim:
`omega-brokers/tests/coverage.rs` compares the registered brokers against
`SystemTopic::ALL` and `ActionKind::ALL`, and `omega`'s `state::tests` asks
the mirror question — whether a unit can name what the daemon publishes. A
hole is a line in one of those files or a failing build, which is what keeps
this section from rotting again.
