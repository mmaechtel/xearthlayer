//! SHA-256 checksum calculation for file verification.
//!
//! This module provides utilities for calculating and verifying file checksums
//! used during package downloads.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::manager::error::{ManagerError, ManagerResult};

/// Buffer size for reading files during checksum calculation (64KB).
const BUFFER_SIZE: usize = 64 * 1024;

/// Calculate SHA-256 checksum of a file.
///
/// # Arguments
///
/// * `path` - Path to the file to checksum
///
/// # Returns
///
/// The lowercase hexadecimal SHA-256 hash of the file contents.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
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

/// Verify that a file matches an expected checksum.
///
/// # Arguments
///
/// * `path` - Path to the file to verify
/// * `expected` - Expected SHA-256 checksum (lowercase hex)
///
/// # Returns
///
/// `Ok(())` if the checksum matches, or an error if it doesn't or if the file cannot be read.
pub fn verify_checksum(path: &Path, expected: &str) -> ManagerResult<()> {
    let actual = calculate_file_checksum(path)?;
    if actual != expected {
        return Err(ManagerError::ChecksumMismatch {
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

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
    fn test_calculate_empty_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("empty.txt");

        File::create(&file_path).unwrap();

        let checksum = calculate_file_checksum(&file_path).unwrap();

        // SHA-256 of empty string
        assert_eq!(
            checksum,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_calculate_nonexistent_file() {
        let result = calculate_file_checksum(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_checksum_match() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let result = verify_checksum(
            &file_path,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_checksum_mismatch() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let result = verify_checksum(&file_path, "wrong_checksum");
        assert!(result.is_err());

        match result {
            Err(ManagerError::ChecksumMismatch { filename, .. }) => {
                assert_eq!(filename, "test.txt");
            }
            _ => panic!("Expected ChecksumMismatch error"),
        }
    }

    #[test]
    fn test_large_file_checksum() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("large.bin");

        // Create a file larger than the buffer size
        let mut file = File::create(&file_path).unwrap();
        let data = vec![0xABu8; 100_000]; // 100KB of 0xAB bytes
        file.write_all(&data).unwrap();

        let checksum = calculate_file_checksum(&file_path).unwrap();

        // Verify checksum is consistent
        let checksum2 = calculate_file_checksum(&file_path).unwrap();
        assert_eq!(checksum, checksum2);
    }
}
