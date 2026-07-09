//! Logging setup (parity with Python `log.py`).
//!
//! Timing helpers (`log_elapsed`) write DEBUG always and WARN when an operation
//! exceeds a threshold so slow paths show up without setting `RUST_LOG`.

use std::fs::OpenOptions;
use std::sync::OnceLock;
use std::time::Instant;

use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

static INIT: OnceLock<()> = OnceLock::new();

/// Default threshold (ms) above which `log_elapsed` emits a WARN.
pub const SLOW_MS: u128 = 100;

/// Log how long an operation took.
///
/// - Always `DEBUG` with `elapsed_ms`
/// - `WARN` when `elapsed_ms >= slow_ms` so hangs surface at default log level
pub fn log_elapsed(op: &str, start: Instant) {
    log_elapsed_threshold(op, start, SLOW_MS);
}

/// Like [`log_elapsed`] with a custom slow threshold.
pub fn log_elapsed_threshold(op: &str, start: Instant, slow_ms: u128) {
    let ms = start.elapsed().as_millis();
    if ms >= slow_ms {
        tracing::warn!(elapsed_ms = ms, "{op} (slow)");
    } else {
        tracing::debug!(elapsed_ms = ms, "{op}");
    }
}

/// Initialize console + file logging.
///
/// Files (in the process CWD):
/// - `magic-lantern-debug.log` — `magic_lantern` DEBUG+ (and `RUST_LOG` if set)
/// - `magic-lantern-error.log` — ERROR+
///
/// Console defaults to `info` (override with `RUST_LOG`).
///
/// Safe to call once; subsequent calls are no-ops.
pub fn init() {
    INIT.get_or_init(|| {
        // Start with fresh logs (Python unlinks then recreates).
        let _ = std::fs::remove_file("magic-lantern-debug.log");
        let _ = std::fs::remove_file("magic-lantern-error.log");

        // Console: respect RUST_LOG, default info (quiet for kiosk).
        let console_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        // Debug file: always capture our crate at debug so timing logs land without env.
        // If RUST_LOG is set, merge it so external crates can be turned up too.
        let file_filter = match EnvFilter::try_from_default_env() {
            Ok(env) => {
                // Ensure our crate stays at least debug even if RUST_LOG is quieter.
                env.add_directive(
                    "magic_lantern=debug"
                        .parse()
                        .expect("static filter directive"),
                )
            }
            Err(_) => EnvFilter::new("magic_lantern=debug"),
        };

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
            .with_level(true)
            .compact()
            .with_filter(console_filter);

        match (debug_file, error_file) {
            (Some(df), Some(ef)) => {
                let debug_layer = fmt::layer()
                    .with_writer(df)
                    .with_ansi(false)
                    .with_target(true)
                    .with_filter(file_filter);
                let error_layer = fmt::layer()
                    .with_writer(ef)
                    .with_ansi(false)
                    .with_filter(tracing_subscriber::filter::LevelFilter::ERROR);
                let _ = tracing_subscriber::registry()
                    .with(console)
                    .with(debug_layer)
                    .with(error_layer)
                    .try_init();
            }
            _ => {
                let _ = tracing_subscriber::registry().with(console).try_init();
            }
        }

        tracing::info!("Application started.");
        tracing::info!("Version: {}", env!("CARGO_PKG_VERSION"));
        tracing::info!(
            "Logging to magic-lantern-debug.log (crate DEBUG+) and magic-lantern-error.log"
        );
        tracing::info!(
            "Set RUST_LOG for console verbosity (e.g. RUST_LOG=magic_lantern=debug)"
        );
        tracing::debug!(
            slow_ms = SLOW_MS,
            "Operations taking ≥ this many ms log as WARN"
        );
    });
}
