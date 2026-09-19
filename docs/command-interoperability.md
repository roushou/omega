# Command interoperability

Commands expose typed operations to UI bindings, schedules, the CLI, and other
commands. The daemon resolves each command ID to its configured provider.
See [isolated command hosts](isolated-commands.md) for execution and lifecycle rules.

## Declare once

Identity belongs to the command, independently of its package and executable:

```rust
use omega::{Command, Percent, platform::audio::Volume};

pub struct SetVolume {
    volume: Volume,
}

impl omega::command::Construct for SetVolume {
    type Dependencies = (Volume,);

    fn construct((volume,): Self::Dependencies) -> Self {
        Self { volume }
    }
}

impl Command for SetVolume {
    type Input = Percent;
    type Output = ();

    const ID: &'static str = "audio.set-volume";
    const DESCRIPTION: &'static str = "Set the output volume";

    async fn call(&self, level: Percent) -> omega::Result<()> {
        self.volume.set(level).await
    }
}
```

Registration with `.command::<SetVolume>()` derives the manifest requirements
from `Dependencies`. It does not construct handlers or contact services. Each
invocation constructs a fresh handler against the provider's settings and shared
connection state. Required readings initialize before user behavior runs.

`#[derive(omega::Command)]` remains an optional convenience for field wiring and
the shorthand command reference. It does not determine identity. Macro-free code
implements `Construct` with tuple dependencies and uses
`CommandRef::<SetVolume>::new()` for references. Derived commands receive
`Construct` automatically from their field wiring; their `Command` implementation
only declares identity, input, output, and behavior.

## Call from another component

Declare `Caller<SetVolume>` in behavior dependencies. Its `call(Percent)` returns
`Effect<()>`, with the output type supplied by the command. Consumers need only
the library defining the command; importing that library starts no process.

A bound `CommandRef::<SetVolume>::new().with(Percent::whole(30))` is data. It can
be passed to a button, document action, or caller without running the command.
Render declarations cannot hold `Caller`: it is an effect, not a reading.

The manifest declares command IDs and expected signatures. Signatures cover the
ID and input/output wire shapes, excluding provider names and descriptions.
Build validation rejects missing, ambiguous, or incompatible providers. Runtime
checks the admitted caller's grants and the selected provider's contract again.
Discovery never grants invocation authority.

## Host and inspect

Register commands in a plugin or a `CommandHost` executable. A command host can
be persistent or one-shot without changing its command types or callers. The
system document chooses lifetime, concurrency, queue bounds, and deadlines.

```sh
omega commands
omega commands --json
omega status desktop-audio
omega run audio.set-volume 40%
```

The CLI preserves arguments as text for the command's input decoder. Typed Rust
calls retain typed values. JSON inspection includes contracts, availability, and
bounded execution diagnostics for isolated hosts; arguments and results are not
stored in that history. Human-readable status and command listings also show
host phases, call counts, startup failures, retry eligibility, and recent command
failures. See [host diagnostics](isolated-commands.md#host-diagnostics).

The daemon never replays a call after timeout or connection loss. A result reports
execution completion, not rollback or guaranteed reversibility. A disconnected
caller does not release capacity while execution continues. Plugin refusals keep
their original codes; uncertain execution reports `OutcomeUnknown`.
