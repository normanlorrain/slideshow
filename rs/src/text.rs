//! Text overlays (Python `text.py`).
//!
//! Uses system FreeSans Bold (or DejaVu Sans Bold) via `fontdue`, approximating
//! pygame `SysFont("freesansbold", …)`.

use std::fs;
use std::path::{Path, PathBuf};

use fontdue::Font;
use image::{Rgba, RgbaImage};

use crate::error::{Error, Result};

/// Normal overlay size (pygame 36).
pub const STYLE_NORMAL: f32 = 36.0;
/// Heading / year size (pygame 72).
pub const STYLE_HEADING: f32 = 72.0;

/// Green used for overlays: `(46, 176, 80)`.
pub const GREEN: [u8; 4] = [46, 176, 80, 255];

pub struct TextRenderer {
    font: Font,
}

impl TextRenderer {
    pub fn new() -> Result<Self> {
        let path = find_sans_bold_font().ok_or_else(|| {
            Error::Display(
                "no system sans-bold font found (tried FreeSansBold, DejaVuSans-Bold, LiberationSans-Bold)"
                    .into(),
            )
        })?;
        tracing::debug!("Text font: {}", path.display());
        let bytes = fs::read(&path)?;
        let font = Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(|e| Error::Display(format!("font parse error: {e}")))?;
        Ok(Self { font })
    }

    /// Render `msg` into an RGBA image (transparent background, green glyphs).
    pub fn create_message(&self, msg: &str, size: f32, colour: [u8; 4]) -> RgbaImage {
        if msg.is_empty() {
            return RgbaImage::new(1, 1);
        }

        // Measure
        let mut width = 0.0f32;
        let mut min_y = 0.0f32;
        let mut max_y = 0.0f32;
        let mut glyphs = Vec::new();

        for ch in msg.chars() {
            let (metrics, bitmap) = self.font.rasterize(ch, size);
            glyphs.push((metrics, bitmap));
            width += metrics.advance_width;
            // ymin is typically negative (baseline-relative)
            let top = metrics.ymin as f32;
            let bot = metrics.ymin as f32 + metrics.height as f32;
            if top < min_y {
                min_y = top;
            }
            if bot > max_y {
                max_y = bot;
            }
        }

        let height = (max_y - min_y).ceil().max(1.0) as u32;
        let width = width.ceil().max(1.0) as u32;
        let mut img = RgbaImage::new(width, height);

        let mut pen_x = 0.0f32;
        for (metrics, bitmap) in glyphs {
            let gx = pen_x + metrics.xmin as f32;
            let gy = metrics.ymin as f32 - min_y;
            blit_glyph(
                &mut img,
                &bitmap,
                metrics.width,
                metrics.height,
                gx.round() as i32,
                gy.round() as i32,
                colour,
            );
            pen_x += metrics.advance_width;
        }

        img
    }

    pub fn message_normal(&self, msg: &str) -> RgbaImage {
        self.create_message(msg, STYLE_NORMAL, GREEN)
    }

    pub fn message_heading(&self, msg: &str) -> RgbaImage {
        self.create_message(msg, STYLE_HEADING, GREEN)
    }
}

fn blit_glyph(
    img: &mut RgbaImage,
    bitmap: &[u8],
    gw: usize,
    gh: usize,
    x0: i32,
    y0: i32,
    colour: [u8; 4],
) {
    let iw = img.width() as i32;
    let ih = img.height() as i32;
    for row in 0..gh {
        for col in 0..gw {
            let coverage = bitmap[row * gw + col];
            if coverage == 0 {
                continue;
            }
            let x = x0 + col as i32;
            let y = y0 + row as i32;
            if x < 0 || y < 0 || x >= iw || y >= ih {
                continue;
            }
            let a = (coverage as u32 * colour[3] as u32 / 255) as u8;
            img.put_pixel(
                x as u32,
                y as u32,
                Rgba([colour[0], colour[1], colour[2], a]),
            );
        }
    }
}

fn find_sans_bold_font() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/truetype/freefont/FreeSansBold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
    ];
    for c in CANDIDATES {
        let p = Path::new(c);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_pause_and_year() {
        let t = TextRenderer::new().expect("system font");
        let pause = t.message_normal("PAUSE");
        assert!(pause.width() > 10);
        assert!(pause.height() > 10);

        let year = t.message_heading("1992");
        assert!(year.width() > pause.width() || year.height() > pause.height());

        // Green-ish opaque pixels present
        let has_green = pause.pixels().any(|p| p.0[1] > 100 && p.0[3] > 0);
        assert!(has_green);
    }
}
