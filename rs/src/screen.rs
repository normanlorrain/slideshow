//! Window / framebuffer (Python `screen.py` via minifb).
//!
//! Production long-term target remains SDL2 (see rust.md §18); minifb works
//! without `libsdl2-dev` and already powers the Phase 0 display spike.

use image::RgbaImage;
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};

use crate::error::{Error, Result};
use crate::rect::{DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, Rect};

/// Display surface: software framebuffer presented through a window.
pub struct Screen {
    window: Window,
    buffer: Vec<u32>,
    width: usize,
    height: usize,
}

impl Screen {
    pub fn new(fullscreen: bool) -> Result<Self> {
        let (width, height) = if fullscreen {
            // Borderless “fullscreen-ish”; true display size is platform-specific.
            // Prefer the configured default unless the user has a typical HD panel.
            (DEFAULT_SCREEN_WIDTH as usize, DEFAULT_SCREEN_HEIGHT as usize)
        } else {
            (DEFAULT_SCREEN_WIDTH as usize, DEFAULT_SCREEN_HEIGHT as usize)
        };

        let mut opts = WindowOptions {
            resize: false,
            scale: Scale::X1,
            ..WindowOptions::default()
        };
        if fullscreen {
            opts.borderless = true;
            opts.title = false;
        }

        let mut window = Window::new("magic-lantern", width, height, opts)
            .map_err(|e| Error::Display(format!("failed to open window: {e}")))?;

        window.set_cursor_visibility(false);
        // Match roughly pygame key repeat (delay 1000ms, interval 100ms) —
        // minifb's built-in repeat is coarser; we mostly use KeyRepeat::No.
        window.set_target_fps(30);

        tracing::info!("Screen size {width} x {height}");

        Ok(Self {
            window,
            buffer: vec![0u32; width * height],
            width,
            height,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn rect(&self) -> Rect {
        Rect::from_size(self.width as i32, self.height as i32)
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    pub fn fill_black(&mut self) {
        self.buffer.fill(0);
    }

    /// Blit an RGBA image at top-left `(x, y)` (clips to screen).
    pub fn blit(&mut self, image: &RgbaImage, x: i32, y: i32) {
        let iw = image.width() as i32;
        let ih = image.height() as i32;
        let sw = self.width as i32;
        let sh = self.height as i32;

        for py in 0..ih {
            let dy = y + py;
            if dy < 0 || dy >= sh {
                continue;
            }
            for px in 0..iw {
                let dx = x + px;
                if dx < 0 || dx >= sw {
                    continue;
                }
                let p = image.get_pixel(px as u32, py as u32).0;
                let [r, g, b, a] = p;
                if a == 0 {
                    continue;
                }
                // minifb 0RGB; simple replace (no alpha blend for photo blits).
                let color = if a == 255 {
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
                } else {
                    // Text overlays use partial alpha — blend onto existing.
                    let dst = self.buffer[dy as usize * self.width + dx as usize];
                    let dr = (dst >> 16) & 0xff;
                    let dg = (dst >> 8) & 0xff;
                    let db = dst & 0xff;
                    let aa = a as u32;
                    let inv = 255 - aa;
                    let nr = (r as u32 * aa + dr * inv) / 255;
                    let ng = (g as u32 * aa + dg * inv) / 255;
                    let nb = (b as u32 * aa + db * inv) / 255;
                    (nr << 16) | (ng << 8) | nb
                };
                self.buffer[dy as usize * self.width + dx as usize] = color;
            }
        }
    }

    /// Present the framebuffer and process window events.
    pub fn present(&mut self) -> Result<()> {
        self.window
            .update_with_buffer(&self.buffer, self.width, self.height)
            .map_err(|e| Error::Display(format!("present failed: {e}")))
    }

    pub fn is_key_pressed(&self, key: Key) -> bool {
        self.window.is_key_pressed(key, KeyRepeat::No)
    }

    /// True if any of the keys were pressed this frame.
    pub fn any_key_pressed(&self, keys: &[Key]) -> bool {
        keys.iter().any(|k| self.is_key_pressed(*k))
    }
}
