//! Configuration for FUSE-based position inference.
//!
//! This module defines configuration for the FUSE request analyzer which
//! infers aircraft position and heading from X-Plane's tile loading patterns
//! when telemetry is unavailable.

use std::time::Duration;

// ==================== FUSE Inference Defaults ====================

/// Default maximum age of requests to consider for inference in seconds.
pub const DEFAULT_FUSE_MAX_REQUEST_AGE_SECS: u64 = 30;

/// Default minimum requests needed before attempting inference.
pub const DEFAULT_FUSE_MIN_REQUESTS_FOR_INFERENCE: usize = 10;

/// Default confidence threshold for using inferred state.
///
/// Below this threshold, falls back to radial prefetch.
pub const DEFAULT_FUSE_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// Default smoothing factor for heading inference.
///
/// Lower values smooth more (0.0-1.0).
pub const DEFAULT_FUSE_HEADING_SMOOTHING: f32 = 0.3;

/// Default number of frontier snapshots to retain for movement detection.
///
/// Used to detect heading from envelope expansion direction over time.
pub const DEFAULT_FUSE_FRONTIER_HISTORY_SIZE: usize = 10;

/// Default cone half-angle for FUSE inference mode in degrees.
///
/// Wider than telemetry mode to account for heading uncertainty.
pub const DEFAULT_FUSE_CONE_HALF_ANGLE: f32 = 45.0;

/// Default prefetch depth beyond the frontier in tiles.
///
/// How many tiles beyond X-Plane's loaded frontier to prefetch.
pub const DEFAULT_FUSE_PREFETCH_DEPTH_TILES: u8 = 4;

/// Default multiplier for lateral buffer width in FUSE mode.
///
/// Lateral buffers are widened by this factor compared to normal mode
/// to account for potential heading estimation errors.
pub const DEFAULT_FUSE_LATERAL_BUFFER_MULTIPLIER: f32 = 1.75;

/// Default extra cone widening when confidence is low in degrees.
///
/// Added to `cone_half_angle` when inference confidence is below threshold.
pub const DEFAULT_FUSE_LOW_CONFIDENCE_CONE_WIDENING: f32 = 15.0;

/// Configuration for FUSE-based inference when telemetry is unavailable.
///
/// When X-Plane's XGPS2 telemetry is disabled or unavailable, the prefetcher
/// can infer aircraft position and heading from the pattern of FUSE tile requests.
///
/// Uses a **dynamic envelope model** that tracks X-Plane's actual tile requests
/// to build a "loaded envelope" and infers movement from frontier expansion.
#[derive(Debug, Clone)]
pub struct FuseInferenceConfig {
    // ==================== Request Tracking ====================
    /// Maximum age of requests to consider for inference in seconds.
    ///
    /// Older requests are pruned from the analysis window.
    /// Default: 30 seconds.
    pub max_request_age_secs: u64,

    /// Minimum requests needed before attempting inference.
    ///
    /// Ensures enough data points for meaningful pattern detection.
    /// Default: 10 requests.
    pub min_requests_for_inference: usize,

    /// Confidence threshold for using inferred state.
    ///
    /// If confidence is below this, falls back to radial prefetch.
    /// Default: 0.5.
    pub confidence_threshold: f32,

    /// Smoothing factor for heading inference (EMA).
    ///
    /// Lower values smooth more (0.0-1.0). Default: 0.3.
    pub heading_smoothing: f32,

    /// Number of frontier snapshots to retain for movement detection.
    ///
    /// Used to detect heading from envelope expansion direction.
    /// Default: 10 snapshots.
    pub frontier_history_size: usize,

    // ==================== Fuzzy Margins ====================
    /// Half-angle of the prefetch cone in degrees.
    ///
    /// Wider than telemetry mode to account for heading uncertainty.
    /// Default: 45°.
    pub cone_half_angle: f32,

    /// Prefetch depth beyond the frontier in tiles.
    ///
    /// How many tiles beyond X-Plane's loaded frontier to prefetch.
    /// Default: 4 tiles.
    pub prefetch_depth_tiles: u8,

    /// Multiplier for lateral buffer width.
    ///
    /// Lateral buffers are widened by this factor compared to normal mode.
    /// Default: 1.75x.
    pub lateral_buffer_multiplier: f32,

    /// Extra cone widening when confidence is low in degrees.
    ///
    /// Added to `cone_half_angle` when inference confidence is below threshold.
    /// Default: 15°.
    pub low_confidence_cone_widening: f32,
}

impl Default for FuseInferenceConfig {
    fn default() -> Self {
        Self {
            // Request tracking
            max_request_age_secs: DEFAULT_FUSE_MAX_REQUEST_AGE_SECS,
            min_requests_for_inference: DEFAULT_FUSE_MIN_REQUESTS_FOR_INFERENCE,
            confidence_threshold: DEFAULT_FUSE_CONFIDENCE_THRESHOLD,
            heading_smoothing: DEFAULT_FUSE_HEADING_SMOOTHING,
            frontier_history_size: DEFAULT_FUSE_FRONTIER_HISTORY_SIZE,

            // Fuzzy margins
            cone_half_angle: DEFAULT_FUSE_CONE_HALF_ANGLE,
            prefetch_depth_tiles: DEFAULT_FUSE_PREFETCH_DEPTH_TILES,
            lateral_buffer_multiplier: DEFAULT_FUSE_LATERAL_BUFFER_MULTIPLIER,
            low_confidence_cone_widening: DEFAULT_FUSE_LOW_CONFIDENCE_CONE_WIDENING,
        }
    }
}

impl FuseInferenceConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the max request age as a Duration.
    pub fn max_request_age(&self) -> Duration {
        Duration::from_secs(self.max_request_age_secs)
    }

    /// Get effective cone half-angle based on confidence.
    ///
    /// When confidence is low, the cone is widened by `low_confidence_cone_widening`
    /// to provide additional safety margin.
    ///
    /// # Arguments
    ///
    /// * `confidence` - Current inference confidence (0.0 to 1.0)
    pub fn effective_cone_half_angle(&self, confidence: f32) -> f32 {
        if confidence < self.confidence_threshold {
            self.cone_half_angle + self.low_confidence_cone_widening
        } else {
            self.cone_half_angle
        }
    }

    /// Get effective lateral buffer depth in tiles.
    ///
    /// Base lateral buffer depth multiplied by `lateral_buffer_multiplier`
    /// to account for inference uncertainty.
    ///
    /// # Arguments
    ///
    /// * `base_lateral_depth` - Base lateral buffer depth in tiles
    pub fn effective_lateral_depth(&self, base_lateral_depth: u8) -> u8 {
        ((base_lateral_depth as f32) * self.lateral_buffer_multiplier).ceil() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuse_config_default() {
        let config = FuseInferenceConfig::default();

        // Request tracking parameters
        assert_eq!(
            config.max_request_age_secs,
            DEFAULT_FUSE_MAX_REQUEST_AGE_SECS
        );
        assert_eq!(
            config.min_requests_for_inference,
            DEFAULT_FUSE_MIN_REQUESTS_FOR_INFERENCE
        );
        assert_eq!(
            config.confidence_threshold,
            DEFAULT_FUSE_CONFIDENCE_THRESHOLD
        );
        assert_eq!(config.heading_smoothing, DEFAULT_FUSE_HEADING_SMOOTHING);
        assert_eq!(
            config.frontier_history_size,
            DEFAULT_FUSE_FRONTIER_HISTORY_SIZE
        );

        // Fuzzy margin parameters
        assert_eq!(config.cone_half_angle, DEFAULT_FUSE_CONE_HALF_ANGLE);
        assert_eq!(
            config.prefetch_depth_tiles,
            DEFAULT_FUSE_PREFETCH_DEPTH_TILES
        );
        assert_eq!(
            config.lateral_buffer_multiplier,
            DEFAULT_FUSE_LATERAL_BUFFER_MULTIPLIER
        );
        assert_eq!(
            config.low_confidence_cone_widening,
            DEFAULT_FUSE_LOW_CONFIDENCE_CONE_WIDENING
        );
    }

    #[test]
    fn test_fuse_config_max_age_duration() {
        let config = FuseInferenceConfig::default();
        assert_eq!(
            config.max_request_age(),
            Duration::from_secs(DEFAULT_FUSE_MAX_REQUEST_AGE_SECS)
        );
    }

    #[test]
    fn test_fuse_effective_cone_half_angle_high_confidence() {
        let config = FuseInferenceConfig::default();
        // Confidence above threshold (0.5) should use base cone angle
        let effective = config.effective_cone_half_angle(0.8);
        assert_eq!(effective, DEFAULT_FUSE_CONE_HALF_ANGLE);
    }

    #[test]
    fn test_fuse_effective_cone_half_angle_low_confidence() {
        let config = FuseInferenceConfig::default();
        // Confidence below threshold should widen the cone
        let effective = config.effective_cone_half_angle(0.3);
        assert_eq!(
            effective,
            DEFAULT_FUSE_CONE_HALF_ANGLE + DEFAULT_FUSE_LOW_CONFIDENCE_CONE_WIDENING
        );
        // 45° + 15° = 60°
        assert_eq!(effective, 60.0);
    }

    #[test]
    fn test_fuse_effective_cone_half_angle_at_threshold() {
        let config = FuseInferenceConfig::default();
        // Confidence exactly at threshold (0.5) should still use base cone
        let effective = config.effective_cone_half_angle(0.5);
        assert_eq!(effective, DEFAULT_FUSE_CONE_HALF_ANGLE);
    }

    #[test]
    fn test_fuse_effective_lateral_depth() {
        let config = FuseInferenceConfig::default();
        // Base depth of 3 tiles × 1.75 multiplier = 5.25 → 6 tiles (ceiling)
        let effective = config.effective_lateral_depth(3);
        assert_eq!(effective, 6);

        // Base depth of 4 tiles × 1.75 = 7 tiles
        let effective = config.effective_lateral_depth(4);
        assert_eq!(effective, 7);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_fuse_fuzzy_margin_constants() {
        // FUSE inference should have generous lateral buffers
        assert!(DEFAULT_FUSE_LATERAL_BUFFER_MULTIPLIER > 1.0);
    }
}
