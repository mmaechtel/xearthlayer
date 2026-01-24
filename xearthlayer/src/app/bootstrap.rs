//! Application bootstrap implementation.
//!
//! This module contains `XEarthLayerApp` which handles the proper initialization
//! sequence for all services, ensuring cache GC daemons are started before
//! any dependent services.

use std::sync::Arc;

use tokio::runtime::Runtime;
use tracing::info;

use super::config::AppConfig;
use super::error::AppError;
use crate::cache::adapters::{DiskCacheBridge, MemoryCacheBridge};
use crate::cache::{Cache, CacheService};
use crate::manager::CacheBridges;
use crate::metrics::MetricsClient;

/// XEarthLayer application with proper service lifecycle management.
///
/// This struct ensures services are started in the correct order:
/// 1. Cache services first (they own their GC daemons)
/// 2. Bridge adapters for backward compatibility
/// 3. Other services can then use the cache infrastructure
///
/// # The GC Bug Fix
///
/// Prior to this implementation, the disk cache GC daemon was wired
/// externally in CLI code (`run.rs`), depending on `get_service()` which
/// returned `None` in TUI mode. By having `DiskCacheProvider` own its GC
/// daemon internally (spawned during `CacheService::start()`), the GC
/// always runs regardless of how the application is started.
///
/// # Example
///
/// ```ignore
/// use xearthlayer::app::{XEarthLayerApp, AppConfig};
///
/// let config = AppConfig::new(cache_dir, provider, service);
/// let app = XEarthLayerApp::start(config).await?;
///
/// // Get bridges for executor integration
/// let memory_bridge = app.memory_bridge();
/// let disk_bridge = app.disk_bridge();
///
/// // Later: graceful shutdown
/// app.shutdown().await;
/// ```
pub struct XEarthLayerApp {
    /// Memory cache service (owns LRU eviction).
    memory_cache_service: CacheService,

    /// Disk cache service (owns GC daemon - THE FIX!).
    disk_cache_service: CacheService,

    /// Memory cache bridge (implements executor::MemoryCache).
    memory_bridge: Arc<MemoryCacheBridge>,

    /// Disk cache bridge (implements executor::DiskCache).
    disk_bridge: Arc<DiskCacheBridge>,

    /// Optional metrics client for telemetry.
    metrics_client: Option<MetricsClient>,

    /// Application configuration (retained for accessors).
    #[allow(dead_code)]
    config: AppConfig,

    /// Optional owned runtime (when created via `start_sync()`).
    ///
    /// When the app is created via `start_sync()`, it owns its own Tokio runtime
    /// to run the cache services. When created via `start()`, this is `None` and
    /// the caller's runtime is used.
    #[allow(dead_code)]
    runtime: Option<Runtime>,
}

impl XEarthLayerApp {
    /// Start the application with the given configuration.
    ///
    /// This method:
    /// 1. Starts the memory cache service
    /// 2. Starts the disk cache service (spawns internal GC daemon!)
    /// 3. Creates bridge adapters for executor integration
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Errors
    ///
    /// Returns an error if any cache service fails to start.
    pub async fn start(config: AppConfig) -> Result<Self, AppError> {
        Self::start_with_metrics(config, None).await
    }

    /// Start the application with metrics support.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `metrics` - Optional metrics client for telemetry
    pub async fn start_with_metrics(
        config: AppConfig,
        metrics: Option<MetricsClient>,
    ) -> Result<Self, AppError> {
        Self::start_with_metrics_internal(config, metrics).await
    }

    /// Start the application synchronously (creates its own runtime).
    ///
    /// This method is useful when calling from a non-async context (like CLI commands).
    /// It creates a dedicated Tokio runtime for the cache services, similar to how
    /// `XEarthLayerService` manages its own runtime.
    ///
    /// The runtime is kept alive for the lifetime of the `XEarthLayerApp` instance,
    /// ensuring the GC daemon continues running.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot be created or cache services fail to start.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use xearthlayer::app::{XEarthLayerApp, AppConfig};
    ///
    /// // In a sync context (e.g., CLI command handler)
    /// let app = XEarthLayerApp::start_sync(config)?;
    ///
    /// // Get bridges for service integration
    /// let bridges = app.cache_bridges();
    /// ```
    pub fn start_sync(config: AppConfig) -> Result<Self, AppError> {
        Self::start_sync_with_metrics(config, None)
    }

    /// Start the application synchronously with metrics support.
    ///
    /// See [`start_sync`](Self::start_sync) for details.
    pub fn start_sync_with_metrics(
        config: AppConfig,
        metrics: Option<MetricsClient>,
    ) -> Result<Self, AppError> {
        // Create a dedicated runtime for cache services
        let runtime = Runtime::new().map_err(|e| AppError::RuntimeCreation(e.to_string()))?;

        // Start cache services on the runtime, then attach the runtime to the app
        let mut app = runtime.block_on(Self::start_with_metrics_internal(config, metrics))?;
        app.runtime = Some(runtime);

        Ok(app)
    }

    /// Internal start method used by both sync and async entry points.
    async fn start_with_metrics_internal(
        config: AppConfig,
        metrics: Option<MetricsClient>,
    ) -> Result<Self, AppError> {
        info!("Starting XEarthLayerApp with self-contained cache services");

        // 1. Start memory cache service FIRST
        let memory_config = config.memory_service_config();
        let memory_cache_service = CacheService::start(memory_config)
            .await
            .map_err(AppError::MemoryCacheStart)?;

        info!(
            max_size_bytes = config.memory_cache.max_size_bytes,
            "Memory cache service started"
        );

        // 2. Start disk cache service (spawns internal GC daemon!)
        let disk_config = config.disk_service_config();
        let disk_cache_service = CacheService::start(disk_config)
            .await
            .map_err(AppError::DiskCacheStart)?;

        info!(
            max_size_bytes = config.disk_cache.max_size_bytes,
            directory = %config.disk_cache.directory.display(),
            gc_interval_secs = config.disk_cache.gc_interval_secs,
            "Disk cache service started with internal GC daemon"
        );

        // 3. Create bridge adapters for executor integration
        let memory_bridge =
            Self::create_memory_bridge(memory_cache_service.cache(), metrics.as_ref().cloned());

        let disk_bridge =
            Self::create_disk_bridge(disk_cache_service.cache(), metrics.as_ref().cloned());

        info!("Cache bridge adapters created for executor integration");

        Ok(Self {
            memory_cache_service,
            disk_cache_service,
            memory_bridge,
            disk_bridge,
            metrics_client: metrics,
            config,
            runtime: None, // Set by caller if sync
        })
    }

    /// Create the memory cache bridge.
    fn create_memory_bridge(
        cache: Arc<dyn Cache>,
        metrics: Option<MetricsClient>,
    ) -> Arc<MemoryCacheBridge> {
        if let Some(m) = metrics {
            Arc::new(MemoryCacheBridge::with_metrics(cache, m))
        } else {
            Arc::new(MemoryCacheBridge::new(cache))
        }
    }

    /// Create the disk cache bridge.
    fn create_disk_bridge(
        cache: Arc<dyn Cache>,
        metrics: Option<MetricsClient>,
    ) -> Arc<DiskCacheBridge> {
        if let Some(m) = metrics {
            Arc::new(DiskCacheBridge::with_metrics(cache, m))
        } else {
            Arc::new(DiskCacheBridge::new(cache))
        }
    }

    /// Get the memory cache bridge for executor integration.
    ///
    /// This bridge implements `executor::MemoryCache` and can be passed
    /// to the job factory and executor daemon.
    pub fn memory_bridge(&self) -> Arc<MemoryCacheBridge> {
        Arc::clone(&self.memory_bridge)
    }

    /// Get the disk cache bridge for executor integration.
    ///
    /// This bridge implements `executor::DiskCache` and can be passed
    /// to the job factory and executor daemon.
    pub fn disk_bridge(&self) -> Arc<DiskCacheBridge> {
        Arc::clone(&self.disk_bridge)
    }

    /// Get cache bridges for `ServiceBuilder` integration.
    ///
    /// This returns a `CacheBridges` struct that can be passed to
    /// `ServiceBuilder::with_cache_bridges()` for clean integration
    /// with the service layer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let app = XEarthLayerApp::start(config).await?;
    /// let service_builder = ServiceBuilder::new(service_config, provider_config, logger)
    ///     .with_cache_bridges(app.cache_bridges());
    /// ```
    pub fn cache_bridges(&self) -> CacheBridges {
        CacheBridges {
            memory: Arc::clone(&self.memory_bridge),
            disk: Arc::clone(&self.disk_bridge),
            runtime_handle: self.runtime_handle(),
        }
    }

    /// Get a handle to the application's Tokio runtime.
    ///
    /// This handle can be used to spawn tasks or run blocking operations
    /// on the runtime that manages the cache services.
    ///
    /// # Panics
    ///
    /// Panics if called when:
    /// - The app was created via `start()` (async) and no external runtime is active
    /// - The app was created via `start_sync()` but its runtime has been dropped
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime
            .as_ref()
            .map(|r| r.handle().clone())
            .unwrap_or_else(|| {
                // If no owned runtime, try to get current runtime handle
                // This works when the app was created via async start() inside a runtime
                tokio::runtime::Handle::current()
            })
    }

    /// Get the raw memory cache for direct access.
    ///
    /// This provides access to the underlying generic cache implementation,
    /// useful for size queries or advanced operations.
    pub fn raw_memory_cache(&self) -> Arc<dyn Cache> {
        self.memory_cache_service.cache()
    }

    /// Get the raw disk cache for direct access.
    pub fn raw_disk_cache(&self) -> Arc<dyn Cache> {
        self.disk_cache_service.cache()
    }

    /// Get the metrics client if configured.
    pub fn metrics_client(&self) -> Option<MetricsClient> {
        self.metrics_client.clone()
    }

    /// Get current memory cache size in bytes.
    pub fn memory_cache_size_bytes(&self) -> u64 {
        self.memory_cache_service.cache().size_bytes()
    }

    /// Get current disk cache size in bytes.
    pub fn disk_cache_size_bytes(&self) -> u64 {
        self.disk_cache_service.cache().size_bytes()
    }

    /// Shutdown the application gracefully.
    ///
    /// This shuts down cache services in reverse order of startup,
    /// ensuring all pending operations complete and GC daemons stop.
    pub async fn shutdown(self) {
        info!("Shutting down XEarthLayerApp");

        // Shutdown in reverse order
        self.disk_cache_service.shutdown().await;
        info!("Disk cache service shut down");

        self.memory_cache_service.shutdown().await;
        info!("Memory cache service shut down");

        info!("XEarthLayerApp shutdown complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{DiskCache, MemoryCache};
    use crate::provider::ProviderConfig;
    use crate::service::ServiceConfig;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_test_config(cache_dir: PathBuf) -> AppConfig {
        let provider = ProviderConfig::bing();
        let service = ServiceConfig::default();
        AppConfig::new(cache_dir, provider, service)
            .with_memory_cache_size(1_000_000)
            .with_disk_cache_size(10_000_000)
    }

    #[tokio::test]
    async fn test_app_start_and_shutdown() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_path_buf());

        let app = XEarthLayerApp::start(config).await.unwrap();

        // Verify services are running
        assert!(app.memory_cache_size_bytes() == 0); // Empty initially
        assert!(app.disk_cache_size_bytes() == 0);

        // Shutdown
        app.shutdown().await;
    }

    #[tokio::test]
    async fn test_app_memory_bridge() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_path_buf());

        let app = XEarthLayerApp::start(config).await.unwrap();
        let bridge = app.memory_bridge();

        // Use the bridge
        bridge.put(100, 200, 15, vec![1, 2, 3]).await;
        let result = bridge.get(100, 200, 15).await;
        assert_eq!(result, Some(vec![1, 2, 3]));

        app.shutdown().await;
    }

    #[tokio::test]
    async fn test_app_disk_bridge() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_path_buf());

        let app = XEarthLayerApp::start(config).await.unwrap();
        let bridge = app.disk_bridge();

        // Use the bridge
        bridge.put(100, 200, 15, 0, 0, vec![1, 2, 3]).await.unwrap();
        let result = bridge.get(100, 200, 15, 0, 0).await;
        assert_eq!(result, Some(vec![1, 2, 3]));

        app.shutdown().await;
    }

    #[tokio::test]
    async fn test_app_with_metrics() {
        let temp_dir = tempdir().unwrap();
        let config = create_test_config(temp_dir.path().to_path_buf());

        // Create a metrics system for testing
        let runtime_handle = tokio::runtime::Handle::current();
        let metrics_system = crate::metrics::MetricsSystem::new(&runtime_handle);
        let metrics_client = metrics_system.client();

        let app = XEarthLayerApp::start_with_metrics(config, Some(metrics_client.clone()))
            .await
            .unwrap();

        assert!(app.metrics_client().is_some());

        app.shutdown().await;
    }
}
