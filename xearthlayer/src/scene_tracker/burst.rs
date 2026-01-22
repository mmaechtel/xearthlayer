//! Burst detection for X-Plane scenery loading patterns.
//!
//! X-Plane loads scenery tiles in bursts:
//! - **Session init**: Large burst when starting a flight
//! - **Boundary crossing**: Burst when crossing DSF tile boundaries
//! - **Texture refresh**: Smaller bursts during normal flight
//!
//! The burst detector identifies these patterns by tracking quiet periods
//! between tile requests. A burst ends when no requests are received for
//! a configurable threshold duration.

use std::time::{Duration, Instant};

use super::model::{DdsTileCoord, LoadingBurst};

/// Configuration for burst detection.
#[derive(Debug, Clone)]
pub struct BurstConfig {
    /// Duration of quiet time that signals end of burst.
    ///
    /// When no tile requests are received for this duration, the current
    /// burst is considered complete.
    pub quiet_threshold: Duration,

    /// Minimum number of tiles for a significant burst.
    ///
    /// Bursts with fewer tiles than this are still tracked but may be
    /// filtered by consumers.
    pub min_tiles: usize,
}

impl Default for BurstConfig {
    fn default() -> Self {
        Self {
            quiet_threshold: Duration::from_millis(500),
            min_tiles: 3,
        }
    }
}

impl BurstConfig {
    /// Create a new burst configuration.
    pub fn new(quiet_threshold: Duration, min_tiles: usize) -> Self {
        Self {
            quiet_threshold,
            min_tiles,
        }
    }
}

/// State machine for detecting loading bursts.
///
/// Tracks tile requests and detects when bursts start and end based on
/// timing patterns.
#[derive(Debug)]
pub struct BurstDetector {
    config: BurstConfig,

    /// Tiles accumulated in the current burst.
    current_tiles: Vec<DdsTileCoord>,

    /// When the current burst started (if active).
    burst_started: Option<Instant>,

    /// Timestamp of the last tile request.
    last_activity: Instant,

    /// Whether a burst is currently active.
    burst_active: bool,
}

impl BurstDetector {
    /// Create a new burst detector with the given configuration.
    pub fn new(config: BurstConfig) -> Self {
        Self {
            config,
            current_tiles: Vec::new(),
            burst_started: None,
            last_activity: Instant::now(),
            burst_active: false,
        }
    }

    /// Create a burst detector with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(BurstConfig::default())
    }

    /// Record a tile access event.
    ///
    /// # Returns
    ///
    /// `Some(LoadingBurst)` if this event completes a previous burst,
    /// `None` if we're still accumulating tiles.
    pub fn record_tile(&mut self, tile: DdsTileCoord, timestamp: Instant) -> Option<LoadingBurst> {
        let completed_burst = self.check_burst_complete(timestamp);

        // Start a new burst if needed
        if !self.burst_active {
            self.burst_active = true;
            self.burst_started = Some(timestamp);
            self.current_tiles.clear();
        }

        // Add tile to current burst
        self.current_tiles.push(tile);
        self.last_activity = timestamp;

        completed_burst
    }

    /// Check if sufficient quiet time has passed to complete the current burst.
    ///
    /// Call this periodically to detect burst completion even without new tiles.
    ///
    /// # Returns
    ///
    /// `Some(LoadingBurst)` if the current burst is complete, `None` otherwise.
    pub fn check_burst_complete(&mut self, now: Instant) -> Option<LoadingBurst> {
        if !self.burst_active {
            return None;
        }

        let elapsed = now.saturating_duration_since(self.last_activity);
        if elapsed >= self.config.quiet_threshold && !self.current_tiles.is_empty() {
            // Burst is complete
            let burst = LoadingBurst {
                tiles: std::mem::take(&mut self.current_tiles),
                started: self.burst_started.unwrap_or(self.last_activity),
                ended: self.last_activity,
            };

            self.burst_active = false;
            self.burst_started = None;

            Some(burst)
        } else {
            None
        }
    }

    /// Check if a burst is currently in progress.
    pub fn is_burst_active(&self) -> bool {
        self.burst_active
    }

    /// Get the tiles accumulated in the current burst.
    pub fn current_burst_tiles(&self) -> &[DdsTileCoord] {
        &self.current_tiles
    }

    /// Get when the current burst started, if active.
    pub fn burst_started(&self) -> Option<Instant> {
        self.burst_started
    }

    /// Get the timestamp of the last activity.
    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    /// Check if the current/most recent burst is significant.
    ///
    /// A burst is significant if it contains at least `min_tiles` tiles.
    pub fn is_significant_burst(&self) -> bool {
        self.current_tiles.len() >= self.config.min_tiles
    }

    /// Get the current configuration.
    pub fn config(&self) -> &BurstConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tile(row: u32) -> DdsTileCoord {
        DdsTileCoord::new(row, 100, 18)
    }

    #[test]
    fn test_burst_detector_basic() {
        let config = BurstConfig {
            quiet_threshold: Duration::from_millis(100),
            min_tiles: 2,
        };
        let mut detector = BurstDetector::new(config);

        let start = Instant::now();

        // Add first tile - starts burst
        let result = detector.record_tile(make_tile(1), start);
        assert!(result.is_none());
        assert!(detector.is_burst_active());

        // Add second tile
        let result = detector.record_tile(make_tile(2), start + Duration::from_millis(10));
        assert!(result.is_none());
        assert!(detector.is_burst_active());

        // Check burst complete after quiet period
        let result = detector.check_burst_complete(start + Duration::from_millis(150));
        assert!(result.is_some());
        let burst = result.unwrap();
        assert_eq!(burst.tile_count(), 2);
        assert!(!detector.is_burst_active());
    }

    #[test]
    fn test_burst_continues_with_activity() {
        let config = BurstConfig {
            quiet_threshold: Duration::from_millis(100),
            min_tiles: 1,
        };
        let mut detector = BurstDetector::new(config);

        let start = Instant::now();

        // Add tiles with short gaps (less than quiet threshold)
        detector.record_tile(make_tile(1), start);
        detector.record_tile(make_tile(2), start + Duration::from_millis(50));
        detector.record_tile(make_tile(3), start + Duration::from_millis(100));

        // Check before quiet threshold
        let result = detector.check_burst_complete(start + Duration::from_millis(150));
        assert!(result.is_none());
        assert!(detector.is_burst_active());
        assert_eq!(detector.current_burst_tiles().len(), 3);
    }

    #[test]
    fn test_new_burst_after_complete() {
        let config = BurstConfig {
            quiet_threshold: Duration::from_millis(50),
            min_tiles: 1,
        };
        let mut detector = BurstDetector::new(config);

        let start = Instant::now();

        // First burst
        detector.record_tile(make_tile(1), start);
        let result = detector.check_burst_complete(start + Duration::from_millis(100));
        assert!(result.is_some());
        assert_eq!(result.unwrap().tile_count(), 1);

        // Second burst
        detector.record_tile(make_tile(2), start + Duration::from_millis(200));
        assert!(detector.is_burst_active());
        assert_eq!(detector.current_burst_tiles().len(), 1);
    }

    #[test]
    fn test_record_tile_completes_previous_burst() {
        let config = BurstConfig {
            quiet_threshold: Duration::from_millis(50),
            min_tiles: 1,
        };
        let mut detector = BurstDetector::new(config);

        let start = Instant::now();

        // First tile
        detector.record_tile(make_tile(1), start);

        // Second tile after quiet threshold - should complete first burst
        let result = detector.record_tile(make_tile(2), start + Duration::from_millis(100));
        assert!(result.is_some());
        let burst = result.unwrap();
        assert_eq!(burst.tile_count(), 1);
        assert_eq!(burst.tiles[0].row, 1);

        // New burst should be active with second tile
        assert!(detector.is_burst_active());
        assert_eq!(detector.current_burst_tiles().len(), 1);
        assert_eq!(detector.current_burst_tiles()[0].row, 2);
    }

    #[test]
    fn test_is_significant_burst() {
        let config = BurstConfig {
            quiet_threshold: Duration::from_millis(100),
            min_tiles: 3,
        };
        let mut detector = BurstDetector::new(config);

        let start = Instant::now();

        // Two tiles - not significant
        detector.record_tile(make_tile(1), start);
        detector.record_tile(make_tile(2), start + Duration::from_millis(10));
        assert!(!detector.is_significant_burst());

        // Three tiles - significant
        detector.record_tile(make_tile(3), start + Duration::from_millis(20));
        assert!(detector.is_significant_burst());
    }

    #[test]
    fn test_burst_timing() {
        let config = BurstConfig::default();
        let mut detector = BurstDetector::new(config);

        let start = Instant::now();
        let mid = start + Duration::from_millis(50);

        detector.record_tile(make_tile(1), start);
        detector.record_tile(make_tile(2), mid);

        assert_eq!(detector.burst_started(), Some(start));
        assert_eq!(detector.last_activity(), mid);
    }

    #[test]
    fn test_default_config() {
        let config = BurstConfig::default();
        assert_eq!(config.quiet_threshold, Duration::from_millis(500));
        assert_eq!(config.min_tiles, 3);
    }
}
