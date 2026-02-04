//! Performance calibrator for initial throughput measurement.
//!
//! Measures tile generation throughput during X-Plane's initial 12° × 12°
//! scenery load to determine the optimal prefetch strategy.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::observer::ThroughputObserver;
use super::types::{PerformanceCalibration, StrategyMode};
use crate::prefetch::adaptive::config::CalibrationConfig;

/// Internal sample data for throughput calculation.
#[derive(Debug, Clone, Copy)]
struct TileSample {
    /// How long it took to generate (milliseconds).
    duration_ms: u64,
}

/// State for the performance calibrator.
#[derive(Debug)]
struct CalibratorState {
    /// Recent tile completion samples.
    samples: VecDeque<TileSample>,
    /// When calibration started.
    start_time: Option<Instant>,
    /// Finalized calibration result.
    calibration: Option<PerformanceCalibration>,
    /// Whether we're in the initial calibration phase.
    is_calibrating: bool,
}

impl CalibratorState {
    fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(1000),
            start_time: None,
            calibration: None,
            is_calibrating: true,
        }
    }
}

/// Performance calibrator for measuring tile generation throughput.
///
/// Collects tile completion samples during X-Plane's initial load and
/// calculates throughput metrics to determine the optimal prefetch strategy.
///
/// # Usage
///
/// 1. Create a calibrator and share it with the job executor
/// 2. Record tile completions via `record_tile_completion()`
/// 3. Call `finalize_calibration()` when initial load completes
/// 4. Retrieve results via `get_calibration()`
///
/// # Thread Safety
///
/// Uses internal `Mutex` for state protection. Safe to share across threads.
pub struct PerformanceCalibrator {
    config: CalibrationConfig,
    state: Mutex<CalibratorState>,
    /// Fast path: atomic counter for total completions (avoids lock contention).
    completion_count: AtomicU64,
}

impl std::fmt::Debug for PerformanceCalibrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerformanceCalibrator")
            .field("config", &self.config)
            .field(
                "completion_count",
                &self.completion_count.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl PerformanceCalibrator {
    /// Create a new performance calibrator.
    pub fn new(config: CalibrationConfig) -> Self {
        Self {
            config,
            state: Mutex::new(CalibratorState::new()),
            completion_count: AtomicU64::new(0),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CalibrationConfig::default())
    }

    /// Get the total number of recorded completions.
    pub fn total_completions(&self) -> u64 {
        self.completion_count.load(Ordering::Relaxed)
    }

    /// Finalize calibration and calculate results.
    ///
    /// Call this when the initial load phase completes (detected via
    /// `FuseLoadMonitor` high→low transition). After this, no more
    /// samples are collected for initial calibration.
    ///
    /// Returns the calibration result, or `None` if insufficient samples.
    pub fn finalize_calibration(&self) -> Option<PerformanceCalibration> {
        let mut state = self.state.lock().unwrap();

        if !state.is_calibrating {
            // Already finalized
            return state.calibration.clone();
        }

        let calibration = self.calculate_calibration(&state);
        state.calibration = calibration.clone();
        state.is_calibrating = false;

        if let Some(ref cal) = calibration {
            tracing::info!(
                throughput = format!("{:.1}", cal.throughput_tiles_per_sec),
                avg_ms = cal.avg_tile_generation_ms,
                stddev_ms = cal.tile_generation_stddev_ms,
                confidence = format!("{:.2}", cal.confidence),
                mode = %cal.recommended_strategy,
                samples = cal.sample_count,
                "Performance calibration complete"
            );
        } else {
            tracing::warn!(
                samples = state.samples.len(),
                min_required = self.config.min_samples,
                "Calibration failed: insufficient samples"
            );
        }

        calibration
    }

    /// Calculate calibration from collected samples.
    fn calculate_calibration(&self, state: &CalibratorState) -> Option<PerformanceCalibration> {
        if state.samples.len() < self.config.min_samples {
            return None;
        }

        let start_time = state.start_time?;
        let now = Instant::now();
        let elapsed = now.duration_since(start_time);
        let elapsed_secs = elapsed.as_secs_f64().max(0.001);

        // Calculate throughput
        let throughput = state.samples.len() as f64 / elapsed_secs;

        // Calculate average and stddev of generation times
        let durations: Vec<u64> = state.samples.iter().map(|s| s.duration_ms).collect();
        let avg_ms = durations.iter().sum::<u64>() / durations.len() as u64;

        let variance = durations
            .iter()
            .map(|&d| {
                let diff = d as f64 - avg_ms as f64;
                diff * diff
            })
            .sum::<f64>()
            / durations.len() as f64;
        let stddev_ms = variance.sqrt() as u64;

        // Calculate confidence based on sample size and variance
        let sample_factor = (state.samples.len() as f64 / 200.0).min(1.0);
        let variance_factor = 1.0 - (stddev_ms as f64 / avg_ms as f64).min(0.5);
        let confidence = (sample_factor * 0.7 + variance_factor * 0.3).clamp(0.0, 1.0);

        // Determine strategy mode
        let mode = self.select_strategy_mode(throughput);

        Some(PerformanceCalibration {
            throughput_tiles_per_sec: throughput,
            avg_tile_generation_ms: avg_ms,
            tile_generation_stddev_ms: stddev_ms,
            confidence,
            recommended_strategy: mode,
            calibrated_at: now,
            baseline_throughput: throughput,
            sample_count: state.samples.len(),
        })
    }

    /// Select strategy mode based on throughput.
    pub(crate) fn select_strategy_mode(&self, throughput: f64) -> StrategyMode {
        if throughput >= self.config.aggressive_threshold {
            StrategyMode::Aggressive
        } else if throughput >= self.config.opportunistic_threshold {
            StrategyMode::Opportunistic
        } else {
            StrategyMode::Disabled
        }
    }

    /// Manually set a calibration (for testing or override).
    pub fn set_calibration(&self, calibration: PerformanceCalibration) {
        let mut state = self.state.lock().unwrap();
        state.calibration = Some(calibration);
        state.is_calibrating = false;
    }

    /// Reset calibration state (for testing).
    #[cfg(test)]
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        *state = CalibratorState::new();
        self.completion_count.store(0, Ordering::Relaxed);
    }
}

impl ThroughputObserver for PerformanceCalibrator {
    fn record_tile_completion(&self, duration: Duration) {
        // Fast path: always increment counter
        self.completion_count.fetch_add(1, Ordering::Relaxed);

        let mut state = self.state.lock().unwrap();

        // Only collect samples during calibration phase
        if !state.is_calibrating {
            return;
        }

        let now = Instant::now();

        // Set start time on first sample
        if state.start_time.is_none() {
            state.start_time = Some(now);
        }

        // Add sample
        state.samples.push_back(TileSample {
            duration_ms: duration.as_millis() as u64,
        });

        // Limit sample buffer size
        while state.samples.len() > 10000 {
            state.samples.pop_front();
        }

        // Auto-finalize after sample duration
        if let Some(start) = state.start_time {
            if now.duration_since(start) >= self.config.sample_duration {
                drop(state); // Release lock before finalize
                self.finalize_calibration();
            }
        }
    }

    fn get_calibration(&self) -> Option<PerformanceCalibration> {
        self.state.lock().unwrap().calibration.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_mode_thresholds() {
        let config = CalibrationConfig::default();
        let calibrator = PerformanceCalibrator::new(config.clone());

        // High throughput → Aggressive
        assert_eq!(
            calibrator.select_strategy_mode(35.0),
            StrategyMode::Aggressive
        );

        // Medium throughput → Opportunistic
        assert_eq!(
            calibrator.select_strategy_mode(20.0),
            StrategyMode::Opportunistic
        );

        // Low throughput → Disabled
        assert_eq!(calibrator.select_strategy_mode(5.0), StrategyMode::Disabled);

        // Edge cases
        assert_eq!(
            calibrator.select_strategy_mode(config.aggressive_threshold),
            StrategyMode::Aggressive
        );
        assert_eq!(
            calibrator.select_strategy_mode(config.opportunistic_threshold),
            StrategyMode::Opportunistic
        );
    }

    #[test]
    fn test_calibrator_records_samples() {
        let config = CalibrationConfig {
            min_samples: 5,
            sample_duration: Duration::from_secs(60),
            ..Default::default()
        };
        let calibrator = PerformanceCalibrator::new(config);

        // Record some samples
        for _ in 0..10 {
            calibrator.record_tile_completion(Duration::from_millis(100));
        }

        assert_eq!(calibrator.total_completions(), 10);
    }

    #[test]
    fn test_calibrator_finalize() {
        let config = CalibrationConfig {
            min_samples: 5,
            sample_duration: Duration::from_secs(60),
            aggressive_threshold: 30.0,
            opportunistic_threshold: 10.0,
            ..Default::default()
        };
        let calibrator = PerformanceCalibrator::new(config);

        // Record samples
        for _ in 0..10 {
            calibrator.record_tile_completion(Duration::from_millis(50));
        }

        // Finalize
        let result = calibrator.finalize_calibration();
        assert!(result.is_some());

        let cal = result.unwrap();
        assert!(cal.throughput_tiles_per_sec > 0.0);
        assert_eq!(cal.sample_count, 10);
    }

    #[test]
    fn test_calibrator_insufficient_samples() {
        let config = CalibrationConfig {
            min_samples: 100, // High threshold
            sample_duration: Duration::from_secs(60),
            ..Default::default()
        };
        let calibrator = PerformanceCalibrator::new(config);

        // Record too few samples
        for _ in 0..5 {
            calibrator.record_tile_completion(Duration::from_millis(100));
        }

        // Finalize should return None
        let result = calibrator.finalize_calibration();
        assert!(result.is_none());
    }
}
