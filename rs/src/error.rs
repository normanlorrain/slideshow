//! Application error types.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("slideshow error: {0}")]
    Slideshow(String),

    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("PDF conversion failed: {0}")]
    Pdf(String),

    /// Bad or unreadable slide file (Python `SlideException`).
    #[error("bad slide file: {0}")]
    Slide(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;
