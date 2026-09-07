# Omega

The desktop configuration daemon: your desktop described as a Rust program.

A _unit_ is a Rust crate that declares what it needs by holding it — a
`Battery` field is the battery topic and the permission to read it — and draws
a view. Omega compiles your configuration, supervises the units it built, owns
the state they read, and reconciles the machine against the document.

## Requirements

- **Linux with a systemd user manager.** `omega daemon install` writes a
  `systemd --user` unit bound to `graphical-session.target`.
- **Rust 1.85+.** A runtime requirement, not just a build one: `omega build`
  compiles your configuration with cargo whenever it changes.
- **[Omarchy](https://omarchy.org)** to draw. Its Quickshell shell is the only
  renderer host. Without it the daemon still runs and units still execute —
  there is simply nowhere to draw.

## Install

Not yet published.

```sh
cargo install --path crates/omega-cli
```

## Quickstart

```sh
omega init          # config workspace, daemon service, renderer
omega new battery   # scaffold a unit
```

`omega new` prints the placement line. Paste it into
`~/.config/omega/system/src/main.rs`:

```rust
omega_document::Modules::widget("battery", battery::UNIT, &battery::Settings { low: 15 })
```

```sh
omega build
```

## Commands

|                              |                                                          |
| ---------------------------- | -------------------------------------------------------- |
| `omega build`                | compile the config, stage the document and unit binaries |
| `omega status`               | what is running, from the observation socket             |
| `omega logs <unit>`          | a unit's own output (`~/.cache/omega/logs/`)             |
| `omega dev <unit>`           | take over a supervised unit in your terminal             |
| `omega run <unit> <command>` | invoke a command surface                                 |
| `omega clean`                | remove the build cache (`--logs` for the logs too)       |
| `omega link [path]`          | build against an omega checkout instead of the registry  |

## Documentation

- [Architecture](docs/architecture.md)
- [Contributing](AGENTS.md)

## License

[MIT](./LICENSE)
