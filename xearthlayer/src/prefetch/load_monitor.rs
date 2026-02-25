//! FUSE load monitoring for diagnostics and scene tracking.
//!
//! This module provides an abstraction for tracking FUSE request load.
//! The `FuseLoadMonitor` trait provides a single-responsibility interface
//! for recording and querying FUSE request counts.
//!
//! # Note
//!
//! As of issue #59, the circuit breaker uses resource pool utilization
//! rather than FUSE request rate for trip decisions. The load monitor is
//! retained for diagnostics logging and scene tracker integration.
//!
//! # Thread Safety
//!
//! Implementations must be `Send + Sync` for use across the async runtime
//! and FUSE threads. `SharedFuseLoadMonitor` uses atomic operations for
//! lock-free updates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Monitors FUSE request load for circuit breaker decisions.
///
/// Implementations track request counts and provide rate calculation data.
/// This trait abstracts the job counting mechanism, allowing prefetchers
/// to depend on a minimal interface rather than raw atomic counters.
///
/// # Example
///
/// ```
/// use xearthlayer::prefetch::{FuseLoadMonitor, SharedFuseLoadMonitor};
/// use std::sync::Arc;
///
/// let monitor = Arc::new(SharedFuseLoadMonitor::new());
///
/// // Record incoming FUSE requests
/// monitor.record_request();
/// monitor.record_request();
///
/// assert_eq!(monitor.total_requests(), 2);
/// ```
pub trait FuseLoadMonitor: Send + Sync {
    /// Record a FUSE request.
    ///
    /// Called by the DDS handler for non-prefetch requests (actual X-Plane
    /// file reads). This drives the circuit breaker's load detection.
    fn record_request(&self);

    /// Get total requests recorded since creation.
    ///
    /// Used by circuit breaker to calculate request rate via delta between
    /// consecutive calls.
    fn total_requests(&self) -> u64;
}

/// Thread-safe load monitor shared across all mounted services.
///
/// Uses atomic counter for lock-free updates, following the pattern
/// established by `HttpConcurrencyLimiter` for high-throughput counters.
///
/// # Usage
///
/// Create a single instance in `MountManager` and share it with:
/// - Each service's DDS handler (for recording requests)
/// - The circuit breaker (for reading request counts)
///
/// ```
/// use xearthlayer::prefetch::SharedFuseLoadMonitor;
/// use std::sync::Arc;
///
/// let monitor = Arc::new(SharedFuseLoadMonitor::new());
///
/// // Clone Arc for each component that needs access
/// let handler_monitor = Arc::clone(&monitor);
/// let circuit_breaker_monitor = Arc::clone(&monitor);
/// ```
#[derive(Debug, Default)]
pub struct SharedFuseLoadMonitor {
    requests: AtomicU64,
}

impl SharedFuseLoadMonitor {
    /// Create a new load monitor with zero initial count.
    pub fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
        }
    }

    /// Create a load monitor from an existing atomic counter.
    ///
    /// Useful during migration when code still uses raw `Arc<AtomicU64>`.
    pub fn from_atomic(counter: Arc<AtomicU64>) -> Self {
        Self {
            requests: AtomicU64::new(counter.load(Ordering::Relaxed)),
        }
    }
}

impl FuseLoadMonitor for SharedFuseLoadMonitor {
    fn record_request(&self) {
        self.requests.fetch_add(1, Ordering::Relaxed);
    }

    fn total_requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_monitor_starts_at_zero() {
        let monitor = SharedFuseLoadMonitor::new();
        assert_eq!(monitor.total_requests(), 0);
    }

    #[test]
    fn test_record_request_increments() {
        let monitor = SharedFuseLoadMonitor::new();

        monitor.record_request();
        assert_eq!(monitor.total_requests(), 1);

        monitor.record_request();
        monitor.record_request();
        assert_eq!(monitor.total_requests(), 3);
    }

    #[test]
    fn test_thread_safe_counting() {
        use std::thread;

        let monitor = Arc::new(SharedFuseLoadMonitor::new());
        let mut handles = vec![];

        // Spawn 10 threads, each recording 100 requests
        for _ in 0..10 {
            let m = Arc::clone(&monitor);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    m.record_request();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(monitor.total_requests(), 1000);
    }

    #[test]
    fn test_trait_object_usage() {
        let monitor: Arc<dyn FuseLoadMonitor> = Arc::new(SharedFuseLoadMonitor::new());

        monitor.record_request();
        monitor.record_request();

        assert_eq!(monitor.total_requests(), 2);
    }

    #[test]
    fn test_from_atomic() {
        let counter = Arc::new(AtomicU64::new(42));
        let monitor = SharedFuseLoadMonitor::from_atomic(counter);

        assert_eq!(monitor.total_requests(), 42);

        monitor.record_request();
        assert_eq!(monitor.total_requests(), 43);
    }

    #[test]
    fn test_default_impl() {
        let monitor = SharedFuseLoadMonitor::default();
        assert_eq!(monitor.total_requests(), 0);
    }
}
