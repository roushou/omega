# Keyboard input

`omega-keyboard` provides logical keys, events, chords and `Keymap<A>` without
Omega, Qt or async dependencies. Applications supply their own action type and
host event adapter. Omega re-exports it as `omega::keyboard`.

```rust
use omega::keyboard::{Chord, Key, Keymap};

#[derive(Clone)]
enum Action { Search, Close }

let keys = Keymap::new()
    .bind(Chord::new(Key::Character('k')).ctrl(), Action::Search)?
    .bind(Chord::new(Key::Escape), Action::Close)?;
```

`bind` and `merge` return `Conflict` when two declarations can match the same
event. Character case is normalized; Shift is an explicit modifier. Additional
modifiers prevent a match. `.repeat()` enables native auto-repeat; `.on_release()`
matches releases independently of prior presses. For static declarations, construct
and check the map once, then clone and adapt its actions for each render.

## Surface integration

Map actions to existing local messages or command bindings. Attach the resulting
map to any view builder, `View`, or `Component`; its scope is that subtree.

```rust
let bindings = keys.clone().map(|action| events.send(action));
Column::new()
    .child(search_field)
    .child(results)
    .shortcuts(bindings)
```

The surface uses its existing `Message` and `update` contract. Key events never
cross the daemon socket. The renderer invokes the declared binding with the same
instance, revision and pending-work checks as buttons. Attaching another map
replaces the previous map; use `merge` to compose maps explicitly.

Native controls handle editing and navigation first. Unconsumed keys reach the
nearest matching subtree, then ancestors, then host fallbacks such as Escape to
close a popup. A match consumes the event even if its action cannot be admitted.
Input-method composition suppresses shortcuts and host fallbacks. Initial focus
belongs to the host root; `Field::autofocus()` can place it in an editor.

## List navigation

Navigation references are UI concepts, separate from shortcut matching:

```rust
use omega::ui::{Column, Field, List, ListTarget};
const RESULTS: ListTarget = ListTarget::new("results");

Column::new()
    .child(Field::new("Search").navigate(RESULTS))
    .child(List::new().target(RESULTS))
```

The field and list must share a parent scope. Components qualify their node
identities, and each instance has its own navigation registry. A target names a
list; it does not prove that the list is present in every conditional render.
Navigation waits for controlled edits to settle before activating a result.

## Boundaries

This API supports logical keys, not physical scan codes. It does not register
global shortcuts, capture raw keyboard input, intercept native editing, or match
multi-chord sequences. Host-specific compositor bindings remain separate.
The Qt adapter preserves AltGraph when Qt reports it; it cannot recover distinctions
that a native backend has already discarded.
