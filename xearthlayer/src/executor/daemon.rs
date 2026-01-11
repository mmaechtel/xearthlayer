//! Job executor daemon for processing DDS tile requests.
//!
//! The [`ExecutorDaemon`] is a long-running background service that:
//! - Receives tile requests via a channel
//! - Checks memory cache for fast-path cache hits
//! - Uses request coalescing to prevent duplicate work
//! - Creates and executes DDS generation jobs
//! - Returns results to callers
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                       ExecutorDaemon                             │
//! │                                                                  │
//! │  JobRequest ──► ┌─────────────┐                                 │
//! │                 │ Cache Check │──► Hit ──► Return immediately   │
//! │                 └──────┬──────┘                                 │
//! │                        │ Miss                                   │
//! │                        ▼                                        │
//! │                 ┌─────────────┐                                 │
//! │                 │  Coalescer  │──► Coalesced ──► Wait for result│
//! │                 └──────┬──────┘                                 │
//! │                        │ New                                    │
//! │                        ▼                                        │
//! │                 ┌─────────────┐                                 │
//! │                 │   Factory   │──► Create DdsGenerateJob        │
//! │                 └──────┬──────┘                                 │
//! │                        ▼                                        │
//! │                 ┌─────────────┐                                 │
//! │                 │  Executor   │──► Run job, wait for completion │
//! │                 └──────┬──────┘                                 │
//! │                        ▼                                        │
//! │                 ┌─────────────┐                                 │
//! │                 │ Cache Read  │──► Return DDS data              │
//! │                 └─────────────┘                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::executor::{ExecutorDaemon, ExecutorDaemonConfig};
//!
//! let config = ExecutorDaemonConfig::default();
//! let (daemon, request_tx) = ExecutorDaemon::new(config, factory, memory_cache);
//!
//! // Start daemon
//! let shutdown = CancellationToken::new();
//! tokio::spawn(daemon.run(shutdown.clone()));
//!
//! // Submit request
//! let (request, response_rx) = JobRequest::fuse(tile, CancellationToken::new());
//! request_tx.send(request).await?;
//! let response = response_rx.await?;
//! ```

use crate::executor::{ExecutorConfig, JobExecutor, JobStatus, JobSubmitter as ExecutorSubmitter};
use crate::fuse::DdsResponse as FuseDdsResponse;
use crate::jobs::DdsJobFactory;
use crate::pipeline::{CoalesceResult, RequestCoalescer};
use crate::runtime::{DdsResponse, JobRequest};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

// =============================================================================
// Configuration
// =============================================================================

/// Default channel capacity for job requests.
pub const DEFAULT_REQUEST_CHANNEL_CAPACITY: usize = 1000;

/// Configuration for the executor daemon.
#[derive(Clone, Debug)]
pub struct ExecutorDaemonConfig {
    /// Job executor configuration.
    pub executor: ExecutorConfig,

    /// Request channel capacity.
    pub channel_capacity: usize,
}

impl Default for ExecutorDaemonConfig {
    fn default() -> Self {
        Self {
            executor: ExecutorConfig::default(),
            channel_capacity: DEFAULT_REQUEST_CHANNEL_CAPACITY,
        }
    }
}

// =============================================================================
// Memory Cache Trait (minimal interface for daemon)
// =============================================================================

/// Minimal interface for memory cache operations.
///
/// This trait allows the daemon to check cache and read results without
/// depending on the full `MemoryCache` trait from the pipeline module.
pub trait DaemonMemoryCache: Send + Sync + 'static {
    /// Gets a tile from the cache.
    fn get(
        &self,
        row: u32,
        col: u32,
        zoom: u8,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<u8>>> + Send + '_>>;
}

/// Blanket implementation for any type implementing the pipeline MemoryCache trait.
impl<T> DaemonMemoryCache for T
where
    T: crate::pipeline::MemoryCache,
{
    fn get(
        &self,
        row: u32,
        col: u32,
        zoom: u8,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<u8>>> + Send + '_>> {
        Box::pin(async move { crate::pipeline::MemoryCache::get(self, row, col, zoom).await })
    }
}

// =============================================================================
// Executor Daemon
// =============================================================================

/// The job executor daemon.
///
/// Owns the job executor and receives requests from producers via channel.
/// Runs as a long-lived background task.
///
/// # Type Parameters
///
/// * `F` - Factory type for creating DDS generation jobs
/// * `M` - Memory cache type for cache lookups
pub struct ExecutorDaemon<F, M>
where
    F: DdsJobFactory,
    M: DaemonMemoryCache,
{
    /// The job executor.
    executor: JobExecutor,

    /// Job submitter for sending jobs to executor.
    submitter: ExecutorSubmitter,

    /// Factory for creating DDS jobs.
    factory: Arc<F>,

    /// Memory cache for fast-path cache hits.
    memory_cache: Arc<M>,

    /// Request coalescer (prevents duplicate work).
    coalescer: Arc<RequestCoalescer>,

    /// Channel receiver for requests.
    request_rx: mpsc::Receiver<JobRequest>,
}

impl<F, M> ExecutorDaemon<F, M>
where
    F: DdsJobFactory,
    M: DaemonMemoryCache,
{
    /// Creates a new daemon with its channel.
    ///
    /// Returns the daemon and a sender that can be cloned for producers.
    ///
    /// # Arguments
    ///
    /// * `config` - Daemon configuration
    /// * `factory` - Factory for creating DDS jobs
    /// * `memory_cache` - Memory cache for fast-path lookups
    pub fn new(
        config: ExecutorDaemonConfig,
        factory: Arc<F>,
        memory_cache: Arc<M>,
    ) -> (Self, mpsc::Sender<JobRequest>) {
        Self::with_coalescer(
            config,
            factory,
            memory_cache,
            Arc::new(RequestCoalescer::new()),
        )
    }

    /// Creates a new daemon with an existing coalescer.
    ///
    /// This allows sharing a coalescer with the legacy pipeline during migration.
    pub fn with_coalescer(
        config: ExecutorDaemonConfig,
        factory: Arc<F>,
        memory_cache: Arc<M>,
        coalescer: Arc<RequestCoalescer>,
    ) -> (Self, mpsc::Sender<JobRequest>) {
        let (request_tx, request_rx) = mpsc::channel(config.channel_capacity);
        let (executor, submitter) = JobExecutor::new(config.executor);

        let daemon = Self {
            executor,
            submitter,
            factory,
            memory_cache,
            coalescer,
            request_rx,
        };

        (daemon, request_tx)
    }

    /// Runs the daemon until shutdown is signalled.
    ///
    /// This is the main event loop that:
    /// - Receives new job requests
    /// - Checks cache for hits
    /// - Runs jobs via the executor
    /// - Returns results to callers
    pub async fn run(self, shutdown: CancellationToken) {
        info!("Executor daemon starting");

        let Self {
            executor,
            submitter,
            factory,
            memory_cache,
            coalescer,
            mut request_rx,
        } = self;

        // Spawn the executor in a separate task
        let executor_shutdown = shutdown.clone();
        let executor_handle = tokio::spawn(async move {
            executor.run(executor_shutdown).await;
        });

        // Main request loop
        loop {
            tokio::select! {
                biased;

                // Check for shutdown
                _ = shutdown.cancelled() => {
                    info!("Executor daemon shutting down");
                    break;
                }

                // Receive new job requests
                Some(request) = request_rx.recv() => {
                    Self::handle_request(
                        request,
                        &submitter,
                        &factory,
                        &memory_cache,
                        &coalescer,
                    ).await;
                }
            }
        }

        // Wait for executor to finish
        let _ = executor_handle.await;
        info!("Executor daemon stopped");
    }

    async fn handle_request(
        request: JobRequest,
        submitter: &ExecutorSubmitter,
        factory: &Arc<F>,
        memory_cache: &Arc<M>,
        coalescer: &Arc<RequestCoalescer>,
    ) {
        let start = Instant::now();
        let tile = request.tile;
        let priority = request.priority;
        let origin = request.origin;

        debug!(
            tile_row = tile.row,
            tile_col = tile.col,
            tile_zoom = tile.zoom,
            priority = ?priority,
            origin = ?origin,
            "Received job request"
        );

        // Check for cancellation first
        if request.cancellation.is_cancelled() {
            debug!(tile = ?tile, "Request already cancelled");
            if let Some(tx) = request.response_tx {
                let _ = tx.send(DdsResponse::empty(start.elapsed()));
            }
            return;
        }

        // Fast path: check memory cache first
        if let Some(data) = memory_cache.get(tile.row, tile.col, tile.zoom).await {
            let duration = start.elapsed();
            debug!(
                tile = ?tile,
                duration_ms = duration.as_millis(),
                "Cache hit"
            );

            if let Some(tx) = request.response_tx {
                let _ = tx.send(DdsResponse::cache_hit(data, duration));
            }
            return;
        }

        // Check coalescer for in-flight request
        let coalesce_result = coalescer.register(tile);

        match coalesce_result {
            CoalesceResult::Coalesced(mut rx) => {
                // Wait for existing request to complete
                debug!(tile = ?tile, "Request coalesced - waiting for in-flight processing");

                if let Some(response_tx) = request.response_tx {
                    tokio::spawn(async move {
                        match rx.recv().await {
                            Ok(result) => {
                                let response =
                                    DdsResponse::from_fuse(FuseDdsResponse::from(result));
                                let _ = response_tx.send(response);
                            }
                            Err(_) => {
                                // Channel closed - send empty response
                                let _ = response_tx.send(DdsResponse::empty(start.elapsed()));
                            }
                        }
                    });
                }
            }
            CoalesceResult::NewRequest { tile, .. } => {
                // Create and submit new job
                let job = factory.create_job(tile, priority);
                let handle = submitter.try_submit_boxed(job);

                match handle {
                    Some(mut handle) => {
                        let memory_cache = Arc::clone(memory_cache);
                        let coalescer = Arc::clone(coalescer);
                        let cancellation = request.cancellation.clone();

                        tokio::spawn(async move {
                            // Wait for job completion
                            tokio::select! {
                                _ = handle.wait() => {
                                    let status = handle.status();
                                    let duration = start.elapsed();

                                    // Read result from cache
                                    let data = if status == JobStatus::Succeeded {
                                        memory_cache.get(tile.row, tile.col, tile.zoom).await
                                            .unwrap_or_default()
                                    } else {
                                        Vec::new()
                                    };

                                    let response = DdsResponse::cache_miss(data.clone(), duration);

                                    // Notify coalescer
                                    let fuse_response = FuseDdsResponse {
                                        data,
                                        cache_hit: false,
                                        duration,
                                    };
                                    coalescer.complete(tile, fuse_response);

                                    // Send response if requested
                                    if let Some(tx) = request.response_tx {
                                        let _ = tx.send(response);
                                    }
                                }
                                _ = cancellation.cancelled() => {
                                    debug!(tile = ?tile, "Job cancelled");
                                    handle.kill();
                                    coalescer.cancel(tile);

                                    if let Some(tx) = request.response_tx {
                                        let _ = tx.send(DdsResponse::empty(start.elapsed()));
                                    }
                                }
                            }
                        });
                    }
                    None => {
                        warn!(tile = ?tile, "Failed to submit job - executor may be shutdown");
                        coalescer.cancel(tile);

                        if let Some(tx) = request.response_tx {
                            let _ = tx.send(DdsResponse::empty(start.elapsed()));
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coord::TileCoord;
    use crate::executor::{ErrorPolicy, Job, JobId, JobResult, Priority, Task};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    fn test_tile() -> TileCoord {
        TileCoord {
            row: 100,
            col: 200,
            zoom: 14,
        }
    }

    /// Mock memory cache for testing
    struct MockMemoryCache {
        data: Mutex<HashMap<(u32, u32, u8), Vec<u8>>>,
    }

    impl MockMemoryCache {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, row: u32, col: u32, zoom: u8, data: Vec<u8>) {
            self.data.lock().unwrap().insert((row, col, zoom), data);
        }
    }

    impl DaemonMemoryCache for MockMemoryCache {
        fn get(
            &self,
            row: u32,
            col: u32,
            zoom: u8,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<u8>>> + Send + '_>>
        {
            let data = self.data.lock().unwrap().get(&(row, col, zoom)).cloned();
            Box::pin(async move { data })
        }
    }

    /// Mock job factory
    struct MockJobFactory;

    /// Mock job that does nothing
    struct MockJob {
        id: JobId,
        priority: Priority,
    }

    impl Job for MockJob {
        fn id(&self) -> JobId {
            self.id.clone()
        }

        fn name(&self) -> &str {
            "MockDdsGenerate"
        }

        fn error_policy(&self) -> ErrorPolicy {
            ErrorPolicy::FailFast
        }

        fn priority(&self) -> Priority {
            self.priority
        }

        fn create_tasks(&self) -> Vec<Box<dyn Task>> {
            vec![] // No tasks - completes immediately
        }

        fn on_complete(&self, result: &JobResult) -> JobStatus {
            if result.failed_tasks.is_empty() {
                JobStatus::Succeeded
            } else {
                JobStatus::Failed
            }
        }
    }

    impl DdsJobFactory for MockJobFactory {
        fn create_job(&self, tile: TileCoord, priority: Priority) -> Box<dyn Job> {
            Box::new(MockJob {
                id: JobId::new(format!("mock-{}_{}_ZL{}", tile.row, tile.col, tile.zoom)),
                priority,
            })
        }
    }

    #[test]
    fn test_config_default() {
        let config = ExecutorDaemonConfig::default();
        assert_eq!(config.channel_capacity, DEFAULT_REQUEST_CHANNEL_CAPACITY);
    }

    #[tokio::test]
    async fn test_daemon_creation() {
        let factory = Arc::new(MockJobFactory);
        let cache = Arc::new(MockMemoryCache::new());

        let (daemon, tx) = ExecutorDaemon::new(ExecutorDaemonConfig::default(), factory, cache);

        // Verify channel is open
        assert!(!tx.is_closed());

        // We can't easily verify the daemon's internal state without running it
        // Just verify it was created successfully
        drop(daemon);
    }

    #[tokio::test]
    async fn test_cache_hit_fast_path() {
        let factory = Arc::new(MockJobFactory);
        let cache = Arc::new(MockMemoryCache::new());

        // Pre-populate cache
        let tile = test_tile();
        cache.insert(tile.row, tile.col, tile.zoom, vec![1, 2, 3]);

        let config = ExecutorDaemonConfig::default();
        let (daemon, tx) = ExecutorDaemon::new(config, factory, cache);

        let shutdown = CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        // Start daemon
        let daemon_handle = tokio::spawn(async move {
            daemon.run(shutdown_clone).await;
        });

        // Send request
        let (request, rx) = JobRequest::fuse(tile, CancellationToken::new());
        tx.send(request).await.unwrap();

        // Should get cache hit response quickly
        let response = tokio::time::timeout(Duration::from_secs(1), rx)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.data, vec![1, 2, 3]);
        assert!(response.cache_hit);

        // Shutdown
        shutdown.cancel();
        let _ = daemon_handle.await;
    }

    #[tokio::test]
    async fn test_prefetch_request_no_response() {
        let factory = Arc::new(MockJobFactory);
        let cache = Arc::new(MockMemoryCache::new());

        let config = ExecutorDaemonConfig::default();
        let (daemon, tx) = ExecutorDaemon::new(config, factory, cache);

        let shutdown = CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        let daemon_handle = tokio::spawn(async move {
            daemon.run(shutdown_clone).await;
        });

        // Send prefetch request (no response channel)
        let request = JobRequest::prefetch(test_tile());
        tx.send(request).await.unwrap();

        // Give it a moment to process
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Shutdown
        shutdown.cancel();
        let _ = daemon_handle.await;
    }

    #[tokio::test]
    async fn test_cancelled_request_returns_empty() {
        let factory = Arc::new(MockJobFactory);
        let cache = Arc::new(MockMemoryCache::new());

        let config = ExecutorDaemonConfig::default();
        let (daemon, tx) = ExecutorDaemon::new(config, factory, cache);

        let shutdown = CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        let daemon_handle = tokio::spawn(async move {
            daemon.run(shutdown_clone).await;
        });

        // Create already-cancelled request
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (request, rx) = JobRequest::fuse(test_tile(), cancellation);
        tx.send(request).await.unwrap();

        // Should get empty response
        let response = tokio::time::timeout(Duration::from_secs(1), rx)
            .await
            .unwrap()
            .unwrap();

        assert!(!response.has_data());

        shutdown.cancel();
        let _ = daemon_handle.await;
    }
}
