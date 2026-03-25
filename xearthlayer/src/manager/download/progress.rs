//! Progress reporting for multi-part downloads.
//!
//! This module provides real-time progress aggregation for parallel downloads,
//! using atomic counters and a dedicated reporter thread.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// State of an individual download part.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum PartState {
    /// Waiting to start.
    #[default]
    Queued,
    /// Actively downloading.
    Downloading,
    /// Download complete, checksum verified.
    Done,
    /// Download failed.
    Failed { reason: String, attempt: u8 },
    /// Retrying after failure.
    Retrying { attempt: u8 },
}

/// Progress snapshot for a single download part.
#[derive(Debug, Clone)]
pub struct PartProgress {
    /// Zero-based index in the parts list.
    pub index: usize,
    /// Filename of this part.
    pub filename: String,
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes for this part (None if unknown).
    pub total_bytes: Option<u64>,
    /// Current state of this part.
    pub state: PartState,
}

/// Aggregate download progress snapshot with per-part detail.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Per-part progress, ordered by part index.
    pub parts: Vec<PartProgress>,
    /// Total bytes downloaded across all parts.
    pub total_bytes_downloaded: u64,
    /// Total expected bytes (None if any part size is unknown).
    pub total_bytes: Option<u64>,
}

/// Callback invoked with download progress snapshots.
pub type DownloadProgressCallback = Box<dyn Fn(&DownloadProgress) + Send + Sync>;

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
    // --- Extended fields for per-part state tracking ---
    /// Per-part total bytes (0 = unknown). Populated from HEAD requests.
    part_totals: Vec<AtomicU64>,
    /// Per-part state (0=Queued, 1=Downloading, 2=Done, 3=Failed, 4=Retrying).
    part_states: Vec<AtomicU8>,
    /// Per-part retry attempt count.
    part_attempts: Vec<AtomicU8>,
    /// Per-part error reasons (written on failure only).
    part_errors: Vec<Mutex<Option<String>>>,
    /// Part filenames (immutable after construction).
    filenames: Vec<String>,
}

impl ProgressCounters {
    /// Create new progress counters for the given number of parts.
    #[cfg(test)]
    pub fn new(num_parts: usize) -> Self {
        Self {
            part_progress: Arc::new((0..num_parts).map(|_| AtomicU64::new(0)).collect()),
            parts_completed: Arc::new(AtomicUsize::new(0)),
            done: Arc::new(AtomicBool::new(false)),
            part_totals: (0..num_parts).map(|_| AtomicU64::new(0)).collect(),
            part_states: (0..num_parts).map(|_| AtomicU8::new(0)).collect(),
            part_attempts: (0..num_parts).map(|_| AtomicU8::new(0)).collect(),
            part_errors: (0..num_parts).map(|_| Mutex::new(None)).collect(),
            filenames: (0..num_parts).map(|i| format!("part_{}", i)).collect(),
        }
    }

    /// Create new progress counters with extended per-part metadata.
    pub fn new_extended(
        num_parts: usize,
        filenames: Vec<String>,
        totals: Vec<Option<u64>>,
    ) -> Self {
        Self {
            part_progress: Arc::new((0..num_parts).map(|_| AtomicU64::new(0)).collect()),
            parts_completed: Arc::new(AtomicUsize::new(0)),
            done: Arc::new(AtomicBool::new(false)),
            part_totals: totals
                .iter()
                .map(|t| AtomicU64::new(t.unwrap_or(0)))
                .collect(),
            part_states: (0..num_parts).map(|_| AtomicU8::new(0)).collect(),
            part_attempts: (0..num_parts).map(|_| AtomicU8::new(0)).collect(),
            part_errors: (0..num_parts).map(|_| Mutex::new(None)).collect(),
            filenames,
        }
    }

    /// Get the total bytes downloaded across all parts.
    #[cfg(test)]
    pub fn total_bytes(&self) -> u64 {
        self.part_progress
            .iter()
            .map(|p| p.load(Ordering::SeqCst))
            .sum()
    }

    /// Get the number of completed parts.
    #[cfg(test)]
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

    // --- Extended state tracking methods ---

    /// Set the state of a part (0=Queued, 1=Downloading, 2=Done, 3=Failed, 4=Retrying).
    pub fn set_part_state(&self, index: usize, state: u8) {
        if index < self.part_states.len() {
            self.part_states[index].store(state, Ordering::SeqCst);
        }
    }

    /// Get the state of a part.
    #[cfg(test)]
    pub fn part_state(&self, index: usize) -> u8 {
        if index < self.part_states.len() {
            self.part_states[index].load(Ordering::SeqCst)
        } else {
            0
        }
    }

    /// Set the error reason for a failed part.
    pub fn set_part_error(&self, index: usize, reason: String) {
        if index < self.part_errors.len() {
            *self.part_errors[index].lock().unwrap() = Some(reason);
        }
    }

    /// Get the error reason for a part (if any).
    #[cfg(test)]
    pub fn part_error(&self, index: usize) -> Option<String> {
        if index < self.part_errors.len() {
            self.part_errors[index].lock().unwrap().clone()
        } else {
            None
        }
    }

    /// Set the retry attempt count for a part.
    pub fn set_part_attempt(&self, index: usize, attempt: u8) {
        if index < self.part_attempts.len() {
            self.part_attempts[index].store(attempt, Ordering::SeqCst);
        }
    }

    /// Build a `DownloadProgress` snapshot from the current counter state.
    pub fn build_snapshot(&self) -> DownloadProgress {
        let num_parts = self.part_progress.len();
        let mut parts = Vec::with_capacity(num_parts);
        let mut total_downloaded: u64 = 0;
        let mut all_totals_known = true;
        let mut total_expected: u64 = 0;

        for i in 0..num_parts {
            let bytes = self.part_progress[i].load(Ordering::SeqCst);
            let part_total = self.part_totals[i].load(Ordering::SeqCst);
            let state_u8 = self.part_states[i].load(Ordering::SeqCst);
            let attempt = self.part_attempts[i].load(Ordering::SeqCst);

            let total_bytes = if part_total > 0 {
                total_expected += part_total;
                Some(part_total)
            } else {
                all_totals_known = false;
                None
            };

            let state = match state_u8 {
                1 => PartState::Downloading,
                2 => PartState::Done,
                3 => {
                    let reason = self.part_errors[i]
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_default();
                    PartState::Failed { reason, attempt }
                }
                4 => PartState::Retrying { attempt },
                _ => PartState::Queued,
            };

            total_downloaded += bytes;
            parts.push(PartProgress {
                index: i,
                filename: self.filenames[i].clone(),
                bytes_downloaded: bytes,
                total_bytes,
                state,
            });
        }

        DownloadProgress {
            parts,
            total_bytes_downloaded: total_downloaded,
            total_bytes: if all_totals_known && total_expected > 0 {
                Some(total_expected)
            } else {
                None
            },
        }
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
    /// Start a reporter that invokes the `DownloadProgressCallback` with per-part snapshots.
    pub fn start_detailed(
        counters: Arc<ProgressCounters>,
        callback: Arc<DownloadProgressCallback>,
    ) -> Self {
        let counters_clone = Arc::clone(&counters);

        let handle = thread::spawn(move || {
            while !counters_clone.is_done() {
                let snapshot = counters_clone.build_snapshot();
                callback(&snapshot);
                thread::sleep(Duration::from_millis(100));
            }
            // Final snapshot
            let snapshot = counters_clone.build_snapshot();
            callback(&snapshot);
        });

        Self {
            handle: Some(handle),
            counters,
        }
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
    fn test_counters_track_per_part_state() {
        let counters = ProgressCounters::new_extended(
            3,
            vec!["a.aa".into(), "a.ab".into(), "a.ac".into()],
            vec![Some(100), Some(200), None],
        );
        assert_eq!(counters.part_state(0), 0); // Queued
        counters.set_part_state(0, 1); // Downloading
        assert_eq!(counters.part_state(0), 1);
        counters.set_part_state(0, 2); // Done
        assert_eq!(counters.part_state(0), 2);
    }

    #[test]
    fn test_counters_store_error_reason() {
        let counters = ProgressCounters::new_extended(
            2,
            vec!["a.aa".into(), "a.ab".into()],
            vec![Some(100), Some(200)],
        );
        counters.set_part_error(0, "connection timeout".to_string());
        assert_eq!(
            counters.part_error(0),
            Some("connection timeout".to_string())
        );
        assert_eq!(counters.part_error(1), None);
    }

    #[test]
    fn test_counters_build_snapshot() {
        let counters = ProgressCounters::new_extended(
            2,
            vec!["a.aa".into(), "a.ab".into()],
            vec![Some(100), Some(200)],
        );
        counters.set_part_state(0, 1); // Downloading
        counters.update_part(0, 50);
        let snapshot = counters.build_snapshot();
        assert_eq!(snapshot.parts.len(), 2);
        assert_eq!(snapshot.parts[0].bytes_downloaded, 50);
        assert_eq!(snapshot.parts[0].total_bytes, Some(100));
        assert_eq!(snapshot.parts[0].state, PartState::Downloading);
        assert_eq!(snapshot.parts[1].state, PartState::Queued);
        assert_eq!(snapshot.total_bytes_downloaded, 50);
        assert_eq!(snapshot.total_bytes, Some(300));
    }

    #[test]
    fn test_counters_build_snapshot_with_failed_state() {
        let counters = ProgressCounters::new_extended(
            2,
            vec!["a.aa".into(), "a.ab".into()],
            vec![Some(100), Some(200)],
        );
        counters.set_part_state(1, 3); // Failed
        counters.set_part_error(1, "connection timeout".to_string());
        let snapshot = counters.build_snapshot();
        match &snapshot.parts[1].state {
            PartState::Failed { reason, .. } => assert_eq!(reason, "connection timeout"),
            other => panic!("Expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn test_counters_build_snapshot_unknown_totals() {
        let counters = ProgressCounters::new_extended(
            2,
            vec!["a.aa".into(), "a.ab".into()],
            vec![Some(100), None], // Second part unknown
        );
        let snapshot = counters.build_snapshot();
        assert!(snapshot.total_bytes.is_none());
    }

    #[test]
    fn test_part_state_default_is_queued() {
        let state = PartState::default();
        assert_eq!(state, PartState::Queued);
    }

    #[test]
    fn test_download_progress_total_bytes_none_when_any_unknown() {
        let progress = DownloadProgress {
            parts: vec![
                PartProgress {
                    index: 0,
                    filename: "part.aa".to_string(),
                    bytes_downloaded: 100,
                    total_bytes: Some(200),
                    state: PartState::Downloading,
                },
                PartProgress {
                    index: 1,
                    filename: "part.ab".to_string(),
                    bytes_downloaded: 0,
                    total_bytes: None,
                    state: PartState::Queued,
                },
            ],
            total_bytes_downloaded: 100,
            total_bytes: None,
        };
        assert!(progress.total_bytes.is_none());
        assert_eq!(progress.total_bytes_downloaded, 100);
    }

    #[test]
    fn test_progress_reporter_lifecycle() {
        use std::sync::atomic::AtomicUsize;

        let counters = Arc::new(ProgressCounters::new_extended(
            2,
            vec!["a.aa".into(), "a.ab".into()],
            vec![Some(500), Some(500)],
        ));
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let callback: DownloadProgressCallback = Box::new(move |_progress: &DownloadProgress| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let reporter = ProgressReporter::start_detailed(Arc::clone(&counters), Arc::new(callback));

        // Let it run a bit
        thread::sleep(Duration::from_millis(50));

        // Stop and check that callback was invoked
        reporter.stop();

        assert!(call_count.load(Ordering::SeqCst) > 0);
    }
}
