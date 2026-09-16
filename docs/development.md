# Developing Omega

Install Rust through rustup with the `rustfmt` and `clippy` components. Omega
requires Rust 1.88 or newer. You also need GLib/GIO development headers, Qt 6 QML
tools and runtime modules, and Python 3 for the release-tooling tests. The package
lists for Ubuntu are in [CI](../.github/workflows/ci.yml).

## Tools and checks

[mise](https://mise.jdx.dev/) pins dprint, cargo-deny, cargo-shear, and actionlint.
It leaves your Rust toolchain selection to rustup. From the repository root:

```sh
mise trust
mise install
mise run check
```

CI runs the same task groups. Cargo commands and the existing shell scripts also
work directly; mise is optional for development and is not an Omega runtime dependency.

| Task                 | Checks                                                   |
| -------------------- | -------------------------------------------------------- |
| `check:rust`         | Rust formatting, Clippy, build, and workspace tests      |
| `check:renderer`     | QML lint and headless interaction tests                  |
| `check:format`       | Markdown, JSON, and TOML formatting                      |
| `check:dependencies` | Unused dependencies, advisories, licenses, and sources   |
| `check:workflows`    | GitHub Actions lint and release-tooling tests            |
| `check:msrv`         | Compatibility with Rust 1.88.0                           |
| `test:e2e`           | Ignored CLI tests that compile scaffolded configurations |

`check` runs the first five groups. Run the more expensive CLI tests when changing
scaffolding or build activation. To run the minimum-version check, first install
its toolchain with `rustup toolchain install 1.88.0 --profile minimal`.

Use `mise run regenerate` after changing renderer property, keyboard, or icon
definitions. It sets `OMEGA_REGENERATE=1` only for the generation commands. Review
the generated diff, then run the normal checks without that flag.

Machine-specific settings belong in gitignored `mise.local.toml`. For example:

```toml
[tasks."check:renderer".env]
QMLLINT = "/usr/lib/qt6/bin/qmllint"
QMLTESTRUNNER = "/usr/lib/qt6/bin/qmltestrunner"
```

## Development daemon

To create a separate development configuration and start a foreground daemon:

```sh
mise run dev:init
mise run dev:daemon
```

In another terminal, `mise run dev:status` inspects that daemon. Stop the foreground
process with Ctrl+C.

These tasks share explicit config, state, cache, shell-configuration, and socket
paths under `target/dev`. `dev:init` uses `omega init --bare`, so it does not install
a service or renderer. Entering the repository does not export these overrides,
and ordinary `omega` commands still use your normal environment.

This separates Omega's files and sockets, not the desktop session. The development
daemon still connects to real platform services; actions can change your desktop.
Use fixtures and preview sessions for tests that must not invoke those services.
The installed systemd daemon does not inherit these task environments.

For a repository path too long for Unix sockets, set a shorter, private development
directory in `mise.local.toml`:

```toml
[vars]
dev_root = "/home/you/.local/state/omega-dev"
```

Keep Wayland, D-Bus, and Hyprland connection variables supplied by your login
session. Omega supplies its own spawn tokens and preview session identities.
