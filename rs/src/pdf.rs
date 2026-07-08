//! PDF pages → temporary PNG images (parity with Python `pdf.py`).
//!
//! Phase 0/1 uses Poppler's `pdftoppm` CLI. Later we may switch to
//! in-process PDFium (`pdfium-render`) per rust.md §18.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::error::{Error, Result};

/// Owns the temporary directory holding rasterized PDF pages.
#[derive(Debug)]
pub struct PdfCache {
    dir: TempDir,
    /// Render DPI (Python uses 600).
    dpi: u32,
    counter: u64,
}

impl PdfCache {
    pub fn new() -> Result<Self> {
        let dir = TempDir::new().map_err(Error::Io)?;
        tracing::debug!("PDF temp dir: {}", dir.path().display());
        Ok(Self {
            dir,
            dpi: 600,
            counter: 0,
        })
    }

    /// Create a cache with a custom DPI (useful for faster tests).
    pub fn with_dpi(dpi: u32) -> Result<Self> {
        let mut c = Self::new()?;
        c.dpi = dpi;
        Ok(c)
    }

    pub fn temp_path(&self) -> &Path {
        self.dir.path()
    }

    /// Rasterize every page of `pdf_path` to PNG files; return their paths
    /// in page order.
    pub fn convert(&mut self, pdf_path: &Path) -> Result<Vec<PathBuf>> {
        if which("pdftoppm").is_none() {
            return Err(Error::Pdf(
                "pdftoppm not found on PATH (install poppler-utils)".into(),
            ));
        }

        self.counter += 1;
        let stem = pdf_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("page");
        // Sanitize for filesystem
        let stem: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let prefix = self
            .dir
            .path()
            .join(format!("{stem}-{}", self.counter));

        let status = Command::new("pdftoppm")
            .args([
                "-png",
                "-r",
                &self.dpi.to_string(),
            ])
            .arg(pdf_path)
            .arg(&prefix)
            .status()
            .map_err(|e| Error::Pdf(format!("failed to spawn pdftoppm: {e}")))?;

        if !status.success() {
            return Err(Error::Pdf(format!("pdftoppm exited with {status}")));
        }

        // pdftoppm writes prefix-1.png, prefix-2.png, ...
        let mut pages = Vec::new();
        let mut n = 1u32;
        loop {
            let candidate = PathBuf::from(format!("{}-{n}.png", prefix.display()));
            if candidate.is_file() {
                tracing::info!("    {}-page-{n}.png", pdf_path.file_name().unwrap_or_default().to_string_lossy());
                pages.push(candidate);
                n += 1;
            } else {
                break;
            }
        }

        if pages.is_empty() {
            return Err(Error::Pdf(format!(
                "no pages rasterized for {}",
                pdf_path.display()
            )));
        }
        Ok(pages)
    }
}

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let p = dir.join(cmd);
            p.is_file().then_some(p)
        })
    })
}

/// Whether PDF conversion is available on this host.
pub fn pdf_available() -> bool {
    which("pdftoppm").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_example_presentation() {
        if !pdf_available() {
            eprintln!("skipping: pdftoppm not available");
            return;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let pdf = root.join("tests/pdfs/Example presentation.pdf");
        // Low DPI for speed in unit tests.
        let mut cache = PdfCache::with_dpi(72).unwrap();
        let pages = cache.convert(&pdf).unwrap();
        assert_eq!(pages.len(), 4, "example PDF has 4 pages");
        for p in &pages {
            assert!(p.is_file(), "missing {}", p.display());
        }
    }
}
