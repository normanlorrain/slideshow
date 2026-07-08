//! magic-lantern — Rust port CLI.
//!
//! Phases 0–3: config, slideshow, slide pipeline, controller/UI, SIGUSR1 reload.

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
    after_help = "To reload the configuration, send it the USR1 signal:\n\n    pkill -USR1 magic-lantern\n\nKeys while running: space pause, q quit, n/→ next, p/← previous, y year.\n\nSee https://github.com/normanlorrain/magic-lantern for more details."
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

    // Outer reload loop (Python cli.py `while runState`).
    loop {
        let config = Config::from_cli(&overrides)?;

        if config.directory.is_some() {
            tracing::info!(
                "Single directory slide show: {}",
                config.directory.as_ref().unwrap().display()
            );
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
