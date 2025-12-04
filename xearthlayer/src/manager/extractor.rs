//! Archive extraction for package installation.
//!
//! This module handles:
//! - Reassembling split archives
//! - Extracting tar.gz archives
//! - Verifying extracted contents

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::error::{ManagerError, ManagerResult};
use super::traits::ArchiveExtractor;

/// Shell-based archive extractor.
///
/// Uses system tools (cat, tar) for archive operations, matching the publisher
/// which uses the same tools for creating archives.
#[derive(Debug, Default)]
pub struct ShellExtractor;

impl ShellExtractor {
    /// Create a new shell-based extractor.
    pub fn new() -> Self {
        Self
    }

    /// Reassemble split archive parts into a single file.
    ///
    /// # Arguments
    ///
    /// * `parts` - Paths to the archive parts in order (e.g., .aa, .ab, .ac)
    /// * `output` - Path for the reassembled archive
    ///
    /// # Returns
    ///
    /// The total size of the reassembled archive in bytes.
    pub fn reassemble(&self, parts: &[PathBuf], output: &Path) -> ManagerResult<u64> {
        if parts.is_empty() {
            return Err(ManagerError::ExtractionFailed {
                path: output.to_path_buf(),
                reason: "No parts provided for reassembly".to_string(),
            });
        }

        // Verify all parts exist
        for part in parts {
            if !part.exists() {
                return Err(ManagerError::ReadFailed {
                    path: part.clone(),
                    source: io::Error::new(io::ErrorKind::NotFound, "Part file not found"),
                });
            }
        }

        // Create parent directory if needed
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| ManagerError::CreateDirFailed {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        // Use cat to reassemble (matching what publisher uses with split)
        let part_strs: Vec<&str> = parts.iter().map(|p| p.to_str().unwrap_or("")).collect();

        // Build cat command: cat part1 part2 part3 > output
        let output_file = File::create(output).map_err(|e| ManagerError::WriteFailed {
            path: output.to_path_buf(),
            source: e,
        })?;

        // We'll manually concatenate since using shell redirection is tricky
        let mut total_size = 0u64;
        let mut writer = io::BufWriter::new(output_file);

        for part_path in parts {
            let file = File::open(part_path).map_err(|e| ManagerError::ReadFailed {
                path: part_path.clone(),
                source: e,
            })?;

            let mut reader = BufReader::new(file);
            let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer

            loop {
                let bytes_read =
                    reader
                        .read(&mut buffer)
                        .map_err(|e| ManagerError::ReadFailed {
                            path: part_path.clone(),
                            source: e,
                        })?;

                if bytes_read == 0 {
                    break;
                }

                writer
                    .write_all(&buffer[..bytes_read])
                    .map_err(|e| ManagerError::WriteFailed {
                        path: output.to_path_buf(),
                        source: e,
                    })?;

                total_size += bytes_read as u64;
            }
        }

        writer.flush().map_err(|e| ManagerError::WriteFailed {
            path: output.to_path_buf(),
            source: e,
        })?;

        // Verify the file was created and report issues with shell commands
        if !output.exists() {
            return Err(ManagerError::ExtractionFailed {
                path: output.to_path_buf(),
                reason: format!(
                    "Failed to reassemble {} parts: {}",
                    parts.len(),
                    part_strs.join(", ")
                ),
            });
        }

        Ok(total_size)
    }

    /// Extract a tar.gz archive to a destination directory.
    fn extract_tar_gz(&self, archive: &Path, dest_dir: &Path) -> ManagerResult<usize> {
        // Create destination directory
        fs::create_dir_all(dest_dir).map_err(|e| ManagerError::CreateDirFailed {
            path: dest_dir.to_path_buf(),
            source: e,
        })?;

        // Use tar to extract
        let output = Command::new("tar")
            .args([
                "-xzf",
                archive.to_str().unwrap_or(""),
                "-C",
                dest_dir.to_str().unwrap_or(""),
            ])
            .output()
            .map_err(|e| ManagerError::ExtractionFailed {
                path: archive.to_path_buf(),
                reason: format!("Failed to run tar: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ManagerError::ExtractionFailed {
                path: archive.to_path_buf(),
                reason: format!("tar extraction failed: {}", stderr.trim()),
            });
        }

        // Count extracted files
        let count = count_files_recursive(dest_dir)?;

        Ok(count)
    }

    /// List contents of a tar.gz archive without extracting.
    fn list_tar_gz(&self, archive: &Path) -> ManagerResult<Vec<String>> {
        let output = Command::new("tar")
            .args(["-tzf", archive.to_str().unwrap_or("")])
            .output()
            .map_err(|e| ManagerError::ExtractionFailed {
                path: archive.to_path_buf(),
                reason: format!("Failed to run tar: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ManagerError::ExtractionFailed {
                path: archive.to_path_buf(),
                reason: format!("tar list failed: {}", stderr.trim()),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();

        Ok(files)
    }
}

impl ArchiveExtractor for ShellExtractor {
    fn extract(&self, archive_path: &Path, dest_dir: &Path) -> ManagerResult<usize> {
        self.extract_tar_gz(archive_path, dest_dir)
    }

    fn list_contents(&self, archive_path: &Path) -> ManagerResult<Vec<String>> {
        self.list_tar_gz(archive_path)
    }
}

/// Count files recursively in a directory.
fn count_files_recursive(dir: &Path) -> ManagerResult<usize> {
    let mut count = 0;

    if !dir.exists() {
        return Ok(0);
    }

    let entries = fs::read_dir(dir).map_err(|e| ManagerError::ReadFailed {
        path: dir.to_path_buf(),
        source: e,
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            count += 1;
        } else if path.is_dir() {
            count += count_files_recursive(&path)?;
        }
    }

    Ok(count)
}

/// Check if required tools are available.
pub fn check_required_tools() -> ManagerResult<()> {
    // Check for tar
    let tar_check = Command::new("tar").arg("--version").output();

    if tar_check.is_err() {
        return Err(ManagerError::ExtractionFailed {
            path: PathBuf::new(),
            reason: "tar command not found. Please install tar.".to_string(),
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
    fn test_shell_extractor_new() {
        let extractor = ShellExtractor::new();
        assert!(format!("{:?}", extractor).contains("ShellExtractor"));
    }

    #[test]
    fn test_reassemble_empty_parts() {
        let temp = TempDir::new().unwrap();
        let extractor = ShellExtractor::new();
        let output = temp.path().join("output.tar.gz");

        let result = extractor.reassemble(&[], &output);
        assert!(result.is_err());
    }

    #[test]
    fn test_reassemble_missing_part() {
        let temp = TempDir::new().unwrap();
        let extractor = ShellExtractor::new();
        let output = temp.path().join("output.tar.gz");
        let missing = temp.path().join("missing.part");

        let result = extractor.reassemble(&[missing], &output);
        assert!(result.is_err());
    }

    #[test]
    fn test_reassemble_single_part() {
        let temp = TempDir::new().unwrap();
        let extractor = ShellExtractor::new();

        // Create a test part file
        let part = temp.path().join("test.part");
        let mut file = File::create(&part).unwrap();
        file.write_all(b"test content").unwrap();

        let output = temp.path().join("output.bin");
        let size = extractor.reassemble(&[part], &output).unwrap();

        assert_eq!(size, 12); // "test content".len()
        assert!(output.exists());

        let content = fs::read_to_string(&output).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_reassemble_multiple_parts() {
        let temp = TempDir::new().unwrap();
        let extractor = ShellExtractor::new();

        // Create test part files
        let part1 = temp.path().join("test.aa");
        let part2 = temp.path().join("test.ab");
        let part3 = temp.path().join("test.ac");

        fs::write(&part1, b"Hello").unwrap();
        fs::write(&part2, b" ").unwrap();
        fs::write(&part3, b"World").unwrap();

        let output = temp.path().join("output.bin");
        let size = extractor
            .reassemble(&[part1, part2, part3], &output)
            .unwrap();

        assert_eq!(size, 11); // "Hello World".len()

        let content = fs::read_to_string(&output).unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn test_check_required_tools() {
        // tar should be available on most systems
        let result = check_required_tools();
        assert!(result.is_ok());
    }

    #[test]
    fn test_count_files_recursive() {
        let temp = TempDir::new().unwrap();

        // Create some files
        fs::write(temp.path().join("file1.txt"), "a").unwrap();
        fs::write(temp.path().join("file2.txt"), "b").unwrap();

        let subdir = temp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file3.txt"), "c").unwrap();

        let count = count_files_recursive(temp.path()).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count_files_empty_dir() {
        let temp = TempDir::new().unwrap();
        let count = count_files_recursive(temp.path()).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_files_nonexistent_dir() {
        let count = count_files_recursive(Path::new("/nonexistent/path")).unwrap();
        assert_eq!(count, 0);
    }
}
