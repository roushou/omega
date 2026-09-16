# Omega base

Shared primitives for Omega, independent of its SDK, protocol, daemon, and host
integrations. This crate uses only the Rust standard library.

## Pipelines

Declare the sequence once, then run it. Each named step accepts a typed input and
returns the next step's input. Construction performs no effects.

```rust
use omega_base::execution::{Operation, Pipeline, Progress, Step};

struct Length;

impl Operation<String> for Length {
    type Output = usize;
    type Error = std::convert::Infallible;

    async fn execute(
        self,
        text: String,
        _: &mut Progress<'_>,
    ) -> Result<usize, Self::Error> {
        Ok(text.len())
    }
}

let pipeline = Pipeline::new()
    .then(Step::new("length", "measure text").using(Length));

// In an async context:
// let run = pipeline.run("hello".into(), &mut observer).await;
// assert_eq!(run.result.unwrap(), 5);
```

`Pipeline::steps()` exposes the declared order before execution. `run` consumes
the pipeline and its owned input, preserving the original error and identifying
the failed step. Its result includes attempt reports and structured progress
details. The first error stops execution; later steps are never invoked.

`Step::replace` substitutes an implementation with the same input, output, and
error types while preserving the step's identity. An unconfigured step fails
explicitly when reached. `testing` provides fixed outcomes, pass-through steps,
pending steps, a releasable gate, and an event recorder. Custom `Operation`
implementations can inspect inputs or simulate domain behavior.

All operations use the same async contract. Synchronous operations simply return
without awaiting. Futures are polled by the caller and need not be Send. Dropping
a polled run reports the active step as interrupted; it does not prove that an
external effect stopped or was undone. A skipped step must still produce the
next input. Observers must not panic.

## Scope

Pipelines own sequencing and observation. Operations own effects; filesystem
journals, service activation, and recovery policy stay with their domains.
There is no scheduler, automatic retry, or persisted pipeline checkpoint.

Durable filesystem recovery remains in `omega-host::recovery`. See
[pipelines and recovery](../../docs/workflows.md) for composition and testing.

Licensed under MIT.
