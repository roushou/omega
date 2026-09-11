//! Audio output and media playback.
//!
//! [`Audio`] observes output state; [`Volume`] controls it from a command or
//! reaction. [`Media`] reports players and their playback state.
//!
//! ```no_run
//! use omega::audio::{Audio, Volume};
//! use omega::ui::{Metric, Section, Slider};
//! use omega::{Command, Percent, Ui, Widget};
//!
//! #[derive(omega::Widget)]
//! struct Panel { audio: Audio }
//!
//! impl Widget for Panel {
//!     fn render(&self) -> Ui {
//!         Section::new("Audio")
//!             .child(Metric::new(self.audio.volume()).label("Output volume"))
//!             .child(Slider::new(self.audio.volume()).on_change(SetVolume))
//!             .into()
//!     }
//! }
//!
//! #[derive(omega::Command)]
//! struct SetVolume { volume: Volume }
//!
//! impl Command for SetVolume {
//!     type Input = Percent;
//!     type Output = ();
//!
//!     async fn call(&self, level: Percent) -> omega::Result<()> {
//!         self.volume.set(level).await
//!     }
//! }
//!
//! omega::plugin!().widget::<Panel>().command::<SetVolume>().run()?;
//! # Ok::<(), omega::Error>(())
//! ```
//!
//! A shared domain does not make a control safe to hold on a widget:
//!
//! ```compile_fail
//! use omega::audio::Volume;
//! #[derive(omega::Widget)]
//! struct Panel { volume: Volume }
//! ```

pub use crate::effect::volume::Volume;
pub use crate::reading::{Audio, Media, Playback, Player};
