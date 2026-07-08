//! Logging setup (parity with Python `log.py`).

use std::fs::OpenOptions;
use std::sync::OnceLock;

use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

static INIT: OnceLock<()> = OnceLock::new();

/// Initialize console + file logging.
///
/// Files (in the process CWD):
/// - `magic-lantern-debug.log` — all events (DEBUG+)
/// - `magic-lantern-error.log` — ERROR+
///
/// Safe to call once; subsequent calls are no-ops.
pub fn init() {
    INIT.get_or_init(|| {
        // Start with fresh logs (Python unlinks then recreates).
        let _ = std::fs::remove_file("magic-lantern-debug.log");
        let _ = std::fs::remove_file("magic-lantern-error.log");

        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        let debug_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("magic-lantern-debug.log")
            .ok();
        let error_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("magic-lantern-error.log")
            .ok();

        let console = fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_level(false)
            .compact();

        let registry = tracing_subscriber::registry().with(filter).with(console);

        match (debug_file, error_file) {
            (Some(df), Some(ef)) => {
                let debug_layer = fmt::layer()
                    .with_writer(df)
                    .with_ansi(false)
                    .with_target(true);
                let error_layer = fmt::layer()
                    .with_writer(ef)
                    .with_ansi(false)
                    .with_filter(tracing_subscriber::filter::LevelFilter::ERROR);
                let _ = registry.with(debug_layer).with(error_layer).try_init();
            }
            _ => {
                let _ = registry.try_init();
            }
        }

        tracing::info!("Application started.");
        tracing::info!("Version: {}", env!("CARGO_PKG_VERSION"));
        tracing::info!("Logging to magic-lantern-debug.log and magic-lantern-error.log");
    });
}
