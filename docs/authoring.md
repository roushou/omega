# Writing plugins and components

Use this guide to choose Omega APIs and compose reusable UI. See the
[setup guide](../README.md) for installation and the
[API reference](https://docs.rs/omega-rs) for method signatures and options.

## Choose an API

| Requirement                                            | API                                                    |
| ------------------------------------------------------ | ------------------------------------------------------ |
| Display system state                                   | `Surface` with `platform` reading handles              |
| Keep selection, query text, or other per-window state  | `Surface::Model` and local messages                    |
| Reuse presentation across surfaces                     | `ui::Component`                                        |
| Expose an operation to controls, schedules, or the CLI | `Command`                                              |
| Respond to a system transition                         | `Reaction`                                             |
| Share records between plugins                          | `record::Own<T>` and `record::Watch<T>`                |
| Configure defaults and placements                      | `system/` with `omega-document` and a host integration |

## Read state

Declare readings as fields. Omega subscribes to them and renders when their
values change. Readings and composite handles live under `omega::platform`,
grouped by domain: `audio`, `power`, `network`, `bluetooth`, `desktop`,
`system`, `session`, `time`, and `applications`.

```rust
use omega::{Surface, View, platform::power::Battery, ui::Text};

#[derive(omega::Surface)]
pub struct Charge {
    battery: Battery,
}

impl Surface for Charge {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        if !self.battery.has_reading() {
            return Text::new("Battery unavailable").muted().into();
        }
        Text::new(self.battery.charge()).into()
    }
}
```

`Model = ()` and `Effects = ()` declare no local data or behavior dependencies.
`Message = Infallible` declares that no local messages can occur; its empty match
is exhaustive. Controls can still invoke registered commands. To add local behavior,
choose a model and message type and implement their transitions in `update`.

Register with `omega::plugin!().surface(Charge)`. Registration makes the surface
available; placement in a bar or window is configured separately.

A required reading delays the first render until its topic has been reported.
Reported absence satisfies that gate: use `has_reading()` to check availability.
To show loading content before the first report, hold `Optional<Battery>` from
`omega::surface` and check `is_pending()`. Accessors remain available on the wrapper.

Use typed measurement values directly in text, controls, and comparisons:
`Text::new(battery.charge())` formats a `Percent`; `Slider::new(level)` accepts one.

## Handle interactions

Commands declare an input type and receive effect handles as fields. Controls
accept a matching typed command reference or a local-message binding.

```rust
use omega::{Command, Percent, platform::audio::Volume, ui::Slider};

#[derive(omega::Command)]
pub struct SetVolume {
    volume: Volume,
}

impl Command for SetVolume {
    const ID: &'static str = "set-volume";

    type Input = Percent;
    type Output = ();

    async fn call(&self, level: Percent) -> omega::Result<()> {
        self.volume.set(level).await
    }
}

let slider = Slider::new(Percent::whole(50)).on_change(SetVolume);
```

Register commands with `.command::<SetVolume>()`. For a button that always sets
one value, bind the input first: `Button::new("Half").on_press(SetVolume.with(Percent::whole(50)))`.

Effect methods submit work when called. Await the returned effect to handle its
result. A timeout does not undo an operation already submitted. Unobserved effect
failures terminate the plugin. Await or poll a receipt to handle failure; explicitly
detaching it leaves failure reporting to the runtime and does not suppress that policy.

From `system/`, use `Actions::invoke(focus::Tick)` for a command with `Input = ()`
or `Actions::invoke_with(audio::SetVolume, Percent::whole(50))` for typed input.
These examples require dependencies on libraries exporting those commands.
The daemon resolves each command to its configured provider.

### Separate command hosts

Use `omega new audio-commands --command-host` to create a command library and
executable in `commands/audio-commands/`. Add the printed registration to your
system document, then run `omega build`. The generated `audio-commands.echo`
command accepts and returns text, so `omega run audio-commands.echo hello` works
without desktop service dependencies.

The library exports command types and `Host::declaration()`. Both the executable
and the system use that declaration. Consumers import the command types and use
`Caller<T>`; importing a library starts no process. See
[isolated command hosts](isolated-commands.md) for deployment policy and limits.

### Command construction

`#[derive(omega::Command)]` wires the declared fields and supplies construction.
The `Command` implementation only defines identity, input, output, and behavior.
A fresh handler is constructed for each call after required readings initialize.

To write the example above without the derive, remove `#[derive(omega::Command)]`
and implement `omega::command::Construct`:

```rust
impl omega::command::Construct for SetVolume {
    type Dependencies = (Volume,);

    fn construct((volume,): Self::Dependencies) -> Self {
        Self { volume }
    }
}
```

Use either generated wiring or an explicit `Construct` implementation for a type.
Both paths derive grants and subscriptions from the dependencies they construct.
Registration only inspects these declarations; it does not construct a handler.
See [command interoperability](command-interoperability.md) for a complete example.

## Workspace controls

`platform::desktop::Workspaces` reports workspace IDs, names, output assignments,
window counts, and global focus. `WorkspaceControl` belongs in commands or surface
effects. Use `switch_to(WorkspaceIndex)` for a numbered workspace and
`switch_to_named(&WorkspaceName)` for a literal name. The types validate command
inputs; a `WorkspaceIndex` command also accepts decimal text from `omega run`.
Destinations need not already exist, so an empty workspace button can create one.

`next()` and `previous()` traverse existing workspaces in compositor order,
including other outputs on Hyprland. Completion acknowledges the compositor;
use the reading to highlight observed focus. Calls are never retried automatically.

## Compose reusable UI

A component takes ordinary values and `Bind<T>` inputs. It has no subscriptions,
registration, or independent lifecycle. The parent surface supplies its state
and behavior.

```rust
use omega::{Percent, View, ui::{Bind, Component, Row, Slider, Text}};

pub struct LevelControl<'a> {
    pub label: &'a str,
    pub level: Percent,
    pub change: Bind<Percent>,
}

impl Component for LevelControl<'_> {
    fn render(&self) -> View {
        Row::new()
            .gap(8)
            .child(Text::new(self.label))
            .child(Slider::new(self.level).on_change(self.change.clone()).fill_width())
            .into()
    }
}
```

Pass the component directly to a container, such as `.child(control)`, or convert
it with `.into()`. This scopes its internal keys. Calling `render()` directly
bypasses that scope. Import `Component` to apply modifiers such as `.padding(8)`
to a component's root; modifiers return a `View`.

Keep private components in ordinary Rust modules. For cross-plugin reuse, create
a library with `omega new desktop-ui --lib --into plugins/audio`, then add other
consumers explicitly. Files and directories do not register components.

## Preserve identity and editing state

Assign `.key(item.id())` to children that can be inserted, removed, or reordered.
Keys must be unique within the view and stable across renders. Use an item's ID,
not its position or a label that can change. Fixed layouts can use automatic
positional keys. Key repeated component instances at the call site; their internal
keys are scoped independently.

For a `List`, keys also identify selection and activation values. `Choice<T>`
separates its typed value from its display label.

Use `Surface` for per-instance models and local messages. `update` changes
the model synchronously; return `Task` for asynchronous work. `Task::replace`
invalidates earlier results under the same task key. Closing cancels managed result
delivery; hiding retains work; destroying discards the model. Cancellation does
not undo external effects already submitted.

For model-owned text, use `TextValue` with `Field::controlled` and `on_change`.
Apply `TextEdit` messages through `TextValue::apply`; use `reset` for an explicit
replacement. This preserves newer typing when rendered values arrive late.
See the [stateful search example](../crates/omega/examples/stateful_search.rs).

## Configure and test

Derive `omega::Config` on settings types that implement `Default`. It implements
`config::Fields` and value conversion; missing fields use the settings type's
declared defaults. `Fields` is the serialization trait, not a derive macro.

Plugin settings are construction inputs. Placement settings override only matching
keys, and changes to plugin settings restart the plugin. Store mutable application
state in the instance model or records. Records survive plugin restarts while the
daemon runs, but are not persisted across daemon restarts.

Use `testing::SurfaceHarness` for any surface, including ones with no local messages.
It retains the model, accepts fixture changes, and processes messages and tasks.
`testing::Drawn::of::<S>(&state)?` constructs a temporary harness for a one-shot
assertion; it runs initialization and respects readiness. Use `testing::Called`
for command results and effects. Tests use synthetic readings and captured effects.

For visual inspection, register component or surface cases with `omega-preview`
and run `omega preview <package>`. Follow the [preview guide](previews.md) for
loading states, interaction, screenshots, and comparisons.

## Shared storage

Use `omega::storage::Store<T>` in behavior for committed key-value reads and writes.
For rendering, declare `Subscribed<Q>` where `Q: Subscription<Storage = T>` and
start it in `Surface::initialize`. The ordinary `Storage` trait defines the key,
value, stable ID, and memory or JSON policy. Handle fields derive manifest access;
there is no additional registration in the system document.

See [shared storage](storage.md) for the API, lifetimes, revision conflicts,
inspection commands, and isolated test fixtures.

## Call another plugin

Add the plugin's library as a Cargo dependency and hold a typed caller in behavior:

```rust,ignore
#[derive(omega::Effects)]
pub struct Effects {
    volume: omega::command::Caller<audio::SetVolume>,
    commands: omega::command::Commands,
}

// Inside async behavior:
effects.volume.call(omega::Percent::whole(30)).await?;
```

`Caller<C>` declares exactly that endpoint in the manifest; there is no additional
permission list in `system/`. `omega check` rejects missing or incompatible targets.
The target uses its own settings and effects. A foreign UI binding must also have
its caller declared in the source surface's effects. Keep callers out of render
readings. `with(input)` constructs a pure invocation; `call(input)` admits work.

Use `const DESCRIPTION: &'static str` on a command for discoverable help text.
Input and output shapes come from the command types. Derive `omega::Output` for a
returned struct; all fields are required when decoding command results. This is
separate from configuration's defaulting behavior. Custom value types implement
`CommandValue` and the strict `IntoValue`/`FromValue` conversions.

`omega commands --json` inspects the catalogue. Plugin behavior can await
`effects.commands.list()` to inspect only its accessible endpoints. Availability
is a snapshot; always handle invocation errors. A timeout does not undo execution,
and calls are never automatically retried against a replacement process.

In a surface test, complete the captured command with ordinary Rust values:

```rust,ignore
let call = panel.take_effect().unwrap().command::<audio::SetVolume>()?;
assert_eq!(*call.input(), omega::Percent::whole(30));
call.complete(Ok(()))?;
panel.complete().await?;

// Complete a catalogue read without constructing protocol messages.
panel.take_effect().unwrap().commands()?
    .entry(audio::SetVolume, true)
    .complete()?;
```
