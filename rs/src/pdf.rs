//! PDF pages → temporary PNG images (parity with Python `pdf.py`).
//!
//! Renders via **PDFium** (`pdfium-render`), Chromium’s PDF engine, in-process.
//! Requires a `libpdfium` shared library at runtime (not linked at compile time).
//! See [`bind_pdfium`] discovery order and `docs/build_rust.md`.
//!
//! ## Temp-dir lifecycle
//!
//! `PdfCache` owns a [`tempfile::TempDir`]. While the cache (and thus the
//! owning [`crate::slideshow::Slideshow`]) is alive, page PNG paths remain
//! valid. When the cache is dropped, the directory and all rasters are removed.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pdfium_render::prelude::*;
use tempfile::TempDir;

use crate::error::{Error, Result};

/// Shared Pdfium binding for the process (library init is process-global).
static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();

/// Pdfium itself is not thread-safe; serialize all document open/render work.
static PDFIUM_LOCK: Mutex<()> = Mutex::new(());

/// Bind to a Pdfium dynamic library.
///
/// Search order:
/// 1. `PDFIUM_LIB_PATH` — full path to `libpdfium.so` / `.dylib` / `.dll`
/// 2. Directory of the current executable
/// 3. `MAGIC_LANTERN_PDFIUM_DIR` — directory containing the platform library
/// 4. Common local install: `~/.local/pdfium/lib`
/// 5. System library path (`Pdfium::bind_to_system_library`)
pub fn bind_pdfium() -> Result<&'static Pdfium> {
    let entry = PDFIUM.get_or_init(|| {
        try_bind_pdfium().map_err(|e| e.to_string())
    });
    match entry {
        Ok(p) => Ok(p),
        Err(msg) => Err(Error::Pdf(msg.clone())),
    }
}

fn try_bind_pdfium() -> Result<Pdfium> {
    // 1. Explicit file path
    if let Ok(path) = std::env::var("PDFIUM_LIB_PATH") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            tracing::info!("PDFium: binding PDFIUM_LIB_PATH={}", p.display());
            return Ok(Pdfium::new(
                Pdfium::bind_to_library(&p).map_err(pdfium_err)?,
            ));
        }
        // Treat as directory if not a file
        if p.is_dir() {
            if let Ok(b) = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&p))
            {
                tracing::info!("PDFium: binding library in PDFIUM_LIB_PATH dir {}", p.display());
                return Ok(Pdfium::new(b));
            }
        }
        return Err(Error::Pdf(format!(
            "PDFIUM_LIB_PATH set but library not found: {path}"
        )));
    }

    // 2. Next to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Ok(b) =
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))
            {
                tracing::info!("PDFium: binding library next to executable {}", dir.display());
                return Ok(Pdfium::new(b));
            }
        }
    }

    // 3. Explicit directory
    if let Ok(dir) = std::env::var("MAGIC_LANTERN_PDFIUM_DIR") {
        let dir = PathBuf::from(dir);
        if let Ok(b) =
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&dir))
        {
            tracing::info!("PDFium: MAGIC_LANTERN_PDFIUM_DIR={}", dir.display());
            return Ok(Pdfium::new(b));
        }
    }

    // 4. ~/.local/pdfium/lib (common manual install location)
    if let Some(home) = std::env::var_os("HOME") {
        let local = PathBuf::from(home).join(".local/pdfium/lib");
        if local.is_dir() {
            if let Ok(b) =
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&local))
            {
                tracing::info!("PDFium: binding {}", local.display());
                return Ok(Pdfium::new(b));
            }
        }
    }

    // 5. System library
    match Pdfium::bind_to_system_library() {
        Ok(b) => {
            tracing::info!("PDFium: bound system library");
            Ok(Pdfium::new(b))
        }
        Err(e) => Err(Error::Pdf(format!(
            "PDFium library not found ({e}). Install libpdfium and set PDFIUM_LIB_PATH, \
             or place libpdfium.so next to the magic-lantern binary. \
             See docs/build_rust.md."
        ))),
    }
}

fn pdfium_err(e: PdfiumError) -> Error {
    Error::Pdf(format!("PDFium: {e:?}"))
}

/// Whether a Pdfium library can be bound on this host.
pub fn pdf_available() -> bool {
    bind_pdfium().is_ok()
}

/// Owns the temporary directory holding rasterized PDF pages.
pub struct PdfCache {
    dir: TempDir,
    /// Render DPI for on-screen slideshows (default 200; Python used 600 for print-ish quality).
    dpi: u32,
    counter: u64,
}

impl std::fmt::Debug for PdfCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfCache")
            .field("dir", &self.dir.path())
            .field("dpi", &self.dpi)
            .field("counter", &self.counter)
            .finish()
    }
}

impl PdfCache {
    pub fn new() -> Result<Self> {
        // Fail early if PDFium cannot be loaded.
        let _ = bind_pdfium()?;
        let dir = TempDir::new().map_err(Error::Io)?;
        tracing::debug!("PDF temp dir: {}", dir.path().display());
        Ok(Self {
            dir,
            dpi: 200,
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
    /// in page order. Names: `{fileName}-page-{n}.png` (0-based, Python style).
    pub fn convert(&mut self, pdf_path: &Path) -> Result<Vec<PathBuf>> {
        let pdfium = bind_pdfium()?;
        let _guard = PDFIUM_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        self.counter += 1;
        let file_name = pdf_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("document.pdf");
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

        let document = pdfium
            .load_pdf_from_file(pdf_path, None)
            .map_err(pdfium_err)?;

        // PDF points are 1/72"; scale to target DPI.
        let scale = self.dpi as f32 / 72.0;
        let render_config = PdfRenderConfig::new().scale_page_by_factor(scale);

        let mut pages = Vec::new();
        for (index, page) in document.pages().iter().enumerate() {
            let image = page
                .render_with_config(&render_config)
                .map_err(pdfium_err)?
                .as_image()
                .map_err(pdfium_err)?;

            let final_name = format!("{safe}-page-{index}.png");
            let mut dest = self.dir.path().join(&final_name);
            if dest.exists() {
                dest = self.dir.path().join(format!(
                    "{}-{}-page-{index}.png",
                    safe, self.counter
                ));
            }

            image
                .save(&dest)
                .map_err(|e| Error::Pdf(format!("save PNG {}: {e}", dest.display())))?;

            tracing::info!("    {}", dest.file_name().unwrap().to_string_lossy());
            pages.push(dest);
        }

        if pages.is_empty() {
            return Err(Error::Pdf(format!(
                "no pages in PDF: {}",
                pdf_path.display()
            )));
        }
        Ok(pages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_example_presentation() {
        if !pdf_available() {
            eprintln!("skipping: PDFium library not available");
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
            assert!(p.starts_with(cache.temp_path()));
        }
    }

    #[test]
    fn temp_dir_cleaned_up_on_drop() {
        if !pdf_available() {
            eprintln!("skipping: PDFium library not available");
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
            eprintln!("skipping: PDFium library not available");
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
        drop(cache);
    }
}
