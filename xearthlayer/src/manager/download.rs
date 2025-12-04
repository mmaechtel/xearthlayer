//! HTTP download manager for package archives.
//!
//! This module provides functionality for downloading package archive parts,
//! including:
//! - HTTP Range request support for resumable downloads
//! - SHA-256 checksum verification
//! - Progress callbacks for UI updates
//! - Parallel download support

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

use super::error::{ManagerError, ManagerResult};
use super::traits::{PackageDownloader, ProgressCallback};

/// Progress callback for multi-part downloads.
/// Arguments: (parts_completed, total_parts, total_bytes_downloaded)
pub type MultiPartProgressCallback = Box<dyn Fn(usize, usize, u64) + Send + Sync>;

/// Default timeout for HTTP requests in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Buffer size for reading/writing during downloads (64KB).
const BUFFER_SIZE: usize = 64 * 1024;

/// HTTP-based package downloader.
///
/// Implements the `PackageDownloader` trait with support for:
/// - Range requests for resuming downloads
/// - SHA-256 checksum verification
/// - Progress reporting
#[derive(Debug)]
pub struct HttpDownloader {
    client: Client,
    timeout: Duration,
}

impl Default for HttpDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpDownloader {
    /// Create a new HTTP downloader with default settings.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    /// Create a new HTTP downloader with custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, timeout }
    }

    /// Download a file with resumption support.
    ///
    /// If the destination file already exists and is a partial download,
    /// this will resume from where it left off (if the server supports Range requests).
    fn download_with_resume(
        &self,
        url: &str,
        dest: &Path,
        expected_checksum: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> ManagerResult<u64> {
        // Check if we can resume a partial download
        let existing_size = if dest.exists() {
            dest.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        // First, make a HEAD request to get the total size
        let head_response =
            self.client
                .head(url)
                .send()
                .map_err(|e| ManagerError::DownloadFailed {
                    url: url.to_string(),
                    reason: e.to_string(),
                })?;

        if !head_response.status().is_success() {
            return Err(ManagerError::DownloadFailed {
                url: url.to_string(),
                reason: format!("HEAD request failed with status {}", head_response.status()),
            });
        }

        let total_size = head_response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Check if we have a complete download already
        if existing_size == total_size && total_size > 0 {
            // Verify checksum if provided
            if let Some(expected) = expected_checksum {
                let actual = calculate_file_checksum(dest)?;
                if actual != expected {
                    // Checksum mismatch - need to re-download
                    fs::remove_file(dest).ok();
                } else {
                    // Already downloaded and verified
                    if let Some(ref cb) = progress {
                        cb(total_size, total_size);
                    }
                    return Ok(total_size);
                }
            } else {
                // No checksum - assume it's good
                if let Some(ref cb) = progress {
                    cb(total_size, total_size);
                }
                return Ok(total_size);
            }
        }

        // Check if server supports Range requests
        let supports_range = head_response
            .headers()
            .get("accept-ranges")
            .map(|v| v.to_str().unwrap_or("") == "bytes")
            .unwrap_or(false);

        // Decide whether to resume or start fresh
        let (start_byte, file) =
            if existing_size > 0 && supports_range && existing_size < total_size {
                // Resume from existing position
                let file = OpenOptions::new().append(true).open(dest).map_err(|e| {
                    ManagerError::WriteFailed {
                        path: dest.to_path_buf(),
                        source: e,
                    }
                })?;
                (existing_size, file)
            } else {
                // Start fresh
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| ManagerError::CreateDirFailed {
                        path: parent.to_path_buf(),
                        source: e,
                    })?;
                }
                let file = File::create(dest).map_err(|e| ManagerError::WriteFailed {
                    path: dest.to_path_buf(),
                    source: e,
                })?;
                (0, file)
            };

        // Make the download request
        let mut request = self.client.get(url);
        if start_byte > 0 {
            request = request.header("Range", format!("bytes={}-", start_byte));
        }

        let mut response = request.send().map_err(|e| {
            if e.is_timeout() {
                ManagerError::Timeout {
                    url: url.to_string(),
                    timeout_secs: self.timeout.as_secs(),
                }
            } else {
                ManagerError::DownloadFailed {
                    url: url.to_string(),
                    reason: e.to_string(),
                }
            }
        })?;

        // Check response status
        let status = response.status();
        if !status.is_success() && status.as_u16() != 206 {
            // 206 = Partial Content (for Range requests)
            return Err(ManagerError::DownloadFailed {
                url: url.to_string(),
                reason: format!("GET request failed with status {}", status),
            });
        }

        // Stream the response to the file
        let mut writer = BufWriter::new(file);
        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut downloaded = start_byte;

        loop {
            let bytes_read =
                response
                    .read(&mut buffer)
                    .map_err(|e| ManagerError::DownloadFailed {
                        url: url.to_string(),
                        reason: format!("Read error: {}", e),
                    })?;

            if bytes_read == 0 {
                break;
            }

            writer
                .write_all(&buffer[..bytes_read])
                .map_err(|e| ManagerError::WriteFailed {
                    path: dest.to_path_buf(),
                    source: e,
                })?;

            downloaded += bytes_read as u64;

            if let Some(ref cb) = progress {
                cb(downloaded, total_size);
            }
        }

        writer.flush().map_err(|e| ManagerError::WriteFailed {
            path: dest.to_path_buf(),
            source: e,
        })?;

        // Verify checksum if provided
        if let Some(expected) = expected_checksum {
            let actual = calculate_file_checksum(dest)?;
            if actual != expected {
                return Err(ManagerError::ChecksumMismatch {
                    filename: dest
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    expected: expected.to_string(),
                    actual,
                });
            }
        }

        Ok(downloaded)
    }
}

impl PackageDownloader for HttpDownloader {
    fn download(
        &self,
        url: &str,
        dest: &Path,
        expected_checksum: Option<&str>,
    ) -> ManagerResult<u64> {
        self.download_with_resume(url, dest, expected_checksum, None)
    }

    fn download_with_progress(
        &self,
        url: &str,
        dest: &Path,
        expected_checksum: Option<&str>,
        on_progress: ProgressCallback,
    ) -> ManagerResult<u64> {
        self.download_with_resume(url, dest, expected_checksum, Some(on_progress))
    }
}

/// Calculate SHA-256 checksum of a file.
pub fn calculate_file_checksum(path: &Path) -> ManagerResult<String> {
    let mut file = File::open(path).map_err(|e| ManagerError::ReadFailed {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| ManagerError::ReadFailed {
                path: path.to_path_buf(),
                source: e,
            })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Download state for tracking multi-part downloads.
#[derive(Debug, Clone)]
pub struct DownloadState {
    /// Total number of parts.
    pub total_parts: usize,
    /// Number of parts downloaded.
    pub downloaded_parts: usize,
    /// Total bytes downloaded.
    pub total_bytes: u64,
    /// List of URLs to download.
    pub urls: Vec<String>,
    /// Corresponding checksums for each URL.
    pub checksums: Vec<String>,
    /// Corresponding destination paths.
    pub destinations: Vec<std::path::PathBuf>,
    /// Parts that failed to download.
    pub failed: Vec<usize>,
}

impl DownloadState {
    /// Create a new download state.
    pub fn new(
        urls: Vec<String>,
        checksums: Vec<String>,
        destinations: Vec<std::path::PathBuf>,
    ) -> Self {
        let total_parts = urls.len();
        Self {
            total_parts,
            downloaded_parts: 0,
            total_bytes: 0,
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

    /// Get the progress as a percentage.
    pub fn progress_percent(&self) -> f64 {
        if self.total_parts == 0 {
            100.0
        } else {
            (self.downloaded_parts as f64 / self.total_parts as f64) * 100.0
        }
    }
}

/// Multi-part downloader for downloading all parts of a package.
#[derive(Debug)]
pub struct MultiPartDownloader {
    downloader: HttpDownloader,
    /// Number of parallel downloads.
    parallel_downloads: usize,
}

impl Default for MultiPartDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiPartDownloader {
    /// Create a new multi-part downloader with default settings.
    pub fn new() -> Self {
        Self {
            downloader: HttpDownloader::new(),
            parallel_downloads: 4,
        }
    }

    /// Create a new multi-part downloader with custom settings.
    pub fn with_settings(timeout: Duration, parallel_downloads: usize) -> Self {
        Self {
            downloader: HttpDownloader::with_timeout(timeout),
            parallel_downloads: parallel_downloads.max(1),
        }
    }

    /// Download all parts of a package.
    ///
    /// Parts are downloaded sequentially or in parallel depending on configuration.
    /// Returns the download state with information about success/failure.
    pub fn download_all(
        &self,
        state: &mut DownloadState,
        on_progress: Option<MultiPartProgressCallback>,
    ) -> ManagerResult<()> {
        use std::thread;

        if self.parallel_downloads <= 1 {
            // Sequential download
            for i in 0..state.total_parts {
                let url = &state.urls[i];
                let checksum = &state.checksums[i];
                let dest = &state.destinations[i];

                match self.downloader.download(url, dest, Some(checksum)) {
                    Ok(bytes) => {
                        state.downloaded_parts += 1;
                        state.total_bytes += bytes;
                        if let Some(ref cb) = on_progress {
                            cb(state.downloaded_parts, state.total_parts, state.total_bytes);
                        }
                    }
                    Err(_) => {
                        state.failed.push(i);
                    }
                }
            }
        } else {
            // Parallel download using thread pool
            let total_downloaded = Arc::new(AtomicU64::new(0));
            let parts_completed = Arc::new(AtomicU64::new(0));
            let failed_parts = Arc::new(std::sync::Mutex::new(Vec::new()));

            // Chunk the work into batches
            let mut handles = Vec::new();
            let batch_size = self.parallel_downloads;

            for batch_start in (0..state.total_parts).step_by(batch_size) {
                let batch_end = (batch_start + batch_size).min(state.total_parts);

                for i in batch_start..batch_end {
                    let url = state.urls[i].clone();
                    let checksum = state.checksums[i].clone();
                    let dest = state.destinations[i].clone();
                    let total_downloaded = Arc::clone(&total_downloaded);
                    let parts_completed = Arc::clone(&parts_completed);
                    let failed_parts = Arc::clone(&failed_parts);
                    let timeout = self.downloader.timeout;

                    let handle = thread::spawn(move || {
                        let downloader = HttpDownloader::with_timeout(timeout);
                        match downloader.download(&url, &dest, Some(&checksum)) {
                            Ok(bytes) => {
                                total_downloaded.fetch_add(bytes, Ordering::SeqCst);
                                parts_completed.fetch_add(1, Ordering::SeqCst);
                            }
                            Err(_) => {
                                failed_parts.lock().unwrap().push(i);
                            }
                        }
                    });

                    handles.push(handle);
                }

                // Wait for this batch to complete
                for handle in handles.drain(..) {
                    handle.join().ok();
                }

                // Report progress after each batch
                if let Some(ref cb) = on_progress {
                    let completed = parts_completed.load(Ordering::SeqCst) as usize;
                    let bytes = total_downloaded.load(Ordering::SeqCst);
                    cb(completed, state.total_parts, bytes);
                }
            }

            // Update state with final results
            state.downloaded_parts = parts_completed.load(Ordering::SeqCst) as usize;
            state.total_bytes = total_downloaded.load(Ordering::SeqCst);
            state.failed = failed_parts.lock().unwrap().clone();
        }

        if state.failed.is_empty() {
            Ok(())
        } else {
            Err(ManagerError::DownloadFailed {
                url: format!("{} parts failed", state.failed.len()),
                reason: format!("Parts {:?} failed to download", state.failed),
            })
        }
    }

    /// Retry failed downloads.
    pub fn retry_failed(&self, state: &mut DownloadState) -> ManagerResult<()> {
        let failed_indices: Vec<usize> = state.failed.drain(..).collect();

        for i in failed_indices {
            let url = &state.urls[i];
            let checksum = &state.checksums[i];
            let dest = &state.destinations[i];

            match self.downloader.download(url, dest, Some(checksum)) {
                Ok(bytes) => {
                    state.downloaded_parts += 1;
                    state.total_bytes += bytes;
                }
                Err(_) => {
                    state.failed.push(i);
                }
            }
        }

        if state.failed.is_empty() {
            Ok(())
        } else {
            Err(ManagerError::DownloadFailed {
                url: format!("{} parts still failed", state.failed.len()),
                reason: format!("Parts {:?} failed to download after retry", state.failed),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_http_downloader_default() {
        let downloader = HttpDownloader::default();
        assert_eq!(downloader.timeout.as_secs(), DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn test_http_downloader_with_timeout() {
        let downloader = HttpDownloader::with_timeout(Duration::from_secs(60));
        assert_eq!(downloader.timeout.as_secs(), 60);
    }

    #[test]
    fn test_calculate_file_checksum() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let checksum = calculate_file_checksum(&file_path).unwrap();

        // SHA-256 of "hello world"
        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

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
    fn test_download_state_progress() {
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
    fn test_multi_part_downloader_default() {
        let downloader = MultiPartDownloader::default();
        assert_eq!(downloader.parallel_downloads, 4);
    }

    #[test]
    fn test_multi_part_downloader_with_settings() {
        let downloader = MultiPartDownloader::with_settings(Duration::from_secs(60), 8);
        assert_eq!(downloader.parallel_downloads, 8);
        assert_eq!(downloader.downloader.timeout.as_secs(), 60);
    }

    #[test]
    fn test_multi_part_downloader_min_parallel() {
        // Should enforce minimum of 1 parallel download
        let downloader = MultiPartDownloader::with_settings(Duration::from_secs(60), 0);
        assert_eq!(downloader.parallel_downloads, 1);
    }

    use std::path::PathBuf;
}
