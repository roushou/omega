//! Logical keyboard events and shortcut matching, independent of any UI runtime.
//!
//! Hosts translate native events into [`KeyEvent`] and decide when a keymap is
//! eligible. Text insertion, focus, IME handling and global registration belong
//! to the host. Matching uses exact modifiers and ignores repeats by default.
//!
//! ```
//! use omega_keyboard::{Chord, Key, KeyEvent, Keymap, Modifiers};
//! let keys = Keymap::new().bind(Chord::new(Key::Character('k')).ctrl(), "search")?;
//! let event = KeyEvent::pressed(Key::Character('K'), Modifiers::CONTROL);
//! assert_eq!(keys.resolve(&event), Some(&"search"));
//! # Ok::<(), omega_keyboard::Conflict>(())
//! ```

mod chord;
mod event;
mod keymap;

pub use chord::Chord;
pub use event::{Key, KeyEvent, Modifiers, Phase};
pub use keymap::{Conflict, Keymap};
