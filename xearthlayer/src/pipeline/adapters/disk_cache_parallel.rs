//! Optimized parallel disk cache adapter.
//!
//! This adapter uses a dedicated thread pool for disk I/O operations to avoid
//! contention with tokio's blocking thread pool. It's optimized for reading
//! many small files (chunks) in parallel.
//!
//! # Global Disk I/O Limiting
//!
//! When multiple packages are mounted, each with its own `ParallelDiskCache`,
//! uncontrolled concurrent disk reads can overwhelm the system. The cache
//! supports an optional shared `ConcurrencyLimiter` to coordinate disk I/O
//! across all cache instances.
//!
//! ```ignore
//! use std::sync::Arc;
//! use xearthlayer::pipeline::{ConcurrencyLimiter, adapters::ParallelDiskCache};
//!
//! // Create a shared limiter
//! let limiter = Arc::new(ConcurrencyLimiter::with_defaults("disk_io"));
//!
//! // Share it across multiple cache instances
//! let cache1 = ParallelDiskCache::with_shared_limiter(path1, "bing", Arc::clone(&limiter));
//! let cache2 = ParallelDiskCache::with_shared_limiter(path2, "bing", Arc::clone(&limiter));
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use crate::pipeline::ConcurrencyLimiter;

/// Parallel disk cache adapter with dedicated I/O threads.
///
/// Uses a concurrency limiter to prevent overwhelming the filesystem with
/// too many concurrent reads. Reads are performed using `spawn_blocking`
/// to avoid blocking the async runtime.
///
/// # Global vs Local Limiting
///
/// The cache can use either:
/// - A **shared limiter** passed via `with_shared_limiter()` for global
///   coordination across multiple cache instances
/// - A **local limiter** created internally (legacy behavior)
///
/// When multiple packages are mounted simultaneously, using a shared limiter
/// prevents the combined disk I/O from overwhelming the system.
///
/// # Performance Tuning
///
/// - For SSDs/NVMe: Higher concurrency (128-256) improves throughput
/// - For HDDs: Lower concurrency (16-32) prevents seek thrashing
/// - Default scaling: `min(num_cpus * 16, 256)`
pub struct ParallelDiskCache {
    cache_dir: PathBuf,
    provider: String,
    /// Concurrency limiter for disk I/O operations
    io_limiter: Arc<ConcurrencyLimiter>,
}

impl ParallelDiskCache {
    /// Creates a new parallel disk cache adapter with a local limiter.
    ///
    /// This creates an internal `ConcurrencyLimiter` that is not shared
    /// with other cache instances. For multi-package scenarios, prefer
    /// `with_shared_limiter()` to coordinate disk I/O globally.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Root directory for the cache
    /// * `provider` - Provider name for directory hierarchy
    /// * `max_concurrent_io` - Maximum concurrent disk operations
    pub fn new(cache_dir: PathBuf, provider: impl Into<String>, max_concurrent_io: usize) -> Self {
        let provider_str = provider.into();
        let label = format!("disk_cache_{}", provider_str);
        Self {
            cache_dir,
            provider: provider_str,
            io_limiter: Arc::new(ConcurrencyLimiter::new(max_concurrent_io, label)),
        }
    }

    /// Creates a new parallel disk cache with a shared concurrency limiter.
    ///
    /// This is the **recommended** constructor when multiple packages are
    /// mounted simultaneously. The shared limiter coordinates disk I/O
    /// across all cache instances to prevent system overload.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Root directory for the cache
    /// * `provider` - Provider name for directory hierarchy
    /// * `limiter` - Shared concurrency limiter for disk I/O
    ///
    /// # Example
    ///
    /// ```ignore
    /// let limiter = Arc::new(ConcurrencyLimiter::with_defaults("global_disk_io"));
    /// let cache = ParallelDiskCache::with_shared_limiter(path, "bing", limiter);
    /// ```
    pub fn with_shared_limiter(
        cache_dir: PathBuf,
        provider: impl Into<String>,
        limiter: Arc<ConcurrencyLimiter>,
    ) -> Self {
        Self {
            cache_dir,
            provider: provider.into(),
            io_limiter: limiter,
        }
    }

    /// Creates a new parallel disk cache with default concurrency.
    ///
    /// Uses the disk I/O optimized scaling formula: `min(num_cpus * 4, 64)`.
    /// This is more conservative than HTTP concurrency because disk I/O
    /// is queue-depth limited (especially on HDDs and SATA SSDs).
    ///
    /// **Note**: This creates a local limiter. For multi-package scenarios,
    /// use `with_shared_limiter()` instead.
    pub fn with_defaults(cache_dir: PathBuf, provider: impl Into<String>) -> Self {
        let provider_str = provider.into();
        Self {
            cache_dir,
            provider: provider_str.clone(),
            io_limiter: Arc::new(ConcurrencyLimiter::with_disk_io_defaults(format!(
                "disk_cache_{}",
                provider_str
            ))),
        }
    }

    /// Returns the cache directory.
    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    /// Returns the provider name.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Constructs the path for a chunk file.
    fn chunk_path(
        &self,
        tile_row: u32,
        tile_col: u32,
        zoom: u8,
        chunk_row: u8,
        chunk_col: u8,
    ) -> PathBuf {
        self.cache_dir
            .join("chunks")
            .join(&self.provider)
            .join(zoom.to_string())
            .join(tile_row.to_string())
            .join(tile_col.to_string())
            .join(format!("{}_{}.jpg", chunk_row, chunk_col))
    }
}

impl crate::pipeline::DiskCache for ParallelDiskCache {
    async fn get(
        &self,
        tile_row: u32,
        tile_col: u32,
        zoom: u8,
        chunk_row: u8,
        chunk_col: u8,
    ) -> Option<Vec<u8>> {
        let path = self.chunk_path(tile_row, tile_col, zoom, chunk_row, chunk_col);

        // Acquire permit from the concurrency limiter to prevent overwhelming disk I/O
        let _permit = self.io_limiter.acquire().await;

        // Use spawn_blocking for the actual disk read
        // This moves the blocking I/O to tokio's blocking thread pool
        tokio::task::spawn_blocking(move || std::fs::read(&path).ok())
            .await
            .ok()
            .flatten()
    }

    async fn put(
        &self,
        tile_row: u32,
        tile_col: u32,
        zoom: u8,
        chunk_row: u8,
        chunk_col: u8,
        data: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        let path = self.chunk_path(tile_row, tile_col, zoom, chunk_row, chunk_col);

        // Acquire permit from the concurrency limiter
        let _permit = self.io_limiter.acquire().await;

        // Use spawn_blocking for the actual disk write
        tokio::task::spawn_blocking(move || {
            // Create parent directories
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, data)
        })
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?
    }
}

/// Batched disk cache that pre-checks file existence.
///
/// This adapter first checks which files exist using a batch stat operation,
/// then only reads the files that are present. This can be faster when many
/// chunks are missing from cache.
pub struct BatchedDiskCache {
    inner: ParallelDiskCache,
}

impl BatchedDiskCache {
    /// Creates a new batched disk cache with a local limiter.
    pub fn new(cache_dir: PathBuf, provider: impl Into<String>, max_concurrent_io: usize) -> Self {
        Self {
            inner: ParallelDiskCache::new(cache_dir, provider, max_concurrent_io),
        }
    }

    /// Creates a new batched disk cache with a shared concurrency limiter.
    pub fn with_shared_limiter(
        cache_dir: PathBuf,
        provider: impl Into<String>,
        limiter: Arc<ConcurrencyLimiter>,
    ) -> Self {
        Self {
            inner: ParallelDiskCache::with_shared_limiter(cache_dir, provider, limiter),
        }
    }

    /// Returns the cache directory.
    pub fn cache_dir(&self) -> &PathBuf {
        self.inner.cache_dir()
    }

    /// Returns the provider name.
    pub fn provider(&self) -> &str {
        self.inner.provider()
    }

    /// Check which chunks exist in cache for a tile (batch operation).
    ///
    /// Returns a 16x16 boolean grid indicating which chunks are cached.
    pub async fn check_tile_chunks(
        &self,
        tile_row: u32,
        tile_col: u32,
        zoom: u8,
    ) -> [[bool; 16]; 16] {
        let cache_dir = self.inner.cache_dir.clone();
        let provider = self.inner.provider.clone();

        tokio::task::spawn_blocking(move || {
            let mut result = [[false; 16]; 16];
            let tile_dir = cache_dir
                .join("chunks")
                .join(&provider)
                .join(zoom.to_string())
                .join(tile_row.to_string())
                .join(tile_col.to_string());

            if !tile_dir.exists() {
                return result;
            }

            // Read directory once and check all files
            if let Ok(entries) = std::fs::read_dir(&tile_dir) {
                for entry in entries.flatten() {
                    let filename = entry.file_name();
                    let name = filename.to_string_lossy();
                    // Parse "row_col.jpg"
                    if let Some(stem) = name.strip_suffix(".jpg") {
                        let parts: Vec<&str> = stem.split('_').collect();
                        if parts.len() == 2 {
                            if let (Ok(row), Ok(col)) =
                                (parts[0].parse::<usize>(), parts[1].parse::<usize>())
                            {
                                if row < 16 && col < 16 {
                                    result[row][col] = true;
                                }
                            }
                        }
                    }
                }
            }

            result
        })
        .await
        .unwrap_or([[false; 16]; 16])
    }
}

impl crate::pipeline::DiskCache for BatchedDiskCache {
    async fn get(
        &self,
        tile_row: u32,
        tile_col: u32,
        zoom: u8,
        chunk_row: u8,
        chunk_col: u8,
    ) -> Option<Vec<u8>> {
        self.inner
            .get(tile_row, tile_col, zoom, chunk_row, chunk_col)
            .await
    }

    async fn put(
        &self,
        tile_row: u32,
        tile_col: u32,
        zoom: u8,
        chunk_row: u8,
        chunk_col: u8,
        data: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        self.inner
            .put(tile_row, tile_col, zoom, chunk_row, chunk_col, data)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::DiskCache;

    #[test]
    fn test_parallel_disk_cache_path_construction() {
        let cache = ParallelDiskCache::new(PathBuf::from("/cache"), "bing", 64);
        let path = cache.chunk_path(100, 200, 16, 5, 10);
        assert_eq!(
            path,
            PathBuf::from("/cache/chunks/bing/16/100/200/5_10.jpg")
        );
    }

    #[tokio::test]
    async fn test_parallel_disk_cache_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ParallelDiskCache::new(temp_dir.path().to_path_buf(), "test", 64);

        // Initially empty
        let result = cache.get(100, 200, 16, 0, 0).await;
        assert!(result.is_none());

        // Put some data
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        cache.put(100, 200, 16, 0, 0, data.clone()).await.unwrap();

        // Should now be cached
        let result = cache.get(100, 200, 16, 0, 0).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), data);
    }

    #[tokio::test]
    async fn test_batched_disk_cache_check_tile_chunks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = BatchedDiskCache::new(temp_dir.path().to_path_buf(), "test", 64);

        // Initially all false
        let exists = cache.check_tile_chunks(100, 200, 16).await;
        assert!(!exists[0][0]);

        // Add some chunks
        cache.put(100, 200, 16, 0, 0, vec![1, 2, 3]).await.unwrap();
        cache.put(100, 200, 16, 5, 10, vec![4, 5, 6]).await.unwrap();

        // Check again
        let exists = cache.check_tile_chunks(100, 200, 16).await;
        assert!(exists[0][0]);
        assert!(exists[5][10]);
        assert!(!exists[1][1]);
    }

    #[tokio::test]
    async fn test_parallel_concurrent_reads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(ParallelDiskCache::new(
            temp_dir.path().to_path_buf(),
            "test",
            64,
        ));

        // Write some chunks
        for row in 0..16u8 {
            for col in 0..16u8 {
                let data = vec![row, col];
                cache.put(100, 200, 16, row, col, data).await.unwrap();
            }
        }

        // Read all chunks concurrently
        let mut handles = Vec::new();
        for row in 0..16u8 {
            for col in 0..16u8 {
                let cache = Arc::clone(&cache);
                handles.push(tokio::spawn(async move {
                    cache.get(100, 200, 16, row, col).await
                }));
            }
        }

        // Wait for all reads
        let results: Vec<_> = futures::future::join_all(handles).await;

        // Verify all succeeded
        for result in results {
            assert!(result.is_ok());
            assert!(result.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn test_shared_limiter_across_caches() {
        let temp_dir1 = tempfile::tempdir().unwrap();
        let temp_dir2 = tempfile::tempdir().unwrap();

        // Create a shared limiter with low concurrency for testing
        let shared_limiter = Arc::new(ConcurrencyLimiter::new(4, "shared_test"));

        // Create two caches sharing the same limiter
        let cache1 = Arc::new(ParallelDiskCache::with_shared_limiter(
            temp_dir1.path().to_path_buf(),
            "provider1",
            Arc::clone(&shared_limiter),
        ));
        let cache2 = Arc::new(ParallelDiskCache::with_shared_limiter(
            temp_dir2.path().to_path_buf(),
            "provider2",
            Arc::clone(&shared_limiter),
        ));

        // Write some data to both caches
        cache1.put(100, 200, 16, 0, 0, vec![1, 2, 3]).await.unwrap();
        cache2.put(100, 200, 16, 0, 0, vec![4, 5, 6]).await.unwrap();

        // Both caches can read their data
        let result1 = cache1.get(100, 200, 16, 0, 0).await;
        let result2 = cache2.get(100, 200, 16, 0, 0).await;

        assert_eq!(result1, Some(vec![1, 2, 3]));
        assert_eq!(result2, Some(vec![4, 5, 6]));

        // Verify the shared limiter is being used by checking peak usage
        // (The peak should be recorded across both cache operations)
        assert!(shared_limiter.peak_in_flight() > 0);
    }

    #[tokio::test]
    async fn test_with_defaults_creates_local_limiter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ParallelDiskCache::with_defaults(temp_dir.path().to_path_buf(), "test");

        // Should work normally with local limiter
        cache.put(100, 200, 16, 0, 0, vec![1, 2, 3]).await.unwrap();
        let result = cache.get(100, 200, 16, 0, 0).await;
        assert_eq!(result, Some(vec![1, 2, 3]));
    }
}
