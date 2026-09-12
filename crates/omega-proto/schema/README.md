# Omega schema

These Protobuf schemas define the messages shared by Omega's daemon, plugins,
configuration tools, and renderer. `omega-proto` generates Rust types from them
at build time. Edit the schemas, not generated Rust or JSON implementations.

## Organization

Schema paths below are relative to `schema/omega/`.

| Schema           | Responsibility                                   |
| ---------------- | ------------------------------------------------ |
| `wire.proto`     | Frames, handshake, and bidirectional requests    |
| `value.proto`    | Generic values for settings and command payloads |
| `state.proto`    | Topic envelope and replicated patches            |
| `state/*.proto`  | State payloads grouped by domain                 |
| `action.proto`   | Actions and keybindings                          |
| `event.proto`    | Events                                           |
| `unit.proto`     | Manifests, surfaces, and capabilities            |
| `ui.proto`       | Declarative view trees                           |
| `document.proto` | Desired desktop configuration                    |

## Contract

Actions, events, and state topics have closed taxonomies. Extensible values and
custom events use explicit protocol fields. Add a state topic to its domain
schema and to the topic envelope; keep existing field tags and enum numbers
stable, and reserve removed fields.

Desired configuration and observed state have separate messages. The daemon
owns topic and view revisions. A topic with its payload unset explicitly reports
absence, such as a missing battery or an unavailable broker. An unpublished topic
is not equivalent to an absent one: plugins wait for their declared topics before
rendering.

Choose topic boundaries and update resolution around what should wake consumers.
For example, the clock topic reports minute-resolution time. Payloads should not
carry redundant constants or duplicate facts that can disagree.

Identifiers and payloads are validated at their boundaries. Wire strings do not
imply arbitrary accepted input, and generated message types alone do not establish
domain validity.

## Requests and failures

Every invocation receives an outcome or refusal on its own stream. Stream IDs
are allocated by parity: daemon even, peer odd. The observation socket uses the
same request and result vocabulary encoded as JSON.

Live requests are authorized before payload validation and routing. Validation in
`omega-proto::action` covers every action kind, including required fields, enum
values, numeric ranges, identifiers, and process/D-Bus string constraints.
Desired documents and scheduled actions use the same validation rules. Document
validation also checks scheduled plugin and command references against the build.
Backend availability and encoding restrictions remain handler responsibilities.

Failure codes distinguish malformed input (`INVALID_ARGUMENT`), unmet
prerequisites (`FAILED_PRECONDITION`), unavailable services (`UNAVAILABLE`),
aggregate admission limits (`RESOURCE_EXHAUSTED`), and oversized individual
payloads (`PAYLOAD_TOO_LARGE`). `DEADLINE_EXCEEDED` means the wait expired; it does
not establish whether an external action completed.

`GetDeployment` is an operator-only snapshot of generation acceptance, the last
reconciliation pass, shell application, and current unit phases. Shell results
carry their own generation because explicit application can precede activation.
A settled reconciliation pass is not a guarantee that all unit processes are
running; their phases remain authoritative.

## Verification

From the repository root:

```sh
cargo test -p omega-proto
cargo test -p omega-brokers --test coverage
```

Protocol tests cover wire shapes and validation. Broker coverage checks that
state and action declarations have implementations. Changes to UI properties
also require regenerating the renderer's readers and running its checks; see the
[renderer guide](https://github.com/roushou/omega/blob/main/crates/omega-renderer/shell/README.md).
