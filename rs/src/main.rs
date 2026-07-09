//! magic-lantern — Rust port CLI.
//!
//! Binary name is `magic-lantern` so `pkill -USR1 magic-lantern` reloads the app.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use magic_lantern::config::{CliOverrides, Config};
use magic_lantern::controller::{self, Controller};
use magic_lantern::error::Error;
use magic_lantern::log_setup;

/// A slide show generator. Specify a directory containing image files
/// or use -c to specify a config file.
#[derive(Debug, Parser)]
#[command(
    name = "magic-lantern",
    version,
    about = "A slide show generator. Specify a directory containing image files or use -c to specify a config file.",
    long_about = "A presentation tool for kiosks, digital signage, and slide shows.\n\n\
Supports PNG, JPEG, BMP, and PDF (each PDF page is converted to an image).\n\n\
Provide either a DIRECTORY of images or a TOML config file with -c / --config-file \
(not both).",
    after_help = "\
Keys while running:
  space          play / pause
  q              quit
  n, right       next image
  p, left        previous image
  y              year overlay on/off

Reload configuration (Unix):
  pkill -USR1 magic-lantern

Environment:
  MAGIC_LANTERN_SEED=<u64>           deterministic RNG for shuffle / album weights
  MAGIC_LANTERN_AUTO_QUIT_SECS=<n>   auto-exit after n seconds (smoke tests)
  RUST_LOG=<filter>                  tracing filter (default: info)

See https://github.com/normanlorrain/magic-lantern for more details."
)]
struct Cli {
    /// Configuration file.
    #[arg(
        short = 'c',
        long = "config-file",
        value_name = "FILE",
        help = "Configuration file."
    )]
    config_file: Option<PathBuf>,

    /// Full screen mode.
    #[arg(short = 'f', long = "fullscreen", help = "Full screen mode")]
    fullscreen: bool,

    /// Shuffle the slides.
    #[arg(short = 's', long = "shuffle", help = "Shuffle the slides")]
    shuffle: bool,

    /// Test mode. Only display the slide names. Specify the number of slides.
    #[arg(
        short = 'd',
        long = "dry-run",
        value_name = "N",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Test mode. Only display the slide names. Specify the number of slides. [x>=1]"
    )]
    dry_run: Option<u64>,

    /// Interval (seconds) between images.
    #[arg(
        short = 'i',
        long = "interval",
        value_name = "SECS",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Interval (seconds) between images. [x>=1]"
    )]
    interval: Option<u64>,

    /// Exclude the given directories. Multiple entries are permitted.
    #[arg(
        short = 'e',
        long = "exclude",
        value_name = "DIR",
        help = "Exclude the given directories. Multiple entries are permitted."
    )]
    exclude: Vec<String>,

    /// Directory containing image files (mutually exclusive with -c).
    #[arg(
        value_name = "DIRECTORY",
        help = "Directory containing image files (mutually exclusive with -c)."
    )]
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
    // Register SIGUSR1 as early as possible (before PDF/config heavy work on reload).
    let _ = magic_lantern::signal_handler::init();

    let overrides = CliOverrides::from(&cli);

    // Outer reload loop (Python cli.py `while runState`).
    loop {
        let config = Config::from_cli(&overrides)?;

        if let Some(ref dir) = config.directory {
            tracing::info!("Single directory slide show: {}", dir.display());
        }

        let should_reload = if config.dry_run.is_some() {
            controller::dry_run(&config)?
        } else {
            let mut ctl = Controller::new(config)?;
            ctl.run()?
        };

        if !should_reload {
            break;
        }
        tracing::info!("Reloading configuration and slideshow…");
    }

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            tracing::info!("Application ended normally");
            ExitCode::SUCCESS
        }
        Err(Error::Config(msg)) => {
            eprintln!("Error: {msg}");
            tracing::error!("Error: {msg}");
            ExitCode::from(1)
        }
        Err(Error::Slideshow(msg)) => {
            eprintln!("Error: {msg}");
            tracing::error!("Error: {msg}");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("Error: {e}");
            tracing::error!("{e}");
            ExitCode::from(1)
        }
    }
}
