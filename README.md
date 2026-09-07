# Omega

The desktop configuration daemon: your desktop described as a Rust program.

A _unit_ is a small Rust crate that declares what it needs by holding it — a
`Battery`, a `Notify` — and draws a view. Omega compiles your configuration,
supervises the units it produced, owns the state they read, and reconciles the
running machine against the document you wrote.

## Requirements

Omega targets one desktop. It is not portable software, and the parts below
are not interchangeable.

- **Linux with a systemd user manager.** `omega daemon install` writes a
  `systemd --user` unit bound to `graphical-session.target`, so the daemon
  starts and stops with your session.
- **A Rust toolchain, 1.85 or newer.** Not only to install omega: `omega
  build` compiles _your_ configuration with cargo every time it changes, so
  the toolchain is a runtime requirement, not a build-time one. The workspace
  is edition 2024.
- **[Omarchy](https://omarchy.org)**, to draw anything. Its Quickshell-based
  shell is the only host omega knows how to install a renderer into, and
  widgets are drawn as Nerd Font glyphs by the bar's own font.

Omarchy is required for _drawing_, not for running. On a machine without it
the daemon still starts and units still run — `omega init` says so rather than
failing — but nothing appears on screen.

## Install

Not yet published. From a checkout:

```sh
cargo install --path crates/omega-cli
```

## Quickstart

```sh
omega init          # found the config, install the daemon, install the renderer
omega new battery   # scaffold a unit
```

`omega new` prints the line that puts your unit in a bar. Paste it into
`~/.config/omega/system/src/main.rs`:

```rust
omega_document::Modules::widget("battery", battery::UNIT, &battery::Settings { low: 15 })
```

Then:

```sh
omega build
```

`omega status` shows what is running, `omega logs <unit>` shows why it is not,
and `omega dev <unit>` takes over a supervised unit so you can run it in your
own terminal against the real daemon.

## License

[MIT](./LICENSE)
