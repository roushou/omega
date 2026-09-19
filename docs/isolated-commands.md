# Isolated command hosts

Implemented in the source tree. Command hosts use protocol version 2; rebuild
configuration executables and the daemon together when adopting these changes.

## Definitions and identities

A command defines a stable operation, typed input/output, dependencies, and
behavior. It does not name its crate, executable, or provider. A command host is
an executable registering one or more commands. The daemon authenticates callers,
resolves providers, supervises processes, and routes results.

Use distinct validated identities:

- `CommandId`: globally namespaced operation, such as `audio.set-volume`.
- `HostId`: configured executable provider.
- `ProcessId`: one supervised process incarnation, not an OS PID.
- `InvocationId`: one admitted command execution.

Signatures hash command identity and canonical input/output shape, excluding
provider identity and descriptive text. Configuration requires one provider per
command; absent dependencies, duplicate providers, incompatible contracts, and
cycles in the provider dependency graph are build errors.

## Authoring

Command identity and construction are ordinary Rust, without macro-generated
package ownership. One associated dependency type supplies both requirements and
construction; authors do not repeat a permission list.

```rust,ignore
impl omega::command::Construct for SetVolume {
    type Dependencies = (Audio, Volume);

    fn construct((audio, volume): Self::Dependencies) -> Self {
        Self { audio, volume }
    }
}

impl Command for SetVolume {
    type Input = Percent;
    type Output = ();

    const ID: &'static str = "audio.set-volume";
    const DESCRIPTION: &'static str = "Set the audio output volume";

    async fn call(&self, level: Percent) -> omega::Result<()> {
        if !self.audio.has_reading() {
            return Err(omega::Error::invalid("Audio unavailable"));
        }
        self.volume.set(level).await
    }
}
```

Construct a fresh handler per invocation in both execution modes. Persistent
hosts retain the connection, readings, and service infrastructure, not implicit
handler memory. Shared state is an explicit dependency. Required readings must
initialize before invoking user behavior, within the startup/execution deadline.

```rust,ignore
pub fn host() -> CommandHost {
    CommandHost::new("desktop-audio", env!("CARGO_PKG_VERSION"))
        .command::<SetVolume>()
        .command::<SetMuted>()
}

fn main() -> omega::Result<()> {
    host().run()
}
```

Registration describes capabilities without constructing handlers or contacting
services. Manifest inspection is side-effect-free with respect to registered
behavior. Consumers declare `Caller<SetVolume>` and call typed inputs. Importing
a library starts nothing. Bound invocations remain pure data, usable by UI,
schedules, and CLI adapters.

## Hosting policy

The desired-state document chooses provider lifetime and finite execution limits.
A `CommandHost` declares executable exports. Its `.deployment()` produces a
`CommandHostDeployment` with lifetime, execution limits, and construction settings;
that configuration has no `run()` method. Passing a declaration directly to
`Document::command_host` uses default deployment settings.
There is no second permission list in system configuration.

```rust,ignore
Document::new()
    .command_host(audio_commands::host()
        .deployment()
        .persistent(StartPolicy::OnDemand)
        .execution(ExecutionPolicy::serial()))?
    .command_host(export_commands::host()
        .deployment()
        .one_shot()
        .execution(ExecutionPolicy::bounded(NonZeroUsize::new(2).unwrap())))?
```

A persistent provider has one active process per incarnation; simultaneous first
calls share its pending startup. A one-shot process receives exactly one assigned
invocation and exits after reporting its result. Both use the same executable
runner, command types, caller API, and daemon-brokered services.

Execution policy declares concurrency, queue capacity, queue timeout, startup
timeout, and execution timeout. Serial execution preserves admission order.
Queues and payload bytes are bounded. Independent providers can execute in
parallel. One-shot execution does not imply an OS security sandbox.

## Daemon ownership

`omega-daemon::process` supplies shared spawn identity, session requests,
subprocess creation, and graceful shutdown. Plugin and command-host registries
apply their different lifetime policies through these primitives. Plugin records
retain UI lifecycle state; command process records contain no presentation fields.

| Owner                | Authoritative state                                                         |
| -------------------- | --------------------------------------------------------------------------- |
| `hosts::Hosts`       | Command provider definitions, settings, generation leases, admission limits |
| `hosts::Processes`   | Command incarnations and their authenticated sessions                       |
| `hosts::Invocations` | Correlated phases, timing, and bounded outcome history                      |
| `PluginRegistry`     | Plugin supervision, manifests, sessions, UI instances, attachments          |
| `process`            | Shared spawn identity, bounded session requests, child creation and reaping |

Plugin and command-host export variants are closed. UI processes remain
persistent. Surfaces, renderer attachment scopes, settings layering, records,
reactions, and lifecycle events preserve their existing contracts.

The command manager coordinates these owners. Admission reserves count and byte
permits before queueing. Worker tasks retain those permits through execution and
process cleanup. Registry locks cover synchronous state changes; effects execute
outside them. A per-provider async gate serializes persistent process creation,
so simultaneous callers cannot duplicate startup. Connection readers remain available during startup and
execution, including commands awaiting service effects or nested commands.

## Invocation lifecycle

Execution phases are tracked independently of reply delivery. Delivery is owned
by the invocation's reply channel; dropping a receiver does not drop its job.
The relevant states are:

```rust,ignore
enum ExecutionState {
    Queued,
    Starting,
    Dispatched { process: ProcessId },
    Finished,
}
enum DeliveryState {
    Waiting,
    Replied,
    Abandoned,
}
```

Admission captures the selected provider contract and a generation lease.
Dispatch pins the invocation to a concrete authenticated process. Replacement
never redirects admitted work. Queue expiry means execution did not start;
timeout after dispatch means effects may have occurred. Never automatically
replay uncertain operations.

Reply delivery or caller abandonment does not release execution capacity. Release
it on terminal execution or confirmed process termination. One-shot result
receipt and process reaping are separate; a process that reports success but does
not exit is stopped under a bounded cleanup policy. Persistent restart applies to
future calls only. Panic ends the process and outstanding calls receive unknown
outcomes where transport remains usable.

Reject provider dependency cycles initially, including same-provider nested
calls, to avoid serial-provider deadlock. Cancellation does not promise rollback.
Long-lived jobs require a separate progress/cancellation contract.

## Transport and trust

Reuse the multiplexed Unix-socket protobuf protocol. Hosts connect to the daemon;
they do not expose HTTP servers or individual listening sockets. Typed Rust calls,
CLI commands, and observation JSON are adapters over the same dispatcher.

A caller submits command ID, expected signature, and input. The daemon resolves
the provider. Isolated execution carries invocation identity and the selected command.
Authentication binds daemon-issued spawn token, peer PID/UID, host definition,
process incarnation, and admitted manifest. One-shot assignment narrows the
process to one invocation. Peers cannot claim additional authority or choose
another process identity. Returned refusals retain their codes.

Native services remain daemon-owned. Reading gates, storage, effect completion,
and command calls use one connection runtime with bounded queues. Arguments are
not logged by default. Expose host/process status and correlated invocation
queue/execution timing through inspection.

## Workspace and crate boundaries

`commands/<name>/` is an optional convention for library/executable packages.
Runtime identity comes from declarations, never directory names. Cargo metadata
identifies runnable target roles; pure libraries are never supervised. Build
artifacts carry typed identities and generation leases. Existing plugin discovery
can remain shorthand for the same artifact records.

Keep existing published crates:

- `omega`: command contracts, dependency construction, command host runner,
  shared connection machinery, plugin authoring.
- `omega-proto`: wire contracts and host/process/invocation identities.
- `omega-daemon`: hosts, processes, command admission/execution, presentations.
- `omega-host`: Cargo target discovery, artifact and filesystem machinery.
- `omega-document`: desired host configuration and execution policy.

## Delivery and verification

1. Independent identity and explicit construction.
2. Shared host/process supervision, including existing plugins.
3. Persistent command hosts, routing, and a complete audio command path.
4. One-shot execution through the same invocation state machine.
5. Inspection, diagnostics, and lifecycle failure coverage.

Acceptance: change an audio provider between persistent and one-shot execution
without changing command or caller implementations. Cover concurrent startup,
queue bounds and ordering, required-reading initialization, replacement during
calls, disconnects, panic, late results, process cleanup, and abandoned callers.
Run the repository's build, test, clippy, formatting, and QML checks.

The daemon's inline `hosts::tests` run with `cargo test`. They use real child
processes and generation leases, with explicit replies at the session boundary.
They cover shared startup, replacement with queued and running calls, abandoned
callers, startup failures, phase deadlines, and forced cleanup. Deadline tests
advance paused Tokio time. Authentication and socket transport have separate
session coverage.

The ignored CLI test `command_hosts_switch_lifetime_without_changing_commands_or_callers`
builds a command executable and exercises concurrent callers, both lifetimes,
panics, queue expiry, and caller disconnection through real sockets:

```sh
cargo test -p omega-cli --test e2e command_hosts_switch_lifetime_without_changing_commands_or_callers -- --ignored
```

## Workspace declaration

Scaffold a command package:

```sh
omega new audio-commands --command-host
```

This creates `commands/audio-commands/` with `lib.rs`, `commands.rs`, `host.rs`,
and `main.rs`. The generated `Echo` command uses an explicit `Construct`
implementation and the ID `audio-commands.echo`. The CLI adds `commands/*` when
needed, marks the package as a command host, and links it to `system`.

Append the printed call to your document builder, then build:

```rust,ignore
Document::new()
    .command_host(audio_commands::Host::declaration())?
```

```sh
omega build
omega run audio-commands.echo hello
```

Use `omega new shared-types --lib --into commands/audio-commands` to add a shared
library dependency. `--command-host`, `--lib`, and explicit `--template` selection
are mutually exclusive. The system source is left for the author to compose.

A runnable command package has a library target exposing its command types and
host declaration, and a binary calling `host().run()`. Its Cargo manifest marks
it for executable discovery:

```toml
[package.metadata.omega]
kind = "command-host"
```

The package can live under `commands/` or `crates/`; add it to Cargo workspace
members and add its library as a dependency of `system/`. The package name locates
the build artifact. `CommandHost::new` supplies runtime identity. Libraries
without executable role metadata are never run.

Use `omega commands --json` to inspect the last 64 completed isolated invocations
and all active invocations. Queue and execution durations use monotonic time.
History is in memory and omits arguments and returned values. `omega run <id>`
resolves a provider without requiring its name.

## Host diagnostics

`omega status <host>` reports idle, starting, running, stopping, backing off, or
failed. Idle means an on-demand host has no process; it is ready for a later call.
Retry timing reports when another startup is eligible. Eager hosts retry
automatically; on-demand hosts still need a new call.

The daemon retains the latest startup failure, bounded to 512 characters, until a
handshake succeeds or the provider configuration is replaced. Recording success
at session connection prevents fast one-shot exits from appearing as startup
failures.

Status includes active and queued invocation counts, including work draining
from older generations. Active includes startup and dispatched work; completed
calls do not count. Each host includes up to five recent failures from the shared
64-completion history, newest completion first. Arguments, result values, and
command error messages are not retained. CLI output shows error codes and queue
and execution times. `omega commands <host>` also shows these diagnostics;
`--json` emits structured data only.
