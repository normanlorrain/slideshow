//! magic-lantern library (Rust port).
//!
//! Phase 1: config, album discovery, slideshow generation, dry-run.
//! Phase 2: slide pipeline (lazy load/unload, EXIF, fit, PDF temp lifecycle).
//! Phase 3: controller / UI, overlays, SIGUSR1 reload.

pub mod album;
pub mod config;
pub mod controller;
pub mod error;
pub mod log_setup;
pub mod pdf;
pub mod rect;
pub mod screen;
pub mod signal_handler;
pub mod slide;
pub mod slideshow;
pub mod text;

pub use config::{AlbumConfig, CliOverrides, Config, Order};
pub use controller::Controller;
pub use error::{Error, Result};
pub use rect::Rect;
pub use slide::Slide;
pub use slideshow::Slideshow;
