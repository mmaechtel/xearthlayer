//! Rolling recalibrator for monitoring throughput during flight.
//!
//! Maintains a sliding window of recent tile completions and periodically
//! checks if throughput has changed significantly from the baseline.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::types::{PerformanceCalibration, RecalibrationResult, StrategyMode};
use crate::prefetch::adaptive::config::CalibrationConfig;

/// Sample for rolling throughput calculation.
#[derive(Debug, Clone, Copy)]
struct RollingSample {
    /// When the sample was recorded.
    timestamp: Instant,
    /// Duration of tile generation (milliseconds).
    #[allow(dead_code)]
    duration_ms: u64,
}

/// Rolling recalibrator for monitoring throughput during flight.
///
/// Maintains a sliding window of recent tile completions and periodically
/// checks if throughput has changed significantly from the baseline.
///
/// # Usage
///
/// ```ignore
/// let mut recalibrator = RollingCalibrator::new(&config);
///
/// // Record samples as tiles complete
/// recalibrator.record_sample(generation_duration);
///
/// // Periodically check for recalibration
/// if let Some(new_mode) = recalibrator.check_recalibration(&baseline_calibration) {
///     // Mode changed - update coordinator
/// }
/// ```
#[derive(Debug)]
pub struct RollingCalibrator {
    /// Recent samples within the rolling window.
    samples: VecDeque<RollingSample>,

    /// Rolling window duration (how much history to consider).
    window_duration: Duration,

    /// Minimum interval between recalibration checks.
    recalibration_interval: Duration,

    /// When we last performed a recalibration check.
    last_recalibration: Option<Instant>,

    /// Degradation threshold (percentage of baseline as decimal).
    degradation_threshold: f64,

    /// Recovery threshold (percentage of baseline as decimal).
    recovery_threshold: f64,

    /// Current mode (tracks changes from recalibration).
    current_mode: Option<StrategyMode>,

    /// Maximum samples to keep (prevents unbounded memory).
    max_samples: usize,
}

impl RollingCalibrator {
    /// Create a new rolling calibrator with the given configuration.
    pub fn new(config: &CalibrationConfig) -> Self {
        Self {
            samples: VecDeque::with_capacity(1000),
            window_duration: config.rolling_window,
            recalibration_interval: config.recalibration_interval,
            last_recalibration: None,
            degradation_threshold: config.degradation_threshold,
            recovery_threshold: config.recovery_threshold,
            current_mode: None,
            max_samples: 10000,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(&CalibrationConfig::default())
    }

    /// Create with explicit parameters (useful for testing).
    pub fn with_params(
        window_duration: Duration,
        recalibration_interval: Duration,
        degradation_threshold: f64,
        recovery_threshold: f64,
    ) -> Self {
        Self {
            samples: VecDeque::with_capacity(1000),
            window_duration,
            recalibration_interval,
            last_recalibration: None,
            degradation_threshold,
            recovery_threshold,
            current_mode: None,
            max_samples: 10000,
        }
    }

    /// Record a tile completion sample.
    ///
    /// Call this for each tile that completes during flight.
    pub fn record_sample(&mut self, duration: Duration) {
        let now = Instant::now();

        self.samples.push_back(RollingSample {
            timestamp: now,
            duration_ms: duration.as_millis() as u64,
        });

        // Prune old samples outside the window
        self.prune_old_samples(now);

        // Limit total samples
        while self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }
    }

    /// Check if recalibration is needed and return new mode if changed.
    ///
    /// # Arguments
    ///
    /// * `baseline` - The initial calibration to compare against
    ///
    /// # Returns
    ///
    /// `Some(new_mode)` if the mode should change, `None` if stable.
    pub fn check_recalibration(
        &mut self,
        baseline: &PerformanceCalibration,
    ) -> Option<StrategyMode> {
        let now = Instant::now();

        // Check if enough time has passed since last recalibration
        if let Some(last) = self.last_recalibration {
            if now.duration_since(last) < self.recalibration_interval {
                return None;
            }
        }

        // Need minimum samples for reliable measurement
        if self.samples.len() < 10 {
            return None;
        }

        self.last_recalibration = Some(now);

        // Calculate current throughput
        let result = self.evaluate_throughput(baseline);

        // Determine if mode should change
        let current = self.current_mode.unwrap_or(baseline.recommended_strategy);

        let new_mode = match result {
            RecalibrationResult::Degraded => {
                let downgraded = current.downgrade();
                if downgraded != current {
                    tracing::info!(
                        from = %current,
                        to = %downgraded,
                        current_throughput = format!("{:.1}", self.calculate_throughput()),
                        baseline = format!("{:.1}", baseline.baseline_throughput),
                        "Rolling recalibration: throughput degraded, downgrading mode"
                    );
                    Some(downgraded)
                } else {
                    None
                }
            }
            RecalibrationResult::Recovered => {
                let upgraded = current.upgrade();
                if upgraded != current {
                    tracing::info!(
                        from = %current,
                        to = %upgraded,
                        current_throughput = format!("{:.1}", self.calculate_throughput()),
                        baseline = format!("{:.1}", baseline.baseline_throughput),
                        "Rolling recalibration: throughput recovered, upgrading mode"
                    );
                    Some(upgraded)
                } else {
                    None
                }
            }
            RecalibrationResult::Stable => None,
        };

        if let Some(mode) = new_mode {
            self.current_mode = Some(mode);
        }

        new_mode
    }

    /// Force a recalibration check (ignoring interval).
    ///
    /// Useful for testing or when conditions change significantly.
    pub fn force_check(&mut self, baseline: &PerformanceCalibration) -> RecalibrationResult {
        self.last_recalibration = Some(Instant::now());
        self.evaluate_throughput(baseline)
    }

    /// Get the current mode (after any recalibrations).
    pub fn current_mode(&self) -> Option<StrategyMode> {
        self.current_mode
    }

    /// Get the number of samples in the rolling window.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Calculate current throughput from samples in the window.
    pub fn calculate_throughput(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }

        // Get time span of samples
        let first = self.samples.front().map(|s| s.timestamp);
        let last = self.samples.back().map(|s| s.timestamp);

        match (first, last) {
            (Some(first), Some(last)) => {
                let elapsed = last.duration_since(first);
                let elapsed_secs = elapsed.as_secs_f64().max(0.001);
                self.samples.len() as f64 / elapsed_secs
            }
            _ => 0.0,
        }
    }

    /// Reset the calibrator state.
    pub fn reset(&mut self) {
        self.samples.clear();
        self.last_recalibration = None;
        self.current_mode = None;
    }

    /// Evaluate throughput against baseline.
    fn evaluate_throughput(&self, baseline: &PerformanceCalibration) -> RecalibrationResult {
        let current = self.calculate_throughput();

        if baseline.is_degraded(current, self.degradation_threshold) {
            RecalibrationResult::Degraded
        } else if baseline.is_recovered(current, self.recovery_threshold) {
            RecalibrationResult::Recovered
        } else {
            RecalibrationResult::Stable
        }
    }

    /// Remove samples older than the rolling window.
    fn prune_old_samples(&mut self, now: Instant) {
        let cutoff = now - self.window_duration;
        while let Some(front) = self.samples.front() {
            if front.timestamp < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_baseline() -> PerformanceCalibration {
        PerformanceCalibration {
            throughput_tiles_per_sec: 25.0,
            avg_tile_generation_ms: 40,
            tile_generation_stddev_ms: 10,
            confidence: 0.9,
            recommended_strategy: StrategyMode::Opportunistic,
            calibrated_at: Instant::now(),
            baseline_throughput: 25.0,
            sample_count: 100,
        }
    }

    #[test]
    fn test_rolling_calibrator_creation() {
        let rc = RollingCalibrator::with_defaults();
        assert_eq!(rc.sample_count(), 0);
        assert!(rc.current_mode().is_none());
    }

    #[test]
    fn test_rolling_calibrator_records_samples() {
        let mut rc = RollingCalibrator::with_defaults();

        for _ in 0..10 {
            rc.record_sample(Duration::from_millis(50));
        }

        assert_eq!(rc.sample_count(), 10);
    }

    #[test]
    fn test_rolling_calibrator_prunes_old_samples() {
        let mut rc = RollingCalibrator::with_params(
            Duration::from_millis(100), // Very short window
            Duration::from_millis(10),
            0.7,
            0.9,
        );

        // Record some samples
        rc.record_sample(Duration::from_millis(50));
        rc.record_sample(Duration::from_millis(50));

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(150));

        // Record new sample - old ones should be pruned
        rc.record_sample(Duration::from_millis(50));

        assert_eq!(rc.sample_count(), 1);
    }

    #[test]
    fn test_rolling_calibrator_throughput_calculation() {
        let mut rc = RollingCalibrator::with_defaults();

        // Record samples rapidly
        for _ in 0..100 {
            rc.record_sample(Duration::from_millis(10));
        }

        let throughput = rc.calculate_throughput();
        // Should be very high since samples are recorded almost instantly
        assert!(throughput > 0.0);
    }

    #[test]
    fn test_rolling_calibrator_stable() {
        let mut rc = RollingCalibrator::with_params(
            Duration::from_secs(300),
            Duration::from_millis(1), // Allow immediate recalibration
            0.7,
            0.9,
        );

        let baseline = test_baseline(); // 25 tiles/sec

        // Simulate ~25 tiles/sec (within stable range)
        for _ in 0..50 {
            rc.record_sample(Duration::from_millis(40));
            std::thread::sleep(Duration::from_millis(1));
        }

        let result = rc.force_check(&baseline);
        // With samples so close together, throughput will be very high
        // which means it's "recovered" (above 90% of baseline)
        assert!(result == RecalibrationResult::Recovered || result == RecalibrationResult::Stable);
    }

    #[test]
    fn test_rolling_calibrator_degradation() {
        let mut rc = RollingCalibrator::with_params(
            Duration::from_secs(300),
            Duration::from_millis(1),
            0.7, // Degrade if < 70% of baseline
            0.9,
        );

        let mut baseline = test_baseline();
        baseline.baseline_throughput = 1000.0; // Very high baseline

        // Record slow samples
        for _ in 0..20 {
            rc.record_sample(Duration::from_millis(100));
            std::thread::sleep(Duration::from_millis(10));
        }

        let result = rc.force_check(&baseline);
        // Current throughput (~50-100/sec) is far below 1000 * 0.7 = 700
        assert_eq!(result, RecalibrationResult::Degraded);
    }

    #[test]
    fn test_rolling_calibrator_mode_downgrade() {
        let mut rc = RollingCalibrator::with_params(
            Duration::from_secs(300),
            Duration::from_millis(1),
            0.7,
            0.9,
        );

        let mut baseline = test_baseline();
        baseline.baseline_throughput = 10000.0; // Very high baseline
        baseline.recommended_strategy = StrategyMode::Aggressive;

        // Record some samples
        for _ in 0..20 {
            rc.record_sample(Duration::from_millis(100));
            std::thread::sleep(Duration::from_millis(5));
        }

        // Check recalibration - should recommend downgrade
        let new_mode = rc.check_recalibration(&baseline);
        assert_eq!(new_mode, Some(StrategyMode::Opportunistic));
        assert_eq!(rc.current_mode(), Some(StrategyMode::Opportunistic));
    }

    #[test]
    fn test_rolling_calibrator_interval_throttle() {
        let mut rc = RollingCalibrator::with_params(
            Duration::from_secs(300),
            Duration::from_secs(60), // 60 second interval
            0.7,
            0.9,
        );

        let baseline = test_baseline();

        // Record samples
        for _ in 0..20 {
            rc.record_sample(Duration::from_millis(40));
        }

        // First check should work
        rc.last_recalibration = None;
        let _ = rc.check_recalibration(&baseline);
        assert!(rc.last_recalibration.is_some());

        // Second check immediately after should be throttled
        let result = rc.check_recalibration(&baseline);
        assert!(result.is_none()); // Throttled
    }

    #[test]
    fn test_rolling_calibrator_reset() {
        let mut rc = RollingCalibrator::with_defaults();

        rc.record_sample(Duration::from_millis(50));
        rc.current_mode = Some(StrategyMode::Aggressive);

        rc.reset();

        assert_eq!(rc.sample_count(), 0);
        assert!(rc.current_mode().is_none());
        assert!(rc.last_recalibration.is_none());
    }
}
