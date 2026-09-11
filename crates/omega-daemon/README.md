# Omega daemon

The runtime behind Omega's desktop configuration. It supervises plugin processes,
publishes system state, authorizes requests, and reconciles the running desktop
with the desired configuration.

This is the daemon library. To run Omega, install
[omega-cli](https://crates.io/crates/omega-cli), which provides the `omega` binary:

```sh
omega daemon
```

`omega daemon install` installs and starts a systemd user service.
`omega daemon status` checks that service; `omega status` reports plugin state.
The [project README](https://github.com/roushou/omega) covers complete setup.

For runtime integration, `Daemon::builder` starts from a filesystem layout and
lets callers set the control and observation sockets explicitly. Brokers are
registered with `add_broker`. The library owns supervision, session identity,
capability policy, state distribution, and graceful shutdown.
Plugins run in separate processes and declare their requirements through the
[SDK](https://crates.io/crates/omega-rs).

[API reference](https://docs.rs/omega-daemon) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.
