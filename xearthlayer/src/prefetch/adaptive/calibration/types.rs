//! Core types for performance calibration.
//!
//! Contains the strategy mode enum, calibration results, and recalibration status.

use std::time::Instant;

/// Recommended strategy mode based on measured throughput.
///
/// This enum represents the system's recommendation for how to trigger
/// prefetch operations based on measured tile generation performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyMode {
    /// High throughput (>30 tiles/sec): Can prefetch at 0.3° trigger.
    ///
    /// System can generate tiles fast enough to complete prefetch well
    /// before X-Plane needs them. Uses position-based triggers for
    /// proactive prefetching.
    Aggressive,

    /// Medium throughput (10-30 tiles/sec): Prefetch on circuit breaker close.
    ///
    /// System can generate tiles, but may not complete before X-Plane's
    /// trigger point. Uses opportunistic prefetching during quiet periods.
    #[default]
    Opportunistic,

    /// Low throughput (<10 tiles/sec): Skip prefetch entirely.
    ///
    /// System is too slow to benefit from prefetching. Tiles won't be
    /// ready before X-Plane needs them, so prefetch is disabled to
    /// avoid wasting resources.
    Disabled,
}

impl StrategyMode {
    /// Get a human-readable description of this mode.
    pub fn description(&self) -> &'static str {
        match self {
            StrategyMode::Aggressive => "position-based trigger (fast connection)",
            StrategyMode::Opportunistic => "circuit breaker trigger (moderate connection)",
            StrategyMode::Disabled => "prefetch disabled (slow connection)",
        }
    }

    /// Downgrade to the next lower mode.
    ///
    /// Used when throughput degrades during rolling recalibration.
    pub fn downgrade(&self) -> Self {
        match self {
            StrategyMode::Aggressive => StrategyMode::Opportunistic,
            StrategyMode::Opportunistic => StrategyMode::Disabled,
            StrategyMode::Disabled => StrategyMode::Disabled,
        }
    }

    /// Upgrade to the next higher mode.
    ///
    /// Used when throughput recovers during rolling recalibration.
    pub fn upgrade(&self) -> Self {
        match self {
            StrategyMode::Aggressive => StrategyMode::Aggressive,
            StrategyMode::Opportunistic => StrategyMode::Aggressive,
            StrategyMode::Disabled => StrategyMode::Opportunistic,
        }
    }
}

impl std::fmt::Display for StrategyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyMode::Aggressive => write!(f, "aggressive"),
            StrategyMode::Opportunistic => write!(f, "opportunistic"),
            StrategyMode::Disabled => write!(f, "disabled"),
        }
    }
}

/// Results of performance calibration.
///
/// Contains measured throughput metrics and the recommended strategy mode.
/// This data is used by the `AdaptivePrefetchCoordinator` to decide when
/// and how to prefetch tiles.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceCalibration {
    /// Tiles generated per second (sustained throughput).
    pub throughput_tiles_per_sec: f64,

    /// Average time to generate one tile (milliseconds).
    pub avg_tile_generation_ms: u64,

    /// Standard deviation of generation time (milliseconds).
    ///
    /// High variance indicates inconsistent performance, which may
    /// affect prefetch timing reliability.
    pub tile_generation_stddev_ms: u64,

    /// Confidence level (0.0 - 1.0) based on sample size and variance.
    ///
    /// Higher confidence means more reliable calibration:
    /// - > 0.9: Excellent (many samples, low variance)
    /// - 0.7-0.9: Good (sufficient samples)
    /// - 0.5-0.7: Fair (limited samples or high variance)
    /// - < 0.5: Poor (calibration may be unreliable)
    pub confidence: f64,

    /// Recommended strategy based on throughput.
    pub recommended_strategy: StrategyMode,

    /// When calibration was performed.
    pub calibrated_at: Instant,

    /// Baseline throughput from initial calibration.
    ///
    /// Used for rolling recalibration to detect degradation/recovery.
    pub baseline_throughput: f64,

    /// Number of samples used in calibration.
    pub sample_count: usize,
}

impl PerformanceCalibration {
    /// Create a default calibration with opportunistic mode.
    ///
    /// Used when calibration hasn't completed yet or failed.
    pub fn default_opportunistic() -> Self {
        Self {
            throughput_tiles_per_sec: 15.0,
            avg_tile_generation_ms: 67,
            tile_generation_stddev_ms: 20,
            confidence: 0.5,
            recommended_strategy: StrategyMode::Opportunistic,
            calibrated_at: Instant::now(),
            baseline_throughput: 15.0,
            sample_count: 0,
        }
    }

    /// Estimate time to complete a batch of tiles (milliseconds).
    ///
    /// Uses average generation time plus one standard deviation for safety.
    pub fn estimate_batch_time_ms(&self, tile_count: usize) -> u64 {
        if self.throughput_tiles_per_sec <= 0.0 {
            return u64::MAX;
        }

        let tiles_per_ms = self.throughput_tiles_per_sec / 1000.0;
        let base_time = tile_count as f64 / tiles_per_ms;

        // Add one stddev for safety margin
        (base_time + self.tile_generation_stddev_ms as f64) as u64
    }

    /// Check if throughput has degraded compared to baseline.
    ///
    /// Returns true if current throughput is below the degradation threshold.
    pub fn is_degraded(&self, current_throughput: f64, threshold: f64) -> bool {
        current_throughput < self.baseline_throughput * threshold
    }

    /// Check if throughput has recovered compared to baseline.
    ///
    /// Returns true if current throughput exceeds the recovery threshold.
    pub fn is_recovered(&self, current_throughput: f64, threshold: f64) -> bool {
        current_throughput > self.baseline_throughput * threshold
    }
}

/// Result of a recalibration check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecalibrationResult {
    /// Throughput is stable - no mode change needed.
    Stable,
    /// Throughput has degraded - downgrade recommended.
    Degraded,
    /// Throughput has recovered - upgrade possible.
    Recovered,
}

impl RecalibrationResult {
    /// Get a human-readable description.
    pub fn as_str(&self) -> &'static str {
        match self {
            RecalibrationResult::Stable => "stable",
            RecalibrationResult::Degraded => "degraded",
            RecalibrationResult::Recovered => "recovered",
        }
    }
}

impl std::fmt::Display for RecalibrationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_mode_thresholds() {
        // Test descriptions
        assert!(StrategyMode::Aggressive.description().contains("position"));
        assert!(StrategyMode::Opportunistic
            .description()
            .contains("circuit"));
        assert!(StrategyMode::Disabled.description().contains("disabled"));
    }

    #[test]
    fn test_strategy_mode_upgrade_downgrade() {
        assert_eq!(
            StrategyMode::Aggressive.downgrade(),
            StrategyMode::Opportunistic
        );
        assert_eq!(
            StrategyMode::Opportunistic.downgrade(),
            StrategyMode::Disabled
        );
        assert_eq!(StrategyMode::Disabled.downgrade(), StrategyMode::Disabled);

        assert_eq!(StrategyMode::Aggressive.upgrade(), StrategyMode::Aggressive);
        assert_eq!(
            StrategyMode::Opportunistic.upgrade(),
            StrategyMode::Aggressive
        );
        assert_eq!(
            StrategyMode::Disabled.upgrade(),
            StrategyMode::Opportunistic
        );
    }

    #[test]
    fn test_strategy_mode_display() {
        assert_eq!(format!("{}", StrategyMode::Aggressive), "aggressive");
        assert_eq!(format!("{}", StrategyMode::Opportunistic), "opportunistic");
        assert_eq!(format!("{}", StrategyMode::Disabled), "disabled");
    }

    #[test]
    fn test_calibration_estimate_batch_time() {
        let cal = PerformanceCalibration {
            throughput_tiles_per_sec: 20.0, // 20 tiles/sec = 50ms/tile
            avg_tile_generation_ms: 50,
            tile_generation_stddev_ms: 10,
            confidence: 0.9,
            recommended_strategy: StrategyMode::Opportunistic,
            calibrated_at: Instant::now(),
            baseline_throughput: 20.0,
            sample_count: 100,
        };

        // 100 tiles at 20/sec = 5000ms, plus 10ms stddev = 5010ms
        let estimate = cal.estimate_batch_time_ms(100);
        assert!(estimate >= 5000);
        assert!(estimate <= 5100);
    }

    #[test]
    fn test_calibration_degradation_detection() {
        let cal = PerformanceCalibration {
            throughput_tiles_per_sec: 30.0,
            avg_tile_generation_ms: 33,
            tile_generation_stddev_ms: 5,
            confidence: 0.9,
            recommended_strategy: StrategyMode::Aggressive,
            calibrated_at: Instant::now(),
            baseline_throughput: 30.0,
            sample_count: 100,
        };

        // 70% threshold: 30 * 0.7 = 21
        assert!(cal.is_degraded(20.0, 0.7)); // 20 < 21
        assert!(!cal.is_degraded(25.0, 0.7)); // 25 > 21

        // 90% threshold: 30 * 0.9 = 27
        assert!(!cal.is_recovered(25.0, 0.9)); // 25 < 27
        assert!(cal.is_recovered(28.0, 0.9)); // 28 > 27
    }

    #[test]
    fn test_default_opportunistic_calibration() {
        let cal = PerformanceCalibration::default_opportunistic();
        assert_eq!(cal.recommended_strategy, StrategyMode::Opportunistic);
        assert!(cal.confidence < 1.0);
    }

    #[test]
    fn test_recalibration_result_display() {
        assert_eq!(RecalibrationResult::Stable.as_str(), "stable");
        assert_eq!(RecalibrationResult::Degraded.as_str(), "degraded");
        assert_eq!(RecalibrationResult::Recovered.as_str(), "recovered");

        assert_eq!(format!("{}", RecalibrationResult::Stable), "stable");
    }
}
