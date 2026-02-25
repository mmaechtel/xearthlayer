//! Circuit breaker for prefetch throttling.
//!
//! Monitors resource pool utilization to detect when the system is under heavy load.
//! When resource saturation is detected, the circuit "opens" to pause prefetching
//! and avoid competing with on-demand requests for bandwidth.
//!
//! # State Machine
//!
//! ```text
//! Closed --[resource_utilization > 90% for open_duration]--> Open
//! Open --[resource_utilization < 90%]--> HalfOpen
//! HalfOpen --[half_open_duration elapsed]--> Closed
//! HalfOpen --[resource_utilization > 90%]--> Open (reset)
//! ```
//!
//! # Design Decision (Issue #59)
//!
//! The circuit breaker uses **resource pool utilization exclusively** for trip
//! decisions. FUSE request rate is logged for observability but does NOT influence
//! the circuit state. This prevents self-tripping where FUSE cache hits (which are
//! near-zero cost) inflate the rate counter and cause unnecessary prefetch pauses.
//!
//! # Thread Safety
//!
//! `CircuitBreaker` implements `PrefetchThrottler` and can be used through
//! `Arc<dyn PrefetchThrottler>`. Interior mutability via `Mutex` ensures
//! thread-safe state updates.

use super::load_monitor::FuseLoadMonitor;
use super::throttler::{PrefetchThrottler, ThrottleState};
use crate::executor::ResourcePools;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Resource pool utilization threshold that triggers the circuit breaker.
///
/// When any resource pool exceeds this utilization fraction, the circuit breaker
/// counts it as high load — the same as high FUSE request rate.
pub const RESOURCE_SATURATION_THRESHOLD: f64 = 0.9;

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Duration resource saturation must sustain to trip the circuit (default: 500ms).
    pub open_duration: Duration,
    /// Duration of low utilization before auto-closing (default: 2s).
    pub half_open_duration: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            open_duration: Duration::from_millis(500),
            half_open_duration: Duration::from_secs(2),
        }
    }
}

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed - prefetch is active (normal operation).
    Closed,
    /// Circuit is open - prefetch is blocked (high X-Plane load detected).
    Open,
    /// Circuit is half-open - testing if safe to resume prefetch.
    HalfOpen,
}

impl CircuitState {
    /// User-friendly display string (NOT circuit breaker jargon).
    ///
    /// Returns terminology suitable for end-user TUI display.
    pub fn display_status(&self) -> &'static str {
        match self {
            CircuitState::Closed => "Active",
            CircuitState::Open => "Paused",
            CircuitState::HalfOpen => "Resuming...",
        }
    }
}

/// Internal mutable state for the circuit breaker.
#[derive(Debug)]
struct CircuitBreakerInner {
    state: CircuitState,
    high_load_start: Option<Instant>,
    half_open_start: Option<Instant>,
}

impl CircuitBreakerInner {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            high_load_start: None,
            half_open_start: None,
        }
    }
}

/// Circuit breaker for prefetch throttling.
///
/// Monitors resource pool utilization and pauses prefetching when the system
/// is under heavy load (resource saturation sustained beyond `open_duration`).
///
/// Implements `PrefetchThrottler` for use through trait objects.
///
/// # Example
///
/// ```
/// use xearthlayer::prefetch::{
///     CircuitBreaker, CircuitBreakerConfig, FuseLoadMonitor,
///     PrefetchThrottler, SharedFuseLoadMonitor,
/// };
/// use xearthlayer::executor::{ResourcePoolConfig, ResourcePools};
/// use std::sync::Arc;
///
/// let load_monitor = Arc::new(SharedFuseLoadMonitor::new());
/// let pools = Arc::new(ResourcePools::new(ResourcePoolConfig::default()));
/// let circuit_breaker = CircuitBreaker::new(
///     CircuitBreakerConfig::default(),
///     load_monitor,
///     pools,
/// );
///
/// // Use through trait object
/// let throttler: Arc<dyn PrefetchThrottler> = Arc::new(circuit_breaker);
/// if throttler.should_throttle() {
///     // Skip prefetch this cycle
/// }
/// ```
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    load_monitor: Arc<dyn FuseLoadMonitor>,
    /// Resource pools for utilization-based trip condition.
    resource_pools: Arc<ResourcePools>,
    inner: Mutex<CircuitBreakerInner>,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("config", &self.config)
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration, load monitor,
    /// and resource pools.
    pub fn new(
        config: CircuitBreakerConfig,
        load_monitor: Arc<dyn FuseLoadMonitor>,
        resource_pools: Arc<ResourcePools>,
    ) -> Self {
        Self {
            config,
            load_monitor,
            resource_pools,
            inner: Mutex::new(CircuitBreakerInner::new()),
        }
    }

    /// Update circuit state based on current resource utilization and return
    /// whether throttling is active.
    ///
    /// Checks resource pool utilization to detect system saturation.
    /// FUSE request total is logged for observability but does NOT influence
    /// circuit state decisions.
    ///
    /// # Returns
    ///
    /// `true` if circuit is open (prefetch should be blocked), `false` otherwise.
    fn update_and_check(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();

        let max_utilization = self.resource_pools.max_utilization();
        let is_high_load = max_utilization > RESOURCE_SATURATION_THRESHOLD;

        // Diagnostic logging (FUSE total for observability, not for decisions)
        let fuse_total = self.load_monitor.total_requests();
        tracing::debug!(
            fuse_requests_total = fuse_total,
            resource_utilization = format!("{:.1}%", max_utilization * 100.0),
            is_high_load,
            state = ?inner.state,
            "Circuit breaker check"
        );

        match inner.state {
            CircuitState::Closed => {
                if is_high_load {
                    if inner.high_load_start.is_none() {
                        inner.high_load_start = Some(Instant::now());
                        tracing::info!(
                            utilization = format!("{:.1}%", max_utilization * 100.0),
                            threshold = format!("{:.0}%", RESOURCE_SATURATION_THRESHOLD * 100.0),
                            "Circuit breaker: resource saturation detected"
                        );
                    }
                    if let Some(start) = inner.high_load_start {
                        if start.elapsed() >= self.config.open_duration {
                            inner.state = CircuitState::Open;
                            inner.high_load_start = None;
                            tracing::info!(
                                utilization = format!("{:.1}%", max_utilization * 100.0),
                                sustained_ms = self.config.open_duration.as_millis(),
                                "Circuit breaker OPENED — prefetch paused"
                            );
                        }
                    }
                } else {
                    if inner.high_load_start.is_some() {
                        tracing::debug!("Circuit breaker: resource load dropped, resetting");
                    }
                    inner.high_load_start = None;
                }
            }
            CircuitState::Open => {
                if !is_high_load {
                    inner.state = CircuitState::HalfOpen;
                    inner.half_open_start = Some(Instant::now());
                    tracing::info!(
                        utilization = format!("{:.1}%", max_utilization * 100.0),
                        "Circuit breaker: load dropped, half-open"
                    );
                }
            }
            CircuitState::HalfOpen => {
                if is_high_load {
                    inner.state = CircuitState::Open;
                    inner.half_open_start = None;
                    tracing::info!(
                        utilization = format!("{:.1}%", max_utilization * 100.0),
                        "Circuit breaker: load spike in half-open, re-opening"
                    );
                } else if let Some(start) = inner.half_open_start {
                    if start.elapsed() >= self.config.half_open_duration {
                        inner.state = CircuitState::Closed;
                        inner.half_open_start = None;
                        tracing::info!("Circuit breaker CLOSED — prefetch resumed");
                    }
                }
            }
        }

        Self::is_open_state(inner.state)
    }

    /// Check if the given state is "open" (prefetch blocked).
    fn is_open_state(state: CircuitState) -> bool {
        matches!(state, CircuitState::Open | CircuitState::HalfOpen)
    }

    /// Get the current circuit state.
    pub fn circuit_state(&self) -> CircuitState {
        self.inner.lock().unwrap().state
    }

    /// Check if the circuit is open (prefetch should be blocked).
    pub fn is_open(&self) -> bool {
        Self::is_open_state(self.inner.lock().unwrap().state)
    }

    /// Check if the circuit is closed (prefetch is allowed).
    pub fn is_closed(&self) -> bool {
        self.inner.lock().unwrap().state == CircuitState::Closed
    }
}

impl PrefetchThrottler for CircuitBreaker {
    fn should_throttle(&self) -> bool {
        self.update_and_check()
    }

    fn state(&self) -> ThrottleState {
        match self.inner.lock().unwrap().state {
            CircuitState::Closed => ThrottleState::Active,
            CircuitState::Open => ThrottleState::Paused,
            CircuitState::HalfOpen => ThrottleState::Resuming,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::load_monitor::SharedFuseLoadMonitor;
    use super::*;
    use std::thread;

    fn create_test_pools(capacity: usize) -> Arc<ResourcePools> {
        use crate::executor::ResourcePoolConfig;
        let config = ResourcePoolConfig {
            network: capacity,
            disk_io: capacity,
            cpu: capacity,
            ..Default::default()
        };
        Arc::new(ResourcePools::new(config))
    }

    fn create_test_circuit_breaker(
        config: CircuitBreakerConfig,
        pools: Arc<ResourcePools>,
    ) -> (CircuitBreaker, Arc<SharedFuseLoadMonitor>) {
        let load_monitor = Arc::new(SharedFuseLoadMonitor::new());
        let cb = CircuitBreaker::new(
            config,
            Arc::clone(&load_monitor) as Arc<dyn FuseLoadMonitor>,
            pools,
        );
        (cb, load_monitor)
    }

    #[test]
    fn test_circuit_breaker_initial_state() {
        let pools = create_test_pools(4);
        let (cb, _) = create_test_circuit_breaker(CircuitBreakerConfig::default(), pools);
        assert_eq!(cb.circuit_state(), CircuitState::Closed);
        assert!(!cb.is_open());
        assert!(cb.is_closed());
    }

    #[test]
    fn test_circuit_breaker_default_config() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.open_duration, Duration::from_millis(500));
        assert_eq!(config.half_open_duration, Duration::from_secs(2));
    }

    #[test]
    fn test_circuit_breaker_ignores_transient_spikes() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(100),
            half_open_duration: Duration::from_millis(50),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // First check
        cb.should_throttle();

        // Brief resource spike (acquire then release before open_duration)
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Closed);

        // Release permits quickly (before open_duration elapses)
        drop(permits);
        thread::sleep(Duration::from_millis(50));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_on_sustained_load() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(50),
            half_open_duration: Duration::from_millis(50),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // Saturate resource pools
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();

        // First check starts tracking
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Closed);

        // Sustained saturation should trip after open_duration
        thread::sleep(Duration::from_millis(60));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Open);
        assert!(cb.is_open());

        drop(permits);
    }

    #[test]
    fn test_circuit_breaker_transitions_to_half_open() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(30),
            half_open_duration: Duration::from_millis(50),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // Trip the circuit with sustained saturation
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        cb.should_throttle();
        thread::sleep(Duration::from_millis(40));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Open);

        // Release permits - load drops, should go to half-open
        drop(permits);
        thread::sleep(Duration::from_millis(10));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::HalfOpen);
        assert!(cb.is_open()); // Still considered "open" for blocking purposes
    }

    #[test]
    fn test_circuit_breaker_half_open_then_closes() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(30),
            half_open_duration: Duration::from_millis(40),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // Trip the circuit
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        cb.should_throttle();
        thread::sleep(Duration::from_millis(40));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Open);

        // Go to half-open
        drop(permits);
        thread::sleep(Duration::from_millis(10));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::HalfOpen);

        // Wait for half-open duration + check
        thread::sleep(Duration::from_millis(50));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Closed);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_circuit_breaker_resets_on_load_spike_in_half_open() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(30),
            half_open_duration: Duration::from_millis(100),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // Trip the circuit and go to half-open
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        cb.should_throttle();
        thread::sleep(Duration::from_millis(40));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Open);

        drop(permits);
        thread::sleep(Duration::from_millis(10));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::HalfOpen);

        // Re-saturate during half-open — should go back to open
        let permits2: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        thread::sleep(Duration::from_millis(10));
        cb.should_throttle();
        assert_eq!(cb.circuit_state(), CircuitState::Open);

        drop(permits2);
    }

    #[test]
    fn test_circuit_breaker_low_load_never_trips() {
        let pools = create_test_pools(4);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(30),
            half_open_duration: Duration::from_millis(30),
        };
        let (cb, _) = create_test_circuit_breaker(config, pools);

        // Many checks with empty pools (no load)
        cb.should_throttle();
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(10));
            cb.should_throttle();
        }

        assert_eq!(cb.circuit_state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_state_display_status() {
        assert_eq!(CircuitState::Closed.display_status(), "Active");
        assert_eq!(CircuitState::Open.display_status(), "Paused");
        assert_eq!(CircuitState::HalfOpen.display_status(), "Resuming...");
    }

    #[test]
    fn test_prefetch_throttler_trait() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(50),
            half_open_duration: Duration::from_millis(50),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // Use through trait
        let throttler: &dyn PrefetchThrottler = &cb;

        // Initial state - not throttling
        throttler.should_throttle();
        assert_eq!(throttler.state(), ThrottleState::Active);

        // Saturate resource pools
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        throttler.should_throttle();

        // Sustained saturation should trip
        thread::sleep(Duration::from_millis(60));
        let is_throttling = throttler.should_throttle();
        assert!(is_throttling);
        assert_eq!(throttler.state(), ThrottleState::Paused);

        drop(permits);
    }

    #[test]
    fn test_circuit_breaker_arc_trait_object() {
        let pools = create_test_pools(4);
        let config = CircuitBreakerConfig::default();
        let load_monitor = Arc::new(SharedFuseLoadMonitor::new());
        let cb = CircuitBreaker::new(config, load_monitor, pools);

        // Can be used as Arc<dyn PrefetchThrottler>
        let throttler: Arc<dyn PrefetchThrottler> = Arc::new(cb);
        assert!(!throttler.should_throttle());
        assert_eq!(throttler.state(), ThrottleState::Active);
    }

    #[test]
    fn test_circuit_breaker_trips_on_resource_saturation() {
        use crate::executor::ResourceType;

        let capacity = 4;
        let pools = create_test_pools(capacity);

        // Acquire all network permits to push utilization to 100%
        let permits: Vec<_> = (0..capacity)
            .filter_map(|_| pools.try_acquire(ResourceType::Network))
            .collect();
        assert!(
            pools.max_utilization() > RESOURCE_SATURATION_THRESHOLD,
            "Pools should be saturated"
        );

        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(30),
            half_open_duration: Duration::from_millis(50),
        };
        let (cb, _) = create_test_circuit_breaker(config, Arc::clone(&pools));

        // First check starts tracking
        cb.should_throttle();

        // Sustained resource saturation should trip the circuit
        thread::sleep(Duration::from_millis(40));
        let is_throttling = cb.should_throttle();
        assert!(
            is_throttling,
            "Should throttle when resource pools are saturated"
        );
        assert_eq!(cb.circuit_state(), CircuitState::Open);

        // Drop permits — pools drain
        drop(permits);

        // Load drops, should transition to half-open
        thread::sleep(Duration::from_millis(10));
        cb.should_throttle();
        assert_eq!(
            cb.circuit_state(),
            CircuitState::HalfOpen,
            "Should transition to half-open when pools drain"
        );

        // Wait for half-open to close
        thread::sleep(Duration::from_millis(60));
        cb.should_throttle();
        assert_eq!(
            cb.circuit_state(),
            CircuitState::Closed,
            "Should close after half-open duration"
        );
    }

    #[test]
    fn test_circuit_breaker_ignores_fuse_rate() {
        let pools = create_test_pools(4);
        let config = CircuitBreakerConfig {
            open_duration: Duration::from_millis(30),
            half_open_duration: Duration::from_millis(50),
        };
        let (cb, monitor) = create_test_circuit_breaker(config, Arc::clone(&pools));

        cb.should_throttle();

        // Simulate massive FUSE rate (1000 requests in 30ms)
        thread::sleep(Duration::from_millis(30));
        for _ in 0..1000 {
            monitor.record_request();
        }
        cb.should_throttle();

        thread::sleep(Duration::from_millis(40));
        for _ in 0..1000 {
            monitor.record_request();
        }
        cb.should_throttle();

        // Should NOT trip — pools are empty
        assert_eq!(
            cb.circuit_state(),
            CircuitState::Closed,
            "High FUSE rate alone should not trip breaker"
        );
    }

    #[test]
    fn test_circuit_breaker_thread_safe() {
        let pools = create_test_pools(4);
        let config = CircuitBreakerConfig::default();
        let load_monitor = Arc::new(SharedFuseLoadMonitor::new());
        let cb = Arc::new(CircuitBreaker::new(
            config,
            Arc::clone(&load_monitor) as Arc<dyn FuseLoadMonitor>,
            pools,
        ));

        let mut handles = vec![];

        // Spawn threads that all call should_throttle
        for _ in 0..4 {
            let cb = Arc::clone(&cb);
            let monitor = Arc::clone(&load_monitor);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    monitor.record_request();
                    cb.should_throttle();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have recorded 400 requests
        assert_eq!(load_monitor.total_requests(), 400);
    }
}
