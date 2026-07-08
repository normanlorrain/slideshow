//! PDF pages → temporary PNG images (parity with Python `pdf.py`).
//!
//! Phase 0/1/2 uses Poppler's `pdftoppm` CLI. Later we may switch to
//! in-process PDFium (`pdfium-render`) per rust.md §18.
//!
//! ## Temp-dir lifecycle
//!
//! `PdfCache` owns a [`tempfile::TempDir`]. While the cache (and thus the
//! owning [`crate::slideshow::Slideshow`]) is alive, page PNG paths remain
//! valid. When the cache is dropped, the directory and all rasters are
//! removed — matching Python's `TemporaryDirectory` cleanup on process exit
//! / GC of the module-level `_tempDir`.

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
    ///
    /// Output names follow Python style: `{fileName}-page-{n}.png` (1-based
    /// page numbers for pdftoppm; Python's pymupdf is 0-based in the name —
    /// we use 1-based page index from pdftoppm which is fine for uniqueness).
    pub fn convert(&mut self, pdf_path: &Path) -> Result<Vec<PathBuf>> {
        if which("pdftoppm").is_none() {
            return Err(Error::Pdf(
                "pdftoppm not found on PATH (install poppler-utils)".into(),
            ));
        }

        self.counter += 1;
        let file_name = pdf_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("document.pdf");
        // Sanitize for filesystem while keeping a stable prefix.
        let safe: String = file_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        let work = self
            .dir
            .path()
            .join(format!("pdf-{}-{}", self.counter, safe));
        // pdftoppm appends -1.png etc. to this prefix
        let prefix = work.with_extension(""); // strip .pdf if present in safe name
        let prefix = PathBuf::from(format!("{}-{}", prefix.display(), self.counter));

        let status = Command::new("pdftoppm")
            .args(["-png", "-r", &self.dpi.to_string()])
            .arg(pdf_path)
            .arg(&prefix)
            .status()
            .map_err(|e| Error::Pdf(format!("failed to spawn pdftoppm: {e}")))?;

        if !status.success() {
            return Err(Error::Pdf(format!("pdftoppm exited with {status}")));
        }

        // pdftoppm writes prefix-1.png, prefix-2.png, ...
        // Rename into Python-like `{file}-page-{n}.png` in the temp dir.
        let mut pages = Vec::new();
        let mut n = 1u32;
        loop {
            let candidate = PathBuf::from(format!("{}-{n}.png", prefix.display()));
            if !candidate.is_file() {
                break;
            }
            // Python: f"{fileName}-page-{page.number}.png" (0-based page.number)
            // We use 0-based in the final name to match Python naming.
            let final_name = format!("{safe}-page-{}.png", n - 1);
            let dest = self.dir.path().join(&final_name);
            // If name collides (same PDF converted twice), uniquify.
            let dest = if dest.exists() {
                self.dir
                    .path()
                    .join(format!("{}-{}-page-{}.png", safe, self.counter, n - 1))
            } else {
                dest
            };
            std::fs::rename(&candidate, &dest).map_err(Error::Io)?;
            tracing::info!("    {}", dest.file_name().unwrap().to_string_lossy());
            pages.push(dest);
            n += 1;
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
        let mut cache = PdfCache::with_dpi(72).unwrap();
        let pages = cache.convert(&pdf).unwrap();
        assert_eq!(pages.len(), 4, "example PDF has 4 pages");
        for (i, p) in pages.iter().enumerate() {
            assert!(p.is_file(), "missing {}", p.display());
            let name = p.file_name().unwrap().to_string_lossy();
            assert!(
                name.contains(&format!("page-{i}")),
                "expected page-{i} in {name}"
            );
            // Paths live under the cache temp dir.
            assert!(p.starts_with(cache.temp_path()));
        }
    }

    #[test]
    fn temp_dir_cleaned_up_on_drop() {
        if !pdf_available() {
            eprintln!("skipping: pdftoppm not available");
            return;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let pdf = root.join("tests/pdfs/Example presentation.pdf");

        let (temp_dir, page0) = {
            let mut cache = PdfCache::with_dpi(36).unwrap();
            let pages = cache.convert(&pdf).unwrap();
            let page0 = pages[0].clone();
            assert!(page0.is_file());
            (cache.temp_path().to_path_buf(), page0)
        };
        // Cache dropped — temp dir and files must be gone.
        assert!(
            !temp_dir.exists(),
            "temp dir should be removed on PdfCache drop: {}",
            temp_dir.display()
        );
        assert!(!page0.exists(), "page file should be removed with temp dir");
    }

    #[test]
    fn slides_from_pdf_can_load_while_cache_alive() {
        if !pdf_available() {
            eprintln!("skipping: pdftoppm not available");
            return;
        }
        use crate::slide::Slide;

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let pdf = root.join("tests/pdfs/Example presentation.pdf");
        let mut cache = PdfCache::with_dpi(36).unwrap();
        let pages = cache.convert(&pdf).unwrap();
        let slide = Slide::new(&pages[0], 5);
        let surface = slide.get_surface_default().unwrap();
        assert!(surface.width() > 0 && surface.height() > 0);
        // Keep cache in scope until after load.
        drop(cache);
    }
}
