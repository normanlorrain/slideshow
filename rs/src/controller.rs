//! Event loop / ringmaster (Python `controller.py`).
//!
//! SDL2 event pump: keyboard, quit, timer (via timeouts), SIGUSR1 reload.
//! Keys: space pause, q quit, n/→ next, p/← previous, y year overlay.

use std::time::{Duration, Instant};

use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::screen::Screen;
use crate::signal_handler::{self, ReloadFlag};
use crate::slideshow::Slideshow;
use crate::text::TextRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Next,
    Previous,
}

/// Owns display + slideshow state for one run (until quit or reload).
pub struct Controller {
    slideshow: Slideshow,
    screen: Screen,
    text: TextRenderer,
    reload: ReloadFlag,
    pause: bool,
    show_year: bool,
    photo_interval: Duration,
    next_due: Instant,
    /// Optional auto-quit for CI / automated smoke tests.
    auto_quit_at: Option<Instant>,
}

impl Controller {
    pub fn new(config: Config) -> Result<Self> {
        let seed = std::env::var("MAGIC_LANTERN_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Install SIGUSR1 before slow work (PDF rasterization).
        let reload = signal_handler::init()?;

        let screen = Screen::new(config.fullscreen)?;
        let text = TextRenderer::new()?;
        let slideshow = Slideshow::new(&config, seed)?;

        let photo_interval = Duration::from_secs(config.interval.max(1));

        let auto_quit_at = std::env::var("MAGIC_LANTERN_AUTO_QUIT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|s| Instant::now() + Duration::from_secs(s));

        Ok(Self {
            slideshow,
            screen,
            text,
            reload,
            pause: false,
            show_year: false,
            photo_interval,
            next_due: Instant::now() + photo_interval,
            auto_quit_at,
        })
    }

    /// Run until quit (`false`) or reload requested (`true`).
    pub fn run(&mut self) -> Result<bool> {
        self.show_new_slide(Direction::Next)?;

        let mut event_pump = self
            .screen
            .sdl()
            .event_pump()
            .map_err(|e| Error::Display(format!("SDL event pump: {e}")))?;

        // Roughly match pygame key repeat: delay 1000ms, interval 100ms.
        // SDL2 enables key repeat by default; we ignore `repeat` events for
        // discrete actions (pause/year) and allow them for next/prev if desired.
        // Here we ignore all OS key-repeat for parity with clear() after keydown.
        loop {
            if self.reload.take() {
                tracing::info!("Got signal. Reloading slide show.");
                return Ok(true);
            }

            if let Some(deadline) = self.auto_quit_at {
                if Instant::now() >= deadline {
                    tracing::info!("Auto-quit timer expired");
                    return Ok(false);
                }
            }

            // How long until the next photo event (or a short poll for signals).
            let wait_ms = if self.pause {
                100u32
            } else {
                let remaining = self
                    .next_due
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(100) as u32;
                remaining.max(1)
            };

            // Block for events (pygame.event.wait style) with timeout.
            for event in event_pump.wait_timeout_iter(wait_ms) {
                match event {
                    Event::Quit { .. } => {
                        tracing::debug!("window quit");
                        return Ok(false);
                    }
                    Event::KeyDown {
                        keycode: Some(key),
                        repeat: false,
                        ..
                    } => match key {
                        Keycode::Q | Keycode::Escape => {
                            tracing::debug!("quit key");
                            return Ok(false);
                        }
                        Keycode::N | Keycode::Right => {
                            self.next()?;
                        }
                        Keycode::P | Keycode::Left => {
                            self.previous()?;
                        }
                        Keycode::Y => {
                            self.toggle_year()?;
                        }
                        Keycode::Space => {
                            self.toggle_pause()?;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Drain any additional pending events without blocking.
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } => return Ok(false),
                    Event::KeyDown {
                        keycode: Some(key),
                        repeat: false,
                        ..
                    } => match key {
                        Keycode::Q | Keycode::Escape => return Ok(false),
                        Keycode::N | Keycode::Right => self.next()?,
                        Keycode::P | Keycode::Left => self.previous()?,
                        Keycode::Y => self.toggle_year()?,
                        Keycode::Space => self.toggle_pause()?,
                        _ => {}
                    },
                    _ => {}
                }
            }

            if !self.pause && Instant::now() >= self.next_due {
                self.show_new_slide(Direction::Next)?;
                tracing::debug!(
                    "Next slide in {} msec",
                    self.photo_interval.as_millis()
                );
                self.arm_timer();
            }
        }
    }

    fn arm_timer(&mut self) {
        self.next_due = Instant::now() + self.photo_interval;
    }

    fn show_new_slide(&mut self, direction: Direction) -> Result<()> {
        let screen_rect = self.screen.rect();

        let mut attempts = 0;
        let slide = loop {
            attempts += 1;
            if attempts > 100 {
                return Err(Error::Slideshow(
                    "too many bad slide files in a row".into(),
                ));
            }
            let slide = match direction {
                Direction::Next => self.slideshow.get_next_slide()?,
                Direction::Previous => match self.slideshow.get_previous_slide() {
                    Ok(s) => s,
                    Err(_) => self.slideshow.get_next_slide()?,
                },
            };
            match slide.get_surface(screen_rect) {
                Ok(_) => break slide,
                Err(Error::Slide(path)) => {
                    tracing::warn!("Bad slide file: {}", path.display());
                    continue;
                }
                Err(e) => return Err(e),
            }
        };

        tracing::debug!("{} interval:{}", slide.filename(), slide.interval());

        self.screen.fill_black();
        let surface = slide.get_surface(screen_rect)?;
        let (x, y) = slide.coordinates(screen_rect)?;
        self.screen.blit(&surface, x, y)?;
        self.show_metadata()?;
        self.screen.present();

        self.photo_interval = Duration::from_secs(slide.interval().max(1));
        Ok(())
    }

    fn show_metadata(&mut self) -> Result<()> {
        if !self.pause && !self.show_year {
            return Ok(());
        }
        let pad = 10i32;
        let slide = self
            .slideshow
            .current()
            .cloned()
            .ok_or_else(|| Error::Slideshow("no current slide".into()))?;

        if self.pause {
            let pause_img = self.text.message_normal("PAUSE");
            self.screen.blit(&pause_img, pad, pad)?;

            let filename = self.text.message_normal(&slide.filename());
            let x = self.screen.width() as i32 - filename.width() as i32 - pad;
            let y = self.screen.height() as i32 - filename.height() as i32 - pad;
            self.screen.blit(&filename, x, y)?;

            let datetime = self.text.message_normal(&slide.datetime());
            let y = self.screen.height() as i32 - datetime.height() as i32 - pad;
            self.screen.blit(&datetime, pad, y)?;
        }

        if self.show_year {
            let dt = slide.datetime();
            let year = if dt.len() >= 4 { &dt[..4] } else { &dt };
            let year_img = self.text.message_heading(year);
            let x = self.screen.width() as i32 - year_img.width() as i32 - pad;
            self.screen.blit(&year_img, x, pad)?;
        }
        Ok(())
    }

    fn toggle_pause(&mut self) -> Result<()> {
        self.pause = !self.pause;
        if self.pause {
            self.show_metadata()?;
            self.screen.present();
        } else {
            self.show_new_slide(Direction::Next)?;
            self.arm_timer();
        }
        Ok(())
    }

    fn toggle_year(&mut self) -> Result<()> {
        self.show_year = !self.show_year;
        if !self.show_year {
            if let Some(slide) = self.slideshow.current().cloned() {
                let screen_rect = self.screen.rect();
                self.screen.fill_black();
                if let Ok(surface) = slide.get_surface(screen_rect) {
                    let (x, y) = slide.coordinates(screen_rect)?;
                    self.screen.blit(&surface, x, y)?;
                }
            }
        }
        self.show_metadata()?;
        self.screen.present();
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.show_new_slide(Direction::Next)?;
        if !self.pause {
            self.arm_timer();
        }
        Ok(())
    }

    fn previous(&mut self) -> Result<()> {
        self.show_new_slide(Direction::Previous)?;
        if !self.pause {
            self.arm_timer();
        }
        Ok(())
    }
}

/// Dry-run path (no window): print slide names.
pub fn dry_run(config: &Config) -> Result<bool> {
    let seed = std::env::var("MAGIC_LANTERN_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let n = config
        .dry_run
        .ok_or_else(|| Error::Config("dry_run not set".into()))?;
    let mut show = Slideshow::new(config, seed)?;
    crate::slideshow::dry_run(&mut show, n)?;
    Ok(false)
}
