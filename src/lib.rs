//! CosmicSnip: snip a region through the COSMIC screenshot portal, then
//! annotate it. The pure parts - the annotation model and the renderer - live
//! here so they are tested without a display.

pub mod annotation;
pub mod app;
pub mod capture;
pub mod clipboard;
pub mod config;
pub mod prefs;
pub mod render;
