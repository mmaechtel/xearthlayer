//! Download strategies for multi-part downloads.
//!
//! This module implements the Strategy pattern for sequential vs parallel
//! downloading of package parts.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::http::HttpDownloader;
use super::progress::{MultiPartProgressCallback, ProgressCounters, ProgressReporter};
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
    /// * `downloader` - HTTP downloader to use
    /// * `on_progress` - Optional progress callback
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an error if downloads failed.
    fn execute(
        &self,
        state: &mut DownloadState,
        downloader: &HttpDownloader,
        on_progress: Option<Arc<MultiPartProgressCallback>>,
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
        downloader: &HttpDownloader,
        on_progress: Option<Arc<MultiPartProgressCallback>>,
    ) -> ManagerResult<()> {
        let total_size = state.total_size;

        for i in 0..state.total_parts {
            let url = &state.urls[i];
            let checksum = &state.checksums[i];
            let dest = &state.destinations[i];

            // Create per-download progress callback for real-time updates
            let base_bytes = state.bytes_downloaded;
            let completed_parts = state.downloaded_parts;
            let total_parts = state.total_parts;

            let result = if let Some(ref cb) = on_progress {
                let cb = Arc::clone(cb);
                let progress_cb: ProgressCallback =
                    Box::new(move |downloaded: u64, _total: u64| {
                        cb(
                            base_bytes + downloaded,
                            total_size,
                            completed_parts,
                            total_parts,
                        );
                    });
                downloader.download_with_progress(url, dest, Some(checksum), progress_cb)
            } else {
                downloader.download(url, dest, Some(checksum))
            };

            match result {
                Ok(bytes) => {
                    state.record_success(bytes);
                    if let Some(ref cb) = on_progress {
                        cb(
                            state.bytes_downloaded,
                            total_size,
                            state.downloaded_parts,
                            state.total_parts,
                        );
                    }
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
/// Downloads multiple parts concurrently using a thread pool,
/// with real-time progress aggregation.
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

impl DownloadStrategy for ParallelStrategy {
    fn execute(
        &self,
        state: &mut DownloadState,
        _downloader: &HttpDownloader,
        on_progress: Option<Arc<MultiPartProgressCallback>>,
    ) -> ManagerResult<()> {
        let total_size = state.total_size;
        let total_parts = state.total_parts;

        // Create shared progress counters
        let counters = Arc::new(ProgressCounters::new(total_parts));
        let failed_parts = Arc::new(std::sync::Mutex::new(Vec::new()));

        // Start progress reporter if callback provided
        let _reporter = on_progress.as_ref().map(|cb| {
            ProgressReporter::start_default(
                Arc::clone(&counters),
                total_size,
                total_parts,
                Arc::clone(cb),
            )
        });

        // Process in batches
        let mut handles = Vec::new();

        for batch_start in (0..total_parts).step_by(self.concurrency) {
            let batch_end = (batch_start + self.concurrency).min(total_parts);

            for i in batch_start..batch_end {
                let url = state.urls[i].clone();
                let checksum = state.checksums[i].clone();
                let dest = state.destinations[i].clone();
                let counters = Arc::clone(&counters);
                let failed_parts = Arc::clone(&failed_parts);
                let timeout = self.timeout;
                let part_index = i;

                let handle = thread::spawn(move || {
                    let downloader = HttpDownloader::with_timeout(timeout);

                    // Create per-download progress callback
                    let counters_clone = Arc::clone(&counters);
                    let progress_cb: ProgressCallback =
                        Box::new(move |downloaded: u64, _total: u64| {
                            counters_clone.update_part(part_index, downloaded);
                        });

                    match downloader.download_with_progress(
                        &url,
                        &dest,
                        Some(&checksum),
                        progress_cb,
                    ) {
                        Ok(bytes) => {
                            counters.mark_completed(part_index, bytes);
                        }
                        Err(_) => {
                            failed_parts.lock().unwrap().push(part_index);
                        }
                    }
                });

                handles.push(handle);
            }

            // Wait for batch to complete
            for handle in handles.drain(..) {
                handle.join().ok();
            }
        }

        // Update state with results
        state.downloaded_parts = counters.completed_parts();
        state.bytes_downloaded = counters.total_bytes();
        state.failed = failed_parts.lock().unwrap().clone();

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
