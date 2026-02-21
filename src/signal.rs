use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::AppError;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

/// Installs the Ctrl+C handler. Safe to call multiple times; only the first call has effect.
pub(crate) fn install_handler_once() {
    INIT.call_once(|| {
        ctrlc::set_handler(|| {
            INTERRUPTED.store(true, Ordering::SeqCst);
        })
        .expect("Failed to set Ctrl+C handler");
    });
}

/// Returns `Err(AppError::Cancelled)` if the interrupt flag is set.
pub(crate) fn check_interrupted() -> Result<(), AppError> {
    if INTERRUPTED.load(Ordering::SeqCst) {
        Err(AppError::Cancelled)
    } else {
        Ok(())
    }
}

/// Returns `true` if the interrupt flag was set, and clears it.
#[allow(dead_code)]
pub(crate) fn check_and_clear_interrupted() -> bool {
    INTERRUPTED.swap(false, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_interrupted_returns_ok_when_not_interrupted() {
        INTERRUPTED.store(false, Ordering::SeqCst);
        assert!(check_interrupted().is_ok());
    }

    #[test]
    fn check_interrupted_returns_cancelled_when_interrupted() {
        INTERRUPTED.store(true, Ordering::SeqCst);
        let err = check_interrupted().unwrap_err();
        assert!(matches!(err, AppError::Cancelled));
        // Clean up
        INTERRUPTED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn check_and_clear_resets_flag() {
        INTERRUPTED.store(true, Ordering::SeqCst);
        assert!(check_and_clear_interrupted());
        assert!(!INTERRUPTED.load(Ordering::SeqCst));
    }
}
