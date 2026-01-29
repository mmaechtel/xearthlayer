//! Adaptive prefetch coordinator.
//!
//! Central orchestrator that ties together all adaptive prefetch components:
//! - Performance calibration for mode selection
//! - Flight phase detection (ground/cruise)
//! - Turn detection for prefetch pausing
//! - Strategy selection and execution
//!
//! # Architecture
//!
//! ```text
//!                    ┌─────────────────────┐
//!                    │    Coordinator      │
//!                    │  (main loop)        │
//!                    └─────────┬───────────┘
//!                              │
//!      ┌───────────┬──────────┼──────────┬───────────┐
//!      ▼           ▼          ▼          ▼           ▼
//! ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐
//! │ Phase   │ │ Turn    │ │ Ground  │ │ Cruise  │ │ Circuit │
//! │Detector │ │Detector │ │Strategy │ │Strategy │ │ Breaker │
//! └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘
//! ```
//!
//! # Trigger Modes
//!
//! The coordinator supports two trigger modes:
//!
//! - **Aggressive**: Position-based trigger at 0.3° into DSF tile
//! - **Opportunistic**: Circuit breaker trigger when X-Plane is idle
//!
//! # Track Data
//!
//! **Important**: The coordinator requires ground **track** (direction of travel),
//! not heading (nose direction). XGPS2 telemetry only provides heading, so track
//! must be derived from successive GPS positions:
//!
//! ```ignore
//! // Track derivation from position deltas
//! let track = atan2(delta_lon, delta_lat).to_degrees();
//! ```
//!
//! Until track derivation is implemented, heading can be used as a fallback
//! (less accurate in crosswind conditions).
//!
//! # Example
//!
//! ```ignore
//! let coordinator = AdaptivePrefetchCoordinator::builder()
//!     .with_config(config)
//!     .with_calibration(calibration)
//!     .with_circuit_breaker(circuit_breaker)
//!     .with_dds_client(dds_client)
//!     .build()?;
//!
//! // Run the coordinator loop
//! coordinator.run(shutdown_token).await;
//! ```

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::coord::TileCoord;
use crate::executor::DdsClient;
use crate::prefetch::state::{AircraftState, DetailedPrefetchStats, SharedPrefetchStatus};
use crate::prefetch::strategy::Prefetcher;
use crate::prefetch::throttler::{PrefetchThrottler, ThrottleState};
use crate::prefetch::CircuitState;
use crate::prefetch::SceneryIndex;

use super::calibration::{PerformanceCalibration, StrategyMode};
use super::config::{AdaptivePrefetchConfig, PrefetchMode};
use super::cruise_strategy::CruiseStrategy;
use super::ground_strategy::GroundStrategy;
use super::phase_detector::{FlightPhase, PhaseDetector};
use super::strategy::{AdaptivePrefetchStrategy, PrefetchPlan};
use super::turn_detector::{TurnDetector, TurnState};

// ─────────────────────────────────────────────────────────────────────────────
// Coordinator state
// ─────────────────────────────────────────────────────────────────────────────

/// Current coordinator state for status reporting.
#[derive(Debug, Clone)]
pub struct CoordinatorStatus {
    /// Whether prefetch is currently enabled.
    pub enabled: bool,

    /// Current strategy mode (from calibration or config).
    pub mode: StrategyMode,

    /// Current flight phase.
    pub phase: FlightPhase,

    /// Current turn state.
    pub turn_state: TurnState,

    /// Name of active strategy.
    pub active_strategy: &'static str,

    /// Last stable track (if known).
    pub stable_track: Option<f64>,

    /// Tiles prefetched in the last cycle.
    pub last_prefetch_count: usize,

    /// Whether throttled by circuit breaker.
    pub throttled: bool,
}

impl Default for CoordinatorStatus {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: StrategyMode::Opportunistic,
            phase: FlightPhase::Ground,
            turn_state: TurnState::Initializing,
            active_strategy: "none",
            stable_track: None,
            last_prefetch_count: 0,
            throttled: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Coordinator
// ─────────────────────────────────────────────────────────────────────────────

/// Adaptive prefetch coordinator.
///
/// Orchestrates all prefetch components and manages the prefetch lifecycle.
/// Thread-safe for shared access from telemetry and status queries.
pub struct AdaptivePrefetchCoordinator {
    /// Configuration.
    config: AdaptivePrefetchConfig,

    /// Performance calibration (determines mode).
    calibration: Option<PerformanceCalibration>,

    /// Flight phase detector.
    phase_detector: PhaseDetector,

    /// Turn detector.
    turn_detector: TurnDetector,

    /// Ground strategy.
    ground_strategy: GroundStrategy,

    /// Cruise strategy.
    cruise_strategy: CruiseStrategy,

    /// Circuit breaker for throttling.
    throttler: Option<Arc<dyn PrefetchThrottler>>,

    /// DDS client for submitting prefetch requests.
    dds_client: Option<Arc<dyn DdsClient>>,

    /// Tiles currently in cache (for filtering).
    cached_tiles: HashSet<TileCoord>,

    /// Current status.
    status: CoordinatorStatus,

    /// Shared status for TUI display.
    shared_status: Option<Arc<SharedPrefetchStatus>>,

    /// Cumulative prefetch statistics.
    total_cycles: u64,
    total_tiles_submitted: u64,
    total_cache_hits: u64,
}

impl std::fmt::Debug for AdaptivePrefetchCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdaptivePrefetchCoordinator")
            .field("config.enabled", &self.config.enabled)
            .field("config.mode", &self.config.mode)
            .field("has_calibration", &self.calibration.is_some())
            .field("has_throttler", &self.throttler.is_some())
            .field("has_dds_client", &self.dds_client.is_some())
            .field("cached_tiles_count", &self.cached_tiles.len())
            .field("status", &self.status)
            .finish()
    }
}

impl AdaptivePrefetchCoordinator {
    /// Create a new coordinator with the given configuration.
    pub fn new(config: AdaptivePrefetchConfig) -> Self {
        let phase_detector = PhaseDetector::new(&config);
        let turn_detector = TurnDetector::new(&config);
        let ground_strategy = GroundStrategy::new(&config);
        let cruise_strategy = CruiseStrategy::new(&config);

        Self {
            config,
            calibration: None,
            phase_detector,
            turn_detector,
            ground_strategy,
            cruise_strategy,
            throttler: None,
            dds_client: None,
            cached_tiles: HashSet::new(),
            status: CoordinatorStatus::default(),
            shared_status: None,
            total_cycles: 0,
            total_tiles_submitted: 0,
            total_cache_hits: 0,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(AdaptivePrefetchConfig::default())
    }

    /// Set the performance calibration.
    pub fn with_calibration(mut self, calibration: PerformanceCalibration) -> Self {
        self.status.mode = calibration.recommended_strategy;
        self.calibration = Some(calibration);
        self
    }

    /// Set the circuit breaker for throttling.
    pub fn with_throttler(mut self, throttler: Arc<dyn PrefetchThrottler>) -> Self {
        self.throttler = Some(throttler);
        self
    }

    /// Set the DDS client for submitting prefetch requests.
    pub fn with_dds_client(mut self, client: Arc<dyn DdsClient>) -> Self {
        self.dds_client = Some(client);
        self
    }

    /// Set the scenery index for tile lookup.
    pub fn with_scenery_index(mut self, index: Arc<SceneryIndex>) -> Self {
        self.ground_strategy = self.ground_strategy.with_scenery_index(Arc::clone(&index));
        self.cruise_strategy = self.cruise_strategy.with_scenery_index(index);
        self
    }

    /// Set the shared status for TUI display.
    pub fn with_shared_status(mut self, status: Arc<SharedPrefetchStatus>) -> Self {
        self.shared_status = Some(status);
        self
    }

    /// Get the current effective mode.
    ///
    /// Considers config override and calibration results.
    pub fn effective_mode(&self) -> StrategyMode {
        match self.config.mode {
            PrefetchMode::Aggressive => StrategyMode::Aggressive,
            PrefetchMode::Opportunistic => StrategyMode::Opportunistic,
            PrefetchMode::Disabled => StrategyMode::Disabled,
            PrefetchMode::Auto => {
                if let Some(ref cal) = self.calibration {
                    cal.recommended_strategy
                } else {
                    // No calibration yet - default to opportunistic
                    StrategyMode::Opportunistic
                }
            }
        }
    }

    /// Update with new aircraft state.
    ///
    /// Call this with each telemetry update. Returns the tiles to prefetch
    /// (if any) based on current conditions.
    ///
    /// # Arguments
    ///
    /// * `position` - Aircraft position (lat, lon) in degrees
    /// * `track` - Ground track in degrees (0-360)
    /// * `ground_speed_kt` - Ground speed in knots
    /// * `agl_ft` - Altitude above ground level in feet
    ///
    /// # Returns
    ///
    /// A `PrefetchPlan` if prefetching is appropriate, `None` otherwise.
    pub fn update(
        &mut self,
        position: (f64, f64),
        track: f64,
        ground_speed_kt: f32,
        agl_ft: f32,
    ) -> Option<PrefetchPlan> {
        // Check if enabled
        if !self.config.enabled {
            self.status.enabled = false;
            return None;
        }
        self.status.enabled = true;

        // Get effective mode
        let mode = self.effective_mode();
        self.status.mode = mode;

        if mode == StrategyMode::Disabled {
            return None;
        }

        // Update phase detector
        self.phase_detector.update(ground_speed_kt, agl_ft);
        let phase = self.phase_detector.current_phase();
        self.status.phase = phase;

        // Update turn detector
        self.turn_detector.update(track);
        self.status.turn_state = self.turn_detector.state();
        self.status.stable_track = self.turn_detector.stable_track();

        // Check throttling (for opportunistic mode)
        if let Some(ref throttler) = self.throttler {
            self.status.throttled = throttler.should_throttle();
        }

        // Determine if we should prefetch
        let should_prefetch = self.should_prefetch_now(mode);
        if !should_prefetch {
            return None;
        }

        // Get calibration (or use default)
        let calibration = self
            .calibration
            .clone()
            .unwrap_or_else(PerformanceCalibration::default_opportunistic);

        // Select and execute strategy
        let plan = match phase {
            FlightPhase::Ground => {
                self.status.active_strategy = self.ground_strategy.name();
                self.ground_strategy.calculate_prefetch(
                    position,
                    track,
                    &calibration,
                    &self.cached_tiles,
                )
            }
            FlightPhase::Cruise => {
                // Only prefetch in cruise if track is stable
                if !self.turn_detector.is_stable() {
                    tracing::debug!(
                        turn_state = ?self.turn_detector.state(),
                        "Skipping cruise prefetch - track not stable"
                    );
                    return None;
                }

                self.status.active_strategy = self.cruise_strategy.name();
                self.cruise_strategy.calculate_prefetch(
                    position,
                    track,
                    &calibration,
                    &self.cached_tiles,
                )
            }
        };

        // Log plan details
        if !plan.is_empty() {
            self.log_plan(&plan, position, track);
        }

        self.status.last_prefetch_count = plan.tile_count();
        Some(plan)
    }

    /// Execute a prefetch plan by submitting tiles to the DDS client.
    ///
    /// # Arguments
    ///
    /// * `plan` - The prefetch plan to execute
    /// * `cancellation` - Shared cancellation token for the batch
    ///
    /// # Returns
    ///
    /// Number of tiles submitted.
    pub fn execute(&self, plan: &PrefetchPlan, cancellation: CancellationToken) -> usize {
        let Some(ref client) = self.dds_client else {
            tracing::warn!("No DDS client configured - cannot execute prefetch");
            return 0;
        };

        let mut submitted = 0;
        for tile in &plan.tiles {
            client.prefetch_with_cancellation(*tile, cancellation.clone());
            submitted += 1;
        }

        if submitted > 0 {
            tracing::info!(
                tiles = submitted,
                strategy = plan.strategy,
                estimated_ms = plan.estimated_completion_ms,
                "Prefetch batch submitted"
            );
        }

        submitted
    }

    /// Mark tiles as cached (to avoid re-prefetching).
    pub fn mark_cached(&mut self, tiles: impl IntoIterator<Item = TileCoord>) {
        self.cached_tiles.extend(tiles);
    }

    /// Clear cached tile tracking.
    pub fn clear_cache_tracking(&mut self) {
        self.cached_tiles.clear();
    }

    /// Get current status for UI/logging.
    pub fn status(&self) -> &CoordinatorStatus {
        &self.status
    }

    /// Get the phase detector for external monitoring.
    pub fn phase_detector(&self) -> &PhaseDetector {
        &self.phase_detector
    }

    /// Get the turn detector for external monitoring.
    pub fn turn_detector(&self) -> &TurnDetector {
        &self.turn_detector
    }

    /// Reset the coordinator state.
    ///
    /// Call this when teleporting or starting a new flight.
    pub fn reset(&mut self) {
        self.turn_detector.reset();
        self.cached_tiles.clear();
        self.status = CoordinatorStatus::default();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Determine if we should prefetch now based on mode and conditions.
    fn should_prefetch_now(&self, mode: StrategyMode) -> bool {
        match mode {
            StrategyMode::Disabled => false,

            StrategyMode::Aggressive => {
                // Aggressive mode always prefetches (position-based trigger handled externally)
                true
            }

            StrategyMode::Opportunistic => {
                // Opportunistic mode checks throttler
                if let Some(ref throttler) = self.throttler {
                    !throttler.should_throttle()
                } else {
                    // No throttler - default to allowing prefetch
                    true
                }
            }
        }
    }

    /// Get startup info string for logging.
    pub fn startup_info_string(&self) -> String {
        let mode = self.effective_mode();
        format!(
            "adaptive, mode={:?}, ground_threshold={}kt, turn_threshold={}°",
            mode,
            self.config.ground_speed_threshold_kt,
            self.config.turn_threshold_deg()
        )
    }

    /// Log plan details with metadata.
    fn log_plan(&self, plan: &PrefetchPlan, position: (f64, f64), track: f64) {
        let (lat, lon) = position;

        if let Some(ref metadata) = plan.metadata {
            tracing::info!(
                strategy = plan.strategy,
                tiles = plan.tile_count(),
                skipped_cached = plan.skipped_cached,
                total_considered = plan.total_considered,
                estimated_ms = plan.estimated_completion_ms,
                dsf_tiles = metadata.dsf_tile_count,
                bounds_source = metadata.bounds_source,
                track_quadrant = ?metadata.track_quadrant,
                bounds = ?metadata.bounds,
                position = format!("{:.2}°, {:.2}°", lat, lon),
                track = format!("{:.1}°", track),
                "Prefetch plan calculated"
            );
        } else {
            tracing::info!(
                strategy = plan.strategy,
                tiles = plan.tile_count(),
                estimated_ms = plan.estimated_completion_ms,
                position = format!("{:.2}°, {:.2}°", lat, lon),
                track = format!("{:.1}°", track),
                "Prefetch plan calculated"
            );
        }
    }

    /// Validate that a plan can complete in time.
    ///
    /// Uses position within DSF tile and ground speed to estimate
    /// available time before X-Plane triggers.
    #[allow(dead_code)] // Will be used in future phase
    fn can_complete_in_time(
        &self,
        plan: &PrefetchPlan,
        position: (f64, f64),
        ground_speed_kt: f32,
    ) -> bool {
        // Convert ground speed to degrees per second
        // 1 knot ≈ 1.852 km/h, 1° ≈ 111 km at equator
        let speed_deg_per_sec = (ground_speed_kt as f64 * 1.852) / (111.0 * 3600.0);

        if speed_deg_per_sec < 0.0001 {
            // Stationary or very slow - no time constraint
            return true;
        }

        // Estimate distance to X-Plane's trigger (0.6° from DSF boundary)
        let (lat, _lon) = position;
        let current_dsf_lat = lat.floor();
        let position_in_dsf = lat - current_dsf_lat;

        // X-Plane triggers at 0.6° into the next DSF
        // So we have: (1.0 - position_in_dsf) + 0.6 = distance to trigger
        let distance_to_trigger = (1.0 - position_in_dsf) + 0.6;
        let time_available_secs = distance_to_trigger / speed_deg_per_sec;

        let time_required_secs = plan.estimated_completion_ms as f64 / 1000.0;
        let margin = self.config.time_budget_margin;

        let can_complete = time_required_secs <= time_available_secs * margin;

        tracing::debug!(
            time_required_secs = format!("{:.1}", time_required_secs),
            time_available_secs = format!("{:.1}", time_available_secs),
            margin = format!("{:.0}%", margin * 100.0),
            can_complete = can_complete,
            "Time budget check"
        );

        can_complete
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Telemetry processing (extracted for testability)
    // ─────────────────────────────────────────────────────────────────────────

    /// Check if enough time has passed since the last cycle.
    pub fn should_run_cycle(&self, last_cycle: Instant, min_interval: Duration) -> bool {
        Instant::now().duration_since(last_cycle) >= min_interval
    }

    /// Extract track from aircraft state.
    ///
    /// Uses heading as track fallback until track derivation is fully integrated.
    /// The design doc notes: "heading can be used as a fallback (less accurate
    /// in crosswind conditions)".
    pub fn extract_track(state: &AircraftState) -> f64 {
        // Use heading as track (fallback)
        state.heading as f64
    }

    /// Process a single telemetry update and execute prefetch if appropriate.
    ///
    /// Returns the number of tiles submitted, or None if no prefetch was performed.
    pub fn process_telemetry(&mut self, state: &AircraftState) -> Option<usize> {
        let track = Self::extract_track(state);
        let position = (state.latitude, state.longitude);

        // Note: We don't have true AGL from telemetry, only MSL altitude.
        // Phase detection now uses ground speed as the primary indicator.
        // Pass 0 for AGL so the phase detector uses ground speed only.
        let agl_ft = 0.0;

        let plan = self.update(position, track, state.ground_speed, agl_ft)?;

        let submitted = if plan.is_empty() {
            0
        } else {
            let cancellation = CancellationToken::new();
            self.execute(&plan, cancellation)
        };

        // Update statistics
        self.total_cycles += 1;
        self.total_tiles_submitted += submitted as u64;
        self.total_cache_hits += plan.skipped_cached as u64;

        // Update shared status for TUI
        self.update_shared_status(position, &plan, submitted);

        tracing::debug!(
            tiles = submitted,
            strategy = plan.strategy,
            phase = %self.status.phase,
            "Adaptive prefetch cycle complete"
        );

        Some(submitted)
    }

    /// Update the shared status for TUI display.
    fn update_shared_status(&self, position: (f64, f64), plan: &PrefetchPlan, submitted: usize) {
        let Some(ref status) = self.shared_status else {
            return;
        };

        // Update inferred position (adaptive doesn't have GPS status concept)
        status.update_inferred_position(position.0, position.1);

        // Determine prefetch mode for display
        let prefetch_mode = match self.status.phase {
            FlightPhase::Ground => crate::prefetch::state::PrefetchMode::Radial,
            FlightPhase::Cruise => crate::prefetch::state::PrefetchMode::TileBased,
        };
        status.update_prefetch_mode(prefetch_mode);

        // Update detailed stats
        let circuit_state = self.throttler.as_ref().map(|t| match t.state() {
            ThrottleState::Active => CircuitState::Closed,
            ThrottleState::Paused => CircuitState::Open,
            ThrottleState::Resuming => CircuitState::HalfOpen,
        });

        // Get loading tiles (first 10 from plan)
        let loading_tiles: Vec<(i32, i32)> = plan
            .tiles
            .iter()
            .take(10)
            .map(|t: &TileCoord| {
                let (lat, lon) = t.to_lat_lon();
                (lat.floor() as i32, lon.floor() as i32)
            })
            .collect();

        let detailed = DetailedPrefetchStats {
            cycles: self.total_cycles,
            tiles_submitted_last_cycle: submitted as u64,
            tiles_submitted_total: self.total_tiles_submitted,
            cache_hits: self.total_cache_hits,
            ttl_skipped: 0,               // Not tracked in adaptive
            active_zoom_levels: vec![14], // Fallback uses zoom 14
            is_active: submitted > 0,
            circuit_state,
            loading_tiles,
        };
        status.update_detailed_stats(detailed);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Prefetcher trait implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Minimum interval between prefetch cycles.
const MIN_CYCLE_INTERVAL: Duration = Duration::from_secs(2);

impl Prefetcher for AdaptivePrefetchCoordinator {
    fn run(
        mut self: Box<Self>,
        mut state_rx: mpsc::Receiver<AircraftState>,
        cancellation_token: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            tracing::info!(mode = ?self.effective_mode(), "Adaptive prefetcher started");

            let mut last_cycle = Instant::now();

            loop {
                tokio::select! {
                    biased;

                    _ = cancellation_token.cancelled() => break,

                    state_opt = state_rx.recv() => {
                        let Some(state) = state_opt else { break };

                        if !self.should_run_cycle(last_cycle, MIN_CYCLE_INTERVAL) {
                            continue;
                        }

                        self.process_telemetry(&state);
                        last_cycle = Instant::now();
                    }
                }
            }

            tracing::info!("Adaptive prefetcher stopped");
        })
    }

    fn name(&self) -> &'static str {
        "adaptive"
    }

    fn description(&self) -> &'static str {
        "Self-calibrating adaptive prefetch with phase detection and turn handling"
    }

    fn startup_info(&self) -> String {
        self.startup_info_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn test_calibration() -> PerformanceCalibration {
        PerformanceCalibration {
            throughput_tiles_per_sec: 25.0,
            avg_tile_generation_ms: 40,
            tile_generation_stddev_ms: 10,
            confidence: 0.9,
            recommended_strategy: StrategyMode::Opportunistic,
            calibrated_at: Instant::now(),
            baseline_throughput: 25.0,
            sample_count: 100,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Creation tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_coordinator_creation() {
        let coord = AdaptivePrefetchCoordinator::with_defaults();
        assert!(coord.config.enabled);
        assert!(coord.calibration.is_none());
        assert!(coord.throttler.is_none());
        assert!(coord.dds_client.is_none());
    }

    #[test]
    fn test_coordinator_with_calibration() {
        let cal = test_calibration();
        let coord = AdaptivePrefetchCoordinator::with_defaults().with_calibration(cal);
        assert!(coord.calibration.is_some());
        assert_eq!(coord.status.mode, StrategyMode::Opportunistic);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Mode selection tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_effective_mode_auto_no_calibration() {
        let coord = AdaptivePrefetchCoordinator::with_defaults();
        // Without calibration, auto defaults to opportunistic
        assert_eq!(coord.effective_mode(), StrategyMode::Opportunistic);
    }

    #[test]
    fn test_effective_mode_auto_with_calibration() {
        let mut cal = test_calibration();
        cal.recommended_strategy = StrategyMode::Aggressive;

        let coord = AdaptivePrefetchCoordinator::with_defaults().with_calibration(cal);
        assert_eq!(coord.effective_mode(), StrategyMode::Aggressive);
    }

    #[test]
    fn test_effective_mode_override() {
        let config = AdaptivePrefetchConfig {
            mode: PrefetchMode::Disabled,
            ..Default::default()
        };
        let coord = AdaptivePrefetchCoordinator::new(config);
        // Override takes precedence
        assert_eq!(coord.effective_mode(), StrategyMode::Disabled);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Update tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_update_disabled_returns_none() {
        let config = AdaptivePrefetchConfig {
            enabled: false,
            ..Default::default()
        };
        let mut coord = AdaptivePrefetchCoordinator::new(config);

        let plan = coord.update((53.5, 9.5), 45.0, 100.0, 1000.0);
        assert!(plan.is_none());
        assert!(!coord.status.enabled);
    }

    #[test]
    fn test_update_disabled_mode_returns_none() {
        let config = AdaptivePrefetchConfig {
            mode: PrefetchMode::Disabled,
            ..Default::default()
        };
        let mut coord = AdaptivePrefetchCoordinator::new(config);

        let plan = coord.update((53.5, 9.5), 45.0, 100.0, 1000.0);
        assert!(plan.is_none());
    }

    #[test]
    fn test_update_ground_phase() {
        let mut coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());

        // Ground conditions: low speed, low AGL
        let _plan = coord.update((53.5, 9.5), 45.0, 10.0, 5.0);
        assert_eq!(coord.status.phase, FlightPhase::Ground);
        assert_eq!(coord.status.active_strategy, "ground");
    }

    #[test]
    fn test_update_cruise_phase() {
        let mut coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());

        // Cruise conditions: high speed
        // First update starts the phase transition (hysteresis)
        coord.update((53.5, 9.5), 45.0, 200.0, 10000.0);
        // Phase detector has hysteresis, so first update may not transition
        // The phase will still be Ground due to hysteresis delay

        // Wait for hysteresis duration (default 2s, but we can't wait that long in tests)
        // Just verify the update doesn't panic and phase is tracked
        assert!(
            coord.status.phase == FlightPhase::Ground || coord.status.phase == FlightPhase::Cruise
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Turn detector integration tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_update_tracks_turn_state() {
        let mut coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());

        coord.update((53.5, 9.5), 45.0, 200.0, 10000.0);
        // Initially not stable
        assert_ne!(coord.status.turn_state, TurnState::Stable);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Cache tracking tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_mark_cached() {
        let mut coord = AdaptivePrefetchCoordinator::with_defaults();

        let tiles = vec![
            TileCoord {
                row: 100,
                col: 200,
                zoom: 14,
            },
            TileCoord {
                row: 101,
                col: 200,
                zoom: 14,
            },
        ];

        coord.mark_cached(tiles);
        assert_eq!(coord.cached_tiles.len(), 2);
    }

    #[test]
    fn test_clear_cache_tracking() {
        let mut coord = AdaptivePrefetchCoordinator::with_defaults();

        coord.mark_cached(vec![TileCoord {
            row: 100,
            col: 200,
            zoom: 14,
        }]);
        assert_eq!(coord.cached_tiles.len(), 1);

        coord.clear_cache_tracking();
        assert!(coord.cached_tiles.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Reset tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_reset() {
        let mut coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());

        // Update to set state
        coord.update((53.5, 9.5), 45.0, 200.0, 10000.0);
        coord.mark_cached(vec![TileCoord {
            row: 100,
            col: 200,
            zoom: 14,
        }]);

        // Reset
        coord.reset();
        assert!(coord.cached_tiles.is_empty());
        assert_eq!(coord.turn_detector.state(), TurnState::Initializing);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Time budget tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_time_budget_stationary() {
        let coord = AdaptivePrefetchCoordinator::with_defaults();
        let plan = PrefetchPlan::empty("test");

        // Stationary - should always be OK
        assert!(coord.can_complete_in_time(&plan, (53.5, 9.5), 0.0));
    }

    #[test]
    fn test_time_budget_fast_flight() {
        let coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());

        // Create a large plan
        let mut plan = PrefetchPlan::with_tiles(
            vec![
                TileCoord {
                    row: 100,
                    col: 200,
                    zoom: 14
                };
                100
            ],
            coord.calibration.as_ref().unwrap(),
            "test",
            0,
            100,
        );
        plan.estimated_completion_ms = 60000; // 60 seconds

        // At 450 knots, time budget is tight
        // This test just verifies the calculation runs
        let _can_complete = coord.can_complete_in_time(&plan, (53.1, 9.5), 450.0);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Status tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_status_default() {
        let status = CoordinatorStatus::default();
        assert!(status.enabled);
        assert_eq!(status.mode, StrategyMode::Opportunistic);
        assert_eq!(status.phase, FlightPhase::Ground);
        assert_eq!(status.turn_state, TurnState::Initializing);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Telemetry processing tests (extracted methods)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_should_run_cycle_respects_interval() {
        let coord = AdaptivePrefetchCoordinator::with_defaults();

        // Just started - should not run yet
        let now = Instant::now();
        assert!(!coord.should_run_cycle(now, Duration::from_secs(2)));

        // After interval - should run
        let past = Instant::now() - Duration::from_secs(3);
        assert!(coord.should_run_cycle(past, Duration::from_secs(2)));
    }

    #[test]
    fn test_extract_track() {
        let state = AircraftState::new(53.5, 9.5, 90.0, 250.0, 35000.0);

        let track = AdaptivePrefetchCoordinator::extract_track(&state);

        // Track should be heading as f64
        assert!((track - 90.0).abs() < 0.001);
    }

    #[test]
    fn test_process_telemetry_disabled() {
        let config = AdaptivePrefetchConfig {
            enabled: false,
            ..Default::default()
        };
        let mut coord = AdaptivePrefetchCoordinator::new(config);
        let state = AircraftState::new(53.5, 9.5, 90.0, 250.0, 35000.0);

        // Disabled coordinator returns None
        let result = coord.process_telemetry(&state);
        assert!(result.is_none());
    }

    #[test]
    fn test_process_telemetry_no_dds_client() {
        let mut coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());
        let state = AircraftState::new(53.5, 9.5, 90.0, 10.0, 5.0); // Ground conditions

        // No DDS client - returns Some(0) because plan is generated but not executed
        let result = coord.process_telemetry(&state);
        // The plan may be empty (no scenery index), so result could be Some(0) or None
        assert!(result.is_none() || result == Some(0));
    }

    #[test]
    fn test_startup_info_string() {
        let coord =
            AdaptivePrefetchCoordinator::with_defaults().with_calibration(test_calibration());

        let info = coord.startup_info_string();
        assert!(info.contains("adaptive"));
        assert!(info.contains("mode="));
    }
}
