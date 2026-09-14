//! Embedded QML assets for shared controls, standalone hosts, and previews.
//! Protocol-derived accessors are checked against their generators.
//! Host-specific installation belongs to `omega-omarchy`.

mod core;
mod desktop;
pub use desktop::Desktop;
mod icons;
mod props;

pub use core::{Asset, Core};
pub use icons::Icons;
pub use props::Props;

mod preview;
pub use preview::Preview;
