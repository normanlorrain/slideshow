//! OS signal interface for slideshow reload (Python `signal.py`).
//!
//! On Unix, `SIGUSR1` sets a process-wide flag polled by the controller.
//! Windows: no-op (same as Python `os.name == "posix"` guard).
//!
//! The handler is installed once; reloads share the same flag so we do not
//! stack multiple `signal-hook` registrations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Shared reload request flag.
#[derive(Clone, Debug)]
pub struct ReloadFlag {
    inner: Arc<AtomicBool>,
}

impl ReloadFlag {
    pub fn take(&self) -> bool {
        self.inner.swap(false, Ordering::SeqCst)
    }

    pub fn is_set(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn request(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }
}

/// Install `SIGUSR1` handler once and return the process-wide flag handle.
pub fn init() -> crate::error::Result<ReloadFlag> {
    let arc = FLAG.get_or_init(|| {
        let a = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        {
            if let Err(e) =
                signal_hook::flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&a))
            {
                tracing::error!("Failed to register SIGUSR1: {e}");
            } else {
                tracing::info!(
                    "Signal handler initialised. We are pid: {}",
                    std::process::id()
                );
                tracing::info!("To reset slideshow run:");
                tracing::info!("pkill -USR1 magic-lantern");
            }
        }
        #[cfg(not(unix))]
        {
            tracing::info!("Signal reload not available on this platform");
        }
        a
    });

    // Clear any stale request left from a previous run.
    arc.store(false, Ordering::SeqCst);
    Ok(ReloadFlag {
        inner: Arc::clone(arc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_take_clears() {
        let f = init().unwrap();
        let _ = f.take(); // clear
        f.request();
        assert!(f.is_set());
        assert!(f.take());
        assert!(!f.take());
    }
}
