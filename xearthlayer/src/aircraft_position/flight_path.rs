//! Flight path history and track calculation.
//!
//! Maintains a history of recent position samples for deriving ground track
//! from position deltas when authoritative track data is unavailable.
//!
//! # Design
//!
//! - Stores last 30 samples at 1Hz (30 seconds of history)
//! - Calculates track as bearing from oldest to newest position
//! - Used as fallback when XGPS2 track is unavailable
//! - History is reusable for other use cases (visualization, analytics)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Default maximum samples to retain (30 seconds at 1Hz).
const DEFAULT_MAX_SAMPLES: usize = 30;

/// Default minimum interval between samples (1Hz = 1 second).
const DEFAULT_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// Minimum distance (in degrees) to calculate a reliable track.
/// ~100m at equator = ~0.001 degrees.
const MIN_DISTANCE_FOR_TRACK_DEG: f64 = 0.001;

/// A single position sample in the flight path history.
#[derive(Debug, Clone, Copy)]
pub struct PositionSample {
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// When this sample was recorded.
    pub timestamp: Instant,
}

impl PositionSample {
    /// Create a new position sample.
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
            timestamp: Instant::now(),
        }
    }

    /// Create a position sample with explicit timestamp (for testing).
    pub fn with_timestamp(latitude: f64, longitude: f64, timestamp: Instant) -> Self {
        Self {
            latitude,
            longitude,
            timestamp,
        }
    }
}

/// Configuration for flight path history.
#[derive(Debug, Clone)]
pub struct FlightPathConfig {
    /// Maximum samples to retain.
    pub max_samples: usize,
    /// Minimum interval between samples.
    pub sample_interval: Duration,
    /// Minimum distance for track calculation.
    pub min_distance_deg: f64,
}

impl Default for FlightPathConfig {
    fn default() -> Self {
        Self {
            max_samples: DEFAULT_MAX_SAMPLES,
            sample_interval: DEFAULT_SAMPLE_INTERVAL,
            min_distance_deg: MIN_DISTANCE_FOR_TRACK_DEG,
        }
    }
}

/// Flight path history - stores recent positions for track calculation.
///
/// # Usage
///
/// ```ignore
/// let mut history = FlightPathHistory::new();
///
/// // Record positions as they arrive
/// history.record_position(lat, lon);
///
/// // Calculate derived track when needed
/// if let Some(track) = history.calculate_track() {
///     println!("Derived track: {:.1}°", track);
/// }
/// ```
#[derive(Debug)]
pub struct FlightPathHistory {
    /// Recent position samples (oldest first).
    samples: VecDeque<PositionSample>,
    /// Configuration.
    config: FlightPathConfig,
    /// Last sample time (for rate limiting).
    last_sample_time: Option<Instant>,
}

impl Default for FlightPathHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl FlightPathHistory {
    /// Create a new flight path history with default configuration.
    pub fn new() -> Self {
        Self::with_config(FlightPathConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: FlightPathConfig) -> Self {
        Self {
            samples: VecDeque::with_capacity(config.max_samples),
            config,
            last_sample_time: None,
        }
    }

    /// Record a new position sample.
    ///
    /// Respects the sample interval - samples arriving too quickly are ignored.
    /// Returns true if the sample was recorded.
    pub fn record_position(&mut self, latitude: f64, longitude: f64) -> bool {
        let now = Instant::now();

        // Rate limiting: only accept samples at configured interval
        if let Some(last) = self.last_sample_time {
            if now.duration_since(last) < self.config.sample_interval {
                return false;
            }
        }

        self.samples
            .push_back(PositionSample::new(latitude, longitude));
        self.last_sample_time = Some(now);

        // Trim to max samples
        while self.samples.len() > self.config.max_samples {
            self.samples.pop_front();
        }

        true
    }

    /// Record a position sample with explicit timestamp (for testing).
    #[cfg(test)]
    pub fn record_position_at(&mut self, latitude: f64, longitude: f64, timestamp: Instant) {
        self.samples.push_back(PositionSample::with_timestamp(
            latitude, longitude, timestamp,
        ));

        // Trim to max samples
        while self.samples.len() > self.config.max_samples {
            self.samples.pop_front();
        }
    }

    /// Calculate ground track from position history.
    ///
    /// Returns the bearing from the oldest to newest position, or None if:
    /// - Insufficient samples (< 2)
    /// - Distance too small for reliable track
    ///
    /// Track is in degrees (0-360), where 0 = North, 90 = East.
    pub fn calculate_track(&self) -> Option<f32> {
        if self.samples.len() < 2 {
            return None;
        }

        let oldest = self.samples.front()?;
        let newest = self.samples.back()?;

        // Calculate distance
        let dlat = newest.latitude - oldest.latitude;
        let dlon = newest.longitude - oldest.longitude;
        let distance = (dlat * dlat + dlon * dlon).sqrt();

        if distance < self.config.min_distance_deg {
            return None; // Too close, track would be noisy
        }

        // Calculate bearing using simple flat-earth approximation
        // For short distances this is accurate enough
        let bearing = calculate_bearing(
            oldest.latitude,
            oldest.longitude,
            newest.latitude,
            newest.longitude,
        );

        Some(bearing as f32)
    }

    /// Get the number of samples in history.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Get the time span of samples in history.
    pub fn time_span(&self) -> Option<Duration> {
        let oldest = self.samples.front()?;
        let newest = self.samples.back()?;
        Some(newest.timestamp.duration_since(oldest.timestamp))
    }

    /// Check if we have enough samples for track calculation.
    pub fn has_sufficient_samples(&self) -> bool {
        self.samples.len() >= 2
    }

    /// Clear all samples.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.last_sample_time = None;
    }

    /// Get an iterator over samples (oldest first).
    pub fn samples(&self) -> impl Iterator<Item = &PositionSample> {
        self.samples.iter()
    }

    /// Get the most recent position.
    pub fn latest_position(&self) -> Option<(f64, f64)> {
        self.samples.back().map(|s| (s.latitude, s.longitude))
    }
}

/// Calculate bearing between two points (flat-earth approximation).
///
/// Returns bearing in degrees (0-360), where 0 = North, 90 = East.
fn calculate_bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;

    // Simple flat-earth bearing calculation
    // atan2(dlon, dlat) gives bearing from north
    let bearing_rad = dlon.atan2(dlat);
    let bearing_deg = bearing_rad.to_degrees();

    // Normalize to 0-360
    if bearing_deg < 0.0 {
        bearing_deg + 360.0
    } else {
        bearing_deg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flight_path_creation() {
        let history = FlightPathHistory::new();
        assert_eq!(history.sample_count(), 0);
        assert!(!history.has_sufficient_samples());
    }

    #[test]
    fn test_record_position() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            sample_interval: Duration::from_millis(1), // Fast for testing
            ..Default::default()
        });

        assert!(history.record_position(53.5, 10.0));
        assert_eq!(history.sample_count(), 1);

        // Second sample immediately should also work with short interval
        std::thread::sleep(Duration::from_millis(2));
        assert!(history.record_position(53.6, 10.1));
        assert_eq!(history.sample_count(), 2);
    }

    #[test]
    fn test_rate_limiting() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            sample_interval: Duration::from_millis(100),
            ..Default::default()
        });

        // First sample accepted
        assert!(history.record_position(53.5, 10.0));

        // Immediate second sample rejected (rate limited)
        assert!(!history.record_position(53.6, 10.1));
        assert_eq!(history.sample_count(), 1);

        // After waiting, sample accepted
        std::thread::sleep(Duration::from_millis(110));
        assert!(history.record_position(53.6, 10.1));
        assert_eq!(history.sample_count(), 2);
    }

    #[test]
    fn test_max_samples_trim() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            max_samples: 5,
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        let base = Instant::now();
        for i in 0..10 {
            history.record_position_at(53.0 + i as f64 * 0.01, 10.0, base + Duration::from_secs(i));
        }

        assert_eq!(history.sample_count(), 5);

        // Oldest should be sample 5 (index 5), newest should be sample 9
        let oldest = history.samples.front().unwrap();
        assert!((oldest.latitude - 53.05).abs() < 0.001);
    }

    #[test]
    fn test_calculate_track_north() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            min_distance_deg: 0.001,
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        let base = Instant::now();
        // Moving north: lat increases, lon same
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(53.1, 10.0, base + Duration::from_secs(10));

        let track = history.calculate_track().unwrap();
        assert!((track - 0.0).abs() < 1.0, "Expected ~0°, got {}°", track);
    }

    #[test]
    fn test_calculate_track_east() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            min_distance_deg: 0.001,
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        let base = Instant::now();
        // Moving east: lon increases, lat same
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(53.0, 10.1, base + Duration::from_secs(10));

        let track = history.calculate_track().unwrap();
        assert!((track - 90.0).abs() < 1.0, "Expected ~90°, got {}°", track);
    }

    #[test]
    fn test_calculate_track_southwest() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            min_distance_deg: 0.001,
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        let base = Instant::now();
        // Moving southwest: lat decreases, lon decreases
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(52.9, 9.9, base + Duration::from_secs(10));

        let track = history.calculate_track().unwrap();
        assert!(
            (track - 225.0).abs() < 5.0,
            "Expected ~225°, got {}°",
            track
        );
    }

    #[test]
    fn test_calculate_track_insufficient_samples() {
        let mut history = FlightPathHistory::new();

        // No samples
        assert!(history.calculate_track().is_none());

        // One sample
        history.record_position(53.0, 10.0);
        assert!(history.calculate_track().is_none());
    }

    #[test]
    fn test_calculate_track_stationary() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            min_distance_deg: 0.001,
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        let base = Instant::now();
        // Stationary: same position
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(53.0, 10.0, base + Duration::from_secs(10));

        // Should return None (no movement)
        assert!(history.calculate_track().is_none());
    }

    #[test]
    fn test_time_span() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        assert!(history.time_span().is_none());

        let base = Instant::now();
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(53.1, 10.0, base + Duration::from_secs(10));
        history.record_position_at(53.2, 10.0, base + Duration::from_secs(20));

        let span = history.time_span().unwrap();
        assert_eq!(span, Duration::from_secs(20));
    }

    #[test]
    fn test_clear() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        let base = Instant::now();
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(53.1, 10.1, base + Duration::from_secs(1));
        assert_eq!(history.sample_count(), 2);

        history.clear();
        assert_eq!(history.sample_count(), 0);
        assert!(history.last_sample_time.is_none());
    }

    #[test]
    fn test_latest_position() {
        let mut history = FlightPathHistory::with_config(FlightPathConfig {
            sample_interval: Duration::from_millis(1),
            ..Default::default()
        });

        assert!(history.latest_position().is_none());

        let base = Instant::now();
        history.record_position_at(53.0, 10.0, base);
        history.record_position_at(53.1, 10.1, base + Duration::from_secs(1));

        let (lat, lon) = history.latest_position().unwrap();
        assert!((lat - 53.1).abs() < 0.001);
        assert!((lon - 10.1).abs() < 0.001);
    }

    #[test]
    fn test_bearing_calculation() {
        // North
        assert!((calculate_bearing(0.0, 0.0, 1.0, 0.0) - 0.0).abs() < 0.1);
        // East
        assert!((calculate_bearing(0.0, 0.0, 0.0, 1.0) - 90.0).abs() < 0.1);
        // South
        assert!((calculate_bearing(0.0, 0.0, -1.0, 0.0) - 180.0).abs() < 0.1);
        // West
        assert!((calculate_bearing(0.0, 0.0, 0.0, -1.0) - 270.0).abs() < 0.1);
    }
}
