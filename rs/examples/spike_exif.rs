//! Phase 0 spike: load a JPEG, read EXIF orientation + date, apply rotation.
//!
//! Usage (from repo root):
//!   cargo run --example spike_exif
//!   cargo run --example spike_exif -- path/to/image.jpg

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use image::imageops;
use image::DynamicImage;

/// Match Python `slide.py`: pygame.transform.rotate is counter-clockwise.
///   EXIF 3 → 180°, EXIF 6 → 270° CCW, EXIF 8 → 90° CCW.
fn apply_orientation_pygame_compatible(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        3 => img.rotate180(),
        6 => img.rotate270(),
        8 => img.rotate90(),
        _ => img,
    }
}

fn pygame_ccw_degrees(orientation: u32) -> Option<u32> {
    match orientation {
        3 => Some(180),
        6 => Some(270),
        8 => Some(90),
        1 => Some(0),
        _ => None,
    }
}

struct ExifInfo {
    orientation: Option<u32>,
    datetime_original: Option<String>,
}

fn read_exif(path: &Path) -> Result<ExifInfo, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader)?;

    let orientation = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0));

    let datetime_original = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .map(|f| f.display_value().with_unit(&exif).to_string());

    Ok(ExifInfo {
        orientation,
        datetime_original,
    })
}

fn default_sample() -> PathBuf {
    PathBuf::from("tests/images/paintings/davinci.jpg")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_sample);

    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        eprintln!("Run from the repository root.");
        std::process::exit(1);
    }

    println!("=== spike_exif ===");
    println!("image: {}", path.display());

    let info = match read_exif(&path) {
        Ok(i) => i,
        Err(e) => {
            println!("EXIF: none or unreadable ({e})");
            ExifInfo {
                orientation: None,
                datetime_original: None,
            }
        }
    };

    match info.orientation {
        Some(o) => {
            println!("orientation tag: {o}");
            println!("pygame CCW degrees: {:?}", pygame_ccw_degrees(o));
        }
        None => println!("orientation tag: (absent, treat as 1)"),
    }
    match &info.datetime_original {
        Some(d) => println!("DateTimeOriginal: {d}"),
        None => println!("DateTimeOriginal: (absent)"),
    }

    let img = image::open(&path)?;
    println!("pixels before: {} x {}", img.width(), img.height());

    let oriented = if let Some(o) = info.orientation {
        apply_orientation_pygame_compatible(img, o)
    } else {
        img
    };
    println!(
        "pixels after orientation: {} x {}",
        oriented.width(),
        oriented.height()
    );

    let fitted = imageops::resize(
        &oriented,
        oriented.width().min(320).max(1),
        oriented.height().min(180).max(1),
        imageops::FilterType::Triangle,
    );
    fitted.save("/tmp/magic-lantern-spike-exif.png")?;
    println!("persisted: /tmp/magic-lantern-spike-exif.png");
    println!("OK");
    Ok(())
}
