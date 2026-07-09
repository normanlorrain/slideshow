//! An album is a collection of slides with sequence / random / atomic order.

use std::path::PathBuf;
use std::time::Instant;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use walkdir::WalkDir;

use crate::config::{AlbumConfig, Order};
use crate::error::{Error, Result};
use crate::log_setup;
use crate::pdf::PdfCache;
use crate::slide::Slide;

const IMAGE_EXTS: &[&str] = &["bmp", "png", "jpg", "jpeg"];

/// Collection of slides from one folder tree.
#[derive(Debug)]
pub struct Album {
    pub order: Order,
    pub weight: u32,
    slides: Vec<Slide>,
    index: usize,
}

impl Album {
    /// Discover images (and optional PDF pages) under `cfg.folder`.
    pub fn open(
        cfg: &AlbumConfig,
        exclude: &[String],
        mut pdf: Option<&mut PdfCache>,
        rng: &mut StdRng,
    ) -> Result<Self> {
        let total = Instant::now();
        tracing::debug!(
            folder = %cfg.folder.display(),
            order = ?cfg.order,
            "Album::open start"
        );

        let mut slides = Vec::new();
        let mut n_images = 0u32;
        let mut n_pdfs = 0u32;

        let walker = WalkDir::new(&cfg.folder).follow_links(false).into_iter();
        // walkdir doesn't filter dirs in-place like os.walk; we skip excluded
        // path components instead.
        let t_walk = Instant::now();
        for entry in walker.filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !exclude.iter().any(|x| x == name.as_ref())
            } else {
                true
            }
        }) {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!("walk error: {err}");
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if name.contains('~') {
                tracing::warn!("Ignoring {name}");
                continue;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();

            if ext == "pdf" {
                tracing::info!("{}  PDF file", name);
                n_pdfs += 1;
                if let Some(ref mut cache) = pdf {
                    let t = Instant::now();
                    match cache.convert(path) {
                        Ok(pages) => {
                            log_setup::log_elapsed(
                                &format!("album PDF convert {name} ({} pages)", pages.len()),
                                t,
                            );
                            for page in pages {
                                slides.push(Slide::new(page, cfg.interval));
                            }
                        }
                        Err(e) => {
                            tracing::error!("PDF convert failed for {}: {e}", path.display());
                        }
                    }
                } else {
                    tracing::warn!(
                        "Skipping PDF {} (no PDF cache / converter)",
                        path.display()
                    );
                }
                continue;
            }

            if IMAGE_EXTS.iter().any(|e| *e == ext) {
                n_images += 1;
                slides.push(Slide::new(path, cfg.interval));
                continue;
            }

            // Skip known non-image junk quietly-ish (credits.txt etc.)
            tracing::warn!("{name}  Unknown file type");
        }
        log_setup::log_elapsed(
            &format!(
                "album walk {} (images={n_images} pdfs={n_pdfs})",
                cfg.folder.display()
            ),
            t_walk,
        );

        if cfg.order == Order::Random {
            slides.shuffle(rng);
        } else {
            slides.sort();
        }

        log_setup::log_elapsed(
            &format!(
                "Album::open total {} ({} slides)",
                cfg.folder.display(),
                slides.len()
            ),
            total,
        );

        Ok(Album {
            order: cfg.order,
            weight: cfg.weight,
            slides,
            index: 0,
        })
    }

    pub fn slide_count(&self) -> usize {
        self.slides.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slides.is_empty()
    }

    /// Next slide. For `Atomic`, returns `None` after one full pass (then
    /// resets index). For sequence/random, wraps forever.
    pub fn next_slide(&mut self) -> Option<Slide> {
        if self.slides.is_empty() {
            return None;
        }
        if self.index >= self.slides.len() {
            self.index = 0;
            if self.order == Order::Atomic {
                return None;
            }
        }
        let slide = self.slides[self.index].clone();
        self.index += 1;
        Some(slide)
    }

    /// Yield all slides for one atomic pass (used by slideshow).
    pub fn next_atomic_group(&mut self) -> Vec<Slide> {
        let mut out = Vec::new();
        while let Some(s) = self.next_slide() {
            out.push(s);
        }
        // next_slide already reset index when it returned None for Atomic
        out
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.slides.iter().map(|s| s.path()).collect()
    }
}

/// Build albums listed in config; skip empty ones (log error).
pub fn build_albums(
    album_cfgs: &[AlbumConfig],
    exclude: &[String],
    mut pdf: Option<&mut PdfCache>,
    rng: &mut StdRng,
) -> Result<Vec<Album>> {
    let mut albums = Vec::new();
    let mut total = 0usize;

    for cfg in album_cfgs {
        let album = Album::open(cfg, exclude, pdf.as_deref_mut(), rng);
        match album {
            Ok(a) if !a.is_empty() => {
                total += a.slide_count();
                albums.push(a);
            }
            Ok(_) => {
                tracing::error!("Album {} is empty", cfg.folder.display());
            }
            Err(e) => {
                tracing::error!("{e}");
            }
        }
    }

    if total == 0 {
        return Err(Error::Slideshow("No images found for slide show.".into()));
    }
    tracing::info!("Slide count: {total}");
    Ok(albums)
}

/// Fixed-seed RNG for deterministic tests / reproducible dry-runs.
pub fn seeded_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlbumConfig, Order};

    fn numbers_cfg() -> AlbumConfig {
        AlbumConfig {
            folder: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/images/numbers"),
            order: Order::Sequence,
            weight: 1,
            interval: 5,
        }
    }

    #[test]
    fn discovers_images_sorted_sequence() {
        let mut rng = seeded_rng(0);
        let album = Album::open(&numbers_cfg(), &[], None, &mut rng).unwrap();
        assert_eq!(album.slide_count(), 3);
        let names: Vec<_> = album
            .paths()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // Lexicographic by full path; basenames start 1_, 2_, 3_
        assert!(names[0].starts_with('1'));
        assert!(names[1].starts_with('2'));
        assert!(names[2].starts_with('3'));
    }

    #[test]
    fn exclude_directory_names() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/images");
        let cfg = AlbumConfig {
            folder: root,
            order: Order::Sequence,
            weight: 1,
            interval: 5,
        };
        let mut rng = seeded_rng(1);
        // Exclude "paintings" and "atomic" and "bitmaps"
        let album = Album::open(
            &cfg,
            &["paintings".into(), "atomic".into(), "bitmaps".into()],
            None,
            &mut rng,
        )
        .unwrap();
        // Only numbers/ left → 3 images
        assert_eq!(album.slide_count(), 3);
    }

    #[test]
    fn random_order_is_deterministic_with_seed() {
        let mut cfg = numbers_cfg();
        cfg.order = Order::Random;
        let mut rng_a = seeded_rng(42);
        let mut rng_b = seeded_rng(42);
        let a = Album::open(&cfg, &[], None, &mut rng_a).unwrap();
        let b = Album::open(&cfg, &[], None, &mut rng_b).unwrap();
        let pa = a.paths();
        let pb = b.paths();
        assert_eq!(pa, pb);
    }

    #[test]
    fn atomic_stops_after_one_pass() {
        let mut rng = seeded_rng(0);
        let mut cfg = numbers_cfg();
        cfg.order = Order::Atomic;
        let mut album = Album::open(&cfg, &[], None, &mut rng).unwrap();
        assert!(album.next_slide().is_some());
        assert!(album.next_slide().is_some());
        assert!(album.next_slide().is_some());
        assert!(album.next_slide().is_none());
        // After stop, another pass works
        assert!(album.next_slide().is_some());
    }

    #[test]
    fn sequence_wraps_forever() {
        let mut rng = seeded_rng(0);
        let mut album = Album::open(&numbers_cfg(), &[], None, &mut rng).unwrap();
        for _ in 0..10 {
            assert!(album.next_slide().is_some());
        }
    }
}
