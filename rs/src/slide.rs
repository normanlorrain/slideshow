//! A single slideshow image with lazy pixel load / unload.
//!
//! Mirrors Python `slide.py`:
//! - load on demand (`get_surface` / `coordinates`)
//! - EXIF orientation (tags 3/6/8) and DateTimeOriginal
//! - fit-to-screen via pygame `Rect.fit`, then smooth scale
//! - unload to free memory (history window)
//!
//! Identity is shared via `Rc` so album, history, and current all refer to
//! the same underlying state — unload from history frees the shared image.

use std::cell::RefCell;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};

use crate::error::{Error, Result};
use crate::rect::{default_screen_rect, Rect};

/// One slide in an album / slideshow history (cheaply cloneable handle).
#[derive(Clone)]
pub struct Slide {
    inner: Rc<RefCell<SlideInner>>,
}

struct SlideInner {
    path: PathBuf,
    interval: u64,
    /// Original pixel size after EXIF orientation (before fit scale).
    width: u32,
    height: u32,
    /// Top-left of fitted image on the screen.
    x: i32,
    y: i32,
    datetime: String,
    orientation: Option<u32>,
    /// Fitted / scaled RGBA buffer ready to blit (Python `surface`).
    image: Option<RgbaImage>,
    image_loaded: bool,
}

impl Slide {
    pub fn new(path: impl Into<PathBuf>, interval: u64) -> Self {
        Self {
            inner: Rc::new(RefCell::new(SlideInner {
                path: path.into(),
                interval,
                width: 0,
                height: 0,
                x: 0,
                y: 0,
                datetime: String::new(),
                orientation: None,
                image: None,
                image_loaded: false,
            })),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.inner.borrow().path.clone()
    }

    pub fn path_ref<R>(&self, f: impl FnOnce(&Path) -> R) -> R {
        f(&self.inner.borrow().path)
    }

    pub fn interval(&self) -> u64 {
        self.inner.borrow().interval
    }

    pub fn filename(&self) -> String {
        self.inner
            .borrow()
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    }

    /// Last two path components, joined with `/` (Python dry-run format).
    pub fn dry_run_label(&self) -> String {
        let path = self.path();
        let parts: Vec<&str> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        match parts.len() {
            0 => String::new(),
            1 => parts[0].to_string(),
            _ => format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]),
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.inner.borrow().image_loaded
    }

    pub fn datetime(&self) -> String {
        self.inner.borrow().datetime.clone()
    }

    pub fn orientation(&self) -> Option<u32> {
        self.inner.borrow().orientation
    }

    pub fn original_size(&self) -> (u32, u32) {
        let b = self.inner.borrow();
        (b.width, b.height)
    }

    /// Fitted top-left on screen (loads if needed).
    pub fn coordinates(&self, screen: Rect) -> Result<(i32, i32)> {
        self.ensure_loaded(screen)?;
        let b = self.inner.borrow();
        Ok((b.x, b.y))
    }

    /// Fitted top-left using the default 1280×720 window.
    pub fn coordinates_default(&self) -> Result<(i32, i32)> {
        self.coordinates(default_screen_rect())
    }

    /// Ensure image is loaded and return a clone of the fitted RGBA buffer.
    pub fn get_surface(&self, screen: Rect) -> Result<RgbaImage> {
        self.ensure_loaded(screen)?;
        let b = self.inner.borrow();
        b.image
            .clone()
            .ok_or_else(|| Error::Slide(b.path.clone()))
    }

    pub fn get_surface_default(&self) -> Result<RgbaImage> {
        self.get_surface(default_screen_rect())
    }

    /// Drop pixel data to free memory (Python `unloadImage`).
    pub fn unload_image(&self) {
        let mut b = self.inner.borrow_mut();
        b.width = 0;
        b.height = 0;
        b.x = 0;
        b.y = 0;
        b.datetime.clear();
        b.orientation = None;
        b.image = None;
        b.image_loaded = false;
    }

    pub fn ensure_loaded(&self, screen: Rect) -> Result<()> {
        if self.inner.borrow().image_loaded {
            return Ok(());
        }
        self.load_image(screen)
    }

    pub fn load_image(&self, screen: Rect) -> Result<()> {
        let path = self.inner.borrow().path.clone();
        tracing::debug!("{}", path.file_name().and_then(|s| s.to_str()).unwrap_or(""));

        let mut img = image::open(&path).map_err(|e| {
            tracing::warn!("failed to load {}: {e}", path.display());
            Error::Slide(path.clone())
        })?;

        let (exif_orientation, datetime) = read_exif_meta(&path);

        if let Some(o) = exif_orientation {
            tracing::debug!("EXIF orientation: {o}");
            img = apply_orientation_pygame(img, o);
        }

        let width = img.width();
        let height = img.height();

        let image_rect = Rect::from_size(width as i32, height as i32);
        let fitted = image_rect.fit(screen);

        // Guard against zero-size fit (degenerate inputs).
        let fw = fitted.w.max(1) as u32;
        let fh = fitted.h.max(1) as u32;

        let scaled = img
            .resize_exact(fw, fh, FilterType::Triangle)
            .to_rgba8();

        {
            let mut b = self.inner.borrow_mut();
            b.width = width;
            b.height = height;
            b.x = fitted.x;
            b.y = fitted.y;
            b.datetime = datetime.unwrap_or_default();
            b.orientation = exif_orientation;
            b.image = Some(scaled);
            b.image_loaded = true;
        }

        tracing::info!(
            "{} ({} x {})",
            path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            width,
            height
        );
        Ok(())
    }
}

/// Match Python `slide.py`: pygame.transform.rotate is counter-clockwise.
/// EXIF 3 → 180°, EXIF 6 → 270° CCW, EXIF 8 → 90° CCW.
pub fn apply_orientation_pygame(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        3 => img.rotate180(),
        6 => img.rotate270(),
        8 => img.rotate90(),
        _ => img,
    }
}

fn read_exif_meta(path: &Path) -> (Option<u32>, Option<String>) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, None),
    };
    let mut reader = BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(_) => return (None, None),
    };

    let orientation = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0));

    let datetime = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .map(|f| f.display_value().with_unit(&exif).to_string());

    (orientation, datetime)
}

impl PartialEq for Slide {
    fn eq(&self, other: &Self) -> bool {
        self.inner.borrow().path == other.inner.borrow().path
    }
}

impl Eq for Slide {}

impl PartialOrd for Slide {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Slide {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.borrow().path.cmp(&other.inner.borrow().path)
    }
}

impl std::fmt::Debug for Slide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let b = self.inner.borrow();
        f.debug_struct("Slide")
            .field("path", &b.path)
            .field("interval", &b.interval)
            .field("loaded", &b.image_loaded)
            .field("size", &(b.width, b.height))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rect::{DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH};
    use std::path::PathBuf;

    fn repo(p: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(p)
    }

    #[test]
    fn lazy_load_and_unload() {
        let path = repo("tests/images/paintings/davinci.jpg");
        let slide = Slide::new(&path, 5);
        assert!(!slide.is_loaded());

        let surface = slide.get_surface_default().unwrap();
        assert!(slide.is_loaded());
        assert!(surface.width() > 0 && surface.height() > 0);

        let (ow, oh) = slide.original_size();
        assert_eq!((ow, oh), (1024, 803));

        let (x, y) = slide.coordinates_default().unwrap();
        let fitted = Rect::from_size(1024, 803).fit(default_screen_rect());
        assert_eq!((x, y), (fitted.x, fitted.y));
        assert_eq!(surface.width(), fitted.w as u32);
        assert_eq!(surface.height(), fitted.h as u32);

        // EXIF date present on davinci
        assert!(!slide.datetime().is_empty());

        slide.unload_image();
        assert!(!slide.is_loaded());
        assert_eq!(slide.original_size(), (0, 0));
        assert!(slide.datetime().is_empty());
    }

    #[test]
    fn shared_identity_unload() {
        let path = repo("tests/images/paintings/monet.jpg");
        let a = Slide::new(&path, 5);
        let b = a.clone();
        a.get_surface_default().unwrap();
        assert!(b.is_loaded());
        b.unload_image();
        assert!(!a.is_loaded());
    }

    #[test]
    fn orientation_transforms_swap_dimensions() {
        // 2x1 image; orientation 6 (90 CW / 270 CCW in pygame) → 1x2
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(2, 1, |x, _| {
            image::Rgb([x as u8 * 100, 0, 0])
        }));
        let rotated = apply_orientation_pygame(img, 6);
        assert_eq!((rotated.width(), rotated.height()), (1, 2));

        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(2, 1, |_, _| {
            image::Rgb([1, 2, 3])
        }));
        let r8 = apply_orientation_pygame(img, 8);
        assert_eq!((r8.width(), r8.height()), (1, 2));
    }

    #[test]
    fn bad_file_returns_slide_error() {
        let slide = Slide::new("/nonexistent/nope.jpg", 1);
        let err = slide.get_surface_default().unwrap_err();
        match err {
            Error::Slide(p) => assert!(p.ends_with("nope.jpg")),
            other => panic!("expected Slide error, got {other}"),
        }
    }

    #[test]
    fn bmp_loads() {
        let path = repo("tests/images/bitmaps/Mercury.bmp");
        let slide = Slide::new(&path, 3);
        let surface = slide.get_surface_default().unwrap();
        assert_eq!(
            (surface.width() <= DEFAULT_SCREEN_WIDTH)
                && (surface.height() <= DEFAULT_SCREEN_HEIGHT),
            true
        );
    }

    #[test]
    fn fitted_surface_fits_inside_screen() {
        let path = repo("tests/images/numbers/1_pexels-padrinan-2249528.jpg");
        let slide = Slide::new(&path, 5);
        let surface = slide.get_surface_default().unwrap();
        assert!(surface.width() <= DEFAULT_SCREEN_WIDTH);
        assert!(surface.height() <= DEFAULT_SCREEN_HEIGHT);
        // Large landscape should fill width or height
        assert!(
            surface.width() == DEFAULT_SCREEN_WIDTH
                || surface.height() == DEFAULT_SCREEN_HEIGHT
        );
    }
}
