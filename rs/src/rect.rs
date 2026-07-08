//! Rectangle helpers, including pygame `Rect.fit` ported exactly.
//!
//! Source: pygame `src_c/rect.c` → `pg_rect_fit`:
//! ```c
//! xratio = (float)self->r.w / (float)argrect->w;
//! yratio = (float)self->r.h / (float)argrect->h;
//! maxratio = (xratio > yratio) ? xratio : yratio;
//! w = (int)(self->r.w / maxratio);
//! h = (int)(self->r.h / maxratio);
//! x = argrect->x + (argrect->w - w) / 2;
//! y = argrect->y + (argrect->h - h) / 2;
//! ```
//!
//! Uses C `float` semantics and truncating casts to `int`.

/// Axis-aligned rectangle (x, y, width, height), matching SDL/pygame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn from_size(w: i32, h: i32) -> Self {
        Self { x: 0, y: 0, w, h }
    }

    /// Fit `self` inside `into` while preserving aspect ratio, centered.
    ///
    /// Exact port of pygame `Rect.fit`.
    pub fn fit(self, into: Rect) -> Rect {
        // pygame uses C float (f32), not double.
        let xratio = self.w as f32 / into.w as f32;
        let yratio = self.h as f32 / into.h as f32;
        let maxratio = if xratio > yratio { xratio } else { yratio };

        // Truncating cast toward zero, same as C `(int)(float)`.
        let w = (self.w as f32 / maxratio) as i32;
        let h = (self.h as f32 / maxratio) as i32;

        // C integer division truncates toward zero.
        let x = into.x + (into.w - w) / 2;
        let y = into.y + (into.h - h) / 2;

        Rect { x, y, w, h }
    }
}

/// Default window size from Python `screen.py` before fullscreen.
pub const DEFAULT_SCREEN_WIDTH: u32 = 1280;
pub const DEFAULT_SCREEN_HEIGHT: u32 = 720;

pub fn default_screen_rect() -> Rect {
    Rect::from_size(DEFAULT_SCREEN_WIDTH as i32, DEFAULT_SCREEN_HEIGHT as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_same_aspect_fills() {
        let img = Rect::from_size(1920, 1080);
        let scr = Rect::from_size(1280, 720);
        let f = img.fit(scr);
        assert_eq!(f, Rect::new(0, 0, 1280, 720));
    }

    #[test]
    fn fit_portrait_letterboxes_horizontally() {
        // rembrandt-ish 689x899 into 1280x720
        let img = Rect::from_size(689, 899);
        let scr = Rect::from_size(1280, 720);
        let f = img.fit(scr);
        // maxratio = 899/720
        let maxratio = 899f32 / 720f32;
        let w = (689f32 / maxratio) as i32;
        let h = (899f32 / maxratio) as i32;
        let x = (1280 - w) / 2;
        let y = (720 - h) / 2;
        assert_eq!(f, Rect::new(x, y, w, h));
        assert_eq!(f.h, 720);
        assert!(f.w < 1280);
        assert!(f.x > 0);
        assert_eq!(f.y, 0);
    }

    #[test]
    fn fit_landscape_letterboxes_vertically() {
        let img = Rect::from_size(1024, 803);
        let scr = Rect::from_size(1280, 720);
        let f = img.fit(scr);
        // xratio=0.8, yratio≈1.115 → max=yratio → height fills
        assert!((803f32 / 720f32) > (1024f32 / 1280f32));
        let maxratio = 803f32 / 720f32;
        let w = (1024f32 / maxratio) as i32;
        let h = (803f32 / maxratio) as i32;
        assert_eq!(f.w, w);
        assert_eq!(f.h, h);
        assert_eq!(f.h, 720);
        assert_eq!(f.y, 0);
        assert_eq!(f.x, (1280 - w) / 2);
    }

    #[test]
    fn fit_square_into_wide() {
        let img = Rect::from_size(100, 100);
        let scr = Rect::from_size(200, 100);
        let f = img.fit(scr);
        // xratio=0.5, yratio=1.0 → max=1 → size stays 100x100, centered x
        assert_eq!(f, Rect::new(50, 0, 100, 100));
    }

    #[test]
    fn fit_identity() {
        let r = Rect::from_size(1280, 720);
        assert_eq!(r.fit(r), r);
    }

    #[test]
    fn fit_uses_float_not_double_truncation() {
        // Values chosen so f32 vs f64 could diverge on cast edges.
        let img = Rect::from_size(1000, 333);
        let scr = Rect::from_size(640, 480);
        let f = img.fit(scr);
        let xratio = 1000f32 / 640f32;
        let yratio = 333f32 / 480f32;
        let maxratio = if xratio > yratio { xratio } else { yratio };
        let w = (1000f32 / maxratio) as i32;
        let h = (333f32 / maxratio) as i32;
        assert_eq!(f.w, w);
        assert_eq!(f.h, h);
    }
}
