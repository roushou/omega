# Omega brokers

Connections to the Linux subsystems Omega observes and controls. Brokers translate
subsystem state into protocol topics and execute the actions those subsystems
support.

Integrations include UPower, NetworkManager, BlueZ, PipeWire, MPRIS, logind,
Hyprland, desktop notifications, and kernel resource readings. A broker owns its
connection and reports how it should be polled or awakened; the daemon manages
its lifecycle and retries.

`Broker` defines that interface and `Brokers` assembles the available integrations.
Capability authorization belongs to the daemon. A broker's ability to execute an
action does not grant permission to request it.

This crate is for Omega's runtime. Plugin authors access system state and controls
through [omega-rs](https://crates.io/crates/omega-rs), without opening separate
connections to these services.

[API reference](https://docs.rs/omega-brokers) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.
