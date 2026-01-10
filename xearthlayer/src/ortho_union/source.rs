//! Ortho source types for the union index.
//!
//! This module defines the types representing individual sources of ortho tiles
//! that can be merged into an [`OrthoUnionIndex`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Type of ortho source.
///
/// Distinguishes between user-provided patches and installed regional packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceType {
    /// User-provided mesh from `~/.xearthlayer/patches/`.
    ///
    /// Patches contain custom elevation/mesh data from airport addon developers.
    /// XEL generates textures dynamically for patches.
    Patch,

    /// Installed regional package (e.g., `na`, `eu`, `sa`).
    ///
    /// Regional packages are downloaded and installed via the package manager.
    RegionalPackage,
}

impl SourceType {
    /// Check if this is a patch source.
    pub fn is_patch(&self) -> bool {
        matches!(self, SourceType::Patch)
    }

    /// Check if this is a regional package source.
    pub fn is_regional_package(&self) -> bool {
        matches!(self, SourceType::RegionalPackage)
    }
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Patch => write!(f, "patch"),
            SourceType::RegionalPackage => write!(f, "package"),
        }
    }
}

/// A single ortho source (patch or regional package).
///
/// Used internally by [`OrthoUnionIndex`] to track where files come from.
/// Sources are sorted by `sort_key` to determine priority (first wins on collision).
///
/// # Sort Key Convention
///
/// - **Patches**: `_patches/{folder_name}` (underscore sorts before letters)
/// - **Regional packages**: `{region}` (e.g., "eu", "na", "sa")
///
/// This ensures patches always have higher priority than regional packages.
///
/// # Example
///
/// ```
/// use xearthlayer::ortho_union::{OrthoSource, SourceType};
///
/// // Create a patch source
/// let patch = OrthoSource::new_patch("KLAX_Mesh", "/home/user/.xearthlayer/patches/KLAX_Mesh");
/// assert_eq!(patch.sort_key, "_patches/KLAX_Mesh");
/// assert!(patch.source_type.is_patch());
///
/// // Create a regional package source
/// let package = OrthoSource::new_package("na", "/home/user/.xearthlayer/packages/na_ortho");
/// assert_eq!(package.sort_key, "na");
/// assert!(package.source_type.is_regional_package());
///
/// // Patches sort before packages
/// assert!(patch.sort_key < package.sort_key);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrthoSource {
    /// Sort key for priority ordering.
    ///
    /// Patches use `_patches/{name}`, packages use `{region}`.
    /// Alphabetical sorting determines priority (first wins).
    pub sort_key: String,

    /// Display name for the source.
    ///
    /// - Patches: folder name (e.g., "KLAX_Mesh")
    /// - Packages: region name (e.g., "North America")
    pub display_name: String,

    /// Real filesystem path to the source directory.
    pub source_path: PathBuf,

    /// Type of source (Patch or RegionalPackage).
    pub source_type: SourceType,

    /// Whether this source is enabled.
    ///
    /// - Patches: always `true`
    /// - Packages: from `InstalledPackage.enabled`
    ///
    /// Disabled sources are not included in the index.
    pub enabled: bool,
}

impl OrthoSource {
    /// Create a new patch source.
    ///
    /// The sort key is automatically prefixed with `_patches/` to ensure
    /// patches sort before regional packages.
    ///
    /// # Arguments
    ///
    /// * `name` - The patch folder name (e.g., "KLAX_Mesh")
    /// * `path` - The filesystem path to the patch folder
    ///
    /// # Example
    ///
    /// ```
    /// use xearthlayer::ortho_union::{OrthoSource, SourceType};
    ///
    /// let source = OrthoSource::new_patch("A_KDEN_Mesh", "/patches/A_KDEN_Mesh");
    ///
    /// assert_eq!(source.sort_key, "_patches/A_KDEN_Mesh");
    /// assert_eq!(source.display_name, "A_KDEN_Mesh");
    /// assert_eq!(source.source_type, SourceType::Patch);
    /// assert!(source.enabled);
    /// ```
    pub fn new_patch(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let name = name.into();
        Self {
            sort_key: format!("_patches/{}", name),
            display_name: name,
            source_path: path.into(),
            source_type: SourceType::Patch,
            enabled: true, // Patches are always enabled
        }
    }

    /// Create a new regional package source.
    ///
    /// The sort key is the region code (lowercase), which determines
    /// priority among packages (alphabetical order).
    ///
    /// # Arguments
    ///
    /// * `region` - The region code (e.g., "na", "eu")
    /// * `path` - The filesystem path to the package folder
    ///
    /// # Example
    ///
    /// ```
    /// use xearthlayer::ortho_union::{OrthoSource, SourceType};
    ///
    /// let source = OrthoSource::new_package("na", "/packages/na_ortho");
    ///
    /// assert_eq!(source.sort_key, "na");
    /// assert_eq!(source.display_name, "na");
    /// assert_eq!(source.source_type, SourceType::RegionalPackage);
    /// assert!(source.enabled);
    /// ```
    pub fn new_package(region: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let region = region.into().to_lowercase();
        Self {
            sort_key: region.clone(),
            display_name: region,
            source_path: path.into(),
            source_type: SourceType::RegionalPackage,
            enabled: true,
        }
    }

    /// Create a new regional package source with explicit enabled state.
    ///
    /// # Example
    ///
    /// ```
    /// use xearthlayer::ortho_union::OrthoSource;
    ///
    /// let disabled = OrthoSource::new_package_with_enabled("na", "/packages/na", false);
    /// assert!(!disabled.enabled);
    /// ```
    pub fn new_package_with_enabled(
        region: impl Into<String>,
        path: impl Into<PathBuf>,
        enabled: bool,
    ) -> Self {
        let mut source = Self::new_package(region, path);
        source.enabled = enabled;
        source
    }

    /// Get the source path.
    pub fn path(&self) -> &Path {
        &self.source_path
    }

    /// Check if this source is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check if this is a patch source.
    pub fn is_patch(&self) -> bool {
        self.source_type.is_patch()
    }

    /// Check if this is a regional package source.
    pub fn is_regional_package(&self) -> bool {
        self.source_type.is_regional_package()
    }
}

impl std::fmt::Display for OrthoSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {})",
            self.display_name,
            self.source_type,
            if self.enabled { "enabled" } else { "disabled" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_type_is_patch() {
        assert!(SourceType::Patch.is_patch());
        assert!(!SourceType::Patch.is_regional_package());
    }

    #[test]
    fn test_source_type_is_regional_package() {
        assert!(SourceType::RegionalPackage.is_regional_package());
        assert!(!SourceType::RegionalPackage.is_patch());
    }

    #[test]
    fn test_source_type_display() {
        assert_eq!(format!("{}", SourceType::Patch), "patch");
        assert_eq!(format!("{}", SourceType::RegionalPackage), "package");
    }

    #[test]
    fn test_ortho_source_new_patch() {
        let source = OrthoSource::new_patch("KLAX_Mesh", "/patches/KLAX_Mesh");

        assert_eq!(source.sort_key, "_patches/KLAX_Mesh");
        assert_eq!(source.display_name, "KLAX_Mesh");
        assert_eq!(source.source_path, PathBuf::from("/patches/KLAX_Mesh"));
        assert_eq!(source.source_type, SourceType::Patch);
        assert!(source.enabled);
    }

    #[test]
    fn test_ortho_source_new_package() {
        let source = OrthoSource::new_package("na", "/packages/na_ortho");

        assert_eq!(source.sort_key, "na");
        assert_eq!(source.display_name, "na");
        assert_eq!(source.source_path, PathBuf::from("/packages/na_ortho"));
        assert_eq!(source.source_type, SourceType::RegionalPackage);
        assert!(source.enabled);
    }

    #[test]
    fn test_ortho_source_package_normalizes_region() {
        let source = OrthoSource::new_package("NA", "/packages/na_ortho");
        assert_eq!(source.sort_key, "na");
        assert_eq!(source.display_name, "na");
    }

    #[test]
    fn test_ortho_source_new_package_with_enabled() {
        let enabled = OrthoSource::new_package_with_enabled("na", "/packages/na", true);
        let disabled = OrthoSource::new_package_with_enabled("eu", "/packages/eu", false);

        assert!(enabled.enabled);
        assert!(!disabled.enabled);
    }

    #[test]
    fn test_ortho_source_patches_sort_before_packages() {
        let patch = OrthoSource::new_patch("KLAX", "/patches/KLAX");
        let package = OrthoSource::new_package("na", "/packages/na");

        // Underscore sorts before letters in ASCII
        assert!(patch.sort_key < package.sort_key);
    }

    #[test]
    fn test_ortho_source_packages_sort_alphabetically() {
        let eu = OrthoSource::new_package("eu", "/packages/eu");
        let na = OrthoSource::new_package("na", "/packages/na");
        let sa = OrthoSource::new_package("sa", "/packages/sa");

        assert!(eu.sort_key < na.sort_key);
        assert!(na.sort_key < sa.sort_key);
    }

    #[test]
    fn test_ortho_source_patches_sort_alphabetically() {
        let a_patch = OrthoSource::new_patch("A_KDEN", "/patches/A_KDEN");
        let b_patch = OrthoSource::new_patch("B_KLAX", "/patches/B_KLAX");

        assert!(a_patch.sort_key < b_patch.sort_key);
    }

    #[test]
    fn test_ortho_source_helper_methods() {
        let patch = OrthoSource::new_patch("KLAX", "/patches/KLAX");
        let package = OrthoSource::new_package("na", "/packages/na");

        assert!(patch.is_patch());
        assert!(!patch.is_regional_package());
        assert!(!package.is_patch());
        assert!(package.is_regional_package());

        assert_eq!(patch.path(), Path::new("/patches/KLAX"));
        assert!(patch.is_enabled());
    }

    #[test]
    fn test_ortho_source_display() {
        let patch = OrthoSource::new_patch("KLAX", "/patches/KLAX");
        assert_eq!(format!("{}", patch), "KLAX (patch, enabled)");

        let disabled = OrthoSource::new_package_with_enabled("na", "/packages/na", false);
        assert_eq!(format!("{}", disabled), "na (package, disabled)");
    }

    #[test]
    fn test_ortho_source_clone() {
        let source = OrthoSource::new_patch("KLAX", "/patches/KLAX");
        let cloned = source.clone();

        assert_eq!(source.sort_key, cloned.sort_key);
        assert_eq!(source.display_name, cloned.display_name);
        assert_eq!(source.source_path, cloned.source_path);
        assert_eq!(source.source_type, cloned.source_type);
        assert_eq!(source.enabled, cloned.enabled);
    }
}
