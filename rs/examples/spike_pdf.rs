//! Phase 0 spike: rasterize page 0 of a PDF to PNG.
//!
//! Strategy A (this spike): shell out to Poppler's `pdftoppm` (already on many
//! Linux desktops; present in this environment via `poppler-utils`).
//!
//! Strategy B (decision target): in-process library — see rust.md Phase 0 notes.
//!
//! Usage (from repo root):
//!   cargo run --example spike_pdf
//!   cargo run --example spike_pdf -- path/to/file.pdf

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn default_sample() -> PathBuf {
    PathBuf::from("tests/pdfs/Example presentation.pdf")
}

fn which(cmd: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let p = dir.join(cmd);
            p.is_file().then_some(p)
        })
    })
}

/// Rasterize first page at ~600 DPI (matches Python pymupdf `get_pixmap(dpi=600)`).
fn convert_page0_pdftoppm(pdf: &Path, out_prefix: &Path) -> Result<PathBuf, String> {
    let pdftoppm = which("pdftoppm").ok_or_else(|| {
        "pdftoppm not found on PATH (install poppler-utils)".to_string()
    })?;

    let status = Command::new(&pdftoppm)
        .args([
            "-png",
            "-r",
            "600",
            "-f",
            "1",
            "-l",
            "1",
            "-singlefile",
        ])
        .arg(pdf)
        .arg(out_prefix)
        .status()
        .map_err(|e| format!("failed to spawn pdftoppm: {e}"))?;

    if !status.success() {
        return Err(format!("pdftoppm exited with {status}"));
    }

    let png = out_prefix.with_extension("png");
    if !png.is_file() {
        return Err(format!("expected output missing: {}", png.display()));
    }
    Ok(png)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdf = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_sample);

    println!("=== spike_pdf ===");
    println!("pdf: {}", pdf.display());

    if !pdf.exists() {
        eprintln!("File not found: {}", pdf.display());
        eprintln!("Run from the repository root.");
        std::process::exit(1);
    }

    if let Some(p) = which("pdftoppm") {
        println!("pdftoppm: {}", p.display());
    } else {
        eprintln!("pdftoppm not found — cannot complete PDF spike.");
        std::process::exit(1);
    }

    let tmp = tempfile::tempdir()?;
    let prefix = tmp.path().join("page0");
    let png = convert_page0_pdftoppm(&pdf, &prefix).map_err(|e| {
        std::io::Error::other(e)
    })?;

    let meta = image::image_dimensions(&png)?;
    println!("raster: {} ({} x {})", png.display(), meta.0, meta.1);

    let dest = PathBuf::from("/tmp/magic-lantern-spike-pdf.png");
    std::fs::copy(&png, &dest)?;
    println!("persisted: {}", dest.display());
    println!("OK");
    Ok(())
}
