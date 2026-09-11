# Omega renderer

The Quickshell renderer for Omega's declarative view trees, together with its
installation and inspection APIs. The renderer runs inside Omarchy's shell and
displays plugin widgets and panels using the shell's theme and layout.

QML and generated property readers are embedded in this crate. The Omega CLI
installs the assets carried by its binary:

```sh
omega shell install
omega shell status
```

Installation prints the command for enabling the plugin in the shell. `omega init`
performs the initial setup, including enabling it. Plugin authors describe their
interfaces with [omega-rs](https://crates.io/crates/omega-rs); they do not need to
write QML or depend on this crate directly.

`Renderer`, `Installed`, and `HostShell` provide the host-side installation APIs.
The renderer consumes the daemon's observation socket using the same protocol
messages in JSON form.

[API reference](https://docs.rs/omega-renderer) ·
[Renderer contract and development](https://github.com/roushou/omega/blob/main/crates/omega-renderer/shell/README.md) ·
[Project](https://github.com/roushou/omega)

Licensed under MIT.
