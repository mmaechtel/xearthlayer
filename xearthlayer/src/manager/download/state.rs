//! Download state management for multi-part downloads.
//!
//! This module provides types for tracking the state of multi-part package
//! downloads, including progress, failures, and completion status.

use std::path::PathBuf;

/// Download state for tracking multi-part downloads.
#[derive(Debug, Clone)]
pub struct DownloadState {
    /// Total number of parts.
    pub total_parts: usize,
    /// Number of parts downloaded.
    pub downloaded_parts: usize,
    /// Total bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total expected size of all parts (from HEAD requests).
    pub total_size: u64,
    /// List of URLs to download.
    pub urls: Vec<String>,
    /// Corresponding checksums for each URL.
    pub checksums: Vec<String>,
    /// Corresponding destination paths.
    pub destinations: Vec<PathBuf>,
    /// Parts that failed to download (by index).
    pub failed: Vec<usize>,
}

impl DownloadState {
    /// Create a new download state.
    pub fn new(urls: Vec<String>, checksums: Vec<String>, destinations: Vec<PathBuf>) -> Self {
        let total_parts = urls.len();
        Self {
            total_parts,
            downloaded_parts: 0,
            bytes_downloaded: 0,
            total_size: 0,
            urls,
            checksums,
            destinations,
            failed: Vec::new(),
        }
    }

    /// Check if the download is complete.
    pub fn is_complete(&self) -> bool {
        self.downloaded_parts == self.total_parts && self.failed.is_empty()
    }

    /// Check if any parts failed.
    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    /// Get the number of failed parts.
    pub fn failure_count(&self) -> usize {
        self.failed.len()
    }

    /// Get the progress as a percentage based on bytes.
    ///
    /// Falls back to part-based progress if total size is unknown.
    pub fn progress_percent(&self) -> f64 {
        if self.total_size == 0 {
            // Fall back to part-based progress if total size unknown
            if self.total_parts == 0 {
                100.0
            } else {
                (self.downloaded_parts as f64 / self.total_parts as f64) * 100.0
            }
        } else {
            (self.bytes_downloaded as f64 / self.total_size as f64) * 100.0
        }
    }

    /// Get the progress as a ratio (0.0 to 1.0).
    pub fn progress_ratio(&self) -> f64 {
        self.progress_percent() / 100.0
    }

    /// Record a successful download of a part.
    pub fn record_success(&mut self, bytes: u64) {
        self.downloaded_parts += 1;
        self.bytes_downloaded += bytes;
    }

    /// Record a failed download of a part.
    pub fn record_failure(&mut self, part_index: usize) {
        self.failed.push(part_index);
    }

    /// Get the failed part indices and clear the failure list.
    pub fn take_failures(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_state_new() {
        let state = DownloadState::new(
            vec!["http://a".to_string(), "http://b".to_string()],
            vec!["abc".to_string(), "def".to_string()],
            vec![PathBuf::from("/a"), PathBuf::from("/b")],
        );

        assert_eq!(state.total_parts, 2);
        assert_eq!(state.downloaded_parts, 0);
        assert!(!state.is_complete());
        assert_eq!(state.progress_percent(), 0.0);
    }

    #[test]
    fn test_download_state_is_complete() {
        let mut state = DownloadState::new(
            vec!["http://a".to_string()],
            vec!["abc".to_string()],
            vec![PathBuf::from("/a")],
        );

        assert!(!state.is_complete());

        state.downloaded_parts = 1;
        assert!(state.is_complete());

        state.failed.push(0);
        assert!(!state.is_complete());
    }

    #[test]
    fn test_download_state_progress_by_parts() {
        let mut state = DownloadState::new(
            vec!["http://a".to_string(), "http://b".to_string()],
            vec!["abc".to_string(), "def".to_string()],
            vec![PathBuf::from("/a"), PathBuf::from("/b")],
        );

        assert_eq!(state.progress_percent(), 0.0);

        state.downloaded_parts = 1;
        assert_eq!(state.progress_percent(), 50.0);

        state.downloaded_parts = 2;
        assert_eq!(state.progress_percent(), 100.0);
    }

    #[test]
    fn test_download_state_progress_by_bytes() {
        let mut state = DownloadState::new(
            vec!["http://a".to_string()],
            vec!["abc".to_string()],
            vec![PathBuf::from("/a")],
        );

        state.total_size = 1000;
        state.bytes_downloaded = 500;

        assert_eq!(state.progress_percent(), 50.0);
        assert_eq!(state.progress_ratio(), 0.5);
    }

    #[test]
    fn test_download_state_record_success() {
        let mut state = DownloadState::new(
            vec!["http://a".to_string()],
            vec!["abc".to_string()],
            vec![PathBuf::from("/a")],
        );

        state.record_success(1024);

        assert_eq!(state.downloaded_parts, 1);
        assert_eq!(state.bytes_downloaded, 1024);
    }

    #[test]
    fn test_download_state_record_failure() {
        let mut state = DownloadState::new(
            vec!["http://a".to_string()],
            vec!["abc".to_string()],
            vec![PathBuf::from("/a")],
        );

        state.record_failure(0);

        assert!(state.has_failures());
        assert_eq!(state.failure_count(), 1);
        assert_eq!(state.failed[0], 0);
    }

    #[test]
    fn test_download_state_take_failures() {
        let mut state = DownloadState::new(
            vec!["http://a".to_string()],
            vec!["abc".to_string()],
            vec![PathBuf::from("/a")],
        );

        state.record_failure(0);
        let failures = state.take_failures();

        assert_eq!(failures, vec![0]);
        assert!(state.failed.is_empty());
    }
}
