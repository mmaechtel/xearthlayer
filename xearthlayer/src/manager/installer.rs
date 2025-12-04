//! Package installer for downloading and installing packages.
//!
//! This module orchestrates the full installation workflow:
//! 1. Fetch package metadata from library
//! 2. Download all archive parts
//! 3. Verify checksums
//! 4. Reassemble and extract archive
//! 5. Install to package directory
//! 6. Clean up temporary files

use std::fs;
use std::path::{Path, PathBuf};

use crate::package::{PackageMetadata, PackageType};

use super::download::{DownloadState, MultiPartDownloader};
use super::error::{ManagerError, ManagerResult};
use super::extractor::ShellExtractor;
use super::local::LocalPackageStore;
use super::traits::{ArchiveExtractor, LibraryClient};

/// Progress callback for installation operations.
///
/// # Arguments
///
/// * `stage` - Current installation stage
/// * `progress` - Progress within the stage (0.0 - 1.0)
/// * `message` - Human-readable message
pub type InstallProgressCallback = Box<dyn Fn(InstallStage, f64, &str) + Send + Sync>;

/// Installation stages for progress reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStage {
    /// Fetching package metadata.
    FetchingMetadata,
    /// Downloading archive parts.
    Downloading,
    /// Verifying checksums.
    Verifying,
    /// Reassembling split archive.
    Reassembling,
    /// Extracting archive contents.
    Extracting,
    /// Installing to final location.
    Installing,
    /// Cleaning up temporary files.
    Cleanup,
    /// Installation complete.
    Complete,
}

impl InstallStage {
    /// Get a human-readable name for the stage.
    pub fn name(&self) -> &'static str {
        match self {
            Self::FetchingMetadata => "Fetching metadata",
            Self::Downloading => "Downloading",
            Self::Verifying => "Verifying",
            Self::Reassembling => "Reassembling",
            Self::Extracting => "Extracting",
            Self::Installing => "Installing",
            Self::Cleanup => "Cleaning up",
            Self::Complete => "Complete",
        }
    }
}

/// Result of a package installation.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// Region of the installed package.
    pub region: String,
    /// Type of the installed package.
    pub package_type: PackageType,
    /// Version of the installed package.
    pub version: semver::Version,
    /// Path where the package was installed.
    pub install_path: PathBuf,
    /// Total bytes downloaded.
    pub bytes_downloaded: u64,
    /// Number of files extracted.
    pub files_extracted: usize,
}

/// Package installer.
///
/// Handles the complete installation workflow including downloading,
/// verification, extraction, and installation.
pub struct PackageInstaller<C: LibraryClient> {
    /// Library client for fetching metadata.
    client: C,
    /// Local package store.
    store: LocalPackageStore,
    /// Directory for temporary download files.
    temp_dir: PathBuf,
    /// Number of parallel downloads.
    parallel_downloads: usize,
}

impl<C: LibraryClient> PackageInstaller<C> {
    /// Create a new package installer.
    ///
    /// # Arguments
    ///
    /// * `client` - Library client for fetching metadata
    /// * `store` - Local package store for installation
    /// * `temp_dir` - Directory for temporary files during installation
    pub fn new(client: C, store: LocalPackageStore, temp_dir: impl Into<PathBuf>) -> Self {
        Self {
            client,
            store,
            temp_dir: temp_dir.into(),
            parallel_downloads: 4,
        }
    }

    /// Set the number of parallel downloads.
    pub fn with_parallel_downloads(mut self, count: usize) -> Self {
        self.parallel_downloads = count.max(1);
        self
    }

    /// Get the local package store.
    pub fn store(&self) -> &LocalPackageStore {
        &self.store
    }

    /// Install a package from a metadata URL.
    ///
    /// # Arguments
    ///
    /// * `metadata_url` - URL to the package metadata file
    /// * `on_progress` - Optional progress callback
    ///
    /// # Returns
    ///
    /// Information about the installed package.
    pub fn install_from_url(
        &self,
        metadata_url: &str,
        on_progress: Option<InstallProgressCallback>,
    ) -> ManagerResult<InstallResult> {
        // Report progress helper
        let report = |stage: InstallStage, progress: f64, message: &str| {
            if let Some(ref cb) = on_progress {
                cb(stage, progress, message);
            }
        };

        // Stage 1: Fetch metadata
        report(
            InstallStage::FetchingMetadata,
            0.0,
            "Fetching package metadata...",
        );
        let metadata = self.client.fetch_metadata(metadata_url)?;
        report(InstallStage::FetchingMetadata, 1.0, "Metadata fetched");

        // Install using the fetched metadata
        self.install_from_metadata(&metadata, on_progress)
    }

    /// Install a package from already-fetched metadata.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The package metadata
    /// * `on_progress` - Optional progress callback
    ///
    /// # Returns
    ///
    /// Information about the installed package.
    pub fn install_from_metadata(
        &self,
        metadata: &PackageMetadata,
        on_progress: Option<InstallProgressCallback>,
    ) -> ManagerResult<InstallResult> {
        let region = &metadata.title;
        let package_type = metadata.package_type;
        let version = metadata.package_version.clone();

        // Report progress helper
        let report = |stage: InstallStage, progress: f64, message: &str| {
            if let Some(ref cb) = on_progress {
                cb(stage, progress, message);
            }
        };

        // Check if already installed
        if self.store.is_installed(region, package_type) {
            let existing = self.store.get(region, package_type)?;
            if existing.version() == &version {
                return Err(ManagerError::AlreadyInstalled {
                    region: region.to_string(),
                    package_type: package_type.to_string(),
                    version: version.to_string(),
                });
            }
        }

        // Create temp directory for this installation
        let install_temp = self.temp_dir.join(format!(
            "install_{}_{}_{}",
            region.to_lowercase(),
            package_type,
            version
        ));
        fs::create_dir_all(&install_temp).map_err(|e| ManagerError::CreateDirFailed {
            path: install_temp.clone(),
            source: e,
        })?;

        // Prepare download state
        let urls: Vec<String> = metadata.parts.iter().map(|p| p.url.clone()).collect();
        let checksums: Vec<String> = metadata.parts.iter().map(|p| p.checksum.clone()).collect();
        let destinations: Vec<PathBuf> = metadata
            .parts
            .iter()
            .map(|p| install_temp.join(&p.filename))
            .collect();

        // Stage 2: Download all parts
        report(
            InstallStage::Downloading,
            0.0,
            &format!("Downloading {} parts...", metadata.parts.len()),
        );

        let mut download_state = DownloadState::new(urls, checksums, destinations.clone());
        let downloader = MultiPartDownloader::with_settings(
            std::time::Duration::from_secs(300),
            self.parallel_downloads,
        );

        // Download without per-part progress (we report at stage level)
        downloader.download_all(&mut download_state, None)?;

        let bytes_downloaded = download_state.total_bytes;
        report(
            InstallStage::Downloading,
            1.0,
            &format!("Downloaded {} bytes", bytes_downloaded),
        );

        // Stage 3: Verify (checksums already verified during download)
        report(
            InstallStage::Verifying,
            1.0,
            "Checksums verified during download",
        );

        // Stage 4: Reassemble archive
        report(InstallStage::Reassembling, 0.0, "Reassembling archive...");
        let archive_path = install_temp.join(&metadata.filename);
        let extractor = ShellExtractor::new();
        extractor.reassemble(&destinations, &archive_path)?;
        report(InstallStage::Reassembling, 1.0, "Archive reassembled");

        // Stage 5: Extract archive
        report(InstallStage::Extracting, 0.0, "Extracting archive...");
        let extract_dir = install_temp.join("extracted");
        let files_extracted = extractor.extract(&archive_path, &extract_dir)?;
        report(
            InstallStage::Extracting,
            1.0,
            &format!("Extracted {} files", files_extracted),
        );

        // Stage 6: Install to final location
        report(InstallStage::Installing, 0.0, "Installing package...");
        let install_path = self.store.install_path(region, package_type);

        // Remove existing package if present
        if install_path.exists() {
            fs::remove_dir_all(&install_path).map_err(|e| ManagerError::WriteFailed {
                path: install_path.clone(),
                source: e,
            })?;
        }

        // Move extracted contents to install path
        // The extracted directory should contain the package folder
        self.move_extracted_contents(&extract_dir, &install_path)?;
        report(InstallStage::Installing, 1.0, "Package installed");

        // Stage 7: Cleanup
        report(InstallStage::Cleanup, 0.0, "Cleaning up temporary files...");
        fs::remove_dir_all(&install_temp).ok(); // Best effort cleanup
        report(InstallStage::Cleanup, 1.0, "Cleanup complete");

        report(InstallStage::Complete, 1.0, "Installation complete");

        Ok(InstallResult {
            region: region.to_string(),
            package_type,
            version,
            install_path,
            bytes_downloaded,
            files_extracted,
        })
    }

    /// Move extracted contents to the install path.
    fn move_extracted_contents(
        &self,
        extract_dir: &Path,
        install_path: &Path,
    ) -> ManagerResult<()> {
        // The archive should contain a single top-level directory (the package folder)
        // We need to move its contents to the install path

        let entries: Vec<_> = fs::read_dir(extract_dir)
            .map_err(|e| ManagerError::ReadFailed {
                path: extract_dir.to_path_buf(),
                source: e,
            })?
            .filter_map(|e| e.ok())
            .collect();

        if entries.len() == 1 && entries[0].path().is_dir() {
            // Single directory - this is the package folder, rename it
            let source = entries[0].path();

            // Create parent directory
            if let Some(parent) = install_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ManagerError::CreateDirFailed {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }

            // Try rename first (fast path if same filesystem)
            if fs::rename(&source, install_path).is_err() {
                // Fall back to recursive copy
                copy_dir_recursive(&source, install_path)?;
            }
        } else {
            // Multiple entries or files at root - move them all
            if let Some(parent) = install_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ManagerError::CreateDirFailed {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }

            fs::create_dir_all(install_path).map_err(|e| ManagerError::CreateDirFailed {
                path: install_path.to_path_buf(),
                source: e,
            })?;

            for entry in entries {
                let source = entry.path();
                let dest = install_path.join(entry.file_name());

                if fs::rename(&source, &dest).is_err() {
                    if source.is_dir() {
                        copy_dir_recursive(&source, &dest)?;
                    } else {
                        fs::copy(&source, &dest).map_err(|e| ManagerError::WriteFailed {
                            path: dest,
                            source: e,
                        })?;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Recursively copy a directory.
fn copy_dir_recursive(source: &Path, dest: &Path) -> ManagerResult<()> {
    fs::create_dir_all(dest).map_err(|e| ManagerError::CreateDirFailed {
        path: dest.to_path_buf(),
        source: e,
    })?;

    for entry in fs::read_dir(source).map_err(|e| ManagerError::ReadFailed {
        path: source.to_path_buf(),
        source: e,
    })? {
        let entry = entry.map_err(|e| ManagerError::ReadFailed {
            path: source.to_path_buf(),
            source: e,
        })?;

        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &dest_path)?;
        } else {
            fs::copy(&source_path, &dest_path).map_err(|e| ManagerError::WriteFailed {
                path: dest_path,
                source: e,
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_stage_name() {
        assert_eq!(InstallStage::FetchingMetadata.name(), "Fetching metadata");
        assert_eq!(InstallStage::Downloading.name(), "Downloading");
        assert_eq!(InstallStage::Verifying.name(), "Verifying");
        assert_eq!(InstallStage::Reassembling.name(), "Reassembling");
        assert_eq!(InstallStage::Extracting.name(), "Extracting");
        assert_eq!(InstallStage::Installing.name(), "Installing");
        assert_eq!(InstallStage::Cleanup.name(), "Cleaning up");
        assert_eq!(InstallStage::Complete.name(), "Complete");
    }

    #[test]
    fn test_install_stage_equality() {
        assert_eq!(InstallStage::Downloading, InstallStage::Downloading);
        assert_ne!(InstallStage::Downloading, InstallStage::Extracting);
    }

    #[test]
    fn test_copy_dir_recursive() {
        use tempfile::TempDir;

        let source_temp = TempDir::new().unwrap();
        let dest_temp = TempDir::new().unwrap();

        // Create source structure
        fs::write(source_temp.path().join("file1.txt"), "hello").unwrap();
        let subdir = source_temp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file2.txt"), "world").unwrap();

        let dest = dest_temp.path().join("copied");
        copy_dir_recursive(source_temp.path(), &dest).unwrap();

        // Verify
        assert!(dest.join("file1.txt").exists());
        assert!(dest.join("subdir").is_dir());
        assert!(dest.join("subdir/file2.txt").exists());

        let content = fs::read_to_string(dest.join("file1.txt")).unwrap();
        assert_eq!(content, "hello");
    }
}
