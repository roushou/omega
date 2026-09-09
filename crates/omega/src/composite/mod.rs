//! Readings that take more than one topic to answer.
//!
//! A [reading] is one topic and no interpretation: `Battery` is the charge,
//! `Mains` is the socket. That is the right shape for a primitive — it is what
//! decides when a widget wakes, and a topic that bundled two things would wake
//! readers of one for changes to the other.
//!
//! But some questions are not one topic. "What is this machine doing about
//! power" needs the charge, whether it is charging, and whether the cable is
//! in — and every unit that wanted that sentence wrote its own three-argument
//! function for it, with its own idea of what a full battery on the wall is.
//!
//! A composite is a field like any other: it declares the union of its parts'
//! topics, costs what they cost, and is safe wherever a reading is. The
//! interpretation lives here once instead of in every unit.
//!
//! [reading]: crate::reading

mod power;

pub use power::{Power, Status};
