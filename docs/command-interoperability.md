# Command interoperability

Status: implemented in the source tree; requires protocol version 2 peers.

The command refactor, typed invocation, adapter migration, discovery, and `omx`
launcher integration share one command contract. The launcher composes typed
local candidates; optional Jev evaluation ranks them without constructing or
executing commands. Live relevance and latency evaluation requires a TypeSafe key.

## Existing behavior to preserve

- Input decoding precedes handler execution.
- The daemon admits only declared commands and authenticates callers using the
  existing session records.
- Execution uses the target plugin's settings and capabilities.
- Commands share one constructed handler and run concurrently. Mutable handler
  state requires synchronization; no implicit per-command serialization is added.
- Pending work has count and byte bounds. Caller abandonment does not release a
  sent request's capacity or replay its effects.
- The connection reader remains available while a command waits for effects or
  another command.
- Refusals retain their protocol code. Result streams remain correlated.

## Contract and ownership

| Concept            | Responsibility                                                           | Owner            |
| ------------------ | ------------------------------------------------------------------------ | ---------------- |
| `CommandId`        | Validated endpoint name, distinct from a surface ID                      | `omega-proto`    |
| `CommandAddress`   | Plugin name plus command ID                                              | `omega-proto`    |
| `CommandEndpoint`  | Input/output wire shape and optional description; paired with an address | `omega-proto`    |
| `Command`          | Typed handler and authoritative declaration                              | `omega::command` |
| `CommandRef<C>`    | Reference to that declaration, without a runtime context                 | `omega::command` |
| `Invocation<C>`    | Reference plus completely bound input, without executing it              | `omega::command` |
| `Caller<C>`        | Wired permission and execution handle for one command                    | `omega::command` |
| `Effect<T = ()>`   | Admitted operation with a typed terminal result                          | `omega::effect`  |
| Command dispatcher | Target resolution, authorization, validation, forwarding                 | `omega-daemon`   |

Use existing crates and modules. Do not introduce a command service crate, a
second request queue, or a mandatory pair of specification and handler types.
Existing plugin library targets can export their command types. An independent
contract crate is a later option if a real dependency cycle requires it.

Keep one author declaration. Registration, references, manifests, and inspection
derive from it. The internal descriptor is a projection, not another document
authors must keep in sync.

## Author API

The existing handler shape remains recognizable. Description defaults to an empty string;
it has no effect on authorization or execution.

```rust,ignore
#[derive(omega::Command)]
pub struct SetVolume {
    volume: omega::platform::audio::Volume,
}

impl omega::Command for SetVolume {
    type Input = omega::Percent;
    type Output = ();

    const DESCRIPTION: &'static str = "Set the output volume";

    async fn call(&self, level: Self::Input) -> omega::Result<()> {
        self.volume.set(level).await
    }
}
```

A consumer declares its dependency where it declares its other effects:

```rust,ignore
#[derive(omega::Effects)]
pub struct Effects {
    volume: omega::command::Caller<audio::SetVolume>,
}

// In behavior: returns Effect<()> for this particular command.
effects.volume.call(omega::Percent::whole(30)).await?;

// A bound invocation is data. Construction does not admit work.
let invocation = audio::SetVolume.with(omega::Percent::whole(30));
```

`with` returns `Invocation<C>` instead of a UI-specific `Bind<()>`. Buttons,
document actions, and the caller handle consume the same invocation. Controls
that supply input, such as a volume slider, still accept `CommandRef<C>` with a
matching input type. Both conversions retain the complete command address.

Commands returning data work through the same API: `Caller<C>::call` yields an
`Effect<C::Output>`. No raw protobuf response is exposed to ordinary callers.

`Caller<C>` is an effect dependency, never a render reading. A surface using a
foreign command binding declares its `Caller<C>` in its `Effects`. Local bindings
need only the existing command registration. A foreign binding without the
declared dependency is rejected; rendering a binding does not grant authority.

## Identity and registration

1. Preserve `(PluginName, CommandId)` through every authoring and wire adapter.
2. Derivation keeps the package-based plugin identity and explicit command-name
   override. Parse both into validated identifiers before constructing manifests.
3. Registering a command under a different plugin owner is an error. Reject
   duplicate addresses before emitting the manifest.
4. Replace command maps keyed by `String` and command validation through
   `SurfaceId` with the appropriate command types.
5. No adapter may infer the target from the current widget when a typed reference
   supplied an explicit owner. A foreign target is routed or explicitly refused.

## Typed input, output, and description

Keep the current argument envelope in the first implementation: zero arguments
for `()`, one for a scalar, one map for a derived input, and explicit raw `Args`.
Do not also redesign the entire value protocol.

Extend input declarations with a transport-neutral shape description covering
those envelopes. Describe supported scalar constraints, records, lists, optional
values, and enum choices. Raw or custom inputs may explicitly report an opaque
shape; they remain callable but cannot automatically generate a form or typed
dynamic input editor.

Structured output must support strict decoding as well as encoding. Implement
the output codec and its shape together, including explicit handling of `()` and
optional values. Do not use `Fields` decoding for command results: its missing-field
defaults intentionally serve a different contract. Do not collapse every
empty-valued command result into unit success before consulting its output codec.

Generated codecs and descriptors use the same field declaration. Built-in codecs
own their constraints. Custom validation in the receiving plugin remains
authoritative; a schema cannot describe every application invariant.

Include a canonical signature fingerprint over address and input/output shape,
excluding descriptions. Typed dependencies declare the expected fingerprint;
build validation rejects incompatible installed endpoints. Invocations carry it
so the daemon checks the actual target session before dispatch, including when
development adoption replaces a plugin. Opaque descriptions cannot prove semantic
compatibility; callers still decode results and authors own changes to such APIs.

`Effect<T = ()>` reuses the current admission queue, receipt, limits, and timeout
accounting. Generic completion and receipt handling preserve `T`; existing domain
controls continue to return `Effect<()>`. Admission remains eager on `call`, as
with current effect handles. Pure `Invocation` construction is separate.

## Authorization

Outgoing command access is an exact dependency set, derived from `Caller<C>`
fields and aggregated into the plugin manifest. There is no separate permission
list in the system document. A dependency declares both address and expected
signature; build/check refuses an absent or incompatible target.

- The operator can invoke registered commands.
- A plugin can invoke its registered local commands and its declared foreign
  command dependencies. `CAPABILITY_SPAWN` no longer grants cross-plugin access.
- The target executes with its own manifest capabilities. Access to its public
  endpoint intentionally delegates that behavior; it does not transfer arbitrary
  platform capabilities to the caller.
- A renderer can only submit a retained binding in its attached scope. Foreign
  invocation is authorized using the source plugin's manifest, never the renderer's
  uid or supplied target.
- Native plugin capabilities remain session policy, not OS sandboxing.

The daemon's command dispatcher is the single owner of these decisions. UI
interaction handling first verifies instance, revision, visibility, binding, and
input, then hands the resolved call to that dispatcher with its source authority.
CLI and document actions use operator authority; SDK calls use plugin authority.
Local UI calls also use this dispatcher instead of maintaining a second endpoint
membership check in the presentation registry.

## Execution and failure

Resolve and admit against one target session snapshot, including its manifest.
Do not check one generation's declaration and then forward to a replacement.
Once admitted, a call is never redirected or replayed after disconnection.

Keep the existing five-second bounded-call model for this deliverable. Timeout
means completion is unknown, not that execution was undone. Sent calls retain
capacity until their terminal answer or session teardown. Dropping a caller's
effect does not cancel the callee. Long-running work should admit a job and expose
its state separately; it is not a reason to add unbounded command waits.

Nested calls are separate bounded operations. No transactional, chain-wide
deadline, or rollback guarantee is implied. Cyclic application calls can exhaust
capacity or time out; they must not block transport reception or cause automatic
retries. Supporting propagated cancellation or routine budgets is separate work.

Handler panic policy is fail-stop. Preserve the incoming stream identity outside
the spawned task. An unwind marks command execution poisoned, refuses new calls,
and reports `OutcomeUnknown` to outstanding admitted callers where transport is
still usable before ending the plugin session. Subsequent calls cannot use the ended session. Do not keep running shared handler
state after a panic. Abort, process death, and broken transport resolve through
disconnect instead; execution may already have affected external systems.

Ordinary handler errors remain per-call refusals. Distinguish input invalidity,
signature mismatch, denied access, unavailable target, capacity exhaustion,
timeout, malformed result, and handler failure. Their protocol mappings have one
owner per error type. Diagnostics identify target and operation without recording
arguments by default.

## Discovery and launcher integration

`omega commands [PLUGIN] [--json]` lists registered endpoints, their availability,
and optionally their full contracts. `omega::command::Commands` supplies the same
catalogue to plugin behavior, restricted to that plugin's declared command access.
Listing never grants permission. The daemon rechecks identity, signature, and
availability when the selected command is invoked.

`omx` composes applications, audio controls, brightness presets, media-player
controls, and known Bluetooth-device actions into one candidate list. Each action
has complete typed input and a stable local ID. Dynamic readings supply device and
player values; command schemas do not claim to describe a device inventory.

Local fuzzy search works without remote services. An opt-in semantic-search button
sends the query and candidate descriptions to TypeSafe's pinned Jev model. Returned
scores can only reorder current candidates. Editing cancels delivery of the old
request; results also carry the query and instance epoch. Selecting a result still
requires explicit activation through the normal typed caller. No shell fragments,
model-generated arguments, retries, or automatic execution are introduced.

## Compatibility and boundaries

Protocol version 2 is required for explicit binding owners, command signatures,
dependencies, and catalogue outcomes. Rebuild plugins and reinstall the renderer
with a matching CLI before activating this source version. There is no persistent
command data to migrate.

Custom `Input` implementations default to an opaque shape. Authors can supply a
shape, but target-side decoding remains authoritative. Dynamic CLI arguments stay
textual and use the target's existing input parser. Typed calls carry strict values
and a signature. Opaque contracts cannot prove semantic compatibility.

This implementation does not add dynamic permission adoption, transactional
routines, propagated cancellation, cross-plugin presentation, or unbounded jobs.
The Jev adapter and application policy live in `omx`, not in Omega's command core.

## Acceptance tests

Required checks are `cargo build`, `cargo test`, `cargo clippy --all-targets`,
`cargo fmt --check`, and `crates/omega-renderer/shell/lint.sh`. Also run
`cargo test -- --ignored` for scaffolding through the real binaries.

- Plugins A and B both declare `set`; a UI in A bound to B's typed command calls B.
- Missing outgoing grants are denied, including renderer-mediated requests;
  declaring one endpoint grants neither another endpoint nor process spawning.
- Wrong input/signature and wrong output are explicit errors. Unit, optional,
  structured, and opaque contracts follow their declared codec semantics.
- An admitted call remains pinned during replacement; a later call uses the new
  compatible session. An incompatible adopted session is refused before execution.
- Parallel, self, and nested calls leave state updates and replies flowing.
- Timeout, caller abandonment, target disconnect, saturation, and late replies
  retain the current capacity guarantees and never replay an operation.
- Panic reports failure where possible and ends the poisoned session without
  invoking additional handlers.
- UI, form, CLI, document action, and plugin caller target the same address with
  equivalent typed input and preserve the same refusal.
- Typed isolated fixtures complete command calls with Rust values or refusals;
  they never connect to a live target. No test author constructs protobuf replies.
