//! magic-lantern library (Rust port).
//!
//! Phase 1: config, album discovery, slideshow generation, dry-run.
//! Phase 2: slide pipeline (lazy load/unload, EXIF, fit, PDF temp lifecycle).

pub mod album;
pub mod config;
pub mod error;
pub mod log_setup;
pub mod pdf;
pub mod rect;
pub mod slide;
pub mod slideshow;

pub use config::{AlbumConfig, CliOverrides, Config, Order};
pub use error::{Error, Result};
pub use rect::Rect;
pub use slide::Slide;
pub use slideshow::Slideshow;
