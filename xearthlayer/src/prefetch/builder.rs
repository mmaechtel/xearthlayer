//! Prefetch strategy utilities.
//!
//! This module provides utilities for handling prefetch strategy configuration.
//! As of v0.4.0, the adaptive prefetch system is the only available strategy.
//!
//! # Legacy Strategy Migration
//!
//! If a configuration file specifies a legacy strategy (radial, heading-aware,
//! tile-based), a warning is logged and the adaptive strategy is used.

/// Check if a strategy string refers to a legacy (deprecated) strategy.
///
/// Returns `true` if the strategy is one of the removed legacy strategies,
/// `false` for "adaptive", "auto", or unknown values.
pub fn is_legacy_strategy(strategy: &str) -> bool {
    matches!(
        strategy.to_lowercase().as_str(),
        "radial" | "heading-aware" | "tile-based"
    )
}

/// Log a warning if a legacy strategy is configured.
///
/// This should be called during startup to inform users that their
/// configured strategy is deprecated.
pub fn warn_if_legacy(strategy: &str) {
    if is_legacy_strategy(strategy) {
        tracing::warn!(
            strategy = %strategy,
            "Configured prefetch strategy '{}' is deprecated - using adaptive",
            strategy
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_strategy_detection() {
        assert!(is_legacy_strategy("radial"));
        assert!(is_legacy_strategy("heading-aware"));
        assert!(is_legacy_strategy("tile-based"));
        assert!(is_legacy_strategy("RADIAL")); // Case-insensitive

        assert!(!is_legacy_strategy("adaptive"));
        assert!(!is_legacy_strategy("auto"));
        assert!(!is_legacy_strategy("unknown"));
    }
}
