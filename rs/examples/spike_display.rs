//! Display spike using the SDL2 backend (same stack as the main app).
//!
//! Usage (from repo root; needs libsdl2-dev or PKG_CONFIG_PATH):
//!   cargo run --example spike_display
//!   cargo run --example spike_display -- path/to/image.jpg

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use image::imageops::FilterType;
use image::GenericImageView;
use magic_lantern::rect::Rect;
use magic_lantern::screen::Screen;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;

fn default_sample() -> PathBuf {
    PathBuf::from("tests/images/paintings/rembrandt.jpg")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_sample);

    println!("=== spike_display (SDL2) ===");
    println!("image: {}", path.display());

    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        std::process::exit(1);
    }

    let mut screen = Screen::new(false)?;
    let img = image::open(&path)?;
    let (iw, ih) = img.dimensions();
    let fit = Rect::from_size(iw as i32, ih as i32).fit(screen.rect());
    println!(
        "source: {iw}x{ih}  fitted: {}x{} at ({},{})",
        fit.w, fit.h, fit.x, fit.y
    );

    let scaled = img
        .resize_exact(fit.w as u32, fit.h as u32, FilterType::Triangle)
        .to_rgba8();

    screen.fill_black();
    screen.blit(&scaled, fit.x, fit.y)?;
    screen.present();

    let mut pump = screen.sdl().event_pump()?;
    let auto = env::var("MAGIC_LANTERN_SPIKE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(Duration::from_secs);
    let start = std::time::Instant::now();

    'running: loop {
        for event in pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'running,
                Event::KeyDown {
                    keycode: Some(Keycode::Q | Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }
        if let Some(d) = auto {
            if start.elapsed() >= d {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    println!("OK");
    Ok(())
}
