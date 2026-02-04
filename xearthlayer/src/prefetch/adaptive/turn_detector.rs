//! Track stability detection for adaptive prefetch.
//!
//! Monitors aircraft ground track to detect turns and determine when track
//! has stabilized. This is critical for band-based prefetching because
//! prefetching during turns wastes resources on tiles that won't be needed.
//!
//! # Track vs Heading
//!
//! This module uses **ground track** (direction of travel over the ground),
//! NOT heading (direction the nose is pointing). The difference matters:
//!
//! - In crosswind, heading may be 10-20° off from track
//! - Track determines which scenery tiles will actually be needed
//! - Heading changes during turns before track settles
//!
//! # State Machine
//!
//! ```text
//! Stable --[track change > turn_threshold]--> Turning
//! Turning --[track deviation < stability_threshold for stability_duration]--> Stable
//! ```
//!
//! # Example
//!
//! ```ignore
//! let detector = TurnDetector::with_defaults();
//!
//! // Update with telemetry
//! detector.update(45.0); // Track 45° (northeast)
//!
//! if detector.is_stable() {
//!     // Safe to prefetch based on current track
//!     let track = detector.stable_track().unwrap();
//! }
//! ```

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::config::AdaptivePrefetchConfig;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration defaults
// ─────────────────────────────────────────────────────────────────────────────

/// Default stability threshold: track must stay within ±5° to be considered stable.
const DEFAULT_STABILITY_THRESHOLD_DEG: f64 = 5.0;

/// Default turn threshold: track change > 15° triggers turn detection.
const DEFAULT_TURN_THRESHOLD_DEG: f64 = 15.0;

/// Default stability duration: track must be stable for 10 seconds.
const DEFAULT_STABILITY_DURATION_SECS: u64 = 10;

/// Maximum track history entries to keep (prevents unbounded memory growth).
const MAX_TRACK_HISTORY: usize = 100;

// ─────────────────────────────────────────────────────────────────────────────
// Turn state
// ─────────────────────────────────────────────────────────────────────────────

/// Current turn detection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    /// Track is stable - safe to prefetch.
    Stable,
    /// Aircraft is turning - prefetch bands are stale.
    Turning,
    /// Initializing - waiting for first stable track.
    Initializing,
}

impl TurnState {
    /// Human-readable description for logging/UI.
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnState::Stable => "Stable",
            TurnState::Turning => "Turning",
            TurnState::Initializing => "Initializing",
        }
    }
}

impl std::fmt::Display for TurnState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Turn detector
// ─────────────────────────────────────────────────────────────────────────────

/// Internal mutable state for the turn detector.
#[derive(Debug)]
struct TurnDetectorInner {
    /// Current state.
    state: TurnState,

    /// Track history (timestamp, track_degrees).
    track_history: VecDeque<(Instant, f64)>,

    /// Last stable track value (degrees).
    last_stable_track: Option<f64>,

    /// Reference track for stability check (degrees).
    /// Updated when track becomes stable or when a turn is detected.
    reference_track: Option<f64>,

    /// When we first detected potential stability.
    stability_start: Option<Instant>,
}

impl TurnDetectorInner {
    fn new() -> Self {
        Self {
            state: TurnState::Initializing,
            track_history: VecDeque::with_capacity(MAX_TRACK_HISTORY),
            last_stable_track: None,
            reference_track: None,
            stability_start: None,
        }
    }
}

/// Detects turns based on ground track changes.
///
/// Thread-safe via interior mutability (Mutex).
///
/// # Usage
///
/// Call `update(track)` with each telemetry update. Then check:
/// - `is_stable()` - whether prefetching is safe
/// - `stable_track()` - the current stable track value
/// - `state()` - detailed state for logging
pub struct TurnDetector {
    /// Stability threshold in degrees (±X°).
    stability_threshold_deg: f64,

    /// Turn threshold in degrees (track change to trigger turn).
    turn_threshold_deg: f64,

    /// Duration track must be stable before prefetch resumes.
    stability_duration: Duration,

    /// Internal mutable state.
    inner: Mutex<TurnDetectorInner>,
}

impl std::fmt::Debug for TurnDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock().unwrap();
        f.debug_struct("TurnDetector")
            .field("stability_threshold_deg", &self.stability_threshold_deg)
            .field("turn_threshold_deg", &self.turn_threshold_deg)
            .field("stability_duration", &self.stability_duration)
            .field("state", &inner.state)
            .field("last_stable_track", &inner.last_stable_track)
            .field("history_len", &inner.track_history.len())
            .finish()
    }
}

impl TurnDetector {
    /// Create a new turn detector with the given configuration.
    pub fn new(config: &AdaptivePrefetchConfig) -> Self {
        Self {
            stability_threshold_deg: config.track_stability_threshold,
            turn_threshold_deg: config.turn_threshold,
            stability_duration: config.track_stability_duration,
            inner: Mutex::new(TurnDetectorInner::new()),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self {
            stability_threshold_deg: DEFAULT_STABILITY_THRESHOLD_DEG,
            turn_threshold_deg: DEFAULT_TURN_THRESHOLD_DEG,
            stability_duration: Duration::from_secs(DEFAULT_STABILITY_DURATION_SECS),
            inner: Mutex::new(TurnDetectorInner::new()),
        }
    }

    /// Create with explicit parameters (useful for testing).
    pub fn with_params(
        stability_threshold_deg: f64,
        turn_threshold_deg: f64,
        stability_duration: Duration,
    ) -> Self {
        Self {
            stability_threshold_deg,
            turn_threshold_deg,
            stability_duration,
            inner: Mutex::new(TurnDetectorInner::new()),
        }
    }

    /// Update the detector with a new track measurement.
    ///
    /// # Arguments
    ///
    /// * `track` - Ground track in degrees (0-360, true north)
    pub fn update(&self, track: f64) {
        let normalized_track = Self::normalize_track(track);
        let now = Instant::now();

        let mut inner = self.inner.lock().unwrap();

        // Add to history
        inner.track_history.push_back((now, normalized_track));

        // Trim old entries
        while inner.track_history.len() > MAX_TRACK_HISTORY {
            inner.track_history.pop_front();
        }

        match inner.state {
            TurnState::Initializing => {
                // Set initial reference and check for stability
                if inner.reference_track.is_none() {
                    inner.reference_track = Some(normalized_track);
                    inner.stability_start = Some(now);
                    tracing::debug!(track = normalized_track, "Turn detector: initial track set");
                }

                if let Some(ref_track) = inner.reference_track {
                    let deviation = Self::track_difference(normalized_track, ref_track);

                    if deviation <= self.stability_threshold_deg {
                        // Track is within threshold - check if stable long enough
                        if let Some(start) = inner.stability_start {
                            if start.elapsed() >= self.stability_duration {
                                inner.state = TurnState::Stable;
                                inner.last_stable_track = Some(normalized_track);
                                tracing::info!(
                                    track = format!("{:.1}°", normalized_track),
                                    "Turn detector: initial track stabilized"
                                );
                            }
                        }
                    } else {
                        // Deviated too much - reset reference
                        inner.reference_track = Some(normalized_track);
                        inner.stability_start = Some(now);
                        tracing::debug!(
                            track = normalized_track,
                            deviation = deviation,
                            "Turn detector: track deviated during init, resetting"
                        );
                    }
                }
            }

            TurnState::Stable => {
                if let Some(last_stable) = inner.last_stable_track {
                    let change = Self::track_difference(normalized_track, last_stable);

                    if change > self.turn_threshold_deg {
                        // Turn detected!
                        inner.state = TurnState::Turning;
                        inner.reference_track = Some(normalized_track);
                        inner.stability_start = None;
                        tracing::info!(
                            from = format!("{:.1}°", last_stable),
                            to = format!("{:.1}°", normalized_track),
                            change = format!("{:.1}°", change),
                            "Turn detector: turn detected"
                        );
                    } else {
                        // Still stable - update last stable track (minor drift is OK)
                        inner.last_stable_track = Some(normalized_track);
                    }
                }
            }

            TurnState::Turning => {
                if let Some(ref_track) = inner.reference_track {
                    let deviation = Self::track_difference(normalized_track, ref_track);

                    if deviation <= self.stability_threshold_deg {
                        // Within threshold - start or continue stability timer
                        if inner.stability_start.is_none() {
                            inner.stability_start = Some(now);
                            tracing::debug!(
                                track = format!("{:.1}°", normalized_track),
                                "Turn detector: track settling, starting stability timer"
                            );
                        }

                        if let Some(start) = inner.stability_start {
                            if start.elapsed() >= self.stability_duration {
                                // Stable!
                                inner.state = TurnState::Stable;
                                inner.last_stable_track = Some(normalized_track);
                                tracing::info!(
                                    track = format!("{:.1}°", normalized_track),
                                    duration_secs = self.stability_duration.as_secs(),
                                    "Turn detector: track stabilized after turn"
                                );
                            }
                        }
                    } else {
                        // Still turning - reset reference to current track
                        inner.reference_track = Some(normalized_track);
                        inner.stability_start = None;
                    }
                }
            }
        }
    }

    /// Check if the track is currently stable.
    ///
    /// Returns `true` if prefetching is safe based on current track.
    pub fn is_stable(&self) -> bool {
        self.inner.lock().unwrap().state == TurnState::Stable
    }

    /// Get the current stable track, if available.
    ///
    /// Returns `None` if track is not stable or still initializing.
    pub fn stable_track(&self) -> Option<f64> {
        let inner = self.inner.lock().unwrap();
        if inner.state == TurnState::Stable {
            inner.last_stable_track
        } else {
            None
        }
    }

    /// Get the current state.
    pub fn state(&self) -> TurnState {
        self.inner.lock().unwrap().state
    }

    /// Get the last known stable track (may not be current).
    ///
    /// Useful for logging even when currently turning.
    pub fn last_stable_track(&self) -> Option<f64> {
        self.inner.lock().unwrap().last_stable_track
    }

    /// Reset the detector to initial state.
    ///
    /// Useful when teleporting aircraft or starting a new flight.
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.state = TurnState::Initializing;
        inner.track_history.clear();
        inner.last_stable_track = None;
        inner.reference_track = None;
        inner.stability_start = None;
        tracing::debug!("Turn detector reset");
    }

    /// Normalize track to 0-360 range.
    fn normalize_track(track: f64) -> f64 {
        ((track % 360.0) + 360.0) % 360.0
    }

    /// Calculate the absolute difference between two tracks.
    ///
    /// Handles wraparound (e.g., 350° to 10° is 20°, not 340°).
    fn track_difference(track1: f64, track2: f64) -> f64 {
        let diff = (track1 - track2).abs();
        if diff > 180.0 {
            360.0 - diff
        } else {
            diff
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ─────────────────────────────────────────────────────────────────────────
    // Helper functions
    // ─────────────────────────────────────────────────────────────────────────

    fn fast_detector() -> TurnDetector {
        // Fast timings for testing
        TurnDetector::with_params(5.0, 15.0, Duration::from_millis(50))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Normalization and difference tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_normalize_track() {
        assert!((TurnDetector::normalize_track(0.0) - 0.0).abs() < 0.001);
        assert!((TurnDetector::normalize_track(360.0) - 0.0).abs() < 0.001);
        assert!((TurnDetector::normalize_track(-90.0) - 270.0).abs() < 0.001);
        assert!((TurnDetector::normalize_track(450.0) - 90.0).abs() < 0.001);
        assert!((TurnDetector::normalize_track(-450.0) - 270.0).abs() < 0.001);
    }

    #[test]
    fn test_track_difference() {
        // Simple cases
        assert!((TurnDetector::track_difference(90.0, 80.0) - 10.0).abs() < 0.001);
        assert!((TurnDetector::track_difference(80.0, 90.0) - 10.0).abs() < 0.001);

        // Wraparound cases
        assert!((TurnDetector::track_difference(350.0, 10.0) - 20.0).abs() < 0.001);
        assert!((TurnDetector::track_difference(10.0, 350.0) - 20.0).abs() < 0.001);

        // Large differences
        assert!((TurnDetector::track_difference(0.0, 180.0) - 180.0).abs() < 0.001);
        assert!((TurnDetector::track_difference(90.0, 270.0) - 180.0).abs() < 0.001);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // State machine tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_initial_state() {
        let detector = fast_detector();
        assert_eq!(detector.state(), TurnState::Initializing);
        assert!(!detector.is_stable());
        assert!(detector.stable_track().is_none());
    }

    #[test]
    fn test_becomes_stable_after_duration() {
        let detector = fast_detector();

        // Feed consistent track
        detector.update(45.0);
        assert_eq!(detector.state(), TurnState::Initializing);

        // Wait for stability duration
        thread::sleep(Duration::from_millis(60));
        detector.update(46.0); // Within ±5° threshold

        assert_eq!(detector.state(), TurnState::Stable);
        assert!(detector.is_stable());
        assert!(detector.stable_track().is_some());
    }

    #[test]
    fn test_stability_resets_on_deviation() {
        let detector = fast_detector();

        detector.update(45.0);
        thread::sleep(Duration::from_millis(30));

        // Deviate beyond threshold during init
        detector.update(60.0); // 15° change > 5° threshold
        assert_eq!(detector.state(), TurnState::Initializing);

        // Need to wait full duration again
        thread::sleep(Duration::from_millis(60));
        detector.update(61.0);

        assert_eq!(detector.state(), TurnState::Stable);
    }

    #[test]
    fn test_turn_detection() {
        let detector = fast_detector();

        // Get stable first
        detector.update(45.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(46.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Now turn
        detector.update(70.0); // 24° change > 15° threshold
        assert_eq!(detector.state(), TurnState::Turning);
        assert!(!detector.is_stable());
    }

    #[test]
    fn test_turn_recovery() {
        let detector = fast_detector();

        // Get stable at 45°
        detector.update(45.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(46.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Turn to 90°
        detector.update(90.0);
        assert_eq!(detector.state(), TurnState::Turning);

        // Start settling at 90° (this sets reference_track and stability_start)
        detector.update(91.0);
        assert_eq!(detector.state(), TurnState::Turning); // Not yet stable

        // Wait for stability duration
        thread::sleep(Duration::from_millis(60));
        detector.update(91.5); // Final update to confirm stability
        assert_eq!(detector.state(), TurnState::Stable);

        // New stable track should be ~91°
        let stable = detector.stable_track().unwrap();
        assert!((stable - 91.5).abs() < 1.0);
    }

    #[test]
    fn test_minor_drift_doesnt_trigger_turn() {
        let detector = fast_detector();

        // Get stable
        detector.update(45.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(46.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Minor drift within threshold
        detector.update(48.0); // 3° change < 15° threshold
        assert_eq!(detector.state(), TurnState::Stable);

        detector.update(50.0); // 5° total change < 15° threshold
        assert_eq!(detector.state(), TurnState::Stable);
    }

    #[test]
    fn test_s_turn_detection() {
        let detector = fast_detector();

        // Get stable
        detector.update(0.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(1.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // First turn right
        detector.update(30.0);
        assert_eq!(detector.state(), TurnState::Turning);

        // Before stabilizing, turn left
        thread::sleep(Duration::from_millis(20));
        detector.update(330.0); // Left turn through north
        assert_eq!(detector.state(), TurnState::Turning);

        // Start settling at 330° (sets reference and starts stability timer)
        detector.update(331.0);
        assert_eq!(detector.state(), TurnState::Turning); // Not yet stable

        // Wait for stability duration
        thread::sleep(Duration::from_millis(60));
        detector.update(331.5);
        assert_eq!(detector.state(), TurnState::Stable);
    }

    #[test]
    fn test_180_degree_turn() {
        let detector = fast_detector();

        // Get stable heading north
        detector.update(0.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(1.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Turn to south
        detector.update(180.0);
        assert_eq!(detector.state(), TurnState::Turning);

        // Start settling at 180° (sets reference and starts stability timer)
        detector.update(181.0);
        assert_eq!(detector.state(), TurnState::Turning); // Not yet stable

        // Wait for stability duration
        thread::sleep(Duration::from_millis(60));
        detector.update(181.5);
        assert_eq!(detector.state(), TurnState::Stable);

        let stable = detector.stable_track().unwrap();
        assert!((stable - 181.5).abs() < 1.0);
    }

    #[test]
    fn test_reset() {
        let detector = fast_detector();

        // Get stable
        detector.update(45.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(46.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Reset
        detector.reset();
        assert_eq!(detector.state(), TurnState::Initializing);
        assert!(detector.stable_track().is_none());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Display tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_turn_state_display() {
        assert_eq!(TurnState::Stable.as_str(), "Stable");
        assert_eq!(TurnState::Turning.as_str(), "Turning");
        assert_eq!(TurnState::Initializing.as_str(), "Initializing");

        assert_eq!(format!("{}", TurnState::Stable), "Stable");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Thread safety tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;

        let detector = Arc::new(fast_detector());
        let mut handles = vec![];

        // Multiple threads updating
        for i in 0..4 {
            let d = Arc::clone(&detector);
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    d.update((i * 10 + j % 10) as f64);
                    let _ = d.is_stable();
                    let _ = d.state();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should not panic
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge case tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_wraparound_north() {
        let detector = fast_detector();

        // Get stable at 350°
        detector.update(350.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(351.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Small turn across north (350° to 10°)
        // This is only a 20° change - should trigger turn
        detector.update(10.0);
        assert_eq!(detector.state(), TurnState::Turning);
    }

    #[test]
    fn test_small_movement_across_north() {
        let detector = fast_detector();

        // Get stable at 358°
        detector.update(358.0);
        thread::sleep(Duration::from_millis(60));
        detector.update(359.0);
        assert_eq!(detector.state(), TurnState::Stable);

        // Tiny movement from 358° to 2° (4° change)
        // Should NOT trigger turn (< 15° threshold)
        detector.update(2.0);
        assert_eq!(detector.state(), TurnState::Stable);
    }
}
