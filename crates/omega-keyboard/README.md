# omega-keyboard

Logical keyboard events and composable shortcut maps, with no dependencies on
Omega, a window system, or an async runtime.

```rust
use omega_keyboard::{Chord, Key, KeyEvent, Keymap, Modifiers};

let keys = Keymap::single(Chord::new(Key::Character('k')).ctrl(), "search");
let input = KeyEvent::pressed(Key::Character('K'), Modifiers::CONTROL);
assert_eq!(keys.resolve(&input), Some(&"search"));
```

`bind` and `merge` reject overlapping bindings. `map` adapts actions without
changing the bindings. Matching uses exact modifiers, case-normalized logical
keys, and separate press/release phases. Auto-repeat is opt-in.

Hosts supply normalized events and own focus, routing, native editing and IME
handling. This crate does not insert text, register global shortcuts, or interpret
backend-specific scan codes. Omega exposes these types through `omega::keyboard`
and connects keymaps to view subtrees using `.shortcuts(...)`.
