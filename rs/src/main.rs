//! magic-lantern — Rust port CLI.
//!
//! Phase 1: configuration, album discovery, slideshow, dry-run.
//! Full UI arrives in later phases (see rust.md).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use magic_lantern::config::{CliOverrides, Config};
use magic_lantern::error::Error;
use magic_lantern::log_setup;
use magic_lantern::slideshow::{self, Slideshow};

/// A slide show generator. Specify a directory containing image files
/// or use -c to specify a config file.
#[derive(Debug, Parser)]
#[command(
    name = "magic-lantern",
    version,
    after_help = "To reload the configuration, send it the USR1 signal:\n\n    pkill -USR1 magic-lantern\n\nSee https://github.com/normanlorrain/magic-lantern for more details."
)]
struct Cli {
    /// Configuration file.
    #[arg(short = 'c', long = "config-file", value_name = "FILE")]
    config_file: Option<PathBuf>,

    /// Full screen mode.
    #[arg(short = 'f', long = "fullscreen", default_value_t = false)]
    fullscreen: bool,

    /// Shuffle the slides.
    #[arg(short = 's', long = "shuffle", default_value_t = false)]
    shuffle: bool,

    /// Test mode. Only display the slide names. Specify the number of slides.
    #[arg(short = 'd', long = "dry-run", value_name = "N", value_parser = clap::value_parser!(u64).range(1..))]
    dry_run: Option<u64>,

    /// Interval (seconds) between images.
    #[arg(short = 'i', long = "interval", value_name = "SECS", value_parser = clap::value_parser!(u64).range(1..))]
    interval: Option<u64>,

    /// Exclude the given directories. Multiple entries are permitted.
    #[arg(short = 'e', long = "exclude", value_name = "DIR")]
    exclude: Vec<String>,

    /// Directory containing image files (mutually exclusive with -c).
    #[arg(value_name = "DIRECTORY")]
    directory: Option<PathBuf>,
}

impl From<&Cli> for CliOverrides {
    fn from(cli: &Cli) -> Self {
        CliOverrides {
            config_file: cli.config_file.clone(),
            directory: cli.directory.clone(),
            fullscreen: cli.fullscreen,
            shuffle: cli.shuffle,
            interval: cli.interval,
            exclude: cli.exclude.clone(),
            dry_run: cli.dry_run.map(|n| n as usize),
        }
    }
}

fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    log_setup::init();

    let overrides = CliOverrides::from(&cli);
    let config = Config::from_cli(&overrides)?;

    if let Some(n) = config.dry_run {
        // Seed 0 by default; allow override for reproducibility.
        let seed = std::env::var("MAGIC_LANTERN_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut show = Slideshow::new(&config, seed)?;
        slideshow::dry_run(&mut show, n)?;
        return Ok(());
    }

    // Phase 1: UI not yet implemented.
    anyhow_ui_not_ready(&config)
}

fn anyhow_ui_not_ready(config: &Config) -> Result<(), Error> {
    let albums = config.albums.len();
    let msg = format!(
        "UI not implemented yet (Phase 1 is dry-run only).\n\
         Loaded {albums} album(s); fullscreen={} shuffle={} interval={}s.\n\
         Use --dry-run N to list slides, e.g.:\n\
           magic-lantern -c tests/example\\ 1.toml --dry-run 10",
        config.fullscreen, config.shuffle, config.interval
    );
    Err(Error::Slideshow(msg))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Config(msg)) => {
            eprintln!("Error: {msg}");
            ExitCode::from(1)
        }
        Err(Error::Slideshow(msg)) => {
            // UI-not-ready is informational on stderr with non-zero so scripts notice.
            eprintln!("{msg}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}
