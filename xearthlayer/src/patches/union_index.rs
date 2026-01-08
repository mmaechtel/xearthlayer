//! Union index for merging multiple patch folders into a single virtual filesystem.
//!
//! This module provides the [`PatchUnionIndex`] which maintains a merged view
//! of all patch folders, similar to an overlay filesystem.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::discovery::PatchInfo;

/// A directory entry in the union filesystem.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// File or directory name.
    pub name: OsString,

    /// Whether this is a directory.
    pub is_dir: bool,

    /// File size (0 for directories).
    pub size: u64,

    /// Modification time.
    pub mtime: SystemTime,
}

impl DirEntry {
    /// Create a new directory entry.
    pub fn new(name: impl Into<OsString>, is_dir: bool, size: u64, mtime: SystemTime) -> Self {
        Self {
            name: name.into(),
            is_dir,
            size,
            mtime,
        }
    }

    /// Create a directory entry from filesystem metadata.
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = path.metadata()?;
        let name = path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();

        Ok(Self {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            mtime: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        })
    }
}

/// Source information for a file in the union index.
#[derive(Debug, Clone)]
pub struct FileSource {
    /// Name of the source patch.
    pub patch_name: String,

    /// Real filesystem path to the file.
    pub real_path: PathBuf,
}

impl FileSource {
    /// Create a new file source.
    pub fn new(patch_name: impl Into<String>, real_path: impl Into<PathBuf>) -> Self {
        Self {
            patch_name: patch_name.into(),
            real_path: real_path.into(),
        }
    }
}

/// Merged index of all files across multiple patch folders.
///
/// The union index provides a single virtual filesystem view where files
/// from multiple patches are merged together. When multiple patches contain
/// the same file path, the first patch (alphabetically by name) wins.
///
/// # Example
///
/// ```ignore
/// use xearthlayer::patches::{PatchUnionIndex, PatchDiscovery};
///
/// let discovery = PatchDiscovery::new("~/.xearthlayer/patches");
/// let patches = discovery.find_valid_patches()?;
/// let index = PatchUnionIndex::build(&patches)?;
///
/// // Resolve a virtual path to its real source
/// if let Some(source) = index.resolve(Path::new("Earth nav data/+30-120/+33-119.dsf")) {
///     println!("Found in patch '{}': {:?}", source.patch_name, source.real_path);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct PatchUnionIndex {
    /// Map from virtual path to file source.
    /// Keys are relative paths (e.g., "Earth nav data/+30-120/+33-119.dsf").
    files: HashMap<PathBuf, FileSource>,

    /// Virtual directory structure for readdir operations.
    /// Keys are directory paths, values are entries in that directory.
    directories: HashMap<PathBuf, Vec<DirEntry>>,

    /// Names of patches included in this index, in priority order.
    patch_names: Vec<String>,

    /// Total file count across all patches.
    total_files: usize,
}

impl PatchUnionIndex {
    /// Create an empty union index.
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            directories: HashMap::new(),
            patch_names: Vec::new(),
            total_files: 0,
        }
    }

    /// Build a union index from a list of patches.
    ///
    /// Patches should already be sorted by priority (alphabetically).
    /// The first patch wins on collision.
    pub fn build(patches: &[PatchInfo]) -> std::io::Result<Self> {
        let mut index = Self::new();

        for patch in patches {
            if patch.is_valid {
                index.add_patch(&patch.name, &patch.path)?;
            }
        }

        Ok(index)
    }

    /// Add a patch to the index.
    ///
    /// Files from this patch are added to the index, but only if the path
    /// doesn't already exist (first patch wins).
    pub fn add_patch(&mut self, name: &str, patch_path: &Path) -> std::io::Result<()> {
        self.patch_names.push(name.to_string());
        self.add_directory_recursive(name, patch_path, &PathBuf::new())?;
        Ok(())
    }

    /// Recursively add a directory and its contents to the index.
    fn add_directory_recursive(
        &mut self,
        patch_name: &str,
        real_dir: &Path,
        virtual_dir: &Path,
    ) -> std::io::Result<()> {
        // Ensure the virtual directory has an entry list
        if !self.directories.contains_key(virtual_dir) {
            self.directories
                .insert(virtual_dir.to_path_buf(), Vec::new());
        }

        for entry in std::fs::read_dir(real_dir)? {
            let entry = entry?;
            let real_path = entry.path();
            let file_name = entry.file_name();
            let virtual_path = virtual_dir.join(&file_name);
            let is_dir = real_path.is_dir();

            // Only add to files index if not already present (first patch wins)
            let already_exists = self.files.contains_key(&virtual_path);
            if !already_exists {
                // Add to files index
                self.files.insert(
                    virtual_path.clone(),
                    FileSource::new(patch_name, &real_path),
                );
                self.total_files += 1;

                // Add to parent directory listing
                if let Some(entries) = self.directories.get_mut(virtual_dir) {
                    // Check if this entry name already exists in the directory listing
                    if !entries.iter().any(|e| e.name == file_name) {
                        if let Ok(dir_entry) = DirEntry::from_path(&real_path) {
                            entries.push(dir_entry);
                        }
                    }
                }
            }

            // Always recurse into directories to merge contents from different patches
            // (even if the directory entry already exists, we need to add unique files)
            if is_dir {
                self.add_directory_recursive(patch_name, &real_path, &virtual_path)?;
            }
        }

        Ok(())
    }

    /// Resolve a virtual path to its real file source.
    ///
    /// Returns `None` if the path doesn't exist in the index.
    pub fn resolve(&self, virtual_path: &Path) -> Option<&FileSource> {
        self.files.get(virtual_path)
    }

    /// Get the real filesystem path for a virtual path.
    ///
    /// This is a convenience method that returns just the path.
    pub fn resolve_path(&self, virtual_path: &Path) -> Option<&PathBuf> {
        self.resolve(virtual_path).map(|s| &s.real_path)
    }

    /// Check if a virtual path exists in the index.
    pub fn contains(&self, virtual_path: &Path) -> bool {
        self.files.contains_key(virtual_path)
    }

    /// Check if a virtual path is a directory.
    pub fn is_directory(&self, virtual_path: &Path) -> bool {
        self.directories.contains_key(virtual_path)
    }

    /// List entries in a virtual directory.
    ///
    /// Returns an empty vec for non-existent directories.
    pub fn list_directory(&self, virtual_path: &Path) -> Vec<&DirEntry> {
        self.directories
            .get(virtual_path)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get the total number of files in the index.
    pub fn file_count(&self) -> usize {
        self.total_files
    }

    /// Get the total number of directories in the index.
    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }

    /// Get the names of patches included in this index, in priority order.
    pub fn patch_names(&self) -> &[String] {
        &self.patch_names
    }

    /// Check if the index is empty (no patches added).
    pub fn is_empty(&self) -> bool {
        self.patch_names.is_empty()
    }

    /// Iterate over all files in the index.
    pub fn files(&self) -> impl Iterator<Item = (&PathBuf, &FileSource)> {
        self.files.iter()
    }
}

impl Default for PatchUnionIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_structure(temp: &TempDir) -> (PatchInfo, PatchInfo) {
        // Create first patch (A_First - highest priority)
        let patch_a = temp.path().join("A_First");
        std::fs::create_dir_all(patch_a.join("Earth nav data/+30-120")).unwrap();
        std::fs::write(
            patch_a.join("Earth nav data/+30-120/+33-119.dsf"),
            b"patch_a dsf",
        )
        .unwrap();
        std::fs::create_dir_all(patch_a.join("terrain")).unwrap();
        std::fs::write(patch_a.join("terrain/shared.ter"), b"patch_a terrain").unwrap();
        std::fs::write(patch_a.join("terrain/unique_a.ter"), b"unique to a").unwrap();

        // Create second patch (B_Second - lower priority)
        let patch_b = temp.path().join("B_Second");
        std::fs::create_dir_all(patch_b.join("Earth nav data/+30-110")).unwrap();
        std::fs::write(
            patch_b.join("Earth nav data/+30-110/+39-105.dsf"),
            b"patch_b dsf",
        )
        .unwrap();
        std::fs::create_dir_all(patch_b.join("terrain")).unwrap();
        std::fs::write(
            patch_b.join("terrain/shared.ter"),
            b"patch_b terrain (should be shadowed)",
        )
        .unwrap();
        std::fs::write(patch_b.join("terrain/unique_b.ter"), b"unique to b").unwrap();

        let info_a = PatchInfo {
            name: "A_First".to_string(),
            path: patch_a,
            dsf_count: 1,
            terrain_count: 2,
            texture_count: 0,
            is_valid: true,
            validation_errors: Vec::new(),
        };

        let info_b = PatchInfo {
            name: "B_Second".to_string(),
            path: patch_b,
            dsf_count: 1,
            terrain_count: 2,
            texture_count: 0,
            is_valid: true,
            validation_errors: Vec::new(),
        };

        (info_a, info_b)
    }

    #[test]
    fn test_empty_index() {
        let index = PatchUnionIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.directory_count(), 0);
    }

    #[test]
    fn test_build_from_patches() {
        let temp = TempDir::new().unwrap();
        let (info_a, info_b) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a, info_b]).unwrap();

        assert!(!index.is_empty());
        assert_eq!(index.patch_names(), &["A_First", "B_Second"]);
    }

    #[test]
    fn test_resolve_path() {
        let temp = TempDir::new().unwrap();
        let (info_a, info_b) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a, info_b]).unwrap();

        // Resolve a file from patch A
        let dsf_path = Path::new("Earth nav data/+30-120/+33-119.dsf");
        let source = index.resolve(dsf_path).expect("Should resolve DSF");
        assert_eq!(source.patch_name, "A_First");

        // Resolve a file from patch B (not in A)
        let dsf_path_b = Path::new("Earth nav data/+30-110/+39-105.dsf");
        let source_b = index
            .resolve(dsf_path_b)
            .expect("Should resolve DSF from B");
        assert_eq!(source_b.patch_name, "B_Second");
    }

    #[test]
    fn test_first_patch_wins_collision() {
        let temp = TempDir::new().unwrap();
        let (info_a, info_b) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a.clone(), info_b]).unwrap();

        // shared.ter exists in both patches - A should win
        let shared_path = Path::new("terrain/shared.ter");
        let source = index
            .resolve(shared_path)
            .expect("Should resolve shared file");
        assert_eq!(source.patch_name, "A_First");

        // Verify the content is from patch A
        let content = std::fs::read(&source.real_path).unwrap();
        assert_eq!(content, b"patch_a terrain");
    }

    #[test]
    fn test_directory_listing() {
        let temp = TempDir::new().unwrap();
        let (info_a, info_b) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a, info_b]).unwrap();

        // List terrain directory - should have merged entries
        let entries = index.list_directory(Path::new("terrain"));

        // Should have: shared.ter (from A), unique_a.ter, unique_b.ter
        let names: Vec<_> = entries
            .iter()
            .map(|e| e.name.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"shared.ter".to_string()));
        assert!(names.contains(&"unique_a.ter".to_string()));
        assert!(names.contains(&"unique_b.ter".to_string()));
    }

    #[test]
    fn test_is_directory() {
        let temp = TempDir::new().unwrap();
        let (info_a, info_b) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a, info_b]).unwrap();

        assert!(index.is_directory(Path::new(""))); // Root
        assert!(index.is_directory(Path::new("Earth nav data")));
        assert!(index.is_directory(Path::new("terrain")));
        assert!(!index.is_directory(Path::new("terrain/shared.ter"))); // File, not directory
    }

    #[test]
    fn test_contains() {
        let temp = TempDir::new().unwrap();
        let (info_a, _) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a]).unwrap();

        assert!(index.contains(Path::new("Earth nav data/+30-120/+33-119.dsf")));
        assert!(index.contains(Path::new("terrain/unique_a.ter")));
        assert!(!index.contains(Path::new("nonexistent.file")));
    }

    #[test]
    fn test_list_root_directory() {
        let temp = TempDir::new().unwrap();
        let (info_a, _) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a]).unwrap();

        // List root directory
        let entries = index.list_directory(Path::new(""));
        let names: Vec<_> = entries
            .iter()
            .map(|e| e.name.to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"Earth nav data".to_string()));
        assert!(names.contains(&"terrain".to_string()));
    }

    #[test]
    fn test_files_iterator() {
        let temp = TempDir::new().unwrap();
        let (info_a, _) = create_test_structure(&temp);

        let index = PatchUnionIndex::build(&[info_a]).unwrap();

        let file_count = index.files().count();
        assert!(file_count > 0);
        assert_eq!(file_count, index.file_count());
    }

    #[test]
    fn test_invalid_patch_skipped() {
        let temp = TempDir::new().unwrap();
        let (mut info_a, _) = create_test_structure(&temp);

        // Mark patch as invalid
        info_a.is_valid = false;

        let index = PatchUnionIndex::build(&[info_a]).unwrap();

        // Should be empty since the only patch was invalid
        assert!(index.is_empty());
    }
}
