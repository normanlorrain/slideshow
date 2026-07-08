//! magic-lantern library (Rust port).
//!
//! Phase 1: config, album discovery, slideshow generation, dry-run.

pub mod album;
pub mod config;
pub mod error;
pub mod log_setup;
pub mod pdf;
pub mod slide;
pub mod slideshow;

pub use config::{AlbumConfig, CliOverrides, Config, Order};
pub use error::{Error, Result};
pub use slideshow::Slideshow;
