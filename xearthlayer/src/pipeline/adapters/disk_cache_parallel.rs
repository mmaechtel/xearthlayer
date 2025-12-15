//! Optimized parallel disk cache adapter.
//!
//! This adapter uses a dedicated thread pool for disk I/O operations to avoid
//! contention with tokio's blocking thread pool. It's optimized for reading
//! many small files (chunks) in parallel.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Parallel disk cache adapter with dedicated I/O threads.
///
/// Uses a semaphore to limit concurrent I/O operations and avoid overwhelming
/// the filesystem. Reads are performed using `spawn_blocking` to avoid blocking
/// the async runtime.
///
/// # Performance Tuning
///
/// - `max_concurrent_io`: Controls max parallel disk operations (default: 64)
/// - For SSDs, higher values (128-256) may improve throughput
/// - For HDDs, lower values (16-32) prevent seek thrashing
pub struct ParallelDiskCache {
    cache_dir: PathBuf,
    provider: String,
    /// Semaphore to limit concurrent I/O
    io_semaphore: Arc<Semaphore>,
}

impl ParallelDiskCache {
    /// Creates a new parallel disk cache adapter.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Root directory for the cache
    /// * `provider` - Provider name for directory hierarchy
    /// * `max_concurrent_io` - Maximum concurrent disk operations
    pub fn new(cache_dir: PathBuf, provider: impl Into<String>, max_concurrent_io: usize) -> Self {
        Self {
            cache_dir,
            provider: provider.into(),
            io_semaphore: Arc::new(Semaphore::new(max_concurrent_io)),
        }
    }

    /// Creates a new parallel disk cache with default concurrency (256).
    ///
    /// This higher default is optimized for SSDs and NVMe drives where
    /// concurrent I/O improves throughput. For HDDs, consider using
    /// `new()` with a lower value (16-32) to prevent seek thrashing.
    pub fn with_defaults(cache_dir: PathBuf, provider: impl Into<String>) -> Self {
        Self::new(cache_dir, provider, 256)
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

        // Acquire semaphore permit to limit concurrent I/O
        let _permit = self.io_semaphore.acquire().await.ok()?;

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

        // Acquire semaphore permit
        let _permit = self
            .io_semaphore
            .acquire()
            .await
            .map_err(|_| std::io::Error::other("semaphore closed"))?;

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
    /// Creates a new batched disk cache.
    pub fn new(cache_dir: PathBuf, provider: impl Into<String>, max_concurrent_io: usize) -> Self {
        Self {
            inner: ParallelDiskCache::new(cache_dir, provider, max_concurrent_io),
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
}
