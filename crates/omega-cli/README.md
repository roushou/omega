# Omega CLI

The `omega` command builds and runs a desktop configuration written in Rust.
It creates plugin projects, manages the daemon and renderer, and lets you inspect
plugins or run them against the live desktop during development.

Omega requires Linux with a systemd user manager and Rust 1.88 or newer. Widgets
are displayed through Omarchy's Quickshell shell. Install this package with
`cargo install omega-cli`; the [project README](https://github.com/roushou/omega)
describes source installation for changes ahead of the published releases.

## Using Omega

```sh
omega init
omega new audio
```

`omega init` creates a Rust workspace in `~/.config/omega`, installs the renderer,
and starts the daemon as a user service. `omega new` scaffolds a plugin and prints
its placement declaration for `system/src/main.rs`. The
[plugin SDK](https://crates.io/crates/omega-rs) provides the types your plugin uses.

```sh
omega build
omega status
omega logs audio
```

A build compiles the configuration and plugins, then stages them for the daemon
to apply. `omega build --watch --debug` rebuilds as you edit. `omega dev audio`
temporarily replaces the supervised plugin with a development process in your
terminal.

`omega daemon` runs the daemon in the foreground. `omega daemon install` makes it
a user service, and `omega daemon status` checks the installed service. Use
`omega link /path/to/omega` to build your configuration against a source checkout,
or `omega link --published` to return to registry dependencies.

Run `omega --help` or `omega <command> --help` for the full command reference.

[Project](https://github.com/roushou/omega) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.
