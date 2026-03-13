//! Prefetch plan execution with backpressure and throttle control.
//!
//! Extracted from `core.rs` to isolate the submission logic that
//! manages backpressure thresholds, transition throttle, and
//! channel-full handling.

use crate::coord::TileCoord;
use crate::executor::DdsClient;

use super::super::strategy::PrefetchPlan;
use super::super::transition_throttle::TransitionThrottle;
use super::core::{
    BACKPRESSURE_DEFER_THRESHOLD, BACKPRESSURE_REDUCED_FRACTION, BACKPRESSURE_REDUCE_THRESHOLD,
    MAX_PENDING_TILES,
};

// ─────────────────────────────────────────────────────────────────────────────
// Result type
// ─────────────────────────────────────────────────────────────────────────────

/// Result of a plan execution attempt.
pub(crate) struct ExecutionResult {
    /// Number of tiles successfully submitted.
    pub submitted: usize,
    /// Tiles that could not be submitted (channel full / throttle overflow).
    pub pending: Vec<TileCoord>,
    /// Whether the entire cycle was deferred due to high load.
    pub deferred: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Execution
// ─────────────────────────────────────────────────────────────────────────────

/// Execute a prefetch plan by submitting tiles to the DDS client.
///
/// Applies backpressure-aware submission based on executor resource utilization:
/// - Load > [`BACKPRESSURE_DEFER_THRESHOLD`]: skips this cycle (deferred)
/// - Load > [`BACKPRESSURE_REDUCE_THRESHOLD`]: submits reduced fraction
/// - Applies transition throttle (takeoff ramp-up)
/// - Stops immediately on `ChannelFull` error
///
/// # Returns
///
/// An [`ExecutionResult`] with submitted count, pending tiles, and defer flag.
pub(crate) fn execute_plan(
    plan: &PrefetchPlan,
    client: &dyn DdsClient,
    transition_throttle: &mut TransitionThrottle,
    cancellation: tokio_util::sync::CancellationToken,
) -> ExecutionResult {
    let _span = tracing::debug_span!(
        target: "profiling",
        "prefetch_execute",
        tile_count = plan.tiles.len(),
        strategy = plan.strategy,
    )
    .entered();

    // Check executor resource utilization before submitting
    let load = client.executor_load();
    if load > BACKPRESSURE_DEFER_THRESHOLD {
        // Store tiles as pending so they're retried when load drops (capped)
        let pending = if plan.tiles.len() > MAX_PENDING_TILES {
            plan.tiles[..MAX_PENDING_TILES].to_vec()
        } else {
            plan.tiles.clone()
        };
        tracing::info!(
            load = format!("{:.1}%", load * 100.0),
            tiles_planned = plan.tiles.len(),
            "Executor backpressure — deferring prefetch cycle, tiles stored as pending"
        );
        return ExecutionResult {
            submitted: 0,
            pending,
            deferred: true,
        };
    }

    // Determine how many tiles to submit based on executor load
    let max_tiles = if load > BACKPRESSURE_REDUCE_THRESHOLD {
        let reduced = ((plan.tiles.len() as f64) * BACKPRESSURE_REDUCED_FRACTION).ceil() as usize;
        tracing::debug!(
            load = format!("{:.1}%", load * 100.0),
            full_plan = plan.tiles.len(),
            reduced_to = reduced,
            "Moderate backpressure — reducing prefetch submission"
        );
        reduced
    } else {
        plan.tiles.len()
    };

    // Apply transition throttle (takeoff ramp-up)
    let max_tiles = if transition_throttle.is_active() {
        let fraction = transition_throttle.fraction();
        if fraction == 0.0 {
            // Store tiles as pending so they're submitted once ramp begins (capped)
            let pending = if plan.tiles.len() > MAX_PENDING_TILES {
                plan.tiles[..MAX_PENDING_TILES].to_vec()
            } else {
                plan.tiles.clone()
            };
            tracing::debug!(
                tiles_deferred = plan.tiles.len(),
                "Transition throttle — grace period, tiles stored as pending"
            );
            return ExecutionResult {
                submitted: 0,
                pending,
                deferred: false,
            };
        }
        let throttled = ((max_tiles as f64) * fraction).ceil() as usize;
        tracing::debug!(
            fraction = format!("{:.0}%", fraction * 100.0),
            full = max_tiles,
            throttled_to = throttled,
            "Transition throttle — ramping up"
        );
        throttled
    } else {
        max_tiles
    };

    // Store tiles beyond the throttle/backpressure cutoff as pending
    let throttle_overflow: Vec<TileCoord> = if max_tiles < plan.tiles.len() {
        plan.tiles[max_tiles..].to_vec()
    } else {
        Vec::new()
    };

    let mut submitted = 0;
    let tiles_to_submit: Vec<TileCoord> = plan.tiles.iter().take(max_tiles).copied().collect();
    let mut channel_remainder = Vec::new();
    for (idx, tile) in tiles_to_submit.iter().enumerate() {
        let request =
            crate::runtime::JobRequest::prefetch_with_cancellation(*tile, cancellation.clone());
        match client.submit(request) {
            Ok(()) => submitted += 1,
            Err(crate::executor::DdsClientError::ChannelFull) => {
                channel_remainder = tiles_to_submit[idx..].to_vec();
                tracing::debug!(
                    submitted,
                    channel_remaining = channel_remainder.len(),
                    "Channel full — storing remainder for next cycle"
                );
                break;
            }
            Err(crate::executor::DdsClientError::ChannelClosed) => {
                tracing::warn!("Executor channel closed — stopping prefetch");
                break;
            }
        }
    }

    // Merge channel remainder + throttle overflow into pending
    let pending = if !channel_remainder.is_empty() || !throttle_overflow.is_empty() {
        let mut combined = channel_remainder;
        combined.extend(throttle_overflow);
        // Apply safety cap to prevent executor flooding
        if combined.len() > MAX_PENDING_TILES {
            tracing::warn!(
                total = combined.len(),
                cap = MAX_PENDING_TILES,
                dropped = combined.len() - MAX_PENDING_TILES,
                "Pending tiles exceed cap — truncating to prevent executor flood"
            );
            combined.truncate(MAX_PENDING_TILES);
        }
        tracing::debug!(
            submitted,
            pending = combined.len(),
            "Storing {} tiles for subsequent cycles",
            combined.len()
        );
        combined
    } else {
        Vec::new()
    };

    if submitted > 0 {
        tracing::info!(
            tiles = submitted,
            strategy = plan.strategy,
            estimated_ms = plan.estimated_completion_ms,
            "Prefetch batch submitted"
        );
    }

    ExecutionResult {
        submitted,
        pending,
        deferred: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefetch::adaptive::coordinator::test_support::{
        test_calibration, test_plan, BackpressureMockClient, CapLimitedDdsClient, HighLoadDdsClient,
    };
    use crate::prefetch::adaptive::transition_throttle::TransitionThrottle;
    use std::sync::Arc;

    fn default_throttle() -> TransitionThrottle {
        TransitionThrottle::new()
    }

    #[test]
    fn test_defers_under_high_backpressure() {
        let client = BackpressureMockClient::new(0.85);
        let mut throttle = default_throttle();
        let plan = test_plan(10);

        let result = execute_plan(
            &plan,
            &client,
            &mut throttle,
            tokio_util::sync::CancellationToken::new(),
        );

        assert_eq!(result.submitted, 0);
        assert!(result.deferred);
        assert_eq!(result.pending.len(), 10);
    }

    #[test]
    fn test_reduces_under_moderate_backpressure() {
        let client = BackpressureMockClient::new(0.6);
        let mut throttle = default_throttle();
        let plan = test_plan(10);

        let result = execute_plan(
            &plan,
            &client,
            &mut throttle,
            tokio_util::sync::CancellationToken::new(),
        );

        assert_eq!(result.submitted, 5);
        assert!(!result.deferred);
    }

    #[test]
    fn test_full_submission_under_low_pressure() {
        let client = BackpressureMockClient::new(0.1);
        let mut throttle = default_throttle();
        let plan = test_plan(10);

        let result = execute_plan(
            &plan,
            &client,
            &mut throttle,
            tokio_util::sync::CancellationToken::new(),
        );

        assert_eq!(result.submitted, 10);
        assert!(result.pending.is_empty());
    }

    #[test]
    fn test_channel_full_stores_remainder() {
        let client = CapLimitedDdsClient::new(5);
        let mut throttle = default_throttle();
        let plan = test_plan(10);

        let result = execute_plan(
            &plan,
            &client,
            &mut throttle,
            tokio_util::sync::CancellationToken::new(),
        );

        assert_eq!(result.submitted, 5);
        assert_eq!(result.pending.len(), 5);
    }

    #[test]
    fn test_pending_capped_at_max() {
        let client = Arc::new(HighLoadDdsClient);
        let mut throttle = default_throttle();

        let tiles: Vec<TileCoord> = (0..5000)
            .map(|i| TileCoord {
                row: 5000 + i,
                col: 8000,
                zoom: 14,
            })
            .collect();
        let cal = test_calibration();
        let plan = PrefetchPlan::with_tiles(tiles, &cal, "boundary", 0, 5000);

        let result = execute_plan(
            &plan,
            client.as_ref(),
            &mut throttle,
            tokio_util::sync::CancellationToken::new(),
        );

        assert_eq!(result.submitted, 0);
        assert!(result.pending.len() <= MAX_PENDING_TILES);
    }
}
