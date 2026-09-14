# Architectural principles

Omega provides an opinionated foundation for users to build their own desktop.
These rules govern implementation and review; they do not assert that every
existing subsystem already satisfies them. [Architecture](architecture.md)
describes implemented contracts, and [open questions](design.md) records known
limitations.

Build small, coherent primitives with explicit semantics and minimal dependencies.
Compose them through layers that own policy, state, and effects. Keep pure
decisions separate from execution, and make identity, scope, lifetime, and failure
explicit at each boundary.

## Mechanisms, policy, and execution

A primitive defines a mechanism and its invariants. Its inputs and outputs must
be meaningful without a particular application or desktop host. Higher layers
select behavior; adapters translate external events and execute effects.

For keyboard handling, the responsibilities are:

| Concern                                                          | Owner              |
| ---------------------------------------------------------------- | ------------------ |
| Logical keys, modifiers, chords, matching, and binding conflicts | Keyboard primitive |
| Focus, subtree routing, shortcut precedence, and native editing  | UI integration     |
| Choosing which shortcut invokes search or selection              | Application        |
| Translating native events or registering global shortcuts        | Host adapter       |

A primitive may enforce strong rules, such as rejecting ambiguous bindings.
Minimal dependencies do not require weak semantics or unlimited configurability.

Business rules can also be pure. Authorization decisions, reconciliation plans,
and state transitions should operate on explicit facts and return decisions or
next state. Execution owns I/O, scheduling, and observations of success or failure.
Clocks, environment lookup, filesystem access, and native services enter through
the execution boundary rather than being discovered inside decision logic.

## Ownership and composition

Each fact and policy has one authoritative owner. Other layers may project,
translate, or validate it at a trust boundary; they must not independently infer a
competing answer. For each composed contract, specify the relevant properties:

- **Identity and scope:** what an identifier names, where it resolves, and whether
  children inherit or isolate access.
- **State and lifetime:** who mutates state, what invalidates it, and what happens
  when its owner disconnects or is replaced.
- **Ordering and cancellation:** which work is serialized, how stale results are
  rejected, and whether cancellation stops execution or only stops waiting.
- **Failure:** where errors are reported and whether success means admission,
  completion, or an observed external change.

Nested components and concurrent instances must preserve those contracts.
Composition must not introduce hidden global state or broaden authority.

## Dependency boundaries

Dependencies point toward the contracts and logic an operation needs. Concrete
hosts and executors adapt to those contracts. Reusable logic must not require a
running daemon or desktop merely to construct it or evaluate a decision.

Use a coherent module when that provides sufficient isolation. Introduce a trait
for a concrete substitution boundary, a generic parameter for meaningful type
variation, or a crate for an independently useful dependency boundary. Extraction
and replacement should be possible at those boundaries without making every type
extensible. Protocol types belong in the protocol because they cross a connection,
not simply because several modules need them.

## Authoring experience

Omega owns workspace conventions and supplies ready-to-use composition through
the SDK. Common tasks should require little setup and expose domain concepts.
Plugin authors should not need to understand transport, supervision, or renderer
internals to compose a surface.

Convenience APIs compose the same underlying contracts and preserve their errors
and scope rules. More advanced uses should build on accessible capabilities
without reimplementing the common path. Defaults belong to the layer that can
justify them, with explicit control where applications need different behavior.

## Review and validation

Trace complete behavior paths, such as input to model update, reading to render,
and configuration to reconciliation. Check boundaries as well as individual
modules for duplicated policy, hidden lifecycle dependencies, changing identifier
semantics, lost errors, and stale state.

Test pure decisions with explicit inputs and outcomes. Test adapters against their
external contracts, and use integration tests to verify ordering, ownership, and
failure across layers. Prefer observable invariants to assertions about private
implementation structure.

For a proposed change, identify the contract, its owner, its dependencies, and the
behavior that verifies it. A useful review question is whether a decision can be
tested without starting the desktop and its execution changed without rewriting
that decision. Findings should name concrete coupling or violated invariants;
structural changes need a demonstrated benefit at that boundary.
