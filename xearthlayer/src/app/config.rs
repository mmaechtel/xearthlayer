//! Application configuration for XEarthLayerApp.
//!
//! This module defines `AppConfig` which combines all configuration needed
//! to bootstrap the application, including cache settings, service settings,
//! and provider configuration.

use std::path::PathBuf;
use std::time::Duration;

use crate::cache::ServiceCacheConfig;
use crate::config::ConfigFile;
use crate::dds::DdsFormat;
use crate::provider::ProviderConfig;
use crate::service::ServiceConfig;

/// Default garbage collection interval for disk cache (in seconds).
///
/// The GC daemon runs at this interval to evict old entries when the cache
/// exceeds its size limit. 60 seconds provides a good balance between
/// responsiveness and avoiding excessive disk I/O.
pub const DEFAULT_GC_INTERVAL_SECS: u64 = 60;

/// Application configuration combining all component configs.
///
/// This is the top-level configuration passed to `XEarthLayerApp::start()`.
/// It provides a unified configuration surface that ensures all components
/// are configured consistently.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Memory cache configuration.
    pub memory_cache: MemoryCacheAppConfig,

    /// Disk cache configuration.
    pub disk_cache: DiskCacheAppConfig,

    /// Provider configuration (Bing, Google, etc.).
    pub provider: ProviderConfig,

    /// Service configuration (texture format, download settings, etc.).
    pub service: ServiceConfig,
}

/// Memory cache configuration for the application.
#[derive(Clone, Debug)]
pub struct MemoryCacheAppConfig {
    /// Maximum cache size in bytes.
    pub max_size_bytes: u64,
}

impl Default for MemoryCacheAppConfig {
    fn default() -> Self {
        // Default 2GB memory cache
        Self {
            max_size_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

/// Disk cache configuration for the application.
#[derive(Clone, Debug)]
pub struct DiskCacheAppConfig {
    /// Root directory for disk cache.
    pub directory: PathBuf,

    /// Maximum cache size in bytes.
    pub max_size_bytes: u64,

    /// Provider name for directory hierarchy (e.g., "bing", "go2").
    pub provider_name: String,

    /// GC interval in seconds.
    pub gc_interval_secs: u64,
}

impl DiskCacheAppConfig {
    /// Create a new disk cache config with defaults.
    pub fn new(directory: PathBuf, provider_name: impl Into<String>) -> Self {
        Self {
            directory,
            max_size_bytes: 20 * 1024 * 1024 * 1024, // 20GB default
            provider_name: provider_name.into(),
            gc_interval_secs: DEFAULT_GC_INTERVAL_SECS,
        }
    }

    /// Set the maximum cache size.
    pub fn with_max_size(mut self, max_size_bytes: u64) -> Self {
        self.max_size_bytes = max_size_bytes;
        self
    }

    /// Set the GC interval.
    pub fn with_gc_interval_secs(mut self, secs: u64) -> Self {
        self.gc_interval_secs = secs;
        self
    }
}

impl AppConfig {
    /// Create a new application config with default caches.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Root directory for caches
    /// * `provider` - Provider configuration
    /// * `service` - Service configuration
    pub fn new(cache_dir: PathBuf, provider: ProviderConfig, service: ServiceConfig) -> Self {
        let provider_name = provider.name();
        Self {
            memory_cache: MemoryCacheAppConfig::default(),
            disk_cache: DiskCacheAppConfig::new(cache_dir, provider_name),
            provider,
            service,
        }
    }

    /// Create application config from CLI configuration file.
    ///
    /// This factory method extracts all necessary settings from the CLI's
    /// `ConfigFile` to create a properly configured `AppConfig`. This keeps
    /// the configuration translation logic in one place rather than scattered
    /// in CLI code.
    ///
    /// # Arguments
    ///
    /// * `config` - The loaded CLI configuration file
    /// * `provider` - Provider configuration (resolved from CLI args and config)
    /// * `service` - Service configuration (built from CLI config)
    pub fn from_config_file(
        config: &ConfigFile,
        provider: ProviderConfig,
        service: ServiceConfig,
    ) -> Self {
        let provider_name = provider.name();
        Self {
            memory_cache: MemoryCacheAppConfig {
                max_size_bytes: config.cache.memory_size as u64,
            },
            disk_cache: DiskCacheAppConfig {
                directory: config.cache.directory.clone(),
                max_size_bytes: config.cache.disk_size as u64,
                provider_name: provider_name.to_string(),
                gc_interval_secs: DEFAULT_GC_INTERVAL_SECS,
            },
            provider,
            service,
        }
    }

    /// Set the memory cache size.
    pub fn with_memory_cache_size(mut self, size_bytes: u64) -> Self {
        self.memory_cache.max_size_bytes = size_bytes;
        self
    }

    /// Set the disk cache size.
    pub fn with_disk_cache_size(mut self, size_bytes: u64) -> Self {
        self.disk_cache.max_size_bytes = size_bytes;
        self
    }

    /// Convert memory cache config to ServiceCacheConfig.
    pub(crate) fn memory_service_config(&self) -> ServiceCacheConfig {
        ServiceCacheConfig::memory(self.memory_cache.max_size_bytes, None)
    }

    /// Convert disk cache config to ServiceCacheConfig.
    pub(crate) fn disk_service_config(&self) -> ServiceCacheConfig {
        ServiceCacheConfig::disk(
            self.disk_cache.max_size_bytes,
            self.disk_cache.directory.clone(),
            Duration::from_secs(self.disk_cache.gc_interval_secs),
            self.disk_cache.provider_name.clone(),
        )
    }

    /// Get the DDS format from service config.
    pub fn dds_format(&self) -> DdsFormat {
        self.service.texture().format()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_cache_config_default() {
        let config = MemoryCacheAppConfig::default();
        assert_eq!(config.max_size_bytes, 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_disk_cache_config_new() {
        let config = DiskCacheAppConfig::new(PathBuf::from("/cache"), "bing");
        assert_eq!(config.directory, PathBuf::from("/cache"));
        assert_eq!(config.provider_name, "bing");
        assert_eq!(config.max_size_bytes, 20 * 1024 * 1024 * 1024);
        assert_eq!(config.gc_interval_secs, 60);
    }

    #[test]
    fn test_disk_cache_config_builder() {
        let config = DiskCacheAppConfig::new(PathBuf::from("/cache"), "google")
            .with_max_size(10 * 1024 * 1024 * 1024)
            .with_gc_interval_secs(120);

        assert_eq!(config.max_size_bytes, 10 * 1024 * 1024 * 1024);
        assert_eq!(config.gc_interval_secs, 120);
    }

    #[test]
    fn test_app_config_new() {
        let provider = ProviderConfig::bing();
        let service = ServiceConfig::default();
        let config = AppConfig::new(PathBuf::from("/cache"), provider, service);

        // Provider name comes from ProviderConfig::name()
        assert_eq!(config.disk_cache.provider_name, "Bing Maps");
    }

    #[test]
    fn test_app_config_with_sizes() {
        let provider = ProviderConfig::bing();
        let service = ServiceConfig::default();
        let config = AppConfig::new(PathBuf::from("/cache"), provider, service)
            .with_memory_cache_size(1024)
            .with_disk_cache_size(2048);

        assert_eq!(config.memory_cache.max_size_bytes, 1024);
        assert_eq!(config.disk_cache.max_size_bytes, 2048);
    }
}
