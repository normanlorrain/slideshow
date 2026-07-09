//! Window / display via **SDL2** (Python `screen.py` / pygame parity).
//!
//! Requires system packages (Debian/Ubuntu):
//!   `libsdl2-dev`
//! Optional for other tools: `libsdl2-ttf-dev`, `libsdl2-image-dev`
//! (this crate uploads RGBA via SDL textures; TTF is not required at link time).

use std::time::Instant;

use image::RgbaImage;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::Sdl;
use sdl2::VideoSubsystem;

use crate::error::{Error, Result};
use crate::log_setup;
use crate::rect::{DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, Rect};

/// SDL2 display: canvas + texture creator for blitting slide/overlay images.
pub struct Screen {
    /// Kept alive for the process lifetime of the display.
    _sdl: Sdl,
    _video: VideoSubsystem,
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    width: u32,
    height: u32,
}

impl Screen {
    pub fn new(fullscreen: bool) -> Result<Self> {
        let total = Instant::now();
        tracing::debug!(fullscreen, "SDL screen init start");

        let t = Instant::now();
        let sdl = sdl2::init().map_err(|e| Error::Display(format!("SDL init: {e}")))?;
        log_setup::log_elapsed("SDL init", t);

        let t = Instant::now();
        let video = sdl
            .video()
            .map_err(|e| Error::Display(format!("SDL video: {e}")))?;
        log_setup::log_elapsed("SDL video subsystem", t);

        // Disable unused subsystems similar to pygame mixer/joystick quit.
        // (Audio/joystick not started by default with video-only use.)

        let mut builder = video.window(
            "magic-lantern",
            DEFAULT_SCREEN_WIDTH,
            DEFAULT_SCREEN_HEIGHT,
        );
        builder.position_centered();
        if fullscreen {
            builder.fullscreen_desktop();
        }

        let t = Instant::now();
        let window = builder
            .build()
            .map_err(|e| Error::Display(format!("SDL window: {e}")))?;
        log_setup::log_elapsed("SDL window create", t);

        let t = Instant::now();
        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|e| Error::Display(format!("SDL canvas: {e}")))?;
        log_setup::log_elapsed("SDL canvas create", t);

        canvas.set_blend_mode(BlendMode::Blend);

        let (width, height) = canvas.output_size().map_err(|e| {
            Error::Display(format!("SDL output size: {e}"))
        })?;

        // Hide mouse (kiosk).
        sdl.mouse().show_cursor(false);

        tracing::info!("Screen size {width} x {height}");
        if fullscreen {
            tracing::debug!("Fullscreen desktop mode");
        }

        let texture_creator = canvas.texture_creator();
        log_setup::log_elapsed("SDL screen init total", total);

        Ok(Self {
            _sdl: sdl,
            _video: video,
            canvas,
            texture_creator,
            width,
            height,
        })
    }

    pub fn width(&self) -> usize {
        self.width as usize
    }

    pub fn height(&self) -> usize {
        self.height as usize
    }

    pub fn rect(&self) -> Rect {
        Rect::from_size(self.width as i32, self.height as i32)
    }

    /// Access the SDL context for the event pump (controller).
    pub fn sdl(&self) -> &Sdl {
        &self._sdl
    }

    pub fn fill_black(&mut self) {
        self.canvas.set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        let _ = self.canvas.clear();
    }

    /// Blit an RGBA image at top-left `(x, y)`.
    pub fn blit(&mut self, image: &RgbaImage, x: i32, y: i32) -> Result<()> {
        let w = image.width();
        let h = image.height();
        if w == 0 || h == 0 {
            return Ok(());
        }

        let total = Instant::now();

        let t = Instant::now();
        let mut texture = self
            .texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, w, h)
            .map_err(|e| Error::Display(format!("texture: {e}")))?;
        log_setup::log_elapsed(&format!("SDL create_texture {w}x{h}"), t);

        texture.set_blend_mode(BlendMode::Blend);

        let t = Instant::now();
        texture
            .with_lock(None, |buffer: &mut [u8], pitch: usize| {
                for row in 0..h as usize {
                    let src_off = row * (w as usize) * 4;
                    let dst_off = row * pitch;
                    let n = (w as usize) * 4;
                    buffer[dst_off..dst_off + n]
                        .copy_from_slice(&image.as_raw()[src_off..src_off + n]);
                }
            })
            .map_err(|e| Error::Display(format!("texture lock: {e}")))?;
        log_setup::log_elapsed(&format!("SDL texture upload {w}x{h}"), t);

        let dest = SdlRect::new(x, y, w, h);
        self.canvas
            .copy(&texture, None, dest)
            .map_err(|e| Error::Display(format!("canvas copy: {e}")))?;
        log_setup::log_elapsed(&format!("SDL blit total {w}x{h} at ({x},{y})"), total);
        Ok(())
    }

    /// Flip / present the backbuffer.
    pub fn present(&mut self) {
        let t = Instant::now();
        self.canvas.present();
        // present_vsync can block ~16ms; use a higher threshold so normal vsync is not WARN.
        log_setup::log_elapsed_threshold("SDL present", t, 50);
    }
}
