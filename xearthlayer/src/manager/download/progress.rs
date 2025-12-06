//! Progress reporting for multi-part downloads.
//!
//! This module provides real-time progress aggregation for parallel downloads,
//! using atomic counters and a dedicated reporter thread.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Progress callback for multi-part downloads with real-time byte-level updates.
///
/// # Arguments
///
/// * `bytes_downloaded` - Total bytes downloaded across all parts
/// * `total_bytes` - Total expected bytes (from HEAD requests)
/// * `parts_completed` - Number of parts fully downloaded
/// * `total_parts` - Total number of parts
pub type MultiPartProgressCallback = Box<dyn Fn(u64, u64, usize, usize) + Send + Sync>;

/// Shared progress counters for parallel downloads.
///
/// This struct holds atomic counters that can be safely shared across
/// download threads, allowing real-time progress aggregation.
#[derive(Debug)]
pub struct ProgressCounters {
    /// Per-part progress counters (bytes downloaded for each part).
    pub part_progress: Arc<Vec<AtomicU64>>,
    /// Number of parts fully completed.
    pub parts_completed: Arc<AtomicUsize>,
    /// Signal to stop the reporter thread.
    pub done: Arc<AtomicBool>,
}

impl ProgressCounters {
    /// Create new progress counters for the given number of parts.
    pub fn new(num_parts: usize) -> Self {
        Self {
            part_progress: Arc::new((0..num_parts).map(|_| AtomicU64::new(0)).collect()),
            parts_completed: Arc::new(AtomicUsize::new(0)),
            done: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the total bytes downloaded across all parts.
    pub fn total_bytes(&self) -> u64 {
        self.part_progress
            .iter()
            .map(|p| p.load(Ordering::SeqCst))
            .sum()
    }

    /// Get the number of completed parts.
    pub fn completed_parts(&self) -> usize {
        self.parts_completed.load(Ordering::SeqCst)
    }

    /// Update progress for a specific part.
    pub fn update_part(&self, part_index: usize, bytes: u64) {
        if part_index < self.part_progress.len() {
            self.part_progress[part_index].store(bytes, Ordering::SeqCst);
        }
    }

    /// Mark a part as completed.
    pub fn mark_completed(&self, part_index: usize, final_bytes: u64) {
        if part_index < self.part_progress.len() {
            self.part_progress[part_index].store(final_bytes, Ordering::SeqCst);
            self.parts_completed.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Signal that all downloads are done.
    pub fn signal_done(&self) {
        self.done.store(true, Ordering::SeqCst);
    }

    /// Check if downloads are done.
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

/// Real-time progress reporter for parallel downloads.
///
/// Spawns a background thread that periodically polls progress counters
/// and invokes a callback with aggregated progress information.
pub struct ProgressReporter {
    handle: Option<JoinHandle<()>>,
    counters: Arc<ProgressCounters>,
}

impl ProgressReporter {
    /// Start a new progress reporter.
    ///
    /// # Arguments
    ///
    /// * `counters` - Shared progress counters
    /// * `total_size` - Total expected bytes (for percentage calculation)
    /// * `total_parts` - Total number of parts
    /// * `callback` - Function to call with progress updates
    /// * `poll_interval` - How often to poll for updates
    pub fn start(
        counters: Arc<ProgressCounters>,
        total_size: u64,
        total_parts: usize,
        callback: Arc<MultiPartProgressCallback>,
        poll_interval: Duration,
    ) -> Self {
        let counters_clone = Arc::clone(&counters);

        let handle = thread::spawn(move || {
            while !counters_clone.is_done() {
                let bytes = counters_clone.total_bytes();
                let completed = counters_clone.completed_parts();
                callback(bytes, total_size, completed, total_parts);
                thread::sleep(poll_interval);
            }

            // Final report
            let bytes = counters_clone.total_bytes();
            let completed = counters_clone.completed_parts();
            callback(bytes, total_size, completed, total_parts);
        });

        Self {
            handle: Some(handle),
            counters,
        }
    }

    /// Start a reporter with default 100ms poll interval.
    pub fn start_default(
        counters: Arc<ProgressCounters>,
        total_size: u64,
        total_parts: usize,
        callback: Arc<MultiPartProgressCallback>,
    ) -> Self {
        Self::start(
            counters,
            total_size,
            total_parts,
            callback,
            Duration::from_millis(100),
        )
    }

    /// Stop the reporter and wait for it to finish.
    #[cfg(test)]
    pub fn stop(mut self) {
        self.counters.signal_done();
        if let Some(handle) = self.handle.take() {
            handle.join().ok();
        }
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.counters.signal_done();
        if let Some(handle) = self.handle.take() {
            handle.join().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_counters_new() {
        let counters = ProgressCounters::new(3);
        assert_eq!(counters.part_progress.len(), 3);
        assert_eq!(counters.total_bytes(), 0);
        assert_eq!(counters.completed_parts(), 0);
        assert!(!counters.is_done());
    }

    #[test]
    fn test_progress_counters_update_part() {
        let counters = ProgressCounters::new(2);

        counters.update_part(0, 500);
        counters.update_part(1, 300);

        assert_eq!(counters.total_bytes(), 800);
    }

    #[test]
    fn test_progress_counters_mark_completed() {
        let counters = ProgressCounters::new(2);

        counters.mark_completed(0, 1000);

        assert_eq!(counters.completed_parts(), 1);
        assert_eq!(counters.part_progress[0].load(Ordering::SeqCst), 1000);
    }

    #[test]
    fn test_progress_counters_signal_done() {
        let counters = ProgressCounters::new(1);

        assert!(!counters.is_done());
        counters.signal_done();
        assert!(counters.is_done());
    }

    #[test]
    fn test_progress_reporter_lifecycle() {
        use std::sync::atomic::AtomicUsize;

        let counters = Arc::new(ProgressCounters::new(2));
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let callback: MultiPartProgressCallback =
            Box::new(move |_bytes, _total, _completed, _parts| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
            });

        let reporter = ProgressReporter::start(
            Arc::clone(&counters),
            1000,
            2,
            Arc::new(callback),
            Duration::from_millis(10),
        );

        // Let it run a bit
        thread::sleep(Duration::from_millis(50));

        // Stop and check that callback was invoked
        reporter.stop();

        assert!(call_count.load(Ordering::SeqCst) > 0);
    }
}
