//! PDF spike using PDFium (same backend as the main app).
//!
//! Usage (from repo root; needs libpdfium — see docs/build_rust.md):
//!   cargo run --example spike_pdf
//!   PDFIUM_LIB_PATH=$HOME/.local/pdfium/lib/libpdfium.so cargo run --example spike_pdf

use std::env;
use std::path::PathBuf;

use magic_lantern::pdf::{pdf_available, PdfCache};

fn default_sample() -> PathBuf {
    PathBuf::from("tests/pdfs/Example presentation.pdf")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdf = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_sample);

    println!("=== spike_pdf (PDFium) ===");
    println!("pdf: {}", pdf.display());

    if !pdf.exists() {
        eprintln!("File not found: {}", pdf.display());
        std::process::exit(1);
    }
    if !pdf_available() {
        eprintln!(
            "PDFium library not found.\n\
             Set PDFIUM_LIB_PATH or place libpdfium.so next to the binary.\n\
             See docs/build_rust.md."
        );
        std::process::exit(1);
    }

    let mut cache = PdfCache::with_dpi(200)?;
    let pages = cache.convert(&pdf)?;
    println!("pages: {}", pages.len());
    for (i, p) in pages.iter().enumerate() {
        let (w, h) = image::image_dimensions(p)?;
        println!("  [{i}] {} ({} x {})", p.display(), w, h);
    }

    if let Some(first) = pages.first() {
        let dest = PathBuf::from("/tmp/magic-lantern-spike-pdf.png");
        std::fs::copy(first, &dest)?;
        println!("persisted first page: {}", dest.display());
    }
    println!("OK");
    Ok(())
}
