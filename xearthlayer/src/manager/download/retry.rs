//! Retry decorator for package downloads.
//!
//! Wraps a `PackageDownloader` with automatic retry on failure,
//! using exponential backoff (2s, 4s, 8s). Handles HTTP 416 and
//! checksum mismatch by deleting partial files before retrying.

use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::manager::error::{ManagerError, ManagerResult};
use crate::manager::traits::{PackageDownloader, ProgressCallback};

/// Maximum number of retry attempts per download.
const MAX_RETRIES: u8 = 3;

/// Base delay for exponential backoff in milliseconds (2s, 4s, 8s).
const BASE_DELAY_MS: u64 = 2000;

/// Decorator that retries failed downloads with exponential backoff.
///
/// Wraps any `PackageDownloader` implementation and automatically retries
/// on failure up to `MAX_RETRIES` times. Implements `PackageDownloader`
/// itself so it can be used as a drop-in replacement.
pub struct RetryDownloader<D: PackageDownloader> {
    inner: D,
}

#[allow(dead_code)] // Used by rewritten ParallelStrategy in Task 7
impl<D: PackageDownloader> RetryDownloader<D> {
    /// Create a new retry wrapper around the given downloader.
    pub fn new(inner: D) -> Self {
        Self { inner }
    }

    /// Check if the error warrants deleting the partial file before retrying.
    pub fn should_delete_partial(err: &ManagerError) -> bool {
        matches!(err, ManagerError::ChecksumMismatch { .. })
            || matches!(err, ManagerError::DownloadFailed { reason, .. } if reason.contains("416"))
    }
}

impl<D: PackageDownloader> PackageDownloader for RetryDownloader<D> {
    fn download(
        &self,
        url: &str,
        dest: &Path,
        expected_checksum: Option<&str>,
    ) -> ManagerResult<u64> {
        let mut last_err = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = BASE_DELAY_MS * 2u64.pow(attempt as u32 - 1);
                thread::sleep(Duration::from_millis(delay));
            }
            match self.inner.download(url, dest, expected_checksum) {
                Ok(bytes) => return Ok(bytes),
                Err(e) => {
                    if Self::should_delete_partial(&e) {
                        let _ = std::fs::remove_file(dest);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap())
    }

    fn download_with_progress(
        &self,
        url: &str,
        dest: &Path,
        expected_checksum: Option<&str>,
        _on_progress: ProgressCallback,
    ) -> ManagerResult<u64> {
        // Note: ParallelStrategy manages progress via ProgressCounters atomics,
        // not through this callback. The retry decorator delegates to inner.download()
        // which already supports resume from partial files.
        self.download(url, dest, expected_checksum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    struct FailNTimes {
        fail_count: AtomicUsize,
        fail_until: usize,
    }

    impl FailNTimes {
        fn new(fail_until: usize) -> Self {
            Self {
                fail_count: AtomicUsize::new(0),
                fail_until,
            }
        }
    }

    impl PackageDownloader for FailNTimes {
        fn download(&self, _url: &str, dest: &Path, _checksum: Option<&str>) -> ManagerResult<u64> {
            let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_until {
                Err(ManagerError::DownloadFailed {
                    url: "test".into(),
                    reason: "simulated failure".into(),
                })
            } else {
                std::fs::write(dest, b"ok").unwrap();
                Ok(2)
            }
        }

        fn download_with_progress(
            &self,
            url: &str,
            dest: &Path,
            checksum: Option<&str>,
            _cb: ProgressCallback,
        ) -> ManagerResult<u64> {
            self.download(url, dest, checksum)
        }
    }

    #[test]
    fn test_retry_succeeds_after_failures() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("test.bin");
        let inner = FailNTimes::new(2); // Fails twice, succeeds on 3rd
        let retry = RetryDownloader::new(inner);
        let result = retry.download("http://test", &dest, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
    }

    #[test]
    fn test_retry_exhausted() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("test.bin");
        let inner = FailNTimes::new(10); // Always fails
        let retry = RetryDownloader::new(inner);
        let result = retry.download("http://test", &dest, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_retry_succeeds_first_try() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("test.bin");
        let inner = FailNTimes::new(0); // Succeeds immediately
        let retry = RetryDownloader::new(inner);
        let result = retry.download("http://test", &dest, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_should_delete_partial_on_checksum_mismatch() {
        let err = ManagerError::ChecksumMismatch {
            filename: "test".into(),
            expected: "abc".into(),
            actual: "def".into(),
        };
        assert!(RetryDownloader::<FailNTimes>::should_delete_partial(&err));
    }

    #[test]
    fn test_should_delete_partial_on_416() {
        let err = ManagerError::DownloadFailed {
            url: "test".into(),
            reason: "HTTP 416 Range Not Satisfiable".into(),
        };
        assert!(RetryDownloader::<FailNTimes>::should_delete_partial(&err));
    }

    #[test]
    fn test_should_not_delete_partial_on_timeout() {
        let err = ManagerError::Timeout {
            url: "test".into(),
            timeout_secs: 30,
        };
        assert!(!RetryDownloader::<FailNTimes>::should_delete_partial(&err));
    }
}
