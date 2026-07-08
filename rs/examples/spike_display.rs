//! Phase 0 spike: open a window, blit a scaled image, quit on key / close.
//!
//! Uses `minifb` so the spike works without `libsdl2-dev` (not installed in
//! this environment). Production display target remains SDL2 — see rust.md.
//!
//! Usage (from repo root):
//!   cargo run --example spike_display
//!   cargo run --example spike_display -- path/to/image.jpg
//!
//! Keys: q or Escape to quit.
//! Optional: MAGIC_LANTERN_SPIKE_SECS=2 auto-closes after N seconds (for CI).

use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use image::imageops::FilterType;
use image::GenericImageView;
use minifb::{Key, KeyRepeat, Window, WindowOptions};

const WIN_W: usize = 1280;
const WIN_H: usize = 720;

fn default_sample() -> PathBuf {
    PathBuf::from("tests/images/numbers/1_pexels-padrinan-2249528.jpg")
}

/// Fit `src` into `dst` preserving aspect ratio (letterbox), same idea as pygame Rect.fit.
fn fit_size(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> (u32, u32, u32, u32) {
    let src_w = src_w.max(1) as f64;
    let src_h = src_h.max(1) as f64;
    let dst_w = dst_w.max(1) as f64;
    let dst_h = dst_h.max(1) as f64;
    let scale = (dst_w / src_w).min(dst_h / src_h);
    let w = (src_w * scale).round().max(1.0) as u32;
    let h = (src_h * scale).round().max(1.0) as u32;
    let x = ((dst_w - w as f64) / 2.0).round().max(0.0) as u32;
    let y = ((dst_h - h as f64) / 2.0).round().max(0.0) as u32;
    (x, y, w, h)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_sample);

    println!("=== spike_display ===");
    println!("image: {}", path.display());
    println!("window: {WIN_W}x{WIN_H}  (keys: q / Escape to quit)");

    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        eprintln!("Run from the repository root.");
        std::process::exit(1);
    }

    let img = image::open(&path)?;
    let (iw, ih) = img.dimensions();
    let (x, y, fw, fh) = fit_size(iw, ih, WIN_W as u32, WIN_H as u32);
    println!("source: {iw}x{ih}  fitted: {fw}x{fh} at ({x},{y})");

    let scaled = img.resize_exact(fw, fh, FilterType::Triangle);
    let rgba = scaled.to_rgba8();

    // minifb wants 0x00RRGGBB (or 0xAARRGGBB depending on config); use 0RGB.
    let mut buffer = vec![0u32; WIN_W * WIN_H];
    for py in 0..fh as usize {
        for px in 0..fw as usize {
            let pixel = rgba.get_pixel(px as u32, py as u32);
            let [r, g, b, _a] = pixel.0;
            let dest_x = x as usize + px;
            let dest_y = y as usize + py;
            if dest_x < WIN_W && dest_y < WIN_H {
                buffer[dest_y * WIN_W + dest_x] =
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            }
        }
    }

    let mut window = Window::new(
        "magic-lantern spike_display",
        WIN_W,
        WIN_H,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    )?;

    window.set_target_fps(30);

    let auto_quit = env::var("MAGIC_LANTERN_SPIKE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs);
    if let Some(d) = auto_quit {
        println!("auto-quit after {d:?} (MAGIC_LANTERN_SPIKE_SECS)");
    }
    let started = Instant::now();

    while window.is_open() {
        if window.is_key_pressed(Key::Q, KeyRepeat::No)
            || window.is_key_pressed(Key::Escape, KeyRepeat::No)
        {
            break;
        }
        if let Some(d) = auto_quit {
            if started.elapsed() >= d {
                break;
            }
        }
        window.update_with_buffer(&buffer, WIN_W, WIN_H)?;
    }

    println!("OK (window closed)");
    Ok(())
}
