//! Symlink management for overlay packages.
//!
//! This module handles creating and removing symlinks for overlay packages
//! in the X-Plane Custom Scenery directory.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::package::{self, PackageType};

use super::{ManagerError, ManagerResult};

/// Marker file name used to identify XEarthLayer symlinks.
const SYMLINK_MARKER: &str = ".xearthlayer_symlink";

/// Create a symlink for an overlay package in the Custom Scenery directory.
///
/// # Arguments
///
/// * `package_path` - Path to the installed overlay package
/// * `custom_scenery_path` - Path to X-Plane's Custom Scenery directory
///
/// # Returns
///
/// The path to the created symlink.
///
/// # Errors
///
/// Returns an error if:
/// - The package doesn't exist
/// - The Custom Scenery directory doesn't exist
/// - A non-symlink file/directory already exists at the target
/// - Symlink creation fails
pub fn create_overlay_symlink(
    package_path: &Path,
    custom_scenery_path: &Path,
) -> ManagerResult<PathBuf> {
    // Verify package exists
    if !package_path.exists() {
        return Err(ManagerError::InvalidPath(format!(
            "package does not exist: {}",
            package_path.display()
        )));
    }

    // Verify Custom Scenery directory exists
    if !custom_scenery_path.exists() {
        return Err(ManagerError::InvalidPath(format!(
            "Custom Scenery directory does not exist: {}",
            custom_scenery_path.display()
        )));
    }

    // Get the package folder name
    let folder_name = package_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            ManagerError::InvalidPath(format!("invalid package path: {}", package_path.display()))
        })?;

    let symlink_path = custom_scenery_path.join(folder_name);

    // Check if something already exists at the target
    if symlink_path.exists() || symlink_path.symlink_metadata().is_ok() {
        // Check if it's already our symlink
        if is_xearthlayer_symlink(&symlink_path)? {
            // Remove old symlink and recreate
            fs::remove_file(&symlink_path).map_err(|e| ManagerError::WriteFailed {
                path: symlink_path.clone(),
                source: e,
            })?;
        } else if symlink_path.is_symlink() {
            // It's a symlink but not ours - remove it anyway (might be stale)
            fs::remove_file(&symlink_path).map_err(|e| ManagerError::WriteFailed {
                path: symlink_path.clone(),
                source: e,
            })?;
        } else {
            // It's a real file/directory - don't touch it
            return Err(ManagerError::SymlinkFailed {
                source: symlink_path.clone(),
                target: package_path.to_path_buf(),
                reason: "a file or directory already exists at the symlink location".to_string(),
            });
        }
    }

    // Create the symlink
    symlink(package_path, &symlink_path).map_err(|e| ManagerError::SymlinkFailed {
        source: package_path.to_path_buf(),
        target: symlink_path.clone(),
        reason: e.to_string(),
    })?;

    // Create marker file in the package to identify this as an XEarthLayer-managed symlink
    let marker_path = package_path.join(SYMLINK_MARKER);
    fs::write(&marker_path, symlink_path.display().to_string())
        .map_err(|e| {
            // Best effort - if marker fails, still return success
            tracing::warn!("Failed to create symlink marker: {}", e);
            ManagerError::WriteFailed {
                path: marker_path,
                source: e,
            }
        })
        .ok();

    Ok(symlink_path)
}

/// Remove an overlay symlink from the Custom Scenery directory.
///
/// # Arguments
///
/// * `region` - The region code (e.g., "na", "eu")
/// * `custom_scenery_path` - Path to X-Plane's Custom Scenery directory
///
/// # Returns
///
/// `Ok(true)` if symlink was removed, `Ok(false)` if no symlink existed.
///
/// # Safety
///
/// This only removes symlinks, never real directories. If a real directory
/// exists at the expected symlink path, it will not be removed.
pub fn remove_overlay_symlink(region: &str, custom_scenery_path: &Path) -> ManagerResult<bool> {
    let folder_name = package::package_mountpoint(region, PackageType::Overlay);
    let symlink_path = custom_scenery_path.join(&folder_name);

    // Check if symlink exists
    if symlink_path.symlink_metadata().is_err() {
        return Ok(false);
    }

    // Safety check: only remove if it's a symlink
    if !symlink_path.is_symlink() {
        return Err(ManagerError::SymlinkFailed {
            source: symlink_path.clone(),
            target: PathBuf::new(),
            reason: "target is not a symlink, refusing to remove".to_string(),
        });
    }

    // Remove the symlink
    fs::remove_file(&symlink_path).map_err(|e| ManagerError::WriteFailed {
        path: symlink_path,
        source: e,
    })?;

    Ok(true)
}

/// Check if a path is an XEarthLayer-managed symlink.
///
/// This checks if the symlink points to a directory containing our marker file.
fn is_xearthlayer_symlink(path: &Path) -> ManagerResult<bool> {
    if !path.is_symlink() {
        return Ok(false);
    }

    // Read symlink target
    let target = fs::read_link(path).map_err(|e| ManagerError::ReadFailed {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Check for marker file
    let marker_path = target.join(SYMLINK_MARKER);
    Ok(marker_path.exists())
}

/// Get the symlink path for an overlay package.
///
/// # Arguments
///
/// * `region` - The region code
/// * `custom_scenery_path` - Path to X-Plane's Custom Scenery directory
pub fn overlay_symlink_path(region: &str, custom_scenery_path: &Path) -> PathBuf {
    let folder_name = package::package_mountpoint(region, PackageType::Overlay);
    custom_scenery_path.join(folder_name)
}

/// Check if an overlay symlink exists for a region.
pub fn overlay_symlink_exists(region: &str, custom_scenery_path: &Path) -> bool {
    let symlink_path = overlay_symlink_path(region, custom_scenery_path);
    symlink_path.is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_mock_overlay_package(dir: &Path, region: &str) -> PathBuf {
        let folder_name = package::package_mountpoint(region, PackageType::Overlay);
        let package_dir = dir.join(&folder_name);
        fs::create_dir_all(&package_dir).unwrap();

        // Create a dummy file
        fs::write(package_dir.join("test.dsf"), "test").unwrap();

        package_dir
    }

    #[test]
    fn test_create_overlay_symlink() {
        let packages_dir = TempDir::new().unwrap();
        let scenery_dir = TempDir::new().unwrap();

        let package_path = create_mock_overlay_package(packages_dir.path(), "na");

        let result = create_overlay_symlink(&package_path, scenery_dir.path());
        assert!(result.is_ok());

        let symlink_path = result.unwrap();
        assert!(symlink_path.is_symlink());
        assert!(symlink_path.join("test.dsf").exists());
    }

    #[test]
    fn test_create_overlay_symlink_replaces_existing() {
        let packages_dir = TempDir::new().unwrap();
        let scenery_dir = TempDir::new().unwrap();

        let package_path = create_mock_overlay_package(packages_dir.path(), "na");

        // Create first symlink
        let symlink1 = create_overlay_symlink(&package_path, scenery_dir.path()).unwrap();
        assert!(symlink1.is_symlink());

        // Create again - should replace
        let symlink2 = create_overlay_symlink(&package_path, scenery_dir.path()).unwrap();
        assert!(symlink2.is_symlink());
        assert_eq!(symlink1, symlink2);
    }

    #[test]
    fn test_create_overlay_symlink_fails_if_dir_exists() {
        let packages_dir = TempDir::new().unwrap();
        let scenery_dir = TempDir::new().unwrap();

        let package_path = create_mock_overlay_package(packages_dir.path(), "na");

        // Create a real directory at the symlink location
        let blocking_dir = scenery_dir.path().join("yzXEL_na_overlay");
        fs::create_dir(&blocking_dir).unwrap();

        let result = create_overlay_symlink(&package_path, scenery_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_overlay_symlink() {
        let packages_dir = TempDir::new().unwrap();
        let scenery_dir = TempDir::new().unwrap();

        let package_path = create_mock_overlay_package(packages_dir.path(), "na");

        // Create symlink
        create_overlay_symlink(&package_path, scenery_dir.path()).unwrap();
        assert!(overlay_symlink_exists("na", scenery_dir.path()));

        // Remove symlink
        let result = remove_overlay_symlink("na", scenery_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(!overlay_symlink_exists("na", scenery_dir.path()));
    }

    #[test]
    fn test_remove_overlay_symlink_nonexistent() {
        let scenery_dir = TempDir::new().unwrap();

        let result = remove_overlay_symlink("na", scenery_dir.path());
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Returns false when nothing to remove
    }

    #[test]
    fn test_remove_overlay_symlink_refuses_real_dir() {
        let scenery_dir = TempDir::new().unwrap();

        // Create a real directory
        let dir_path = scenery_dir.path().join("yzXEL_na_overlay");
        fs::create_dir(&dir_path).unwrap();

        let result = remove_overlay_symlink("na", scenery_dir.path());
        assert!(result.is_err());

        // Directory should still exist
        assert!(dir_path.exists());
    }

    #[test]
    fn test_overlay_symlink_path() {
        let scenery_dir = PathBuf::from("/path/to/Custom Scenery");
        let path = overlay_symlink_path("na", &scenery_dir);
        assert_eq!(
            path,
            PathBuf::from("/path/to/Custom Scenery/yzXEL_na_overlay")
        );
    }

    #[test]
    fn test_overlay_symlink_exists() {
        let packages_dir = TempDir::new().unwrap();
        let scenery_dir = TempDir::new().unwrap();

        assert!(!overlay_symlink_exists("na", scenery_dir.path()));

        let package_path = create_mock_overlay_package(packages_dir.path(), "na");
        create_overlay_symlink(&package_path, scenery_dir.path()).unwrap();

        assert!(overlay_symlink_exists("na", scenery_dir.path()));
    }
}
