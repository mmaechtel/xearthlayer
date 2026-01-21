//! Tile submission with backpressure and completion tracking.
//!
//! This module provides the `TileSubmitter` which handles the async submission
//! of tiles to the DDS executor with:
//! - Backpressure via bounded channel sends
//! - Concurrent request limiting via sliding window
//! - Completion tracking with FuturesUnordered
//! - Batched progress reporting
//!
//! # Design
//!
//! The submitter maintains a "sliding window" of concurrent requests:
//! 1. Submit initial batch up to MAX_CONCURRENT
//! 2. As each request completes, submit another (if available)
//! 3. Track successes/failures and send batched progress updates
//!
//! This prevents overwhelming the executor while maintaining high throughput.

use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::coord::TileCoord;
use crate::executor::{DdsClient, Priority};
use crate::runtime::{DdsResponse, JobRequest, RequestOrigin};

use super::config::PrewarmProgress;

/// Maximum concurrent tile requests in flight.
const MAX_CONCURRENT: usize = 500;

/// Number of completions before sending a progress update.
const PROGRESS_BATCH_SIZE: usize = 50;

/// Result of a tile submission run.
#[derive(Debug, Clone)]
pub struct SubmissionResult {
    /// Number of tiles that completed successfully.
    pub tiles_completed: usize,
    /// Number of tiles that failed.
    pub tiles_failed: usize,
    /// Whether the submission was cancelled.
    pub was_cancelled: bool,
}

/// Configuration for tile submission.
#[derive(Debug, Clone)]
pub struct SubmitterConfig {
    /// Maximum concurrent requests in flight.
    pub max_concurrent: usize,
    /// Completions before sending progress update.
    pub progress_batch_size: usize,
}

impl Default for SubmitterConfig {
    fn default() -> Self {
        Self {
            max_concurrent: MAX_CONCURRENT,
            progress_batch_size: PROGRESS_BATCH_SIZE,
        }
    }
}

/// Handles tile submission with backpressure and completion tracking.
///
/// The submitter maintains a sliding window of concurrent requests, submitting
/// new tiles as previous ones complete. This provides optimal throughput while
/// respecting executor capacity.
pub struct TileSubmitter {
    dds_client: Arc<dyn DdsClient>,
    config: SubmitterConfig,
}

impl TileSubmitter {
    /// Create a new tile submitter.
    pub fn new(dds_client: Arc<dyn DdsClient>) -> Self {
        Self {
            dds_client,
            config: SubmitterConfig::default(),
        }
    }

    /// Create a new tile submitter with custom configuration.
    pub fn with_config(dds_client: Arc<dyn DdsClient>, config: SubmitterConfig) -> Self {
        Self { dds_client, config }
    }

    /// Submit tiles and track their completion.
    ///
    /// Uses backpressure-based submission: maintains up to `max_concurrent`
    /// requests in flight, submitting new tiles as previous ones complete.
    /// Progress updates are sent in batches for efficiency.
    ///
    /// # Arguments
    ///
    /// * `tiles` - Tiles to submit for generation
    /// * `cache_hits` - Number of tiles that were already cached (for progress reporting)
    /// * `progress_tx` - Channel for progress updates
    /// * `cancellation` - Token for cancellation
    ///
    /// # Returns
    ///
    /// `SubmissionResult` with completion statistics.
    pub async fn submit_and_track(
        &self,
        tiles: Vec<TileCoord>,
        cache_hits: usize,
        progress_tx: mpsc::Sender<PrewarmProgress>,
        cancellation: CancellationToken,
    ) -> SubmissionResult {
        let total_to_generate = tiles.len();

        if tiles.is_empty() {
            return SubmissionResult {
                tiles_completed: 0,
                tiles_failed: 0,
                was_cancelled: false,
            };
        }

        // Try to get async sender for backpressure support
        let sender = match self.dds_client.sender() {
            Some(s) => s,
            None => {
                warn!("DdsClient does not support async submission, using fallback");
                return self
                    .submit_fallback(tiles, cache_hits, progress_tx, cancellation)
                    .await;
            }
        };

        // Backpressure-based submission with sliding window
        let mut pending_futures = FuturesUnordered::new();
        let mut tiles_iter = tiles.into_iter();
        let mut submitted = 0usize;

        // Submit initial batch
        for tile in tiles_iter.by_ref().take(self.config.max_concurrent) {
            let (tx, rx) = oneshot::channel();
            let request = JobRequest {
                tile,
                priority: Priority::PREFETCH,
                cancellation: cancellation.child_token(),
                response_tx: Some(tx),
                origin: RequestOrigin::Prewarm,
            };

            if sender.send(request).await.is_err() {
                warn!("Executor channel closed during submission");
                break;
            }
            pending_futures.push(rx);
            submitted += 1;
        }

        info!(initial_submitted = submitted, "Initial batch submitted");

        // Process completions and submit more
        let mut tiles_completed = 0usize;
        let mut tiles_failed = 0usize;
        let mut pending_completed = 0usize;
        let mut pending_failed = 0usize;

        loop {
            // Check for cancellation
            if cancellation.is_cancelled() {
                let remaining = pending_futures.len() + tiles_iter.len();
                info!(
                    tiles_completed,
                    tiles_remaining = remaining,
                    "Submission cancelled"
                );
                let _ = progress_tx
                    .send(PrewarmProgress::Cancelled {
                        tiles_completed,
                        tiles_pending: remaining,
                    })
                    .await;
                return SubmissionResult {
                    tiles_completed,
                    tiles_failed,
                    was_cancelled: true,
                };
            }

            tokio::select! {
                Some(result) = pending_futures.next() => {
                    match result {
                        Ok(response) if response.is_success() => {
                            tiles_completed += 1;
                            pending_completed += 1;
                        }
                        _ => {
                            tiles_failed += 1;
                            pending_failed += 1;
                        }
                    }

                    // Send batched progress updates
                    if pending_completed + pending_failed >= self.config.progress_batch_size {
                        let _ = progress_tx.try_send(PrewarmProgress::BatchProgress {
                            completed: pending_completed,
                            cached: 0,
                            failed: pending_failed,
                        });
                        pending_completed = 0;
                        pending_failed = 0;
                    }

                    // Submit another tile if available
                    if let Some(tile) = tiles_iter.next() {
                        let (tx, rx) = oneshot::channel();
                        let request = JobRequest {
                            tile,
                            priority: Priority::PREFETCH,
                            cancellation: cancellation.child_token(),
                            response_tx: Some(tx),
                            origin: RequestOrigin::Prewarm,
                        };
                        if sender.send(request).await.is_ok() {
                            pending_futures.push(rx);
                        }
                    }
                }
                else => {
                    // No more pending futures - we're done
                    break;
                }
            }
        }

        // Send any remaining progress
        if pending_completed > 0 || pending_failed > 0 {
            let _ = progress_tx.try_send(PrewarmProgress::BatchProgress {
                completed: pending_completed,
                cached: 0,
                failed: pending_failed,
            });
        }

        info!(
            tiles_completed,
            tiles_failed, cache_hits, total_to_generate, "Submission complete"
        );

        let _ = progress_tx
            .send(PrewarmProgress::Complete {
                tiles_completed,
                cache_hits,
                failed: tiles_failed,
            })
            .await;

        SubmissionResult {
            tiles_completed,
            tiles_failed,
            was_cancelled: false,
        }
    }

    /// Fallback submission for clients without async sender support.
    ///
    /// Submits all tiles at once and waits for completions. This may fail
    /// if the channel is bounded and full.
    async fn submit_fallback(
        &self,
        tiles: Vec<TileCoord>,
        cache_hits: usize,
        progress_tx: mpsc::Sender<PrewarmProgress>,
        cancellation: CancellationToken,
    ) -> SubmissionResult {
        let total_to_generate = tiles.len();

        // Submit all at once via request_dds_with_options
        let pending_futures: FuturesUnordered<_> = tiles
            .iter()
            .map(|tile| {
                self.dds_client.request_dds_with_options(
                    *tile,
                    Priority::PREFETCH,
                    RequestOrigin::Prewarm,
                    cancellation.child_token(),
                )
            })
            .collect();

        self.wait_for_completions(
            pending_futures,
            total_to_generate,
            cache_hits,
            progress_tx,
            cancellation,
        )
        .await
    }

    /// Wait for all pending futures to complete.
    async fn wait_for_completions(
        &self,
        mut pending_futures: FuturesUnordered<oneshot::Receiver<DdsResponse>>,
        total_to_generate: usize,
        cache_hits: usize,
        progress_tx: mpsc::Sender<PrewarmProgress>,
        cancellation: CancellationToken,
    ) -> SubmissionResult {
        let mut tiles_completed = 0usize;
        let mut tiles_failed = 0usize;
        let mut pending_completed = 0usize;
        let mut pending_failed = 0usize;

        while let Some(result) = pending_futures.next().await {
            // Check for cancellation
            if cancellation.is_cancelled() {
                let remaining = pending_futures.len();
                info!(
                    tiles_completed,
                    tiles_remaining = remaining,
                    "Completion wait cancelled"
                );
                let _ = progress_tx
                    .send(PrewarmProgress::Cancelled {
                        tiles_completed,
                        tiles_pending: remaining,
                    })
                    .await;
                return SubmissionResult {
                    tiles_completed,
                    tiles_failed,
                    was_cancelled: true,
                };
            }

            match result {
                Ok(response) if response.is_success() => {
                    tiles_completed += 1;
                    pending_completed += 1;
                }
                _ => {
                    tiles_failed += 1;
                    pending_failed += 1;
                }
            }

            // Send batched progress updates
            if pending_completed + pending_failed >= self.config.progress_batch_size {
                let _ = progress_tx.try_send(PrewarmProgress::BatchProgress {
                    completed: pending_completed,
                    cached: 0,
                    failed: pending_failed,
                });
                pending_completed = 0;
                pending_failed = 0;
            }
        }

        // Send any remaining progress
        if pending_completed > 0 || pending_failed > 0 {
            let _ = progress_tx.try_send(PrewarmProgress::BatchProgress {
                completed: pending_completed,
                cached: 0,
                failed: pending_failed,
            });
        }

        info!(
            tiles_completed,
            tiles_failed, cache_hits, total_to_generate, "Completion wait finished"
        );

        let _ = progress_tx
            .send(PrewarmProgress::Complete {
                tiles_completed,
                cache_hits,
                failed: tiles_failed,
            })
            .await;

        SubmissionResult {
            tiles_completed,
            tiles_failed,
            was_cancelled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SubmitterConfig::default();
        assert_eq!(config.max_concurrent, 500);
        assert_eq!(config.progress_batch_size, 50);
    }

    #[test]
    fn test_submission_result() {
        let result = SubmissionResult {
            tiles_completed: 100,
            tiles_failed: 5,
            was_cancelled: false,
        };
        assert_eq!(result.tiles_completed, 100);
        assert_eq!(result.tiles_failed, 5);
        assert!(!result.was_cancelled);
    }
}
