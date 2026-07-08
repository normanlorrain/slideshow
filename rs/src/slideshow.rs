//! Infinite slide generator over weighted albums, with history for previous.
//!
//! Mirrors Python `slideshow.py` semantics:
//! - `shuffle`: weighted random album each draw
//! - otherwise: round-robin albums
//! - `atomic` albums yield their full sequence, then the outer picker continues
//! - history keeps the last 10 slides for previous/next navigation

use std::collections::VecDeque;

use rand::distributions::WeightedIndex;
use rand::prelude::*;
use rand::rngs::StdRng;

use crate::album::{build_albums, Album};
use crate::config::{Config, Order};
use crate::error::{Error, Result};
use crate::pdf::PdfCache;
use crate::slide::Slide;

const HISTORY_LIMIT: usize = 10;

/// Slideshow state machine (replaces Python module-level globals).
pub struct Slideshow {
    albums: Vec<Album>,
    weights: Vec<u32>,
    shuffle: bool,
    /// Next album index for non-shuffle round-robin (`itertools.cycle`).
    album_cursor: usize,
    history: VecDeque<Slide>,
    /// 0 = live edge; negative = browsing history (Python list negative indices).
    history_cursor: isize,
    current: Option<Slide>,
    /// In-progress atomic album yield (`yield from album`).
    pending: VecDeque<Slide>,
    rng: StdRng,
    /// Kept alive so PDF page temp files remain valid.
    _pdf: Option<PdfCache>,
}

impl Slideshow {
    /// Build from config. `seed` makes album shuffle / weighted picks deterministic.
    pub fn new(config: &Config, seed: u64) -> Result<Self> {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut pdf = PdfCache::new().ok();
        let albums = build_albums(
            &config.albums,
            &config.exclude,
            pdf.as_mut(),
            &mut rng,
        )?;
        let weights: Vec<u32> = albums.iter().map(|a| a.weight.max(1)).collect();

        Ok(Self {
            albums,
            weights,
            shuffle: config.shuffle,
            album_cursor: 0,
            history: VecDeque::new(),
            history_cursor: 0,
            current: None,
            pending: VecDeque::new(),
            rng,
            _pdf: pdf,
        })
    }

    /// Images only (skip PDF conversion) — for fast unit tests.
    pub fn new_images_only(config: &Config, seed: u64) -> Result<Self> {
        let mut rng = StdRng::seed_from_u64(seed);
        let albums = build_albums(&config.albums, &config.exclude, None, &mut rng)?;
        let weights: Vec<u32> = albums.iter().map(|a| a.weight.max(1)).collect();
        Ok(Self {
            albums,
            weights,
            shuffle: config.shuffle,
            album_cursor: 0,
            history: VecDeque::new(),
            history_cursor: 0,
            current: None,
            pending: VecDeque::new(),
            rng,
            _pdf: None,
        })
    }

    pub fn current(&self) -> Option<&Slide> {
        self.current.as_ref()
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn get_next_slide(&mut self) -> Result<Slide> {
        // If browsing history, step toward the live edge first.
        if self.history_cursor < 0 {
            self.history_cursor += 1;
        }

        let slide = if self.history_cursor < 0 {
            let idx = Self::python_history_index(self.history.len(), self.history_cursor)?;
            self.history[idx].clone()
        } else {
            let slide = self.next_from_generator()?;
            self.history.push_back(slide.clone());
            if self.history.len() > HISTORY_LIMIT {
                if let Some(mut old) = self.history.pop_front() {
                    old.unload_image();
                }
            }
            slide
        };

        self.current = Some(slide.clone());
        Ok(slide)
    }

    pub fn get_previous_slide(&mut self) -> Result<Slide> {
        if self.history.is_empty() {
            return Err(Error::Slideshow("no history".into()));
        }
        // Python:
        //   if cursor == 0: cursor = -2
        //   else: cursor -= 1
        //   cursor = max(cursor, -len(history))
        if self.history_cursor == 0 {
            self.history_cursor = -2;
        } else {
            self.history_cursor -= 1;
        }
        let min = -(self.history.len() as isize);
        if self.history_cursor < min {
            self.history_cursor = min;
        }
        let idx = Self::python_history_index(self.history.len(), self.history_cursor)?;
        let slide = self.history[idx].clone();
        self.current = Some(slide.clone());
        Ok(slide)
    }

    /// Map Python-style negative history index onto a `VecDeque` index.
    ///
    /// When `cursor >= 0` we are at the live edge (last item). When `cursor`
    /// is negative, Python `list[cursor]` semantics apply.
    fn python_history_index(len: usize, cursor: isize) -> Result<usize> {
        if len == 0 {
            return Err(Error::Slideshow("empty history".into()));
        }
        let idx = if cursor >= 0 {
            len - 1
        } else {
            let i = len as isize + cursor;
            if i < 0 {
                return Err(Error::Slideshow("history cursor out of range".into()));
            }
            i as usize
        };
        if idx >= len {
            return Err(Error::Slideshow("history cursor out of range".into()));
        }
        Ok(idx)
    }

    fn pick_album_index(&mut self) -> usize {
        if self.shuffle {
            let dist =
                WeightedIndex::new(&self.weights).expect("weights non-empty and non-zero");
            dist.sample(&mut self.rng)
        } else {
            let idx = self.album_cursor % self.albums.len();
            self.album_cursor = self.album_cursor.wrapping_add(1);
            idx
        }
    }

    fn next_from_generator(&mut self) -> Result<Slide> {
        if let Some(slide) = self.pending.pop_front() {
            return Ok(slide);
        }

        for _ in 0..10_000 {
            let album_idx = self.pick_album_index();
            if self.albums[album_idx].order == Order::Atomic {
                let group = self.albums[album_idx].next_atomic_group();
                if group.is_empty() {
                    continue;
                }
                let mut iter = group.into_iter();
                let first = iter.next().expect("non-empty group");
                self.pending.extend(iter);
                return Ok(first);
            } else if let Some(slide) = self.albums[album_idx].next_slide() {
                return Ok(slide);
            }
        }
        Err(Error::Slideshow("failed to pick next slide".into()))
    }
}

/// Dry-run: print `i parent/name` for the next `n` slides (Python format).
pub fn dry_run(slideshow: &mut Slideshow, n: usize) -> Result<()> {
    for i in 0..n {
        let slide = slideshow.get_next_slide()?;
        println!("{i} {}", slide.dry_run_label());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlbumConfig, CliOverrides, Config, Order};
    use std::path::PathBuf;

    fn repo(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    }

    fn numbers_only_config(shuffle: bool) -> Config {
        Config {
            config_file: None,
            directory: None,
            fullscreen: false,
            shuffle,
            interval: 5,
            exclude: vec![],
            dry_run: None,
            weight: 1,
            albums: vec![AlbumConfig {
                folder: repo("tests/images/numbers"),
                order: if shuffle {
                    Order::Random
                } else {
                    Order::Sequence
                },
                weight: 1,
                interval: 5,
            }],
        }
    }

    fn multi_album_config(shuffle: bool) -> Config {
        Config {
            config_file: None,
            directory: None,
            fullscreen: false,
            shuffle,
            interval: 5,
            exclude: vec![],
            dry_run: None,
            weight: 1,
            albums: vec![
                AlbumConfig {
                    folder: repo("tests/images/numbers"),
                    order: Order::Sequence,
                    weight: 1,
                    interval: 5,
                },
                AlbumConfig {
                    folder: repo("tests/images/atomic"),
                    order: Order::Atomic,
                    weight: 1,
                    interval: 5,
                },
                AlbumConfig {
                    folder: repo("tests/images/paintings"),
                    order: Order::Sequence,
                    weight: 1,
                    interval: 5,
                },
            ],
        }
    }

    #[test]
    fn sequence_single_album_cycles_sorted() {
        let cfg = numbers_only_config(false);
        let mut show = Slideshow::new_images_only(&cfg, 0).unwrap();
        let mut names = Vec::new();
        for _ in 0..6 {
            names.push(show.get_next_slide().unwrap().filename().to_string());
        }
        // 3 images, sorted, repeated
        assert_eq!(names[0], names[3]);
        assert_eq!(names[1], names[4]);
        assert_eq!(names[2], names[5]);
        assert!(names[0].starts_with('1'));
        assert!(names[1].starts_with('2'));
        assert!(names[2].starts_with('3'));
    }

    #[test]
    fn weighted_shuffle_is_deterministic_with_seed() {
        let cfg = multi_album_config(true);
        let mut a = Slideshow::new_images_only(&cfg, 12345).unwrap();
        let mut b = Slideshow::new_images_only(&cfg, 12345).unwrap();
        let sa: Vec<_> = (0..20)
            .map(|_| a.get_next_slide().unwrap().path.clone())
            .collect();
        let sb: Vec<_> = (0..20)
            .map(|_| b.get_next_slide().unwrap().path.clone())
            .collect();
        assert_eq!(sa, sb);

        let mut c = Slideshow::new_images_only(&cfg, 99999).unwrap();
        let sc: Vec<_> = (0..20)
            .map(|_| c.get_next_slide().unwrap().path.clone())
            .collect();
        assert_ne!(sa, sc);
    }

    #[test]
    fn atomic_group_stays_together_when_not_shuffling() {
        // Non-shuffle: albums cycle numbers → atomic → paintings → …
        // When atomic is picked, all atomic slides appear consecutively.
        let cfg = multi_album_config(false);
        let mut show = Slideshow::new_images_only(&cfg, 0).unwrap();
        // First album (numbers): one slide
        let s0 = show.get_next_slide().unwrap();
        assert!(s0.path.to_string_lossy().contains("numbers"));
        // Second album (atomic): entire group
        let mut atomic_run = Vec::new();
        for _ in 0..3 {
            let s = show.get_next_slide().unwrap();
            assert!(
                s.path.to_string_lossy().contains("atomic"),
                "expected atomic slide, got {}",
                s.path.display()
            );
            atomic_run.push(s.filename().to_string());
        }
        assert_eq!(atomic_run.len(), 3);
        // Next: paintings
        let s = show.get_next_slide().unwrap();
        assert!(s.path.to_string_lossy().contains("paintings"));
    }

    #[test]
    fn history_previous_and_next() {
        let cfg = numbers_only_config(false);
        let mut show = Slideshow::new_images_only(&cfg, 0).unwrap();
        let a = show.get_next_slide().unwrap();
        let b = show.get_next_slide().unwrap();
        let _c = show.get_next_slide().unwrap();
        assert_ne!(a.path, b.path);

        let back = show.get_previous_slide().unwrap();
        // From live edge (c), previous jumps to b (cursor -2 → second last)
        assert_eq!(back.path, b.path);

        let back2 = show.get_previous_slide().unwrap();
        assert_eq!(back2.path, a.path);

        let forward = show.get_next_slide().unwrap();
        assert_eq!(forward.path, b.path);
    }

    #[test]
    fn history_capped_at_ten() {
        let cfg = numbers_only_config(false);
        let mut show = Slideshow::new_images_only(&cfg, 0).unwrap();
        for _ in 0..25 {
            show.get_next_slide().unwrap();
        }
        assert_eq!(show.history_len(), 10);
    }

    #[test]
    fn dry_run_from_example_config_images() {
        let path = repo("tests/example 1.toml");
        let cli = CliOverrides {
            config_file: Some(path),
            // Drop the pdfs album by excluding nothing but using images-only builder
            ..CliOverrides::default()
        };
        let mut cfg = Config::from_cli(&cli).unwrap();
        // Remove pdf album for this test (images only)
        cfg.albums.retain(|a| !a.folder.ends_with("pdfs"));
        let mut show = Slideshow::new_images_only(&cfg, 1).unwrap();
        let s = show.get_next_slide().unwrap();
        let label = s.dry_run_label();
        assert!(label.contains('/'), "label should be parent/name: {label}");
    }
}
