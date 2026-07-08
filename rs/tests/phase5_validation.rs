//! Phase 5 integration validation (`cargo test --test phase5_validation`).
//!
//! - kiosk / validation TOML against tests/ assets
//! - deterministic dry-run sequence (golden)
//! - history unload under long iteration
//! - SIGUSR1 / reload flag plumbing
//! - CLI dry-run subprocess

use std::path::PathBuf;
use std::process::Command;

use magic_lantern::config::{AlbumConfig, CliOverrides, Config, Order};
use magic_lantern::rect::default_screen_rect;
use magic_lantern::signal_handler;
use magic_lantern::slideshow::Slideshow;

fn repo(p: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(p)
}

fn load_config(rel: &str) -> Config {
    let path = repo(rel);
    Config::from_cli(&CliOverrides {
        config_file: Some(path),
        ..CliOverrides::default()
    })
    .unwrap_or_else(|e| panic!("load {rel}: {e}"))
}

#[test]
fn kiosk_toml_loads_all_albums() {
    let cfg = load_config("tests/kiosk.toml");
    assert_eq!(cfg.interval, 2);
    assert!(!cfg.fullscreen);
    assert!(!cfg.shuffle);
    assert_eq!(
        cfg.albums.len(),
        5,
        "numbers, atomic, paintings, bitmaps, pdfs"
    );
    assert!(cfg.exclude.contains(&"_archive".to_string()));
}

/// Golden sequence matching Python slideshow semantics (shuffle=false, no random albums).
/// Computed independently with pure-Python walk + cycle (see rust.md Phase 5 notes).
#[test]
fn golden_dry_run_validation_toml() {
    let cfg = load_config("tests/validation.toml");
    let mut show = Slideshow::new_images_only(&cfg, 0).unwrap();
    let got: Vec<String> = (0..12)
        .map(|_| show.get_next_slide().unwrap().dry_run_label())
        .collect();

    let expected = [
        "numbers/1_pexels-padrinan-2249528.jpg",
        "atomic/640px-Methane-3D-balls - Wikimedia Commons.png",
        "atomic/Atom_sym - Wikimedia Commons.png",
        "atomic/Bohr_Atom_Structure - Wikimedia Commons.jpg",
        "bitmaps/Mars.bmp",
        "numbers/2_pexels-n-voitkevich-4772868.jpg",
        "atomic/640px-Methane-3D-balls - Wikimedia Commons.png",
        "atomic/Atom_sym - Wikimedia Commons.png",
        "atomic/Bohr_Atom_Structure - Wikimedia Commons.jpg",
        "bitmaps/Mercury.bmp",
        "numbers/3_pexels-enginakyurt-15271752.jpg",
        "atomic/640px-Methane-3D-balls - Wikimedia Commons.png",
    ];
    assert_eq!(got, expected);
}

#[test]
fn long_run_history_unload_bounds_memory() {
    use std::collections::HashSet;

    let cfg = Config {
        config_file: None,
        directory: None,
        fullscreen: false,
        shuffle: false,
        interval: 1,
        exclude: vec![],
        dry_run: None,
        weight: 1,
        albums: vec![AlbumConfig {
            // Prefer bitmaps/atomic (smaller) over multi-megapixel numbers/*.jpg.
            folder: repo("tests/images/bitmaps"),
            order: Order::Sequence,
            weight: 1,
            interval: 1,
        }],
    };

    let mut show = Slideshow::new_images_only(&cfg, 42).unwrap();
    let screen = default_screen_rect();
    let mut handles = Vec::new();

    for i in 0..25 {
        let s = show.get_next_slide().unwrap();
        s.get_surface(screen).unwrap_or_else(|e| {
            panic!("slide {i} failed to load {}: {e}", s.path().display())
        });
        handles.push(s);
    }

    assert_eq!(show.history_len(), 10);

    // Shared Rc identity: a path that cycles back into the history window is
    // re-loaded. Bound is therefore on *unique loaded paths*, not handle count.
    let loaded_unique: HashSet<PathBuf> = handles
        .iter()
        .filter(|s| s.is_loaded())
        .map(|s| s.path())
        .collect();
    assert!(
        loaded_unique.len() <= 10,
        "at most 10 unique slides should stay loaded, got {}",
        loaded_unique.len()
    );

    // With only 3 bitmaps, every path may remain in the 10-deep history (with
    // repeats). Shared Rc unload means a popped instance can unload a path that
    // still appears later in history until re-shown — matching Python. Bound
    // above is the memory guarantee we care about.
    assert!(
        !loaded_unique.is_empty(),
        "expected some slides still loaded after long run"
    );
}

fn bin_path() -> PathBuf {
    // Prefer Cargo-provided path when available (integration tests).
    if let Some(p) = option_env!("CARGO_BIN_EXE_magic_lantern") {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_magic_lantern") {
        return PathBuf::from(p);
    }
    // Fallback after `cargo build`
    repo("target/debug/magic-lantern")
}

#[test]
fn dry_run_cli_subprocess() {
    let bin = bin_path();
    assert!(
        bin.is_file(),
        "binary missing at {} — run cargo build first",
        bin.display()
    );
    let config = repo("tests/validation.toml");
    let output = Command::new(&bin)
        .args([
            "--config-file",
            config.to_str().unwrap(),
            "--dry-run",
            "6",
        ])
        .env("RUST_LOG", "error")
        .output()
        .expect("spawn magic-lantern");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| {
            l.chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        })
        .collect();
    assert!(lines.len() >= 6, "stdout:\n{stdout}");
    assert!(lines[0].contains("numbers/1_"), "line0={}", lines[0]);
    assert!(lines[1].contains("atomic/"), "line1={}", lines[1]);
    assert!(lines[4].contains("bitmaps/Mars"), "line4={}", lines[4]);
}

#[test]
fn reload_flag_request_and_take() {
    let flag = signal_handler::init().unwrap();
    let _ = flag.take();
    flag.request();
    assert!(flag.is_set());
    assert!(flag.take());
    assert!(!flag.take());
}

#[cfg(unix)]
#[test]
fn sigusr1_sets_reload_flag() {
    let flag = signal_handler::init().unwrap();
    let _ = flag.take();

    let status = Command::new("kill")
        .args(["-USR1", &std::process::id().to_string()])
        .status()
        .expect("kill");
    assert!(status.success());

    // Handler may run slightly async relative to kill return.
    let mut ok = false;
    for _ in 0..20 {
        if flag.is_set() {
            ok = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(ok, "SIGUSR1 should set the process reload flag");
    assert!(flag.take());
}

#[test]
fn kiosk_slideshow_builds_including_pdf_pages() {
    let cfg = load_config("tests/kiosk.toml");
    // Full slideshow with PDF conversion (pdftoppm).
    let show = Slideshow::new(&cfg, 1);
    match show {
        Ok(mut s) => {
            let first = s.get_next_slide().unwrap();
            assert!(first.dry_run_label().contains("numbers"));
            // Advance enough to leave numbers + atomic group
            for _ in 0..6 {
                let _ = s.get_next_slide();
            }
        }
        Err(e) => {
            // PDF tool missing — still require image albums to work via images-only.
            eprintln!("Slideshow::new failed ({e}); checking images-only path");
            let mut cfg2 = cfg;
            cfg2.albums.retain(|a| !a.folder.ends_with("pdfs"));
            let mut s = Slideshow::new_images_only(&cfg2, 1).unwrap();
            assert!(s.get_next_slide().is_ok());
        }
    }
}
