//! A single slideshow image (lazy pixel load comes in a later phase).

use std::path::{Path, PathBuf};

/// One slide in an album / slideshow history.
#[derive(Debug, Clone)]
pub struct Slide {
    pub path: PathBuf,
    pub interval: u64,
    /// True when the image has been loaded into memory (Phase 2+).
    pub image_loaded: bool,
}

impl Slide {
    pub fn new(path: impl Into<PathBuf>, interval: u64) -> Self {
        Self {
            path: path.into(),
            interval,
            image_loaded: false,
        }
    }

    pub fn filename(&self) -> &str {
        self.path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
    }

    /// Last two path components, joined with `/` (Python dry-run format).
    pub fn dry_run_label(&self) -> String {
        let parts: Vec<&str> = self
            .path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        match parts.len() {
            0 => String::new(),
            1 => parts[0].to_string(),
            _ => format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]),
        }
    }

    /// Drop any loaded image data to free memory (no-op until Phase 2).
    pub fn unload_image(&mut self) {
        self.image_loaded = false;
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl PartialEq for Slide {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for Slide {}

impl PartialOrd for Slide {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Slide {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}
