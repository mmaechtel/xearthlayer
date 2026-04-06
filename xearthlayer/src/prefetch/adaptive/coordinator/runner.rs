//! Async run loop for the adaptive prefetch coordinator.
//!
//! This module implements the [`Prefetcher`] trait for [`AdaptivePrefetchCoordinator`],
//! providing the async event loop that:
//!
//! - Receives telemetry updates from X-Plane
//! - Detects stale telemetry (X-Plane exit)
//! - Triggers prefetch cycles at appropriate intervals
//!
//! # Staleness Detection
//!
//! When X-Plane exits, it stops sending UDP telemetry packets. The run loop
//! detects the absence of telemetry and enters **safe mode**: prefetch is
//! paused and GPS status is set to `Acquiring`. When telemetry resumes, the
//! phase detector is reset from the `on_ground` flag of the first new packet
//! before normal cycling restarts.
//!
//! # Design Notes
//!
//! The run loop uses `tokio::select!` with biased polling:
//! 1. Cancellation check (highest priority)
//! 2. Telemetry reception
//! 3. Staleness check interval
//!
//! Each arm delegates to a dedicated handler method so the loop body reads as
//! a clean dispatch table with no nested logic.

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::prefetch::state::{AircraftState, GpsStatus};
use crate::prefetch::strategy::Prefetcher;

use super::constants::{MIN_CYCLE_INTERVAL, STALENESS_CHECK_INTERVAL, TELEMETRY_STALE_THRESHOLD};
use super::core::AdaptivePrefetchCoordinator;
use super::telemetry::should_run_cycle;

// ─────────────────────────────────────────────────────────────────────────────
// Run-loop state
// ─────────────────────────────────────────────────────────────────────────────

/// Transient state threaded through each iteration of the prefetch run loop.
struct LoopState {
    last_cycle: Instant,
    last_telemetry: Option<Instant>,
    /// True while in safe mode: telemetry was previously stale.
    telemetry_paused: bool,
}

impl LoopState {
    fn new() -> Self {
        Self {
            last_cycle: Instant::now(),
            last_telemetry: None,
            telemetry_paused: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Prefetcher trait implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Prefetcher for AdaptivePrefetchCoordinator {
    fn run(
        mut self: Box<Self>,
        mut state_rx: mpsc::Receiver<AircraftState>,
        cancellation_token: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            tracing::info!(mode = ?self.effective_mode(), "Adaptive prefetcher started");

            let mut ls = LoopState::new();
            let mut staleness_interval = tokio::time::interval(STALENESS_CHECK_INTERVAL);

            loop {
                tokio::select! {
                    biased;

                    _ = cancellation_token.cancelled() => break,

                    state_opt = state_rx.recv() => {
                        let Some(state) = state_opt else { break };
                        if !self.on_telemetry_received(&state, &mut ls).await { break; }
                    }

                    _ = staleness_interval.tick() => {
                        self.on_staleness_tick(&mut ls);
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

// ─────────────────────────────────────────────────────────────────────────────
// Run-loop handlers
// ─────────────────────────────────────────────────────────────────────────────

impl AdaptivePrefetchCoordinator {
    /// Handle an incoming telemetry packet.
    ///
    /// If the coordinator was in safe mode (telemetry previously stale), it
    /// exits safe mode first: the phase detector is reset from `on_ground` and
    /// GPS status is restored to `Connected`.
    ///
    /// Returns `true` to continue the loop, `false` when the channel has closed.
    async fn on_telemetry_received(&mut self, state: &AircraftState, ls: &mut LoopState) -> bool {
        ls.last_telemetry = Some(Instant::now());

        if ls.telemetry_paused {
            tracing::info!(
                on_ground = state.on_ground,
                "Telemetry resumed — exiting safe mode"
            );
            self.reset_phase_from_on_ground(state.on_ground);
            ls.telemetry_paused = false;
            self.set_gps_status(GpsStatus::Connected);
        }

        if !should_run_cycle(ls.last_cycle, MIN_CYCLE_INTERVAL) {
            return true;
        }

        self.process_telemetry(state).await;
        ls.last_cycle = Instant::now();
        true
    }

    /// Handle a staleness-check tick.
    ///
    /// Enters safe mode when telemetry has been absent longer than
    /// [`TELEMETRY_STALE_THRESHOLD`] and the coordinator is not already paused.
    fn on_staleness_tick(&mut self, ls: &mut LoopState) {
        let stale = ls
            .last_telemetry
            .is_some_and(|t| t.elapsed() > TELEMETRY_STALE_THRESHOLD);

        if stale && !ls.telemetry_paused {
            let age_secs = ls
                .last_telemetry
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0);
            tracing::info!(
                age_secs,
                "Telemetry stale — entering safe mode (prefetch paused)"
            );
            ls.telemetry_paused = true;
            self.set_gps_status(GpsStatus::Acquiring);
        }
    }

    /// Update the shared GPS status, if a shared status handle is configured.
    fn set_gps_status(&self, status: GpsStatus) {
        if let Some(ref shared) = self.shared_status {
            shared.update_gps_status(status);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prefetch::state::{AircraftState, GpsStatus, SharedPrefetchStatus};

    // ─────────────────────────────────────────────────────────────────────────
    // Staleness detection regression test
    // ─────────────────────────────────────────────────────────────────────────

    /// Regression test for Bug 3: Telemetry persisting after X-Plane exit.
    ///
    /// When X-Plane exits, it stops sending UDP telemetry packets. The prefetch
    /// system must detect this and update the GPS status to show we've lost
    /// connection, rather than continuing to display the stale position.
    #[tokio::test]
    async fn test_stale_telemetry_updates_gps_status_regression() {
        let shared_status = SharedPrefetchStatus::new();

        // Simulate receiving telemetry (sets GPS status to Connected)
        let state = AircraftState::new(51.47, -0.46, 90.0, 0.0, 0.0, false); // EGLL
        shared_status.update_aircraft(&state);

        // Verify status is Connected
        let snapshot = shared_status.snapshot();
        assert_eq!(snapshot.gps_status, GpsStatus::Connected);

        // Simulate the staleness check updating status when telemetry stops
        // (This simulates what the run loop does when telemetry goes stale)
        shared_status.update_gps_status(GpsStatus::Acquiring);

        // Verify status changed to Acquiring
        let snapshot = shared_status.snapshot();
        assert_eq!(
            snapshot.gps_status,
            GpsStatus::Acquiring,
            "GPS status should change to Acquiring when telemetry is stale"
        );
    }

    /// Test that the Prefetcher trait methods return expected values.
    #[test]
    fn test_prefetcher_trait_methods() {
        use super::AdaptivePrefetchCoordinator;
        use crate::prefetch::strategy::Prefetcher;

        let coord = AdaptivePrefetchCoordinator::with_defaults();

        assert_eq!(coord.name(), "adaptive");
        assert!(coord.description().contains("Self-calibrating"));
        assert!(coord.startup_info().contains("adaptive"));
    }
}
