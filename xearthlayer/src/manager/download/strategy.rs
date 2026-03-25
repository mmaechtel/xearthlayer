//! Download strategies for multi-part downloads.
//!
//! This module implements the Strategy pattern for sequential vs parallel
//! downloading of package parts.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::http::HttpDownloader;
use super::progress::{
    DownloadProgress, DownloadProgressCallback, PartState, ProgressCounters, ProgressReporter,
};
use super::semaphore::CountingSemaphore;
use super::state::DownloadState;
use crate::manager::error::ManagerResult;
use crate::manager::traits::{PackageDownloader, ProgressCallback};

/// Strategy for downloading multiple parts.
pub trait DownloadStrategy: Send + Sync {
    /// Execute the download strategy.
    ///
    /// # Arguments
    ///
    /// * `state` - Download state to update
    /// * `on_progress` - Optional progress callback
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an error if downloads failed.
    fn execute(
        &self,
        state: &mut DownloadState,
        on_progress: Option<Arc<DownloadProgressCallback>>,
    ) -> ManagerResult<()>;
}

/// Sequential download strategy.
///
/// Downloads parts one at a time, suitable for low-bandwidth connections
/// or when parallel downloads are not needed.
#[derive(Debug, Default)]
pub struct SequentialStrategy;

impl SequentialStrategy {
    /// Create a new sequential strategy.
    pub fn new() -> Self {
        Self
    }
}

impl DownloadStrategy for SequentialStrategy {
    fn execute(
        &self,
        state: &mut DownloadState,
        on_progress: Option<Arc<DownloadProgressCallback>>,
    ) -> ManagerResult<()> {
        let total_parts = state.total_parts;
        let downloader = HttpDownloader::new();

        for i in 0..total_parts {
            let url = &state.urls[i];
            let checksum = &state.checksums[i];
            let dest = &state.destinations[i];

            // Create per-download progress callback for real-time updates
            let base_bytes = state.bytes_downloaded;
            let completed_parts = state.downloaded_parts;

            let result = if let Some(ref cb) = on_progress {
                let cb = Arc::clone(cb);
                let part_sizes = state.part_sizes.clone();
                let total_bytes_so_far = base_bytes;
                let total_size: Option<u64> = if part_sizes.iter().all(|s| s.is_some()) {
                    Some(part_sizes.iter().map(|s| s.unwrap_or(0)).sum())
                } else {
                    None
                };

                let progress_cb: ProgressCallback =
                    Box::new(move |downloaded: u64, _total: u64| {
                        let parts: Vec<_> = (0..total_parts)
                            .map(|idx| {
                                let (part_state, part_bytes) = if idx < completed_parts {
                                    (PartState::Done, part_sizes[idx].unwrap_or(0))
                                } else if idx == i {
                                    (PartState::Downloading, downloaded)
                                } else {
                                    (PartState::Queued, 0)
                                };
                                super::progress::PartProgress {
                                    index: idx,
                                    filename: format!("part_{}", idx),
                                    bytes_downloaded: part_bytes,
                                    total_bytes: part_sizes[idx],
                                    state: part_state,
                                }
                            })
                            .collect();

                        let total_downloaded = total_bytes_so_far + downloaded;
                        let snapshot = DownloadProgress {
                            parts,
                            total_bytes_downloaded: total_downloaded,
                            total_bytes: total_size,
                        };
                        cb(&snapshot);
                    });
                downloader.download_with_progress(url, dest, Some(checksum), progress_cb)
            } else {
                downloader.download(url, dest, Some(checksum))
            };

            match result {
                Ok(bytes) => {
                    state.record_success(bytes);
                }
                Err(_) => {
                    state.record_failure(i);
                }
            }
        }

        Ok(())
    }
}

/// Parallel download strategy.
///
/// Downloads multiple parts concurrently using a semaphore-based sliding window,
/// with real-time per-part progress reporting and inline retry with exponential backoff.
#[derive(Debug)]
pub struct ParallelStrategy {
    /// Maximum number of concurrent downloads.
    pub concurrency: usize,
    /// Timeout for each download.
    pub timeout: Duration,
}

impl ParallelStrategy {
    /// Create a new parallel strategy.
    ///
    /// # Arguments
    ///
    /// * `concurrency` - Maximum number of concurrent downloads (minimum 1)
    /// * `timeout` - Timeout for each download
    pub fn new(concurrency: usize, timeout: Duration) -> Self {
        Self {
            concurrency: concurrency.max(1),
            timeout,
        }
    }
}

impl Default for ParallelStrategy {
    fn default() -> Self {
        Self::new(4, Duration::from_secs(300))
    }
}

/// Maximum number of retry attempts per part.
const MAX_RETRIES: u8 = 3;

/// Base delay for exponential backoff (2 seconds).
const BASE_RETRY_DELAY: Duration = Duration::from_secs(2);

impl DownloadStrategy for ParallelStrategy {
    fn execute(
        &self,
        state: &mut DownloadState,
        on_progress: Option<Arc<DownloadProgressCallback>>,
    ) -> ManagerResult<()> {
        let total_parts = state.total_parts;

        // Build filenames from destination paths
        let filenames: Vec<String> = state
            .destinations
            .iter()
            .map(|d| {
                d.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("part_{}", 0))
            })
            .collect();

        // Create shared progress counters with extended metadata
        let counters = Arc::new(ProgressCounters::new_extended(
            total_parts,
            filenames,
            state.part_sizes.clone(),
        ));

        // Start progress reporter if callback provided
        let _reporter = on_progress
            .as_ref()
            .map(|cb| ProgressReporter::start_detailed(Arc::clone(&counters), Arc::clone(cb)));

        // Create semaphore for concurrency limiting
        let semaphore = Arc::new(CountingSemaphore::new(self.concurrency));

        // Spawn all threads (semaphore gates actual concurrency)
        let mut handles = Vec::with_capacity(total_parts);

        for i in 0..total_parts {
            let url = state.urls[i].clone();
            let checksum = state.checksums[i].clone();
            let dest = state.destinations[i].clone();
            let counters = Arc::clone(&counters);
            let semaphore = Arc::clone(&semaphore);
            let timeout = self.timeout;

            let handle = thread::spawn(move || {
                // Acquire permit — blocks until a slot is available
                let _permit = semaphore.acquire();

                // Mark as downloading
                counters.set_part_state(i, 1); // Downloading
                counters.set_part_attempt(i, 1);

                let mut last_error = String::new();

                for attempt in 1..=MAX_RETRIES {
                    if attempt > 1 {
                        // Mark as retrying
                        counters.set_part_state(i, 4); // Retrying
                        counters.set_part_attempt(i, attempt);

                        // Reset byte counter for retry
                        counters.update_part(i, 0);

                        // Exponential backoff: 2s, 4s, 8s
                        let delay = BASE_RETRY_DELAY * 2u32.pow(attempt as u32 - 2);
                        thread::sleep(delay);

                        // Back to downloading
                        counters.set_part_state(i, 1); // Downloading
                    }

                    let downloader = HttpDownloader::with_timeout(timeout);

                    // Create per-download progress callback
                    let counters_clone = Arc::clone(&counters);
                    let part_index = i;
                    let progress_cb: ProgressCallback =
                        Box::new(move |downloaded: u64, _total: u64| {
                            counters_clone.update_part(part_index, downloaded);
                        });

                    let result = downloader.download_with_progress(
                        &url,
                        &dest,
                        Some(checksum.as_str()),
                        progress_cb,
                    );

                    match result {
                        Ok(bytes) => {
                            counters.mark_completed(i, bytes);
                            counters.set_part_state(i, 2); // Done
                            return Some(bytes);
                        }
                        Err(e) => {
                            last_error = e.to_string();
                            // Remove partial file for clean retry
                            std::fs::remove_file(&dest).ok();
                        }
                    }
                }

                // All retries exhausted
                counters.set_part_state(i, 3); // Failed
                counters.set_part_attempt(i, MAX_RETRIES);
                counters.set_part_error(i, last_error);
                None
            });

            handles.push((i, handle));
        }

        // Collect results
        let mut failed_parts = Vec::new();
        let mut total_bytes = 0u64;
        let mut completed = 0usize;

        for (index, handle) in handles {
            match handle.join() {
                Ok(Some(bytes)) => {
                    total_bytes += bytes;
                    completed += 1;
                }
                Ok(None) => {
                    failed_parts.push(index);
                }
                Err(_) => {
                    // Thread panicked
                    failed_parts.push(index);
                }
            }
        }

        // Signal done and let reporter emit final snapshot
        counters.signal_done();

        // Update state with results
        state.downloaded_parts = completed;
        state.bytes_downloaded = total_bytes;
        state.failed = failed_parts;

        // Reporter will be dropped here, which stops it cleanly

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_strategy_new() {
        let strategy = SequentialStrategy::new();
        // Just verify it creates without panic
        let _ = strategy;
    }

    #[test]
    fn test_parallel_strategy_new() {
        let strategy = ParallelStrategy::new(8, Duration::from_secs(60));
        assert_eq!(strategy.concurrency, 8);
        assert_eq!(strategy.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_parallel_strategy_min_concurrency() {
        let strategy = ParallelStrategy::new(0, Duration::from_secs(60));
        assert_eq!(strategy.concurrency, 1);
    }

    #[test]
    fn test_parallel_strategy_default() {
        let strategy = ParallelStrategy::default();
        assert_eq!(strategy.concurrency, 4);
        assert_eq!(strategy.timeout, Duration::from_secs(300));
    }
}
