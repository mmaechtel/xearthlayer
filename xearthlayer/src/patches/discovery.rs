//! Patch folder discovery and validation.
//!
//! This module provides functionality to discover and validate patch tiles
//! in the configured patches directory.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::prefetch::tile_based::DsfTileCoord;

/// Errors that can occur during patch validation.
#[derive(Debug, Error, Clone)]
pub enum ValidationError {
    /// The patch folder is missing the Earth nav data directory.
    #[error("Missing 'Earth nav data' directory")]
    MissingEarthNavData,

    /// The patch folder has no DSF files.
    #[error("No DSF files found in 'Earth nav data'")]
    NoDsfFiles,

    /// The patch folder path doesn't exist.
    #[error("Patch folder does not exist: {0}")]
    FolderNotFound(PathBuf),

    /// I/O error during validation.
    #[error("I/O error: {0}")]
    IoError(String),
}

/// Information about a discovered patch.
#[derive(Debug, Clone)]
pub struct PatchInfo {
    /// Name of the patch (folder name).
    pub name: String,

    /// Full path to the patch folder.
    pub path: PathBuf,

    /// Number of DSF files found.
    pub dsf_count: usize,

    /// Number of terrain (.ter) files found.
    pub terrain_count: usize,

    /// Number of texture (.dds) files found.
    pub texture_count: usize,

    /// 1°×1° DSF regions owned by this patch (lat, lon pairs).
    ///
    /// Populated from DSF filenames in `Earth nav data/`. Each entry represents
    /// a region where this patch provides the authoritative scenery data.
    pub dsf_regions: Vec<(i32, i32)>,

    /// Whether this patch passed validation.
    pub is_valid: bool,

    /// Validation errors, if any.
    pub validation_errors: Vec<ValidationError>,
}

impl PatchInfo {
    /// Create a new PatchInfo for a valid patch.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            dsf_count: 0,
            terrain_count: 0,
            texture_count: 0,
            dsf_regions: Vec::new(),
            is_valid: true,
            validation_errors: Vec::new(),
        }
    }

    /// Add a validation error and mark as invalid.
    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.validation_errors.push(error);
    }
}

/// Result of validating a patch folder.
#[derive(Debug, Clone)]
pub struct PatchValidation {
    /// Whether the patch is valid (has required structure).
    pub is_valid: bool,

    /// Validation errors encountered.
    pub errors: Vec<ValidationError>,

    /// Number of DSF files found.
    pub dsf_count: usize,

    /// Number of terrain files found.
    pub terrain_count: usize,

    /// Number of texture files found.
    pub texture_count: usize,
}

impl PatchValidation {
    /// Create a validation result indicating success.
    pub fn valid(dsf_count: usize, terrain_count: usize, texture_count: usize) -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            dsf_count,
            terrain_count,
            texture_count,
        }
    }

    /// Create a validation result indicating failure.
    pub fn invalid(errors: Vec<ValidationError>) -> Self {
        Self {
            is_valid: false,
            errors,
            dsf_count: 0,
            terrain_count: 0,
            texture_count: 0,
        }
    }
}

/// Discovers and validates patch tiles in a directory.
#[derive(Debug, Clone)]
pub struct PatchDiscovery {
    /// Root patches directory.
    patches_dir: PathBuf,
}

impl PatchDiscovery {
    /// Create a new patch discovery for the given directory.
    pub fn new(patches_dir: impl Into<PathBuf>) -> Self {
        Self {
            patches_dir: patches_dir.into(),
        }
    }

    /// Get the patches directory.
    pub fn patches_dir(&self) -> &Path {
        &self.patches_dir
    }

    /// Check if the patches directory exists.
    pub fn exists(&self) -> bool {
        self.patches_dir.exists() && self.patches_dir.is_dir()
    }

    /// Find all patch folders in the patches directory.
    ///
    /// Returns patches sorted alphabetically by folder name (priority order).
    /// Invalid patches are included but marked as invalid.
    pub fn find_patches(&self) -> Result<Vec<PatchInfo>, std::io::Error> {
        if !self.exists() {
            return Ok(Vec::new());
        }

        let mut patches = Vec::new();

        for entry in std::fs::read_dir(&self.patches_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Skip non-directories
            if !path.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden folders
            if name.starts_with('.') {
                continue;
            }

            let validation = self.validate_patch(&path);
            let mut info = PatchInfo::new(&name, &path);
            info.dsf_count = validation.dsf_count;
            info.terrain_count = validation.terrain_count;
            info.texture_count = validation.texture_count;
            info.dsf_regions = extract_dsf_regions(&path);
            info.is_valid = validation.is_valid;
            info.validation_errors = validation.errors;

            patches.push(info);
        }

        // Sort alphabetically by name (determines priority)
        patches.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(patches)
    }

    /// Find only valid patches (ready for mounting).
    pub fn find_valid_patches(&self) -> Result<Vec<PatchInfo>, std::io::Error> {
        Ok(self
            .find_patches()?
            .into_iter()
            .filter(|p| p.is_valid)
            .collect())
    }

    /// Validate a single patch folder.
    pub fn validate_patch(&self, patch_path: &Path) -> PatchValidation {
        let mut errors = Vec::new();

        // Check folder exists
        if !patch_path.exists() {
            return PatchValidation::invalid(vec![ValidationError::FolderNotFound(
                patch_path.to_path_buf(),
            )]);
        }

        // Check for Earth nav data directory
        let earth_nav_data = patch_path.join("Earth nav data");
        if !earth_nav_data.exists() {
            errors.push(ValidationError::MissingEarthNavData);
        }

        // Count DSF files
        let dsf_count = if earth_nav_data.exists() {
            count_files_recursive(&earth_nav_data, "dsf")
        } else {
            0
        };

        if dsf_count == 0 && earth_nav_data.exists() {
            errors.push(ValidationError::NoDsfFiles);
        }

        // Count terrain files (optional but useful info)
        let terrain_dir = patch_path.join("terrain");
        let terrain_count = if terrain_dir.exists() {
            count_files_recursive(&terrain_dir, "ter")
        } else {
            0
        };

        // Count texture files (optional - XEL generates these)
        let textures_dir = patch_path.join("textures");
        let texture_count = if textures_dir.exists() {
            count_files_recursive(&textures_dir, "dds")
        } else {
            0
        };

        if errors.is_empty() {
            PatchValidation::valid(dsf_count, terrain_count, texture_count)
        } else {
            let mut validation = PatchValidation::invalid(errors);
            validation.dsf_count = dsf_count;
            validation.terrain_count = terrain_count;
            validation.texture_count = texture_count;
            validation
        }
    }

    /// Check if a specific patch name exists and is valid.
    pub fn has_valid_patch(&self, name: &str) -> bool {
        let patch_path = self.patches_dir.join(name);
        let validation = self.validate_patch(&patch_path);
        validation.is_valid
    }
}

/// Extract DSF region coordinates from a patch directory.
///
/// Scans `Earth nav data/` for `.dsf` files and parses their filenames
/// (e.g., `+33-119.dsf` → `(33, -119)`) to determine which 1°×1° regions
/// the patch covers. Ownership follows the DSF — if a patch provides the
/// DSF for a region, it owns all resources in that region.
pub fn extract_dsf_regions(patch_path: &Path) -> Vec<(i32, i32)> {
    let earth_nav = patch_path.join("Earth nav data");
    if !earth_nav.exists() {
        return Vec::new();
    }

    let mut regions = Vec::new();
    collect_dsf_regions_recursive(&earth_nav, &mut regions);
    regions
}

/// Recursively collect DSF region coordinates from a directory.
fn collect_dsf_regions_recursive(dir: &Path, regions: &mut Vec<(i32, i32)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dsf_regions_recursive(&path, regions);
        } else if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
            if let Some(coord) = DsfTileCoord::from_dsf_filename(filename) {
                regions.push((coord.lat, coord.lon));
            }
        }
    }
}

/// Count files with a given extension recursively.
fn count_files_recursive(dir: &Path, extension: &str) -> usize {
    let mut count = 0;

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path, extension);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
            {
                count += 1;
            }
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_patch(
        temp: &TempDir,
        name: &str,
        with_dsf: bool,
        with_terrain: bool,
    ) -> PathBuf {
        let patch_dir = temp.path().join(name);
        std::fs::create_dir_all(&patch_dir).unwrap();

        // Create Earth nav data structure
        let earth_nav = patch_dir.join("Earth nav data").join("+30-120");
        std::fs::create_dir_all(&earth_nav).unwrap();

        if with_dsf {
            std::fs::write(earth_nav.join("+33-119.dsf"), b"fake dsf").unwrap();
        }

        if with_terrain {
            let terrain = patch_dir.join("terrain");
            std::fs::create_dir_all(&terrain).unwrap();
            std::fs::write(terrain.join("12345_67890_BI18.ter"), b"fake terrain").unwrap();
        }

        patch_dir
    }

    #[test]
    fn test_discovery_empty_dir() {
        let temp = TempDir::new().unwrap();
        let discovery = PatchDiscovery::new(temp.path());

        let patches = discovery.find_patches().unwrap();
        assert!(patches.is_empty());
    }

    #[test]
    fn test_discovery_nonexistent_dir() {
        let discovery = PatchDiscovery::new("/nonexistent/path");
        assert!(!discovery.exists());

        let patches = discovery.find_patches().unwrap();
        assert!(patches.is_empty());
    }

    #[test]
    fn test_discovery_finds_valid_patch() {
        let temp = TempDir::new().unwrap();
        create_test_patch(&temp, "A_KLAX_Mesh", true, true);

        let discovery = PatchDiscovery::new(temp.path());
        let patches = discovery.find_patches().unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].name, "A_KLAX_Mesh");
        assert!(patches[0].is_valid);
        assert_eq!(patches[0].dsf_count, 1);
        assert_eq!(patches[0].terrain_count, 1);
    }

    #[test]
    fn test_discovery_invalid_patch_no_dsf() {
        let temp = TempDir::new().unwrap();
        create_test_patch(&temp, "A_Empty", false, true);

        let discovery = PatchDiscovery::new(temp.path());
        let patches = discovery.find_patches().unwrap();

        assert_eq!(patches.len(), 1);
        assert!(!patches[0].is_valid);
        assert!(patches[0]
            .validation_errors
            .iter()
            .any(|e| matches!(e, ValidationError::NoDsfFiles)));
    }

    #[test]
    fn test_discovery_invalid_patch_no_earth_nav_data() {
        let temp = TempDir::new().unwrap();
        let patch_dir = temp.path().join("BadPatch");
        std::fs::create_dir_all(&patch_dir).unwrap();
        // Create just terrain, no Earth nav data
        std::fs::create_dir_all(patch_dir.join("terrain")).unwrap();

        let discovery = PatchDiscovery::new(temp.path());
        let patches = discovery.find_patches().unwrap();

        assert_eq!(patches.len(), 1);
        assert!(!patches[0].is_valid);
        assert!(patches[0]
            .validation_errors
            .iter()
            .any(|e| matches!(e, ValidationError::MissingEarthNavData)));
    }

    #[test]
    fn test_discovery_alphabetical_sorting() {
        let temp = TempDir::new().unwrap();
        create_test_patch(&temp, "C_Third", true, true);
        create_test_patch(&temp, "A_First", true, true);
        create_test_patch(&temp, "B_Second", true, true);

        let discovery = PatchDiscovery::new(temp.path());
        let patches = discovery.find_patches().unwrap();

        assert_eq!(patches.len(), 3);
        assert_eq!(patches[0].name, "A_First");
        assert_eq!(patches[1].name, "B_Second");
        assert_eq!(patches[2].name, "C_Third");
    }

    #[test]
    fn test_discovery_skips_hidden_folders() {
        let temp = TempDir::new().unwrap();
        create_test_patch(&temp, "A_Visible", true, true);

        // Create hidden folder
        let hidden = temp.path().join(".hidden");
        std::fs::create_dir_all(hidden.join("Earth nav data")).unwrap();

        let discovery = PatchDiscovery::new(temp.path());
        let patches = discovery.find_patches().unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].name, "A_Visible");
    }

    #[test]
    fn test_find_valid_patches_only() {
        let temp = TempDir::new().unwrap();
        create_test_patch(&temp, "A_Valid", true, true);
        create_test_patch(&temp, "B_Invalid", false, true);

        let discovery = PatchDiscovery::new(temp.path());
        let valid_patches = discovery.find_valid_patches().unwrap();

        assert_eq!(valid_patches.len(), 1);
        assert_eq!(valid_patches[0].name, "A_Valid");
    }

    #[test]
    fn test_has_valid_patch() {
        let temp = TempDir::new().unwrap();
        create_test_patch(&temp, "KLAX_Mesh", true, true);

        let discovery = PatchDiscovery::new(temp.path());
        assert!(discovery.has_valid_patch("KLAX_Mesh"));
        assert!(!discovery.has_valid_patch("Nonexistent"));
    }

    #[test]
    fn test_validate_patch_directly() {
        let temp = TempDir::new().unwrap();
        let patch_path = create_test_patch(&temp, "TestPatch", true, true);

        let discovery = PatchDiscovery::new(temp.path());
        let validation = discovery.validate_patch(&patch_path);

        assert!(validation.is_valid);
        assert_eq!(validation.dsf_count, 1);
        assert_eq!(validation.terrain_count, 1);
        assert_eq!(validation.texture_count, 0);
    }

    // =========================================================================
    // extract_dsf_regions tests (Issue #51)
    // =========================================================================

    #[test]
    fn test_extract_dsf_regions_from_patch() {
        let temp = TempDir::new().unwrap();
        let patch_dir = temp.path().join("LIPX_Mesh");
        // DSF files live in subdirectories of "Earth nav data/"
        let nav_dir = patch_dir.join("Earth nav data").join("+40+010");
        std::fs::create_dir_all(&nav_dir).unwrap();
        std::fs::write(nav_dir.join("+45+011.dsf"), b"dsf1").unwrap();
        std::fs::write(nav_dir.join("+45+012.dsf"), b"dsf2").unwrap();

        let regions = extract_dsf_regions(&patch_dir);

        assert_eq!(regions.len(), 2);
        assert!(regions.contains(&(45, 11)));
        assert!(regions.contains(&(45, 12)));
    }

    #[test]
    fn test_extract_dsf_regions_empty_patch() {
        let temp = TempDir::new().unwrap();
        let patch_dir = temp.path().join("EmptyPatch");
        std::fs::create_dir_all(patch_dir.join("Earth nav data")).unwrap();

        let regions = extract_dsf_regions(&patch_dir);
        assert!(regions.is_empty());
    }

    #[test]
    fn test_extract_dsf_regions_no_earth_nav_data() {
        let temp = TempDir::new().unwrap();
        let patch_dir = temp.path().join("NoPatch");
        std::fs::create_dir_all(&patch_dir).unwrap();

        let regions = extract_dsf_regions(&patch_dir);
        assert!(regions.is_empty());
    }

    #[test]
    fn test_extract_dsf_regions_negative_coords() {
        let temp = TempDir::new().unwrap();
        let patch_dir = temp.path().join("SouthPatch");
        let nav_dir = patch_dir.join("Earth nav data").join("-50+010");
        std::fs::create_dir_all(&nav_dir).unwrap();
        std::fs::write(nav_dir.join("-46+012.dsf"), b"dsf").unwrap();

        let regions = extract_dsf_regions(&patch_dir);

        assert_eq!(regions.len(), 1);
        assert!(regions.contains(&(-46, 12)));
    }

    #[test]
    fn test_extract_dsf_regions_zero_coords() {
        let temp = TempDir::new().unwrap();
        let patch_dir = temp.path().join("ZeroPatch");
        let nav_dir = patch_dir.join("Earth nav data").join("+00+000");
        std::fs::create_dir_all(&nav_dir).unwrap();
        std::fs::write(nav_dir.join("+00+000.dsf"), b"dsf").unwrap();

        let regions = extract_dsf_regions(&patch_dir);

        assert_eq!(regions.len(), 1);
        assert!(regions.contains(&(0, 0)));
    }

    #[test]
    fn test_patch_info_has_dsf_regions() {
        let temp = TempDir::new().unwrap();
        let _patch_dir = create_test_patch(&temp, "RegionPatch", true, true);
        // The test helper creates +33-119.dsf inside Earth nav data/+30-120/

        let discovery = PatchDiscovery::new(temp.path());
        let patches = discovery.find_patches().unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].dsf_regions.len(), 1);
        assert!(patches[0].dsf_regions.contains(&(33, -119)));
    }
}
