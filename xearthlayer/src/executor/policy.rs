//! Policy types for job execution control.
//!
//! This module defines the policy types that control how jobs and tasks handle
//! errors, retries, and scheduling priority.
//!
//! # Policy Types
//!
//! - [`ErrorPolicy`]: How a job handles task/child failures
//! - [`RetryPolicy`]: How a task handles transient failures
//! - [`Priority`]: Task scheduling priority (higher = more important)
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::executor::{ErrorPolicy, RetryPolicy, Priority};
//!
//! // Job that accepts 80% success rate (prefetch use case)
//! let error_policy = ErrorPolicy::PartialSuccess { threshold: 0.8 };
//!
//! // Task with exponential backoff retries
//! let retry_policy = RetryPolicy::exponential(3);
//!
//! // On-demand task that should preempt prefetch
//! let priority = Priority::ON_DEMAND;
//! ```

use std::time::Duration;

// =============================================================================
// Retry Policy Constants
// =============================================================================

/// Default initial delay for exponential backoff (100ms).
pub const DEFAULT_INITIAL_DELAY_MS: u64 = 100;

/// Default maximum delay for exponential backoff (30 seconds).
pub const DEFAULT_MAX_DELAY_SECS: u64 = 30;

/// Default multiplier for exponential backoff.
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;

// =============================================================================
// Priority Constants
// =============================================================================

/// Priority value for on-demand requests from X-Plane.
pub const PRIORITY_ON_DEMAND: i32 = 100;

/// Priority value for background prefetch work.
pub const PRIORITY_PREFETCH: i32 = 0;

/// Priority value for housekeeping tasks.
pub const PRIORITY_HOUSEKEEPING: i32 = -50;

/// How a job handles task/child failures.
///
/// This policy determines whether a job continues executing after a task fails,
/// and how it determines its final success/failure status.
#[derive(Clone, Debug, PartialEq)]
pub enum ErrorPolicy {
    /// Stop job immediately on first failure.
    ///
    /// The job will cancel any pending tasks and report failure as soon as
    /// any task fails. This is the default policy and is suitable for jobs
    /// where any failure invalidates the entire operation.
    FailFast,

    /// Continue executing remaining tasks despite failures.
    ///
    /// All tasks will be attempted regardless of individual failures.
    /// The job will fail only if all tasks have been attempted and at least
    /// one failed.
    ContinueOnError,

    /// Job succeeds if at least `threshold` fraction of work succeeds.
    ///
    /// Useful for prefetching where partial success is acceptable. For example,
    /// a tile prefetch job might succeed if 80% of DDS files are generated.
    ///
    /// # Fields
    ///
    /// * `threshold` - Minimum success ratio (0.0 - 1.0). A value of 0.8 means
    ///   80% of tasks must succeed for the job to be considered successful.
    PartialSuccess {
        /// Minimum success ratio (0.0 - 1.0).
        threshold: f64,
    },

    /// Custom completion logic (defer to `Job::on_complete`).
    ///
    /// The job's `on_complete` method will be called to determine the final
    /// status based on the full execution results.
    Custom,
}

impl Default for ErrorPolicy {
    fn default() -> Self {
        Self::FailFast
    }
}

impl ErrorPolicy {
    /// Creates a partial success policy with the given threshold.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Minimum success ratio (0.0 - 1.0)
    ///
    /// # Panics
    ///
    /// Panics if threshold is not in the range 0.0..=1.0
    pub fn partial_success(threshold: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&threshold),
            "threshold must be between 0.0 and 1.0"
        );
        Self::PartialSuccess { threshold }
    }
}

/// How a task handles transient failures.
///
/// This policy controls automatic retry behavior for tasks that fail due to
/// transient issues (network timeouts, temporary service unavailability, etc.).
#[derive(Clone, Debug, PartialEq)]
pub enum RetryPolicy {
    /// No retries - fail immediately on error.
    None,

    /// Fixed number of retries with constant delay between attempts.
    ///
    /// Use this for simple retry scenarios where the delay doesn't need
    /// to increase between attempts.
    Fixed {
        /// Maximum number of attempts (including the initial attempt).
        max_attempts: u32,
        /// Delay between retry attempts.
        delay: Duration,
    },

    /// Exponential backoff with configurable parameters.
    ///
    /// The delay doubles after each failed attempt, up to a maximum delay.
    /// This is the recommended policy for network operations to avoid
    /// overwhelming services that may be temporarily overloaded.
    ExponentialBackoff {
        /// Maximum number of attempts (including the initial attempt).
        max_attempts: u32,
        /// Initial delay after the first failure.
        initial_delay: Duration,
        /// Maximum delay cap (delay won't exceed this).
        max_delay: Duration,
        /// Multiplier applied to delay after each failure (typically 2.0).
        multiplier: f64,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::None
    }
}

impl RetryPolicy {
    /// Creates an exponential backoff policy with sensible defaults.
    ///
    /// Uses:
    /// - Initial delay: 100ms ([`DEFAULT_INITIAL_DELAY_MS`])
    /// - Max delay: 30 seconds ([`DEFAULT_MAX_DELAY_SECS`])
    /// - Multiplier: 2.0 ([`DEFAULT_BACKOFF_MULTIPLIER`])
    ///
    /// # Arguments
    ///
    /// * `max_attempts` - Maximum number of attempts (including initial)
    pub fn exponential(max_attempts: u32) -> Self {
        Self::ExponentialBackoff {
            max_attempts,
            initial_delay: Duration::from_millis(DEFAULT_INITIAL_DELAY_MS),
            max_delay: Duration::from_secs(DEFAULT_MAX_DELAY_SECS),
            multiplier: DEFAULT_BACKOFF_MULTIPLIER,
        }
    }

    /// Creates a fixed retry policy.
    ///
    /// # Arguments
    ///
    /// * `max_attempts` - Maximum number of attempts (including initial)
    /// * `delay` - Fixed delay between attempts
    pub fn fixed(max_attempts: u32, delay: Duration) -> Self {
        Self::Fixed { max_attempts, delay }
    }

    /// Calculates the delay for a given attempt number.
    ///
    /// # Arguments
    ///
    /// * `attempt` - The attempt number (1-based, where 1 is the first retry)
    ///
    /// # Returns
    ///
    /// The delay to wait before the retry, or `None` if no more retries are allowed.
    pub fn delay_for_attempt(&self, attempt: u32) -> Option<Duration> {
        match self {
            Self::None => None,
            Self::Fixed { max_attempts, delay } => {
                if attempt < *max_attempts {
                    Some(*delay)
                } else {
                    None
                }
            }
            Self::ExponentialBackoff {
                max_attempts,
                initial_delay,
                max_delay,
                multiplier,
            } => {
                if attempt < *max_attempts {
                    // Calculate exponential delay: initial_delay * multiplier^(attempt-1)
                    let factor = multiplier.powi((attempt - 1) as i32);
                    let delay_ms = initial_delay.as_millis() as f64 * factor;
                    let delay = Duration::from_millis(delay_ms.min(max_delay.as_millis() as f64) as u64);
                    Some(delay.min(*max_delay))
                } else {
                    None
                }
            }
        }
    }

    /// Returns the maximum number of attempts for this policy.
    pub fn max_attempts(&self) -> u32 {
        match self {
            Self::None => 1,
            Self::Fixed { max_attempts, .. } => *max_attempts,
            Self::ExponentialBackoff { max_attempts, .. } => *max_attempts,
        }
    }
}

/// Task scheduling priority.
///
/// Tasks are queued by priority (higher values execute first), then by FIFO
/// order within the same priority level. This ensures that high-priority
/// on-demand requests are served before background prefetch work.
///
/// # Priority Levels
///
/// - [`Priority::ON_DEMAND`] (100): X-Plane requests that need immediate response
/// - [`Priority::PREFETCH`] (0): Background prefetch work
/// - [`Priority::HOUSEKEEPING`] (-50): Cleanup and maintenance tasks
///
/// # Example
///
/// ```ignore
/// use xearthlayer::executor::Priority;
///
/// // Custom priority between prefetch and housekeeping
/// let low_priority = Priority(-25);
///
/// // Compare priorities
/// assert!(Priority::ON_DEMAND > Priority::PREFETCH);
/// assert!(Priority::PREFETCH > Priority::HOUSEKEEPING);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(pub i32);

impl Priority {
    /// On-demand requests from X-Plane FUSE layer.
    ///
    /// These must be served immediately - X-Plane is waiting for the response.
    /// This is the highest priority level.
    pub const ON_DEMAND: Priority = Priority(PRIORITY_ON_DEMAND);

    /// Background prefetch work.
    ///
    /// Lower priority than on-demand, runs when X-Plane is idle.
    /// This is the default priority for jobs.
    pub const PREFETCH: Priority = Priority(PRIORITY_PREFETCH);

    /// Housekeeping tasks (cache cleanup, index rebuilding).
    ///
    /// Lowest priority, runs when nothing else needs resources.
    pub const HOUSEKEEPING: Priority = Priority(PRIORITY_HOUSEKEEPING);

    /// Creates a new priority with the given value.
    ///
    /// Higher values mean higher priority.
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    /// Returns the numeric priority value.
    pub fn value(&self) -> i32 {
        self.0
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self::PREFETCH
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::ON_DEMAND => write!(f, "OnDemand(100)"),
            Self::PREFETCH => write!(f, "Prefetch(0)"),
            Self::HOUSEKEEPING => write!(f, "Housekeeping(-50)"),
            Self(v) => write!(f, "Priority({})", v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_policy_default() {
        assert_eq!(ErrorPolicy::default(), ErrorPolicy::FailFast);
    }

    #[test]
    fn test_error_policy_partial_success() {
        let policy = ErrorPolicy::partial_success(0.8);
        assert_eq!(policy, ErrorPolicy::PartialSuccess { threshold: 0.8 });
    }

    #[test]
    #[should_panic(expected = "threshold must be between 0.0 and 1.0")]
    fn test_error_policy_invalid_threshold() {
        ErrorPolicy::partial_success(1.5);
    }

    #[test]
    fn test_retry_policy_none() {
        let policy = RetryPolicy::None;
        assert_eq!(policy.max_attempts(), 1);
        assert_eq!(policy.delay_for_attempt(1), None);
    }

    #[test]
    fn test_retry_policy_fixed() {
        let policy = RetryPolicy::fixed(3, Duration::from_millis(100));
        assert_eq!(policy.max_attempts(), 3);
        assert_eq!(policy.delay_for_attempt(1), Some(Duration::from_millis(100)));
        assert_eq!(policy.delay_for_attempt(2), Some(Duration::from_millis(100)));
        assert_eq!(policy.delay_for_attempt(3), None); // No more retries
    }

    #[test]
    fn test_retry_policy_exponential() {
        let policy = RetryPolicy::ExponentialBackoff {
            max_attempts: 4,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        };

        assert_eq!(policy.max_attempts(), 4);
        assert_eq!(policy.delay_for_attempt(1), Some(Duration::from_millis(100))); // 100ms
        assert_eq!(policy.delay_for_attempt(2), Some(Duration::from_millis(200))); // 200ms
        assert_eq!(policy.delay_for_attempt(3), Some(Duration::from_millis(400))); // 400ms
        assert_eq!(policy.delay_for_attempt(4), None); // No more retries
    }

    #[test]
    fn test_retry_policy_exponential_respects_max_delay() {
        let policy = RetryPolicy::ExponentialBackoff {
            max_attempts: 10,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
        };

        // After a few attempts, delay should be capped at max_delay
        assert!(policy.delay_for_attempt(5).unwrap() <= Duration::from_secs(5));
    }

    #[test]
    fn test_retry_policy_exponential_convenience() {
        let policy = RetryPolicy::exponential(3);
        assert_eq!(policy.max_attempts(), 3);
        // Should use default values
        if let RetryPolicy::ExponentialBackoff {
            initial_delay,
            max_delay,
            multiplier,
            ..
        } = policy
        {
            assert_eq!(initial_delay, Duration::from_millis(DEFAULT_INITIAL_DELAY_MS));
            assert_eq!(max_delay, Duration::from_secs(DEFAULT_MAX_DELAY_SECS));
            assert_eq!(multiplier, DEFAULT_BACKOFF_MULTIPLIER);
        } else {
            panic!("Expected ExponentialBackoff");
        }
    }

    #[test]
    fn test_priority_constants() {
        assert_eq!(Priority::ON_DEMAND.value(), PRIORITY_ON_DEMAND);
        assert_eq!(Priority::PREFETCH.value(), PRIORITY_PREFETCH);
        assert_eq!(Priority::HOUSEKEEPING.value(), PRIORITY_HOUSEKEEPING);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::ON_DEMAND > Priority::PREFETCH);
        assert!(Priority::PREFETCH > Priority::HOUSEKEEPING);
        assert!(Priority::ON_DEMAND > Priority::HOUSEKEEPING);
    }

    #[test]
    fn test_priority_default() {
        assert_eq!(Priority::default(), Priority::PREFETCH);
    }

    #[test]
    fn test_priority_custom() {
        let custom = Priority::new(50);
        assert!(custom > Priority::PREFETCH);
        assert!(custom < Priority::ON_DEMAND);
    }

    #[test]
    fn test_priority_display() {
        assert_eq!(format!("{}", Priority::ON_DEMAND), "OnDemand(100)");
        assert_eq!(format!("{}", Priority::PREFETCH), "Prefetch(0)");
        assert_eq!(format!("{}", Priority::HOUSEKEEPING), "Housekeeping(-50)");
        assert_eq!(format!("{}", Priority::new(42)), "Priority(42)");
    }
}
