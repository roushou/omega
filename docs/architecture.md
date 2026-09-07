# Architecture

The design and rationale.

## Summary

Omega turns the machine into a project, not a pile of dotfiles. One git repo,
one language, an SDK that hides Omarchy and Hyprland internals. Configuration,
plugins, and small apps all live in it. Editing it feels like Neovim config —
save and it's true — but it's typed, testable, and reproducible on a new laptop.

The ancestor is Emacs: a uniform API, introspectable, no boundary between user
and developer. The failures to avoid are Emacs's too — no isolation, no types,
one language as a cage. Omega keeps the uniform surface, adds a real type
system, and uses the process boundary so the language stays a choice rather
than a cage.

## Goals

1. **The typed path is shorter than the bash path.** For the fifty most common
   operations, the SDK must be at least as terse as the shell one-liner it
   replaces. If `run("brightnessctl set 5%+")` is one line and the SDK is
   fifteen, the SDK becomes decoration on a pile of shell scripts. This is the
   whole adoption question.
2. **Blast radius of one.** Any edit touches exactly one thing. A keybind
   change does not restart the bar. This is the property to protect above all
   others.
3. **Scales down.** A fresh install is five lines in one file, growing into the
   full tree the way `init.lua` grows into `lua/`.
4. **A config that doesn't compile never takes the desktop down.**

## Non-goals

- **Not Nix.** Correct, powerful, and cold enough that only the committed 2%
  get in.
- **Not a reimplementation** of NetworkManager, PipeWire, UPower, or Hyprland.
  Omega is a broker over them, never a replacement.
- **Not a full OS.** The tractable version is a programmable layer on top of
  Omarchy, reusing its shell and compositor.

## Architecture

### Two planes

The **configuration plane** compiles to a state document that a reconciler
converges the machine toward — plan and diff, not a script. The **runtime
plane** is units running as supervised processes.

The split solves the bootstrap paradox: a committed state document boots a
toolchain-free machine. It also guarantees that a config which doesn't compile
can never take the desktop down — the last good state document keeps running.

### The daemon

A Rust daemon is the trust boundary, state owner, supervisor, and reconciler.
It brokers over NetworkManager, PipeWire, UPower, and Hyprland; it never
reimplements them. No user code ever runs in its address space.

### State

The daemon owns all state; units are stateless. State is replicated into
units, so `omega.battery.level` is a local memory read, not a round-trip.
Unit-owned data goes to a per-unit keyspace in the daemon (`unit.<name>.<key>`,
writable by that unit alone and only with `CAPABILITY_STATE_WRITE`).

Replication is last-value-wins on the _value_: a source that polls an
unchanged reading produces no revision and wakes nobody. Each unit receives
only the topics its manifest declares, narrowed at runtime by `Subscribe`; a
subscriber that falls behind is resynchronized with a full snapshot rather
than left silently stale.

The payoff is that hot reload is kill-and-restart — ~10ms, invisible — and
crash recovery is the same code path. A compiled unit with a 2s rebuild feels
the same as an interpreted one. **Liveness is a property of the process model,
not the language.**

### The protocol is the boundary

Units only ever send frames. No native modules, no FFI. The protocol is what
makes the runtime swappable and what keeps the language from becoming a cage.

### One unit type, many surfaces

Not widget/plugin/app/script/service — one definition declaring which surfaces
it exposes: bar, panel, command, keybind, schedule, agent tool. That is what
"compose your own system" means concretely.

## The language

**Rust everywhere. TOML for data.**

The config is one Rust workspace. TOML covers the declarative ninety percent —
keybinds, widgets, settings, bar layout — validated against the schema with no
compiler involved. Rust covers the remaining ten percent — event handlers,
derived state, custom units, small apps.

The decision rests on one principle: **the config is a contract with the
machine, and the compiler is the enforcement.** When your config compiles, it's
true. A keybind to a nonexistent action, an unhandled event, a malformed
surface — these are compile errors, not runtime surprises.

This works because the ontology is expressed exactly once. The protobuf schema
generates Rust types. The daemon uses them. TOML is validated against them.
The config crate compiles against them. `Keybind` in the daemon and `Keybind`
in your config are the same type. There is no translation layer, no codegen
glue, no boundary where a lie can live.

The cost of Rust is the compile loop, and it is bounded: the ninety percent
you touch daily is TOML and needs no compiler, while the ten percent that is
Rust is written once and stabilized. Two seconds of rebuild is a fair price for
code that must be correct, and the audience — Arch and Hyprland users — already
lives in a build-from-source culture. xmonad is the precedent: it works, and
the audience accepts the on-ramp.

## Protocol and schema

protobuf as the IDL, no gRPC. Length-prefixed frames over a Unix socket,
`SO_PEERCRED` for unforgeable identity, canonical JSON mapping for a readable
debug mode.

protobuf gives you messages, not a protocol: multiplexed streams, bounded
queues, last-value-wins coalescing on state topics, request ids for idempotent
retry, and handshake version negotiation all still have to be designed.
**Agonize over the schema, not the codec** — encoding is swappable, ontology is
permanent. Model a desktop, not Hyprland.

The schema is the single source of truth. Generated Rust types are shared by
the daemon and every user crate.

## Manifests

Metadata is a statically-analyzable export in the unit source, extracted at
build into a `unit.toml` the daemon reads. Capabilities cannot be
self-declared at runtime. Hand-written TOML exists only for code-free
declarative units.

## Runtime tiers

1. **Declarative** — TOML, zero processes. Covers most widgets. Built first; it
   is the biggest memory win.
2. **Native** — compiled Rust units, run via `execve`. The code tier; nearly
   free to support since units are already processes.
3. **WASM** — future, only if a registry needs untrusted distribution.

No JS runtimes, no Lua. The protocol boundary keeps the door open for other
tiers later without committing to them now.

## UI

Declarative view trees rendered by the shell, designed as a reconciler target.
A first-party plugin bridges into Quattro, dynamically instantiating QML so
units inherit Omarchy theming for free. Normal Wayland toplevels need none of
this.

A shm/dma-buf pixel escape hatch exists for the rare widget that needs real
drawing. An offscreen test renderer is built alongside the QML one to keep the
abstraction honest — it doubles as the compositor-free test harness.

## Project structure

```
~/.config/omega/           # source of truth; build output never lands here
  Cargo.toml  Cargo.lock   # workspace — members under units/ are the units
  system/                  # → state document; one entry point, no side effects
  policy/                  # event handlers — imperative, compiled to units
  units/                   # workspace of independent packages
  hosts/                   # per-machine composition, no branches
  lib/  tests/  secrets/   # sops/age encrypted
```

Build output goes to `~/.local/state/omega/`, caches to `~/.cache/omega/`.

`hosts/` exists from day one — it is the difference between nicer dotfiles and
infrastructure.

## Build and bootstrap

The config dir holds source only. `omega build` compiles the workspace:
`system/` into the state document, `policy/` and `units/` into unit binaries,
manifests into `unit.toml`. Output lands in `~/.local/state/omega/`.

**Surfaces:** a widget surface renders (pulled once per instance so a unit
learns its configuration, pushed thereafter); a command surface is called —
by `omega run`, or by another unit that holds `CAPABILITY_SPAWN`. Both
directions of the protocol are now used: the daemon invokes units as well as
serving them, with stream ids split by parity so each side answers only what
it asked.

**Events and actions:** events are derived from state transitions in the
daemon (the AC events and battery thresholds come from `battery.charging` and
`battery.level`, so reality and its announcements cannot disagree) and
delivered only to units whose manifests declare them. Actions are authorized
per action kind — `RunCommand` costs `CAPABILITY_SPAWN`, `Lock` costs
`CAPABILITY_SYSTEM_CONTROL` — with the check ahead of the implementation, so
an action the daemon cannot yet perform is never a grant. `RunCommand` is the
one action implemented; the rest answer `UNIMPLEMENTED`.

**Implemented today:** `system/` is a crate whose one entry point emits a
`StateDocument` as canonical protobuf JSON; `omega build` runs it, validates
that every unit it names was built, and stages `document.json` beside
`units.toml`. The daemon reconciles two domains — which units run, and the
session environment — planning before applying. `policy/` and `hosts/` are not
built yet; per-host composition is expressible today by branching on
`Host::name()` inside `system/`.

The state document is committed. A new machine boots from it directly, with no
toolchain — the bootstrap paradox is resolved by separating what the machine
runs (the document and the binaries) from what the user edits (the source).
cargo is a build-time concern on the dev machine, never a boot-time
requirement.

## The rules that decide whether this lives

1. **The typed path must be shorter than the bash path** for the fifty most
   common operations.
2. **Don't become Nix.**
3. **Scope honestly.** The full version is an OS platform and a multi-year
   effort. The tractable version is a programmable layer on top of Omarchy.

## First thing to build

One vertical slice: three protobuf messages, a daemon with one state source and
one supervisor, the Rust SDK, the QML bridge plugin, and a battery widget in
the bar.

Save the file, watch it change, confirm nothing else on the desktop flinches.
That single loop validates the protocol, the state mirror, the supervisor, the
bridge, and the reload story at once.

Then build the four other unit shapes — a keybind mutating persistent state, a
small app, a system setting, a background service — and judge honestly whether
each is nicer than the QML-and-bash version. Where it's more awkward, the
ontology is wrong, and that is information no amount of design work can give
you.
