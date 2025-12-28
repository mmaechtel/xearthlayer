//! CPU concurrency limiter for CPU-bound operations.
//!
//! This module provides a priority-aware limiter for CPU-bound work (assembly,
//! encoding) that prioritizes on-demand requests over prefetch requests,
//! while ensuring prefetch always has guaranteed minimum capacity.
//!
//! # Design
//!
//! The limiter uses three semaphore pools:
//! - **Priority pool**: Reserved exclusively for on-demand (high-priority) requests
//! - **Shared pool**: Available to both on-demand and prefetch requests
//! - **Prefetch pool**: Reserved exclusively for prefetch (low-priority) requests
//!
//! ```text
//! Total Permits: 20
//! ├── Priority Pool: 8 (40%) - on-demand only
//! ├── Shared Pool:   8 (40%) - on-demand and prefetch
//! └── Prefetch Pool: 4 (20%) - prefetch only (guaranteed minimum)
//!
//! On-Demand: Can use priority + shared (up to 16), waits on priority pool
//! Prefetch:  Can use prefetch + shared (up to 12), waits on prefetch pool
//! ```
//!
//! Both on-demand and prefetch will WAIT on their dedicated pools if necessary.
//! This ensures:
//! - On-demand requests are never blocked by prefetch (priority pool is exclusive)
//! - Prefetch requests always make progress (prefetch pool is exclusive)
//! - Both can benefit from shared pool when available (bonus capacity)

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Default percentage of permits reserved for priority (on-demand) requests.
pub const DEFAULT_PRIORITY_RESERVE_PERCENT: usize = 40;

/// Default percentage of permits reserved for prefetch requests.
pub const DEFAULT_PREFETCH_RESERVE_PERCENT: usize = 20;

/// Minimum permits to reserve for priority requests.
pub const MIN_PRIORITY_RESERVE: usize = 2;

/// Minimum permits to reserve for prefetch requests.
pub const MIN_PREFETCH_RESERVE: usize = 2;

/// Priority level for requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPriority {
    /// High priority - on-demand requests from X-Plane.
    /// Can use both priority and shared pools.
    High,
    /// Low priority - background prefetch requests.
    /// Can use prefetch pool (guaranteed) and shared pool (non-blocking).
    Low,
}

/// CPU concurrency limiter for CPU-bound operations.
///
/// Uses three pools to ensure:
/// - High-priority (on-demand) requests are never blocked by prefetch
/// - Low-priority (prefetch) requests always have guaranteed minimum capacity
#[derive(Debug)]
pub struct CPUConcurrencyLimiter {
    /// Semaphore for priority (on-demand only) permits
    priority_semaphore: Arc<Semaphore>,

    /// Semaphore for shared (both on-demand and prefetch) permits
    shared_semaphore: Arc<Semaphore>,

    /// Semaphore for prefetch-only permits (guaranteed capacity)
    prefetch_semaphore: Arc<Semaphore>,

    /// Number of priority permits
    priority_permits: usize,

    /// Number of shared permits
    shared_permits: usize,

    /// Number of prefetch-reserved permits
    prefetch_permits: usize,

    /// Current number of high-priority operations in flight.
    /// Uses Arc to allow permits to be 'static and work with spawned tasks.
    high_priority_in_flight: Arc<AtomicUsize>,

    /// Current number of low-priority operations in flight.
    /// Uses Arc to allow permits to be 'static and work with spawned tasks.
    low_priority_in_flight: Arc<AtomicUsize>,

    /// Label for debugging
    label: String,
}

impl CPUConcurrencyLimiter {
    /// Creates a new priority limiter with the specified total permits and pool percentages.
    ///
    /// # Arguments
    ///
    /// * `total_permits` - Total number of concurrent operations allowed
    /// * `priority_reserve_percent` - Percentage of permits reserved for high-priority (on-demand)
    /// * `prefetch_reserve_percent` - Percentage of permits reserved for low-priority (prefetch)
    /// * `label` - Human-readable label for logging
    ///
    /// The shared pool gets the remaining percentage (100 - priority - prefetch).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 20 total permits: 8 priority (40%) + 8 shared (40%) + 4 prefetch (20%)
    /// let limiter = CPUConcurrencyLimiter::new(20, 40, 20, "cpu_bound");
    /// ```
    pub fn new(
        total_permits: usize,
        priority_reserve_percent: usize,
        prefetch_reserve_percent: usize,
        label: impl Into<String>,
    ) -> Self {
        assert!(total_permits > 0, "total_permits must be > 0");
        assert!(
            priority_reserve_percent + prefetch_reserve_percent <= 100,
            "priority + prefetch reserve must be <= 100%"
        );

        // Calculate priority permits (reserved for on-demand)
        let priority_permits =
            ((total_permits * priority_reserve_percent) / 100).max(MIN_PRIORITY_RESERVE);

        // Calculate prefetch permits (guaranteed capacity for prefetch)
        let prefetch_permits =
            ((total_permits * prefetch_reserve_percent) / 100).max(MIN_PREFETCH_RESERVE);

        // Ensure we don't over-allocate
        let reserved = priority_permits + prefetch_permits;
        let (priority_permits, prefetch_permits, shared_permits) = if reserved >= total_permits {
            // Not enough for shared pool - scale down proportionally
            let scale = (total_permits - 1) as f64 / reserved as f64;
            let priority = ((priority_permits as f64 * scale).floor() as usize).max(1);
            let prefetch = ((prefetch_permits as f64 * scale).floor() as usize).max(1);
            let shared = total_permits.saturating_sub(priority + prefetch).max(1);
            (priority, prefetch, shared)
        } else {
            // Normal case - remaining goes to shared
            (priority_permits, prefetch_permits, total_permits - reserved)
        };

        let label_str: String = label.into();

        tracing::info!(
            total = total_permits,
            priority = priority_permits,
            shared = shared_permits,
            prefetch = prefetch_permits,
            label = %label_str,
            "Created CPU concurrency limiter with three pools"
        );

        Self {
            priority_semaphore: Arc::new(Semaphore::new(priority_permits)),
            shared_semaphore: Arc::new(Semaphore::new(shared_permits)),
            prefetch_semaphore: Arc::new(Semaphore::new(prefetch_permits)),
            priority_permits,
            shared_permits,
            prefetch_permits,
            high_priority_in_flight: Arc::new(AtomicUsize::new(0)),
            low_priority_in_flight: Arc::new(AtomicUsize::new(0)),
            label: label_str,
        }
    }

    /// Creates a limiter with default settings for CPU-bound work.
    ///
    /// Uses modest over-subscription (1.25x cores) with:
    /// - 40% reserved for on-demand (priority)
    /// - 20% reserved for prefetch (guaranteed)
    /// - 40% shared (both can use)
    pub fn with_defaults(label: impl Into<String>) -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);

        // Modest over-subscription keeps cores busy during brief I/O waits
        let total = ((cpus as f64 * 1.25).ceil() as usize).max(cpus + 2);

        Self::new(
            total,
            DEFAULT_PRIORITY_RESERVE_PERCENT,
            DEFAULT_PREFETCH_RESERVE_PERCENT,
            label,
        )
    }

    /// Creates a limiter with custom pool percentages.
    ///
    /// # Arguments
    ///
    /// * `priority_percent` - Percentage for on-demand only pool (0-100)
    /// * `prefetch_percent` - Percentage for prefetch only pool (0-100)
    /// * `label` - Human-readable label
    ///
    /// Shared pool gets the remainder (100 - priority - prefetch).
    pub fn with_percentages(
        priority_percent: usize,
        prefetch_percent: usize,
        label: impl Into<String>,
    ) -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);

        let total = ((cpus as f64 * 1.25).ceil() as usize).max(cpus + 2);

        Self::new(total, priority_percent, prefetch_percent, label)
    }

    /// Acquires a permit with the specified priority.
    ///
    /// # Behavior by Priority
    ///
    /// - **High priority**: Tries priority pool first, then shared pool. Always waits.
    /// - **Low priority**: Tries prefetch pool first (fast path), then shared pool (bonus),
    ///   then waits on prefetch pool (guaranteed capacity).
    ///
    /// # Arguments
    ///
    /// * `priority` - The request priority level
    ///
    /// # Returns
    ///
    /// Always returns a permit. Both priorities will wait if necessary.
    pub async fn acquire(&self, priority: RequestPriority) -> Option<CPUPermit> {
        match priority {
            RequestPriority::High => {
                // High priority: try priority pool first, then shared
                // This ensures on-demand always gets a permit

                // First, try the priority pool (fast path)
                if let Ok(permit) = self.priority_semaphore.clone().try_acquire_owned() {
                    self.high_priority_in_flight.fetch_add(1, Ordering::Relaxed);
                    return Some(CPUPermit {
                        _permit: PermitInner::Priority(permit),
                        in_flight: Arc::clone(&self.high_priority_in_flight),
                    });
                }

                // Priority pool full, try shared pool (also fast path)
                if let Ok(permit) = self.shared_semaphore.clone().try_acquire_owned() {
                    self.high_priority_in_flight.fetch_add(1, Ordering::Relaxed);
                    return Some(CPUPermit {
                        _permit: PermitInner::Shared(permit),
                        in_flight: Arc::clone(&self.high_priority_in_flight),
                    });
                }

                // Both pools busy - wait on priority pool (on-demand should wait)
                let permit = self
                    .priority_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .expect("priority semaphore closed");

                self.high_priority_in_flight.fetch_add(1, Ordering::Relaxed);
                Some(CPUPermit {
                    _permit: PermitInner::Priority(permit),
                    in_flight: Arc::clone(&self.high_priority_in_flight),
                })
            }
            RequestPriority::Low => {
                // Low priority: try prefetch pool first (fast path),
                // then shared pool (bonus capacity), then wait on prefetch (guaranteed)

                // First, try the prefetch-reserved pool (fast path)
                if let Ok(permit) = self.prefetch_semaphore.clone().try_acquire_owned() {
                    self.low_priority_in_flight.fetch_add(1, Ordering::Relaxed);
                    return Some(CPUPermit {
                        _permit: PermitInner::Prefetch(permit),
                        in_flight: Arc::clone(&self.low_priority_in_flight),
                    });
                }

                // Prefetch pool full, try shared pool (bonus capacity, non-blocking)
                if let Ok(permit) = self.shared_semaphore.clone().try_acquire_owned() {
                    self.low_priority_in_flight.fetch_add(1, Ordering::Relaxed);
                    return Some(CPUPermit {
                        _permit: PermitInner::Shared(permit),
                        in_flight: Arc::clone(&self.low_priority_in_flight),
                    });
                }

                // Both pools busy - wait on prefetch pool (guaranteed capacity)
                // This is the key difference from the old design: prefetch WAITS
                // for its dedicated capacity instead of backing off
                let permit = self
                    .prefetch_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .expect("prefetch semaphore closed");

                self.low_priority_in_flight.fetch_add(1, Ordering::Relaxed);
                Some(CPUPermit {
                    _permit: PermitInner::Prefetch(permit),
                    in_flight: Arc::clone(&self.low_priority_in_flight),
                })
            }
        }
    }

    /// Acquires a high-priority permit (convenience method).
    ///
    /// This is the most common case - on-demand requests from X-Plane.
    /// Always returns a permit (waits if necessary).
    pub async fn acquire_high(&self) -> CPUPermit {
        self.acquire(RequestPriority::High)
            .await
            .expect("high priority acquire should always succeed")
    }

    /// Acquires a low-priority permit (convenience method).
    ///
    /// For prefetch requests. Will wait on the prefetch pool if necessary,
    /// ensuring prefetch always gets its guaranteed capacity.
    pub async fn acquire_low(&self) -> CPUPermit {
        self.acquire(RequestPriority::Low)
            .await
            .expect("low priority acquire should always succeed")
    }

    /// Returns the label for this limiter.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the total number of permits (priority + shared + prefetch).
    pub fn total_permits(&self) -> usize {
        self.priority_permits + self.shared_permits + self.prefetch_permits
    }

    /// Returns the number of priority-reserved permits.
    pub fn priority_permits(&self) -> usize {
        self.priority_permits
    }

    /// Returns the number of shared permits.
    pub fn shared_permits(&self) -> usize {
        self.shared_permits
    }

    /// Returns the number of prefetch-reserved permits.
    pub fn prefetch_permits(&self) -> usize {
        self.prefetch_permits
    }

    /// Returns the current number of high-priority operations in flight.
    pub fn high_priority_in_flight(&self) -> usize {
        self.high_priority_in_flight.load(Ordering::Relaxed)
    }

    /// Returns the current number of low-priority operations in flight.
    pub fn low_priority_in_flight(&self) -> usize {
        self.low_priority_in_flight.load(Ordering::Relaxed)
    }

    /// Returns the total number of operations in flight.
    pub fn total_in_flight(&self) -> usize {
        self.high_priority_in_flight() + self.low_priority_in_flight()
    }

    /// Returns available permits in the priority pool.
    pub fn priority_available(&self) -> usize {
        self.priority_semaphore.available_permits()
    }

    /// Returns available permits in the shared pool.
    pub fn shared_available(&self) -> usize {
        self.shared_semaphore.available_permits()
    }

    /// Returns available permits in the prefetch pool.
    pub fn prefetch_available(&self) -> usize {
        self.prefetch_semaphore.available_permits()
    }
}

/// Internal permit type to track which pool the permit came from.
/// The permit is held only for its RAII behavior (release on drop).
#[allow(dead_code)]
enum PermitInner {
    Priority(OwnedSemaphorePermit),
    Shared(OwnedSemaphorePermit),
    Prefetch(OwnedSemaphorePermit),
}

/// A permit from the CPU limiter.
///
/// While held, counts against either the priority, shared, or prefetch pool.
/// Automatically released when dropped.
pub struct CPUPermit {
    _permit: PermitInner,
    in_flight: Arc<AtomicUsize>,
}

impl Drop for CPUPermit {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_limiter_three_pools() {
        let limiter = CPUConcurrencyLimiter::new(20, 40, 20, "test");
        assert_eq!(limiter.total_permits(), 20);
        assert_eq!(limiter.priority_permits(), 8); // 40% of 20
        assert_eq!(limiter.prefetch_permits(), 4); // 20% of 20
        assert_eq!(limiter.shared_permits(), 8); // Remaining 40%
    }

    #[test]
    fn test_minimum_reserves() {
        // Even with low percentages, should have at least minimums
        let limiter = CPUConcurrencyLimiter::new(10, 5, 5, "test");
        assert!(limiter.priority_permits() >= MIN_PRIORITY_RESERVE);
        assert!(limiter.prefetch_permits() >= MIN_PREFETCH_RESERVE);
    }

    #[test]
    fn test_shared_has_at_least_one() {
        // Should always leave at least 1 for shared
        let limiter = CPUConcurrencyLimiter::new(5, 50, 50, "test");
        assert!(limiter.shared_permits() >= 1);
    }

    #[test]
    fn test_with_percentages() {
        let limiter = CPUConcurrencyLimiter::with_percentages(30, 30, "test");
        // 30% priority + 30% prefetch = 40% shared
        let total = limiter.total_permits();
        // Verify pools exist and sum to total
        assert_eq!(
            limiter.priority_permits() + limiter.shared_permits() + limiter.prefetch_permits(),
            total
        );
    }

    #[tokio::test]
    async fn test_high_priority_always_succeeds() {
        let limiter = CPUConcurrencyLimiter::new(12, 40, 20, "test");
        // 5 priority + 5 shared + 2 prefetch = 12

        // Acquire all permits that high priority can use (priority + shared)
        let mut permits = Vec::new();
        let high_can_use = limiter.priority_permits() + limiter.shared_permits();
        for _ in 0..high_can_use {
            permits.push(limiter.acquire_high().await);
        }

        assert_eq!(limiter.high_priority_in_flight(), high_can_use);

        // Release and verify
        drop(permits);
        assert_eq!(limiter.high_priority_in_flight(), 0);
    }

    #[tokio::test]
    async fn test_low_priority_has_guaranteed_capacity() {
        let limiter = CPUConcurrencyLimiter::new(12, 40, 20, "test");
        // With 20% reserved, prefetch should have guaranteed capacity

        // Fill shared pool with high priority
        let mut high_permits = Vec::new();
        for _ in 0..limiter.shared_permits() {
            if let Ok(permit) = limiter.shared_semaphore.clone().try_acquire_owned() {
                high_permits.push(permit);
            }
        }

        // Even with shared pool full, prefetch should still get permits from prefetch pool
        let _low_permit = limiter.acquire_low().await;
        // If we get here, prefetch has guaranteed capacity (acquire_low always succeeds)
        assert!(
            limiter.low_priority_in_flight() > 0,
            "Prefetch should have guaranteed capacity"
        );
    }

    #[tokio::test]
    async fn test_low_priority_uses_both_pools() {
        let limiter = CPUConcurrencyLimiter::new(12, 40, 20, "test");

        // Fill prefetch pool first
        let mut low_permits = Vec::new();
        for _ in 0..limiter.prefetch_permits() {
            low_permits.push(limiter.acquire_low().await);
        }

        // Should still be able to get more from shared pool (fast path)
        // Since prefetch pool is full, the next acquire will use shared
        let _extra = limiter.acquire_low().await;
        assert!(
            limiter.low_priority_in_flight() > limiter.prefetch_permits(),
            "Should be able to use shared pool too"
        );
    }

    #[tokio::test]
    async fn test_pools_are_independent() {
        let limiter = Arc::new(CPUConcurrencyLimiter::new(12, 40, 20, "test"));

        // Fill all priority permits
        let mut priority_permits = Vec::new();
        for _ in 0..limiter.priority_permits() {
            if let Ok(permit) = limiter.priority_semaphore.clone().try_acquire_owned() {
                priority_permits.push(permit);
            }
        }

        // Prefetch pool should still be available
        assert!(
            limiter.prefetch_available() > 0,
            "Prefetch pool should be unaffected by priority pool"
        );

        // Low priority should still get a permit (and it always succeeds now)
        let _low_permit = limiter.acquire_low().await;
        assert!(
            limiter.low_priority_in_flight() > 0,
            "Low priority should work when only priority pool is full"
        );
    }

    #[tokio::test]
    async fn test_high_cannot_use_prefetch_pool() {
        let limiter = CPUConcurrencyLimiter::new(6, 34, 33, "test");
        // Should be roughly: 2 priority + 2 shared + 2 prefetch

        // Fill priority and shared pools
        let mut permits = Vec::new();
        let high_max = limiter.priority_permits() + limiter.shared_permits();
        for _ in 0..high_max {
            permits.push(limiter.acquire_high().await);
        }

        // Prefetch pool should still be available (high can't touch it)
        assert!(
            limiter.prefetch_available() > 0,
            "Prefetch pool should remain available"
        );
    }
}
