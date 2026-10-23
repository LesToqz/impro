//! Shared fixture, camera paths and timing harness for the graph renderer
//! and layout measurements. Every candidate binary links this and only
//! supplies a [`harness::Renderer`].

pub mod fixture;
pub mod harness;
#[cfg(feature = "labels")]
pub mod labels;
#[cfg(feature = "gl")]
pub mod egl;
#[cfg(feature = "glscene")]
pub mod glscene;
