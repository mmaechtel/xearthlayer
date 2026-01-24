//! Service orchestrator for XEarthLayer backend.
//!
//! This module provides `ServiceOrchestrator` which coordinates the startup,
//! operation, and shutdown of all XEarthLayer backend services.
//!
//! # Architecture
//!
//! The orchestrator owns and manages:
//! - **Cache services** (via `XEarthLayerApp`) - memory and disk caches with GC
//! - **Aircraft Position & Telemetry (APT)** - unified position aggregation
//! - **Prefetch system** - predictive tile caching
//! - **Scene tracker** - FUSE load monitoring
//! - **FUSE mounts** (via `MountManager`) - virtual filesystem
//!
//! # Startup Sequence
//!
//! 1. Cache services start first (GC daemons auto-start)
//! 2. Service builder creates XEarthLayerService with cache bridges
//! 3. APT module starts telemetry reception
//! 4. Prefetch system subscribes to APT telemetry
//! 5. FUSE mounts become active
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::service::{ServiceOrchestrator, OrchestratorConfig};
//!
//! let config = OrchestratorConfig::from_config_file(...);
//! let orchestrator = ServiceOrchestrator::start(config).await?;
//!
//! // Access telemetry for UI
//! let snapshot = orchestrator.telemetry_snapshot();
//!
//! // Graceful shutdown
//! orchestrator.shutdown().await;
//! ```

use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::aircraft_position::{
    spawn_position_logger, AircraftPositionBroadcaster, SharedAircraftPosition, StateAggregator,
    TelemetryReceiver, TelemetryReceiverConfig, DEFAULT_LOG_INTERVAL,
};
use crate::app::XEarthLayerApp;
use crate::executor::{DdsClient, MemoryCache};
use crate::log::TracingLogger;
use crate::manager::{LocalPackageStore, MountManager, ServiceBuilder};
use crate::metrics::TelemetrySnapshot;
use crate::ortho_union::OrthoUnionIndex;
use crate::prefetch::{
    FuseRequestAnalyzer, PrefetchStrategy, PrefetcherBuilder, SceneryIndex, SharedPrefetchStatus,
};
use crate::runtime::SharedRuntimeHealth;

use super::error::ServiceError;
use super::orchestrator_config::OrchestratorConfig;
use super::XEarthLayerService;

/// Handle to a running prefetch system.
pub struct PrefetchHandle {
    /// Join handle for the prefetch task.
    #[allow(dead_code)]
    handle: JoinHandle<()>,
}

/// Result of mounting consolidated ortho.
pub struct MountResult {
    /// Whether mounting succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Number of sources mounted.
    pub source_count: usize,
    /// Number of files indexed.
    pub file_count: usize,
    /// Names of patches mounted.
    pub patch_names: Vec<String>,
    /// Regions of packages mounted.
    pub package_regions: Vec<String>,
    /// Mountpoint path.
    pub mountpoint: std::path::PathBuf,
}

/// Coordinates startup and operation of all XEarthLayer backend services.
///
/// This is the main entry point for the backend daemon. It encapsulates all
/// service orchestration that was previously scattered in `run.rs`, providing
/// a clean API for the CLI/TUI layer.
pub struct ServiceOrchestrator {
    /// Cache infrastructure (owns GC daemons).
    cache_app: Option<XEarthLayerApp>,

    /// Aircraft position provider (APT module).
    aircraft_position: SharedAircraftPosition,

    /// Prefetch status for UI display.
    prefetch_status: Arc<SharedPrefetchStatus>,

    /// Prefetch system handle.
    prefetch_handle: Option<PrefetchHandle>,

    /// Mount manager (owns FUSE mounts).
    mount_manager: MountManager,

    /// Service builder for creating services (consumed on mount).
    service_builder: Option<ServiceBuilder>,

    /// Runtime health for control plane monitoring.
    runtime_health: Option<SharedRuntimeHealth>,

    /// Maximum concurrent jobs (for UI display).
    max_concurrent_jobs: usize,

    /// FUSE request analyzer for position inference.
    fuse_analyzer: Option<Arc<FuseRequestAnalyzer>>,

    /// Scenery index for prefetching.
    scenery_index: Arc<SceneryIndex>,

    /// Master cancellation token.
    cancellation: CancellationToken,

    /// Configuration (retained for accessors).
    config: OrchestratorConfig,
}

impl ServiceOrchestrator {
    /// Start all backend services with the given configuration.
    ///
    /// This performs the complete startup sequence:
    /// 1. Starts cache services (GC daemons auto-start)
    /// 2. Creates service builder with cache bridges
    /// 3. Creates mount manager
    /// 4. Returns orchestrator ready for mounting
    ///
    /// Note: FUSE mounting is done separately via `mount_consolidated_ortho()`
    /// to allow progress callbacks for the TUI loading screen.
    pub fn start(config: OrchestratorConfig) -> Result<Self, ServiceError> {
        info!("Starting ServiceOrchestrator");

        let cancellation = CancellationToken::new();
        let prefetch_status = SharedPrefetchStatus::new();

        // Create FUSE request analyzer for position inference (if prefetch enabled)
        let fuse_analyzer = if config.prefetch_enabled() {
            Some(Arc::new(FuseRequestAnalyzer::new(
                crate::prefetch::FuseInferenceConfig::default(),
            )))
        } else {
            None
        };

        // 1. Start cache services (GC daemons auto-start)
        let cache_app = if config.cache_enabled() {
            match XEarthLayerApp::start_sync(config.app_config.clone()) {
                Ok(app) => {
                    info!(
                        memory_size = config.app_config.memory_cache.max_size_bytes,
                        disk_size = config.app_config.disk_cache.max_size_bytes,
                        cache_dir = %config.app_config.disk_cache.directory.display(),
                        "Cache services started with internal GC daemon"
                    );
                    Some(app)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to start cache services");
                    None
                }
            }
        } else {
            None
        };

        // 2. Create service builder with cache bridges and disk I/O profile
        let logger: Arc<dyn crate::log::Logger> = Arc::new(TracingLogger);
        let mut service_builder = ServiceBuilder::with_disk_io_profile(
            config.service.clone(),
            config.provider.clone(),
            logger,
            config.disk_io_profile,
        );

        // Wire cache bridges from XEarthLayerApp
        if let Some(ref app) = cache_app {
            service_builder = service_builder.with_cache_bridges(app.cache_bridges());
        }

        // 3. Create mount manager with Custom Scenery path
        let mount_manager = MountManager::with_scenery_path(&config.custom_scenery_path);

        // Wire load monitor for circuit breaker integration
        let load_monitor = mount_manager.load_monitor();
        service_builder = service_builder.with_load_monitor(Arc::clone(&load_monitor));

        // Wire FUSE analyzer callback for position inference
        if let Some(ref analyzer) = fuse_analyzer {
            service_builder = service_builder.with_tile_request_callback(analyzer.callback());
        }

        // 4. Create APT module (starts later with mount)
        let (apt_broadcast_tx, _apt_broadcast_rx) = broadcast::channel(16);
        let apt_aggregator = StateAggregator::new(apt_broadcast_tx);
        let aircraft_position = SharedAircraftPosition::new(apt_aggregator);

        // Create empty scenery index (populated during mount)
        let scenery_index = Arc::new(SceneryIndex::with_defaults());

        info!("ServiceOrchestrator initialized (mount pending)");

        Ok(Self {
            cache_app,
            aircraft_position,
            prefetch_status,
            prefetch_handle: None,
            mount_manager,
            service_builder: Some(service_builder),
            runtime_health: None,
            max_concurrent_jobs: 0,
            fuse_analyzer,
            scenery_index,
            cancellation,
            config,
        })
    }

    /// Mount consolidated ortho scenery.
    ///
    /// This mounts all ortho sources (patches + packages) into a single FUSE mount.
    /// Call this after `start()` to activate the FUSE filesystem.
    ///
    /// For TUI mode, use `mount_consolidated_ortho_with_progress()` instead.
    pub fn mount_consolidated_ortho(&mut self, store: &LocalPackageStore) -> MountResult {
        let service_builder = self
            .service_builder
            .take()
            .expect("Service builder should be set - was mount_consolidated_ortho called twice?");

        let result = self.mount_manager.mount_consolidated_ortho(
            &self.config.patches_dir,
            store,
            &service_builder,
        );

        // Wire runtime health after mount
        self.wire_runtime_health();

        MountResult {
            success: result.success,
            error: result.error,
            source_count: result.source_count,
            file_count: result.file_count,
            patch_names: result.patch_names,
            package_regions: result.package_regions,
            mountpoint: result.mountpoint,
        }
    }

    /// Mount with progress callback for TUI loading screen.
    pub fn mount_consolidated_ortho_with_progress<F>(
        &mut self,
        store: &LocalPackageStore,
        progress_callback: Option<F>,
    ) -> MountResult
    where
        F: Fn(crate::ortho_union::IndexBuildProgress) + Send + Sync + 'static,
    {
        let service_builder = self
            .service_builder
            .take()
            .expect("Service builder should be set - was mount_consolidated_ortho called twice?");

        let callback = progress_callback
            .map(|f| Arc::new(f) as crate::ortho_union::IndexBuildProgressCallback);

        let result = self.mount_manager.mount_consolidated_ortho_with_progress(
            &self.config.patches_dir,
            store,
            &service_builder,
            callback,
        );

        // Wire runtime health after mount
        self.wire_runtime_health();

        MountResult {
            success: result.success,
            error: result.error,
            source_count: result.source_count,
            file_count: result.file_count,
            patch_names: result.patch_names,
            package_regions: result.package_regions,
            mountpoint: result.mountpoint,
        }
    }

    /// Wire runtime health from mounted service.
    fn wire_runtime_health(&mut self) {
        if let Some(service) = self.mount_manager.get_service() {
            self.runtime_health = service.runtime_health();
            self.max_concurrent_jobs = service.max_concurrent_jobs();
        }
    }

    /// Start the APT telemetry receiver.
    ///
    /// This starts listening for X-Plane UDP telemetry on the configured port.
    /// Must be called after mounting to have a runtime handle available.
    pub fn start_apt_telemetry(&self) -> Result<(), ServiceError> {
        let service = self
            .mount_manager
            .get_service()
            .ok_or_else(|| ServiceError::NotStarted("No service available for APT".into()))?;

        let runtime_handle = service.runtime_handle().clone();
        let telemetry_port = self.config.prefetch.udp_port;
        let (telemetry_tx, mut telemetry_rx) = mpsc::channel(32);

        let telemetry_config = TelemetryReceiverConfig {
            port: telemetry_port,
            ..Default::default()
        };
        let receiver = TelemetryReceiver::new(telemetry_config, telemetry_tx);
        let apt_cancellation = self.cancellation.clone();
        let logger_cancellation = apt_cancellation.clone();

        // Start the UDP receiver
        runtime_handle.spawn(async move {
            tokio::select! {
                result = receiver.start() => {
                    match result {
                        Ok(Ok(())) => tracing::debug!("APT telemetry receiver stopped"),
                        Ok(Err(e)) => tracing::warn!("APT telemetry receiver error: {}", e),
                        Err(e) => tracing::warn!("APT telemetry receiver task failed: {}", e),
                    }
                }
                _ = apt_cancellation.cancelled() => {
                    tracing::debug!("APT telemetry receiver cancelled");
                }
            }
        });

        // Bridge task: forward telemetry states to APT aggregator
        let aircraft_position = self.aircraft_position.clone();
        runtime_handle.spawn(async move {
            while let Some(state) = telemetry_rx.recv().await {
                aircraft_position.receive_telemetry(state);
            }
        });

        // Periodic position logger for flight analysis (DEBUG level only)
        if tracing::enabled!(tracing::Level::DEBUG) {
            spawn_position_logger(
                self.aircraft_position.clone(),
                logger_cancellation,
                DEFAULT_LOG_INTERVAL,
            );
        }

        info!(port = telemetry_port, "APT telemetry receiver started");
        Ok(())
    }

    /// Start the prefetch system.
    ///
    /// This starts the prefetch daemon that predictively caches tiles
    /// based on aircraft position and heading.
    pub fn start_prefetch(&mut self) -> Result<(), ServiceError> {
        if !self.config.prefetch_enabled() {
            info!("Prefetch disabled by configuration");
            return Ok(());
        }

        let service = self
            .mount_manager
            .get_service()
            .ok_or_else(|| ServiceError::NotStarted("No service available for prefetch".into()))?;

        let dds_client = service
            .dds_client()
            .ok_or_else(|| ServiceError::NotStarted("DDS client not available".into()))?;

        let runtime_handle = service.runtime_handle().clone();

        // Try legacy adapter first, then new cache bridge architecture
        if let Some(memory_cache) = service.memory_cache_adapter() {
            self.start_prefetch_with_cache(&runtime_handle, dds_client, memory_cache)?;
        } else if let Some(memory_cache) = service.memory_cache_bridge() {
            self.start_prefetch_with_cache(&runtime_handle, dds_client, memory_cache)?;
        } else {
            tracing::warn!("Memory cache not available, prefetch disabled");
            return Ok(());
        }

        Ok(())
    }

    /// Internal helper to start prefetch with a specific memory cache type.
    fn start_prefetch_with_cache<M: MemoryCache + 'static>(
        &mut self,
        runtime_handle: &Handle,
        dds_client: Arc<dyn DdsClient>,
        memory_cache: Arc<M>,
    ) -> Result<(), ServiceError> {
        use crate::prefetch::AircraftState as PrefetchAircraftState;

        // Create channel for prefetch telemetry data
        let (state_tx, state_rx) = mpsc::channel(32);

        // Bridge APT telemetry to prefetch channel
        let mut apt_rx = self.aircraft_position.subscribe();
        let bridge_cancel = self.cancellation.clone();
        runtime_handle.spawn(async move {
            loop {
                tokio::select! {
                    biased;

                    _ = bridge_cancel.cancelled() => {
                        tracing::debug!("APT-to-prefetch telemetry bridge cancelled");
                        break;
                    }

                    result = apt_rx.recv() => {
                        match result {
                            Ok(apt_state) => {
                                let prefetch_state = PrefetchAircraftState::new(
                                    apt_state.latitude,
                                    apt_state.longitude,
                                    apt_state.heading,
                                    apt_state.ground_speed,
                                    apt_state.altitude,
                                );
                                if state_tx.send(prefetch_state).await.is_err() {
                                    tracing::debug!("Prefetch channel closed");
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::debug!("APT broadcast channel closed");
                                break;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::trace!("APT-to-prefetch bridge lagged by {} messages", n);
                            }
                        }
                    }
                }
            }
        });

        // Build prefetcher
        let config = &self.config.prefetch;
        let mut builder = PrefetcherBuilder::new()
            .memory_cache(memory_cache)
            .dds_client(dds_client)
            .strategy(&config.strategy)
            .shared_status(Arc::clone(&self.prefetch_status))
            .cone_half_angle(config.cone_angle)
            .inner_radius_nm(config.inner_radius_nm)
            .outer_radius_nm(config.outer_radius_nm)
            .radial_radius(config.radial_radius)
            .max_tiles_per_cycle(config.max_tiles_per_cycle)
            .cycle_interval_ms(config.cycle_interval_ms)
            .with_circuit_breaker_throttler(
                self.mount_manager.load_monitor(),
                config.circuit_breaker.clone(),
            );

        // Wire FUSE analyzer
        if let Some(ref analyzer) = self.fuse_analyzer {
            builder = builder.with_fuse_analyzer(Arc::clone(analyzer));
        }

        // Wire scenery index if available
        if self.scenery_index.tile_count() > 0 {
            builder = builder.with_scenery_index(Arc::clone(&self.scenery_index));
        }

        // Parse strategy
        let strategy: PrefetchStrategy = config.strategy.parse().unwrap_or(PrefetchStrategy::Auto);

        // Build and start the prefetcher
        let prefetcher_cancel = self.cancellation.clone();
        match strategy {
            PrefetchStrategy::TileBased => {
                // Tile-based prefetcher requires DDS access channel and OrthoUnionIndex
                if let (Some(access_rx), Some(ortho_index)) = (
                    self.mount_manager.take_dds_access_receiver(),
                    self.mount_manager.ortho_union_index(),
                ) {
                    builder = builder.tile_based_rows_ahead(config.tile_based_rows_ahead);
                    let prefetcher = builder.build_tile_based(ortho_index, access_rx);

                    let handle = runtime_handle.spawn(async move {
                        prefetcher.run(state_rx, prefetcher_cancel).await;
                    });

                    self.prefetch_handle = Some(PrefetchHandle { handle });
                    info!(strategy = "tile-based", "Prefetch system started");
                } else {
                    // Fall back to radial
                    let prefetcher = builder.strategy("radial").build();
                    let handle = runtime_handle.spawn(async move {
                        prefetcher.run(state_rx, prefetcher_cancel).await;
                    });
                    self.prefetch_handle = Some(PrefetchHandle { handle });
                    info!(strategy = "radial (fallback)", "Prefetch system started");
                }
            }
            _ => {
                let prefetcher = builder.build();
                let handle = runtime_handle.spawn(async move {
                    prefetcher.run(state_rx, prefetcher_cancel).await;
                });
                self.prefetch_handle = Some(PrefetchHandle { handle });
                info!(strategy = %config.strategy, "Prefetch system started");
            }
        }

        Ok(())
    }

    /// Update the scenery index (called after building).
    pub fn set_scenery_index(&mut self, index: Arc<SceneryIndex>) {
        self.scenery_index = index;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Accessors for TUI integration
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the aircraft position provider for TUI display.
    pub fn aircraft_position(&self) -> SharedAircraftPosition {
        self.aircraft_position.clone()
    }

    /// Get the prefetch status for TUI display.
    pub fn prefetch_status(&self) -> Arc<SharedPrefetchStatus> {
        Arc::clone(&self.prefetch_status)
    }

    /// Get runtime health for control plane display.
    pub fn runtime_health(&self) -> Option<SharedRuntimeHealth> {
        self.runtime_health.clone()
    }

    /// Get maximum concurrent jobs for UI display.
    pub fn max_concurrent_jobs(&self) -> usize {
        self.max_concurrent_jobs
    }

    /// Get aggregated telemetry snapshot for UI display.
    pub fn telemetry_snapshot(&self) -> TelemetrySnapshot {
        self.mount_manager.aggregated_telemetry()
    }

    /// Get the cancellation token (for coordinating shutdown).
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Get the OrthoUnionIndex if available.
    pub fn ortho_union_index(&self) -> Option<Arc<OrthoUnionIndex>> {
        self.mount_manager.ortho_union_index()
    }

    /// Get mutable access to the mount manager.
    pub fn mount_manager(&mut self) -> &mut MountManager {
        &mut self.mount_manager
    }

    /// Get the underlying service if mounted.
    pub fn service(&self) -> Option<&XEarthLayerService> {
        self.mount_manager.get_service()
    }

    /// Get the configuration.
    pub fn config(&self) -> &OrchestratorConfig {
        &self.config
    }

    /// Get the scenery index.
    pub fn scenery_index(&self) -> &Arc<SceneryIndex> {
        &self.scenery_index
    }

    /// Get the FUSE analyzer for position inference.
    pub fn fuse_analyzer(&self) -> Option<Arc<FuseRequestAnalyzer>> {
        self.fuse_analyzer.clone()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Shutdown
    // ─────────────────────────────────────────────────────────────────────────

    /// Gracefully shutdown all services.
    ///
    /// This shuts down services in reverse order of startup:
    /// 1. Cancel prefetch
    /// 2. Unmount FUSE
    /// 3. Shutdown cache services
    pub fn shutdown(mut self) {
        info!("Shutting down ServiceOrchestrator");

        // 1. Cancel all async tasks
        self.cancellation.cancel();

        // 2. Unmount all FUSE filesystems
        self.mount_manager.unmount_all();
        info!("FUSE mounts unmounted");

        // 3. Shutdown cache services (GC daemons stop)
        if let Some(app) = self.cache_app.take() {
            // Note: XEarthLayerApp's Drop impl handles async shutdown
            drop(app);
            info!("Cache services shut down");
        }

        info!("ServiceOrchestrator shutdown complete");
    }
}
