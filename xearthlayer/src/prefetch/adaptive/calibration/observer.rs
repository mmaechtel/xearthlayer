//! Throughput observer trait and factory functions.
//!
//! Provides the abstraction for recording tile completions and
//! observing calibration results.

use std::sync::Arc;
use std::time::Duration;

use super::calibrator::PerformanceCalibrator;
use super::types::PerformanceCalibration;
use crate::prefetch::adaptive::config::CalibrationConfig;

/// Observer trait for recording tile generation completions.
///
/// Implementations receive notifications when tiles are generated,
/// allowing throughput measurement without tight coupling to the
/// job executor.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` for use across async tasks
/// and the job executor's completion callbacks.
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use xearthlayer::prefetch::adaptive::{ThroughputObserver, PerformanceCalibrator};
///
/// let calibrator = Arc::new(PerformanceCalibrator::new(config));
///
/// // In job executor completion callback:
/// calibrator.record_tile_completion(generation_duration);
///
/// // Later, get calibration results:
/// if let Some(calibration) = calibrator.get_calibration() {
///     println!("Throughput: {:.1} tiles/sec", calibration.throughput_tiles_per_sec);
/// }
/// ```
pub trait ThroughputObserver: Send + Sync {
    /// Record completion of a tile generation.
    ///
    /// Called by the job executor when a tile finishes generating.
    /// The duration is the time from job start to completion.
    fn record_tile_completion(&self, duration: Duration);

    /// Get the current calibration, if available.
    ///
    /// Returns `None` if calibration hasn't completed yet.
    fn get_calibration(&self) -> Option<PerformanceCalibration>;

    /// Check if calibration is complete.
    fn is_calibrated(&self) -> bool {
        self.get_calibration().is_some()
    }
}

/// Shared throughput observer for use across the system.
///
/// Wraps a `PerformanceCalibrator` in an `Arc` for convenient sharing.
pub type SharedThroughputObserver = Arc<dyn ThroughputObserver>;

/// Create a shared throughput observer with default configuration.
pub fn create_throughput_observer() -> SharedThroughputObserver {
    Arc::new(PerformanceCalibrator::with_defaults())
}

/// Create a shared throughput observer with custom configuration.
pub fn create_throughput_observer_with_config(
    config: CalibrationConfig,
) -> SharedThroughputObserver {
    Arc::new(PerformanceCalibrator::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throughput_observer_trait() {
        let calibrator: Arc<dyn ThroughputObserver> =
            Arc::new(PerformanceCalibrator::with_defaults());

        assert!(!calibrator.is_calibrated());

        calibrator.record_tile_completion(Duration::from_millis(100));
        assert_eq!(calibrator.get_calibration(), None);
    }

    #[test]
    fn test_create_shared_observer() {
        let observer = create_throughput_observer();
        observer.record_tile_completion(Duration::from_millis(50));
        assert!(!observer.is_calibrated());
    }
}
