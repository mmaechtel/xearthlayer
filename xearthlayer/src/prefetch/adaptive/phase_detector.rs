//! Flight phase detection for adaptive prefetch.
//!
//! Detects whether the aircraft is on the ground or in cruise flight
//! to select the appropriate prefetch strategy.
//!
//! # Detection Logic
//!
//! ```text
//! Ground: GS < 40kt AND AGL < 20ft
//! Cruise: GS > 40kt OR AGL > 20ft
//! ```
//!
//! The OR condition for cruise supports rotorcraft and slow experimental
//! aircraft that may fly slowly but are clearly airborne.

use std::time::Instant;

use super::config::AdaptivePrefetchConfig;

/// Flight phase for strategy selection.
///
/// The prefetch system uses different strategies based on flight phase:
/// - Ground: Ring-based prefetch around current position
/// - Cruise: Band-based prefetch ahead of track
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlightPhase {
    /// Aircraft is on the ground (taxi, parking, runway).
    ///
    /// Condition: Ground speed < 40kt AND AGL < 20ft
    #[default]
    Ground,

    /// Aircraft is in flight (takeoff, cruise, approach).
    ///
    /// Condition: Ground speed > 40kt OR AGL > 20ft
    /// The OR handles slow rotorcraft that are clearly airborne.
    Cruise,
}

impl FlightPhase {
    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            FlightPhase::Ground => "ground operations",
            FlightPhase::Cruise => "cruise flight",
        }
    }
}

impl std::fmt::Display for FlightPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlightPhase::Ground => write!(f, "ground"),
            FlightPhase::Cruise => write!(f, "cruise"),
        }
    }
}

/// Detects flight phase transitions.
///
/// Monitors ground speed and AGL to determine whether the aircraft
/// is on the ground or in cruise flight.
///
/// # Hysteresis
///
/// To prevent rapid phase switching during transition (e.g., takeoff roll),
/// the detector requires the new phase conditions to persist for a short
/// duration before confirming the transition.
#[derive(Debug)]
pub struct PhaseDetector {
    /// Current detected phase.
    current_phase: FlightPhase,

    /// Ground speed threshold (knots).
    ground_speed_threshold_kt: f32,

    /// AGL threshold (feet).
    agl_threshold_ft: f32,

    /// When the current phase was entered.
    phase_entered_at: Instant,

    /// Pending phase transition (if conditions met but not yet confirmed).
    pending_transition: Option<(FlightPhase, Instant)>,

    /// Hysteresis duration (how long conditions must persist).
    pub(crate) hysteresis_duration: std::time::Duration,
}

impl PhaseDetector {
    /// Create a new phase detector with the given configuration.
    pub fn new(config: &AdaptivePrefetchConfig) -> Self {
        Self {
            current_phase: FlightPhase::Ground,
            ground_speed_threshold_kt: config.ground_speed_threshold_kt,
            agl_threshold_ft: config.agl_threshold_ft,
            phase_entered_at: Instant::now(),
            pending_transition: None,
            hysteresis_duration: std::time::Duration::from_secs(2),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(&AdaptivePrefetchConfig::default())
    }

    /// Get the current flight phase.
    pub fn current_phase(&self) -> FlightPhase {
        self.current_phase
    }

    /// Update phase detection with new telemetry data.
    ///
    /// # Arguments
    ///
    /// * `ground_speed_kt` - Current ground speed in knots
    /// * `agl_ft` - Current altitude above ground level in feet
    ///
    /// # Returns
    ///
    /// `true` if the phase changed, `false` otherwise.
    pub fn update(&mut self, ground_speed_kt: f32, agl_ft: f32) -> bool {
        let detected_phase = self.detect_phase(ground_speed_kt, agl_ft);

        if detected_phase == self.current_phase {
            // Same phase, clear any pending transition
            self.pending_transition = None;
            return false;
        }

        let now = Instant::now();

        // Check if we have a pending transition to this phase
        if let Some((pending_phase, started_at)) = self.pending_transition {
            if pending_phase == detected_phase {
                // Same pending phase, check if hysteresis duration passed
                if now.duration_since(started_at) >= self.hysteresis_duration {
                    // Confirm transition
                    let old_phase = self.current_phase;
                    self.current_phase = detected_phase;
                    self.phase_entered_at = now;
                    self.pending_transition = None;

                    tracing::info!(
                        from = %old_phase,
                        to = %detected_phase,
                        ground_speed_kt = ground_speed_kt,
                        agl_ft = agl_ft,
                        "Flight phase transition"
                    );

                    return true;
                }
                // Still waiting for hysteresis
                return false;
            }
        }

        // Start a new pending transition
        self.pending_transition = Some((detected_phase, now));
        false
    }

    /// Detect phase based on current telemetry (without hysteresis).
    fn detect_phase(&self, ground_speed_kt: f32, agl_ft: f32) -> FlightPhase {
        // Ground: GS < threshold AND AGL < threshold
        // Cruise: GS > threshold OR AGL > threshold
        let is_slow = ground_speed_kt < self.ground_speed_threshold_kt;
        let is_low = agl_ft < self.agl_threshold_ft;

        if is_slow && is_low {
            FlightPhase::Ground
        } else {
            FlightPhase::Cruise
        }
    }

    /// How long the current phase has been active.
    pub fn phase_duration(&self) -> std::time::Duration {
        self.phase_entered_at.elapsed()
    }

    /// Force a specific phase (for testing).
    #[cfg(test)]
    pub fn set_phase(&mut self, phase: FlightPhase) {
        self.current_phase = phase;
        self.phase_entered_at = Instant::now();
        self.pending_transition = None;
    }
}

impl Default for PhaseDetector {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flight_phase_display() {
        assert_eq!(format!("{}", FlightPhase::Ground), "ground");
        assert_eq!(format!("{}", FlightPhase::Cruise), "cruise");
    }

    #[test]
    fn test_flight_phase_description() {
        assert_eq!(FlightPhase::Ground.description(), "ground operations");
        assert_eq!(FlightPhase::Cruise.description(), "cruise flight");
    }

    #[test]
    fn test_phase_detector_initial_state() {
        let detector = PhaseDetector::with_defaults();
        assert_eq!(detector.current_phase(), FlightPhase::Ground);
    }

    #[test]
    fn test_phase_detector_ground_conditions() {
        let detector = PhaseDetector::with_defaults();

        // Slow and low = ground
        assert_eq!(detector.detect_phase(20.0, 10.0), FlightPhase::Ground);
        assert_eq!(detector.detect_phase(0.0, 0.0), FlightPhase::Ground);
        assert_eq!(detector.detect_phase(39.0, 19.0), FlightPhase::Ground);
    }

    #[test]
    fn test_phase_detector_cruise_conditions() {
        let detector = PhaseDetector::with_defaults();

        // Fast OR high = cruise
        assert_eq!(detector.detect_phase(100.0, 10.0), FlightPhase::Cruise); // Fast, low
        assert_eq!(detector.detect_phase(20.0, 100.0), FlightPhase::Cruise); // Slow, high
        assert_eq!(detector.detect_phase(100.0, 1000.0), FlightPhase::Cruise); // Fast, high
        assert_eq!(detector.detect_phase(41.0, 10.0), FlightPhase::Cruise); // Just above GS threshold
        assert_eq!(detector.detect_phase(10.0, 21.0), FlightPhase::Cruise); // Just above AGL threshold
    }

    #[test]
    fn test_phase_detector_rotorcraft_hover() {
        let detector = PhaseDetector::with_defaults();

        // Helicopter hovering: slow but clearly airborne
        assert_eq!(detector.detect_phase(5.0, 50.0), FlightPhase::Cruise);
        assert_eq!(detector.detect_phase(0.0, 100.0), FlightPhase::Cruise);
    }

    #[test]
    fn test_phase_detector_hysteresis() {
        let mut detector = PhaseDetector::with_defaults();
        detector.hysteresis_duration = std::time::Duration::from_millis(10);

        // Start on ground
        assert_eq!(detector.current_phase(), FlightPhase::Ground);

        // First update with cruise conditions - should NOT transition yet
        let changed = detector.update(100.0, 1000.0);
        assert!(!changed);
        assert_eq!(detector.current_phase(), FlightPhase::Ground);

        // Wait for hysteresis
        std::thread::sleep(std::time::Duration::from_millis(15));

        // Second update - should transition now
        let changed = detector.update(100.0, 1000.0);
        assert!(changed);
        assert_eq!(detector.current_phase(), FlightPhase::Cruise);
    }

    #[test]
    fn test_phase_detector_cancelled_transition() {
        let mut detector = PhaseDetector::with_defaults();
        detector.hysteresis_duration = std::time::Duration::from_millis(50);

        // Start on ground
        assert_eq!(detector.current_phase(), FlightPhase::Ground);

        // Start transition to cruise
        detector.update(100.0, 1000.0);
        assert_eq!(detector.current_phase(), FlightPhase::Ground);

        // Before hysteresis completes, go back to ground conditions
        detector.update(10.0, 5.0);
        assert_eq!(detector.current_phase(), FlightPhase::Ground);

        // Wait and update with ground conditions - should stay ground
        std::thread::sleep(std::time::Duration::from_millis(60));
        let changed = detector.update(10.0, 5.0);
        assert!(!changed);
        assert_eq!(detector.current_phase(), FlightPhase::Ground);
    }
}
