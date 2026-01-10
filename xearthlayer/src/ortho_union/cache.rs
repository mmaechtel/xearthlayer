//! Index caching for fast startup.
//!
//! This module provides caching of the built [`OrthoUnionIndex`] to avoid
//! expensive directory scanning on every startup. The cache is invalidated
//! when sources change (paths, modification times, or configuration).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::index::OrthoUnionIndex;
use super::source::OrthoSource;

/// Cache key for validating cached index.
///
/// The cache is valid only if all fields match the current configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexCacheKey {
    /// XEarthLayer version (cache format may change).
    pub version: String,

    /// Sorted list of source paths.
    pub source_paths: Vec<PathBuf>,

    /// Modification times of source root directories.
    /// Stored as duration since UNIX_EPOCH for serialization.
    pub source_mtimes_secs: Vec<u64>,

    /// Hash of configuration affecting index building.
    pub config_hash: u64,
}

impl IndexCacheKey {
    /// Create a cache key from sources using defaults.
    ///
    /// This is a convenience method that derives the cache key from the sources
    /// themselves, using their paths and modification times.
    pub fn from_sources(sources: &[OrthoSource]) -> Self {
        Self::compute(sources, true).unwrap_or_else(|_| Self {
            version: crate::VERSION.to_string(),
            source_paths: sources.iter().map(|s| s.source_path.clone()).collect(),
            source_mtimes_secs: vec![0; sources.len()],
            config_hash: 0,
        })
    }

    /// Compute cache key from current sources and configuration.
    pub fn compute(sources: &[OrthoSource], patches_enabled: bool) -> io::Result<Self> {
        let mut source_paths = Vec::with_capacity(sources.len());
        let mut source_mtimes_secs = Vec::with_capacity(sources.len());

        for source in sources {
            source_paths.push(source.source_path.clone());

            // Get modification time of source directory
            let mtime = source
                .source_path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);

            let secs = mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            source_mtimes_secs.push(secs);
        }

        // Hash configuration that affects index building
        let mut hasher = DefaultHasher::new();
        patches_enabled.hash(&mut hasher);
        let config_hash = hasher.finish();

        Ok(Self {
            version: crate::VERSION.to_string(),
            source_paths,
            source_mtimes_secs,
            config_hash,
        })
    }
}

/// Cached index data.
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexCache {
    /// Cache key for validation.
    pub key: IndexCacheKey,

    /// The cached index.
    pub index: OrthoUnionIndex,

    /// When the cache was created (secs since UNIX_EPOCH).
    pub created_at_secs: u64,
}

impl IndexCache {
    /// Create a new cache entry.
    pub fn new(key: IndexCacheKey, index: OrthoUnionIndex) -> Self {
        let created_at_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            key,
            index,
            created_at_secs,
        }
    }

    /// Load cache from file.
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);

        bincode::deserialize_from(reader).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to deserialize index cache: {}", e),
            )
        })
    }

    /// Save cache to file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to temp file first, then rename for atomicity
        let temp_path = path.with_extension("tmp");
        let file = std::fs::File::create(&temp_path)?;
        let writer = BufWriter::new(file);

        bincode::serialize_into(writer, self)
            .map_err(|e| io::Error::other(format!("Failed to serialize index cache: {}", e)))?;

        // Atomic rename
        std::fs::rename(&temp_path, path)?;

        Ok(())
    }

    /// Check if this cache is valid for the given key.
    pub fn is_valid(&self, current_key: &IndexCacheKey) -> bool {
        self.key == *current_key
    }

    /// Get cache age in seconds.
    pub fn age_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        now.saturating_sub(self.created_at_secs)
    }

    /// Get human-readable cache age.
    pub fn age_human(&self) -> String {
        let secs = self.age_secs();

        if secs < 60 {
            format!("{}s ago", secs)
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        }
    }
}

/// Get the default cache file path.
pub fn default_cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".xearthlayer").join("ortho_union_index.cache"))
}

/// Try to load a valid cached index.
///
/// Returns `None` if:
/// - Cache file doesn't exist
/// - Cache is invalid (key mismatch)
/// - Cache is corrupted
pub fn try_load_cached_index(
    cache_path: &Path,
    current_key: &IndexCacheKey,
) -> Option<OrthoUnionIndex> {
    let cache = IndexCache::load(cache_path).ok()?;

    if cache.is_valid(current_key) {
        tracing::info!(
            age = %cache.age_human(),
            sources = cache.index.source_count(),
            files = cache.index.file_count(),
            "Using cached ortho union index"
        );
        Some(cache.index)
    } else {
        tracing::debug!("Cache invalid: key mismatch");
        None
    }
}

/// Save index to cache.
pub fn save_index_cache(
    cache_path: &Path,
    key: IndexCacheKey,
    index: &OrthoUnionIndex,
) -> io::Result<()> {
    let cache = IndexCache::new(key, index.clone());
    cache.save(cache_path)?;

    tracing::info!(
        path = %cache_path.display(),
        sources = index.source_count(),
        files = index.file_count(),
        "Saved ortho union index to cache"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ortho_union::SourceType;
    use tempfile::TempDir;

    fn create_test_source(temp: &TempDir, name: &str) -> OrthoSource {
        let path = temp.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        OrthoSource {
            sort_key: name.to_string(),
            display_name: name.to_string(),
            source_path: path,
            source_type: SourceType::RegionalPackage,
            enabled: true,
        }
    }

    #[test]
    fn test_cache_key_compute() {
        let temp = TempDir::new().unwrap();
        let sources = vec![
            create_test_source(&temp, "eu"),
            create_test_source(&temp, "na"),
        ];

        let key = IndexCacheKey::compute(&sources, true).unwrap();

        assert_eq!(key.version, crate::VERSION);
        assert_eq!(key.source_paths.len(), 2);
        assert_eq!(key.source_mtimes_secs.len(), 2);
    }

    #[test]
    fn test_cache_key_equality() {
        let temp = TempDir::new().unwrap();
        let sources = vec![create_test_source(&temp, "na")];

        let key1 = IndexCacheKey::compute(&sources, true).unwrap();
        let key2 = IndexCacheKey::compute(&sources, true).unwrap();

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_config_hash_differs() {
        let temp = TempDir::new().unwrap();
        let sources = vec![create_test_source(&temp, "na")];

        let key1 = IndexCacheKey::compute(&sources, true).unwrap();
        let key2 = IndexCacheKey::compute(&sources, false).unwrap();

        assert_ne!(key1.config_hash, key2.config_hash);
    }

    #[test]
    fn test_cache_save_and_load() {
        let temp = TempDir::new().unwrap();
        let cache_path = temp.path().join("test.cache");

        let key = IndexCacheKey {
            version: "test".to_string(),
            source_paths: vec![PathBuf::from("/test")],
            source_mtimes_secs: vec![12345],
            config_hash: 999,
        };

        let index = OrthoUnionIndex::default();
        let cache = IndexCache::new(key.clone(), index);

        cache.save(&cache_path).unwrap();

        let loaded = IndexCache::load(&cache_path).unwrap();
        assert!(loaded.is_valid(&key));
    }

    #[test]
    fn test_cache_age_human() {
        let key = IndexCacheKey {
            version: "test".to_string(),
            source_paths: vec![],
            source_mtimes_secs: vec![],
            config_hash: 0,
        };

        let mut cache = IndexCache::new(key, OrthoUnionIndex::default());

        // Just created - should be "0s ago" or similar
        let age = cache.age_human();
        assert!(age.contains("s ago") || age.contains("0"));

        // Simulate 2 hours ago
        cache.created_at_secs -= 7200;
        assert_eq!(cache.age_human(), "2h ago");
    }
}
