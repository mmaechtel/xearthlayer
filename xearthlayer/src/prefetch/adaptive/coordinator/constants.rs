//! Constants for the adaptive prefetch coordinator.
//!
//! These timing constants control the coordinator's behavior:
//! - Cycle intervals for prefetch execution
//! - Staleness detection for telemetry timeout

use std::time::Duration;

/// Minimum interval between prefetch cycles.
///
/// Prevents excessive prefetch activity when telemetry updates are frequent.
/// Two seconds provides a good balance between responsiveness and efficiency.
pub const MIN_CYCLE_INTERVAL: Duration = Duration::from_secs(2);

/// How long before telemetry is considered stale.
///
/// When X-Plane exits, it stops sending UDP telemetry packets. After this
/// duration without new telemetry, the GPS status is set to "Acquiring"
/// to indicate we've lost the connection.
///
/// # Design Rationale
///
/// - Must be > 3s to tolerate brief network hiccups
/// - Must be < 10s to promptly detect X-Plane exit
pub const TELEMETRY_STALE_THRESHOLD: Duration = Duration::from_secs(5);

/// How often to check for stale telemetry.
///
/// This interval determines how quickly we detect that X-Plane has stopped
/// sending telemetry. The check is lightweight (just comparing timestamps).
pub const STALENESS_CHECK_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_interval_is_reasonable() {
        // Too short = excessive CPU usage
        // Too long = sluggish prefetch response
        assert!(MIN_CYCLE_INTERVAL >= Duration::from_secs(1));
        assert!(MIN_CYCLE_INTERVAL <= Duration::from_secs(5));
    }

    #[test]
    fn test_staleness_threshold_is_reasonable() {
        // Regression test for Bug 3: Telemetry persisting after X-Plane exit
        // The staleness threshold should be long enough to tolerate brief network
        // hiccups but short enough to detect X-Plane exit promptly.
        assert!(
            TELEMETRY_STALE_THRESHOLD >= Duration::from_secs(3),
            "Threshold too short - may false-positive on network hiccups"
        );
        assert!(
            TELEMETRY_STALE_THRESHOLD <= Duration::from_secs(10),
            "Threshold too long - user will see stale position for too long"
        );
    }

    #[test]
    fn test_staleness_check_interval_is_reasonable() {
        // The check interval should be frequent enough to detect staleness
        // soon after it occurs, but not so frequent as to waste CPU.
        assert!(
            STALENESS_CHECK_INTERVAL <= Duration::from_secs(2),
            "Check interval too long - staleness detection will be delayed"
        );
        assert!(
            STALENESS_CHECK_INTERVAL >= Duration::from_millis(500),
            "Check interval too short - unnecessary CPU usage"
        );
    }
}
