//! Prefetch throttling abstraction.
//!
//! This module provides a trait for controlling when prefetching should pause,
//! following the Dependency Inversion Principle. Prefetchers depend on this
//! abstract interface rather than concrete circuit breaker implementations.
//!
//! # Design
//!
//! The `PrefetchThrottler` trait follows the pattern established by
//! `PrefetchCondition` - a simple predicate interface that prefetchers
//! can query without understanding the underlying implementation.
//!
//! This allows different throttling strategies:
//! - Circuit breaker (current implementation)
//! - Rate limiting
//! - Resource-based throttling
//! - Testing mocks (always/never throttle)

use std::fmt;

/// User-friendly throttle states.
///
/// These states describe what the user sees, not circuit breaker internals.
/// This follows the principle of abstraction - hide implementation details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleState {
    /// Prefetch is active - tiles are being pre-cached normally.
    Active,

    /// Prefetch is paused due to high load (X-Plane is loading scenery).
    Paused,

    /// Testing if safe to resume prefetching.
    Resuming,
}

impl ThrottleState {
    /// Get a short description for display.
    pub fn as_str(&self) -> &'static str {
        match self {
            ThrottleState::Active => "Active",
            ThrottleState::Paused => "Paused",
            ThrottleState::Resuming => "Resuming",
        }
    }

    /// Check if prefetching is currently allowed.
    pub fn is_active(&self) -> bool {
        matches!(self, ThrottleState::Active | ThrottleState::Resuming)
    }
}

impl fmt::Display for ThrottleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Determines whether prefetching should be throttled.
///
/// Abstracts the circuit breaker from prefetchers - they only know
/// "should I pause?" not "why". This follows the `PrefetchCondition`
/// pattern for simple, mockable predicates.
///
/// # Example
///
/// ```
/// use xearthlayer::prefetch::{PrefetchThrottler, ThrottleState};
/// use std::sync::Arc;
///
/// fn prefetch_loop(throttler: Arc<dyn PrefetchThrottler>) {
///     if throttler.should_throttle() {
///         // Skip this prefetch cycle - system is under load
///         return;
///     }
///     // Proceed with prefetching...
/// }
/// ```
///
/// # Implementors
///
/// - `CircuitBreaker` - Throttles based on FUSE request rate
/// - `NeverThrottle` - Testing: always allows prefetch
/// - `AlwaysThrottle` - Testing: never allows prefetch
pub trait PrefetchThrottler: Send + Sync {
    /// Check if prefetching should be paused.
    ///
    /// Called each prefetch cycle. Returns `true` if prefetch should skip
    /// this cycle due to high system load.
    fn should_throttle(&self) -> bool;

    /// Get current throttle state for display/debugging.
    fn state(&self) -> ThrottleState;
}

/// Testing throttler that never throttles.
///
/// Useful for unit tests where you want prefetch to always proceed.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeverThrottle;

impl PrefetchThrottler for NeverThrottle {
    fn should_throttle(&self) -> bool {
        false
    }

    fn state(&self) -> ThrottleState {
        ThrottleState::Active
    }
}

/// Testing throttler that always throttles.
///
/// Useful for testing throttle handling in prefetchers.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysThrottle;

impl PrefetchThrottler for AlwaysThrottle {
    fn should_throttle(&self) -> bool {
        true
    }

    fn state(&self) -> ThrottleState {
        ThrottleState::Paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_throttle_state_as_str() {
        assert_eq!(ThrottleState::Active.as_str(), "Active");
        assert_eq!(ThrottleState::Paused.as_str(), "Paused");
        assert_eq!(ThrottleState::Resuming.as_str(), "Resuming");
    }

    #[test]
    fn test_throttle_state_is_active() {
        assert!(ThrottleState::Active.is_active());
        assert!(!ThrottleState::Paused.is_active());
        assert!(ThrottleState::Resuming.is_active());
    }

    #[test]
    fn test_throttle_state_display() {
        assert_eq!(format!("{}", ThrottleState::Active), "Active");
        assert_eq!(format!("{}", ThrottleState::Paused), "Paused");
        assert_eq!(format!("{}", ThrottleState::Resuming), "Resuming");
    }

    #[test]
    fn test_never_throttle() {
        let throttler = NeverThrottle;

        assert!(!throttler.should_throttle());
        assert_eq!(throttler.state(), ThrottleState::Active);
    }

    #[test]
    fn test_always_throttle() {
        let throttler = AlwaysThrottle;

        assert!(throttler.should_throttle());
        assert_eq!(throttler.state(), ThrottleState::Paused);
    }

    #[test]
    fn test_trait_object_usage() {
        let throttler: Arc<dyn PrefetchThrottler> = Arc::new(NeverThrottle);
        assert!(!throttler.should_throttle());

        let throttler: Arc<dyn PrefetchThrottler> = Arc::new(AlwaysThrottle);
        assert!(throttler.should_throttle());
    }
}
