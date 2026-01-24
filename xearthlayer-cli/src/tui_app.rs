//! TUI Application module for XEarthLayer CLI.
//!
//! This module contains the TUI (Terminal User Interface) application logic,
//! separated from the command-line argument parsing and service orchestration.
//!
//! # Architecture
//!
//! - `run_tui()` - Interactive TUI application with dashboard and event loop
//! - `run_headless()` - Simple headless mode for non-TTY environments
//! - `TuiAppConfig` - Configuration struct for TUI initialization
//!
//! The `run.rs` command acts as a thin front controller that:
//! 1. Loads and validates configuration
//! 2. Creates the `ServiceOrchestrator`
//! 3. Delegates to `run_tui()` or `run_headless()`

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use xearthlayer::aircraft_position::SharedAircraftPosition;
use xearthlayer::config::ConfigFile;
use xearthlayer::manager::{create_consolidated_overlay, InstalledPackage, LocalPackageStore};
use xearthlayer::ortho_union::{IndexBuildPhase, IndexBuildProgress};
use xearthlayer::prefetch::{
    load_cache, save_cache, CacheLoadResult, IndexingProgress,
    PrewarmProgress as LibPrewarmProgress, SceneryIndex, SceneryIndexConfig, SharedPrefetchStatus,
};
use xearthlayer::service::{PrewarmOrchestrator, ServiceOrchestrator};
use xearthlayer::xplane::XPlaneEnvironment;

use crate::error::CliError;
use crate::ui::{
    self, Dashboard, DashboardConfig, DashboardEvent, DashboardState, LoadingPhase,
    LoadingProgress, PrewarmProgress,
};

/// Configuration for starting the TUI application.
pub struct TuiAppConfig<'a> {
    /// Service orchestrator for all backend services.
    pub orchestrator: &'a mut ServiceOrchestrator,
    /// Local package store for package discovery.
    pub store: &'a LocalPackageStore,
    /// Shutdown signal from signal handler.
    pub shutdown: Arc<AtomicBool>,
    /// Configuration file.
    pub config: &'a ConfigFile,
    /// Prefetch status for UI display.
    pub prefetch_status: Arc<SharedPrefetchStatus>,
    /// Unified aircraft position provider (APT module).
    pub aircraft_position: SharedAircraftPosition,
    /// Ortho packages to mount.
    pub ortho_packages: Vec<&'a InstalledPackage>,
    /// Whether prefetch is enabled.
    pub prefetch_enabled: bool,
    /// Airport ICAO code for prewarm (if specified).
    pub airport_icao: Option<String>,
    /// X-Plane environment for apt.dat lookup and resource paths.
    pub xplane_env: Option<XPlaneEnvironment>,
    /// Custom Scenery path for overlay symlinks.
    pub custom_scenery_path: &'a Path,
}

/// Run the TUI application with dashboard.
///
/// This function:
/// 1. Starts the TUI immediately in Loading state for OrthoUnionIndex building
/// 2. Mounts consolidated ortho with progress updates
/// 3. Creates overlay symlinks
/// 4. Builds SceneryIndex for prefetching
/// 5. When indexing completes, optionally prewarm cache around airport
/// 6. Starts the prefetcher and transitions to Running
///
/// Returns the cancellation token for cleanup coordination.
pub fn run_tui(config: TuiAppConfig) -> Result<CancellationToken, CliError> {
    use std::sync::Mutex;

    let TuiAppConfig {
        orchestrator,
        store,
        shutdown,
        config: cfg,
        prefetch_status,
        aircraft_position,
        ortho_packages,
        prefetch_enabled,
        airport_icao,
        xplane_env,
        custom_scenery_path,
    } = config;

    let dashboard_config = DashboardConfig {
        memory_cache_max: cfg.cache.memory_size,
        disk_cache_max: cfg.cache.disk_size,
        provider_name: cfg.provider.provider_type.clone(),
    };

    // Create initial loading progress for OrthoUnionIndex building
    let loading_progress = LoadingProgress::new(ortho_packages.len());

    // Start dashboard in Loading state
    let initial_state = DashboardState::Loading(loading_progress);
    let mut dashboard = Dashboard::with_state(dashboard_config, shutdown.clone(), initial_state)
        .map_err(|e| CliError::Config(format!("Failed to create dashboard: {}", e)))?
        .with_prefetch_status(Arc::clone(&prefetch_status))
        .with_aircraft_position(aircraft_position.clone());

    // Draw initial loading screen immediately
    dashboard
        .draw_loading()
        .map_err(|e| CliError::Config(format!("Dashboard draw error: {}", e)))?;

    // Phase 1: Mount consolidated ortho with progress callback
    let progress_state = Arc::new(Mutex::new(LoadingProgress::new(ortho_packages.len())));
    let progress_state_clone = Arc::clone(&progress_state);

    // Progress callback - plain closure, orchestrator wraps it in Arc internally
    let progress_callback = move |progress: IndexBuildProgress| {
        let mut state = progress_state_clone.lock().unwrap();
        state.phase = match progress.phase {
            IndexBuildPhase::Discovering => LoadingPhase::Discovering,
            IndexBuildPhase::CheckingCache => LoadingPhase::CheckingCache,
            IndexBuildPhase::Scanning => LoadingPhase::Scanning,
            IndexBuildPhase::Merging => LoadingPhase::Merging,
            IndexBuildPhase::SavingCache => LoadingPhase::SavingCache,
            IndexBuildPhase::Complete => LoadingPhase::Complete,
        };
        state.current_package = progress.current_source.clone().unwrap_or_default();
        state.packages_scanned = progress.sources_complete;
        state.total_packages = progress.sources_total;
        state.tiles_indexed = progress.files_scanned;
        state.using_cache = progress.using_cache;
    };

    // Mount with progress - run in a separate thread so we can update dashboard
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let progress_state_for_draw = Arc::clone(&progress_state);

    let mount_result = std::thread::scope(|s| {
        // Spawn mount thread within scope
        let _mount_handle = s.spawn(|| {
            let result =
                orchestrator.mount_consolidated_ortho_with_progress(store, Some(progress_callback));
            let _ = result_tx.send(result);
        });

        // Update dashboard while mount is in progress
        let tick_rate = Duration::from_millis(50);
        loop {
            // Check for result (non-blocking)
            match result_rx.try_recv() {
                Ok(result) => break result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Not ready yet - update dashboard and wait
                    let current_progress = progress_state_for_draw.lock().unwrap().clone();
                    dashboard.update_loading_progress(current_progress);
                    if let Err(e) = dashboard.draw_loading() {
                        tracing::warn!(error = %e, "Dashboard draw error during mount");
                    }
                    std::thread::sleep(tick_rate);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Thread finished without sending result - unexpected
                    tracing::error!("Mount thread disconnected without sending result");
                    break xearthlayer::service::MountResult {
                        success: false,
                        error: Some("Mount thread disconnected unexpectedly".to_string()),
                        source_count: 0,
                        file_count: 0,
                        patch_names: vec![],
                        package_regions: vec![],
                        mountpoint: std::path::PathBuf::new(),
                    };
                }
            }
        }
    });

    if !mount_result.success {
        let error_msg = mount_result.error.as_deref().unwrap_or("Unknown error");
        return Err(CliError::Serve(
            xearthlayer::service::ServiceError::IoError(std::io::Error::other(format!(
                "Failed to mount consolidated ortho: {}",
                error_msg
            ))),
        ));
    }

    // Phase 2: Create overlay symlinks
    let overlay_result = create_consolidated_overlay(store, custom_scenery_path);
    if let Some(ref error) = overlay_result.error {
        tracing::warn!(error = %error, "Failed to create consolidated overlay");
    }

    // Wire in runtime health for TUI display (job queue depth, etc.)
    if let Some(runtime_health) = orchestrator.runtime_health() {
        let max_concurrent_jobs = orchestrator.max_concurrent_jobs();
        dashboard = dashboard.with_runtime_health(runtime_health, max_concurrent_jobs);
    }

    // Start APT TelemetryReceiver using orchestrator
    if let Err(e) = orchestrator.start_apt_telemetry() {
        tracing::warn!(error = %e, "Failed to start APT telemetry receiver");
    }

    // Phase 3: Build SceneryIndex for prefetching
    // Collect packages for indexing
    let packages_for_index: Vec<_> = ortho_packages
        .iter()
        .map(|p| (p.region().to_string(), p.path.clone()))
        .collect();

    // Try to load scenery index from cache first
    let (scenery_index, cache_loaded) = match load_cache(&packages_for_index) {
        CacheLoadResult::Loaded {
            tiles,
            total_tiles,
            sea_tiles,
        } => {
            tracing::info!(
                tiles = total_tiles,
                sea = sea_tiles,
                "Loaded scenery index from cache"
            );

            // Update loading progress to show cache was used
            let mut loading = LoadingProgress::new(packages_for_index.len());
            loading.tiles_indexed = total_tiles;
            loading.packages_scanned = packages_for_index.len();
            loading.current_package = "Cache loaded".to_string();
            dashboard.update_loading_progress(loading);

            // Create index from cached tiles
            let index = Arc::new(SceneryIndex::from_tiles(
                tiles,
                SceneryIndexConfig::default(),
            ));
            (index, true)
        }
        CacheLoadResult::Stale { reason } => {
            tracing::info!(reason = %reason, "Scenery cache is stale, rebuilding");
            (Arc::new(SceneryIndex::with_defaults()), false)
        }
        CacheLoadResult::NotFound => {
            tracing::info!("No scenery cache found, building index");
            (Arc::new(SceneryIndex::with_defaults()), false)
        }
        CacheLoadResult::Invalid { error } => {
            tracing::warn!(error = %error, "Scenery cache invalid, rebuilding");
            (Arc::new(SceneryIndex::with_defaults()), false)
        }
    };

    // If cache wasn't loaded, build index from scratch
    let (progress_tx, mut progress_rx) = mpsc::channel::<IndexingProgress>(32);
    if !cache_loaded {
        let index_for_build = Arc::clone(&scenery_index);
        let packages_for_cache = packages_for_index.clone();

        // Spawn indexing task
        std::thread::spawn(move || {
            let mut total_indexed = 0usize;
            let total_packages = packages_for_cache.len();

            for (idx, (region, path)) in packages_for_cache.iter().enumerate() {
                let _ = progress_tx.blocking_send(IndexingProgress::PackageStarted {
                    name: region.clone(),
                    index: idx,
                    total: total_packages,
                });

                match index_for_build.build_from_package(path) {
                    Ok(count) => {
                        total_indexed += count;
                        let _ = progress_tx.blocking_send(IndexingProgress::PackageCompleted {
                            name: region.clone(),
                            tiles: count,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            region = %region,
                            error = %e,
                            "Failed to index scenery package"
                        );
                    }
                }
            }

            // Send completion
            let _ = progress_tx.blocking_send(IndexingProgress::Complete {
                total: total_indexed,
                land: index_for_build.land_tile_count(),
                sea: index_for_build.sea_tile_count(),
            });

            // Save cache after indexing
            if let Err(e) = save_cache(&index_for_build, &packages_for_cache) {
                tracing::warn!(error = %e, "Failed to save scenery cache");
            }
        });
    } else {
        // Send immediate completion for cache-loaded case
        let total = scenery_index.tile_count();
        let land = scenery_index.land_tile_count();
        let sea = scenery_index.sea_tile_count();
        let _ = progress_tx.blocking_send(IndexingProgress::Complete { total, land, sea });
    }

    // Track state transitions
    let mut indexing_complete = cache_loaded;
    let mut prewarm_active = false;
    let mut prewarm_complete = false;
    let mut prefetcher_started = false;
    let mut prewarm_handle: Option<xearthlayer::service::PrewarmHandle> = None;

    // Main event loop
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let runtime_handle = tokio::runtime::Handle::current();

    loop {
        // Poll for events
        match dashboard.poll_event() {
            Ok(Some(DashboardEvent::Quit)) => break,
            Ok(Some(DashboardEvent::Cancel)) => {
                // Cancel prewarm if active
                if prewarm_active && !prewarm_complete {
                    tracing::info!("Prewarm cancelled by user");
                    if let Some(ref handle) = prewarm_handle {
                        handle.cancel();
                    }
                    prewarm_complete = true;
                    prewarm_active = false;
                }
            }
            Ok(None) => {}
            Err(e) => return Err(CliError::Config(format!("Dashboard error: {}", e))),
        }

        // Check for index progress updates (non-blocking)
        while let Ok(progress) = progress_rx.try_recv() {
            match progress {
                IndexingProgress::PackageStarted { name, index, total } => {
                    let mut loading = LoadingProgress::new(total);
                    loading.packages_scanned = index;
                    loading.scanning(&name);
                    dashboard.update_loading_progress(loading);
                }
                IndexingProgress::PackageCompleted { tiles, .. } => {
                    if let DashboardState::Loading(ref progress) = dashboard.state().clone() {
                        let mut updated = progress.clone();
                        updated.package_completed(tiles);
                        dashboard.update_loading_progress(updated);
                    }
                }
                IndexingProgress::TileProgress { tiles_indexed } => {
                    if let DashboardState::Loading(ref progress) = dashboard.state().clone() {
                        let mut updated = progress.clone();
                        updated.tiles_indexed = tiles_indexed;
                        dashboard.update_loading_progress(updated);
                    }
                }
                IndexingProgress::Complete { total, land, sea } => {
                    tracing::info!(total, land, sea, "Scenery index complete");
                    indexing_complete = true;
                }
            }
        }

        // After indexing, transition to Running state and start prewarm in background
        if indexing_complete && !prewarm_active && !prewarm_complete && !prefetcher_started {
            dashboard.transition_to_running();

            // Start prewarm in background if airport specified
            if let Some(ref icao) = airport_icao {
                let prewarm_config = orchestrator.config().prewarm.clone();
                match PrewarmOrchestrator::start(
                    orchestrator,
                    icao,
                    xplane_env.as_ref(),
                    &aircraft_position,
                    &prewarm_config,
                    &runtime_handle,
                ) {
                    Ok(result) => {
                        tracing::info!(
                            icao = %icao,
                            airport = %result.airport_name,
                            tiles = result.estimated_tiles,
                            "Starting prewarm in background"
                        );
                        prewarm_handle = Some(result.handle);
                        prewarm_active = true;

                        let prewarm_progress = PrewarmProgress::new(icao, result.estimated_tiles);
                        dashboard.update_prewarm_progress(prewarm_progress);
                    }
                    Err(e) => {
                        tracing::warn!("Prewarm skipped: {}", e);
                        prewarm_complete = true;
                    }
                }
            } else {
                prewarm_complete = true;
            }

            // Start prefetcher immediately (doesn't wait for prewarm)
            if prefetch_enabled {
                if scenery_index.tile_count() > 0 {
                    orchestrator.set_scenery_index(Arc::clone(&scenery_index));
                }
                if let Err(e) = orchestrator.start_prefetch() {
                    tracing::warn!(error = %e, "Failed to start prefetch system");
                } else {
                    prefetcher_started = true;
                }
            } else {
                prefetcher_started = true;
            }
        }

        // Handle prewarm progress updates
        if let Some(ref mut handle) = prewarm_handle {
            while let Ok(progress) = handle.try_recv_progress() {
                match progress {
                    LibPrewarmProgress::Starting { total_tiles } => {
                        if let Some(prewarm) = dashboard.prewarm_status().cloned() {
                            let mut updated = prewarm;
                            updated.total_tiles = total_tiles;
                            dashboard.update_prewarm_progress(updated);
                        }
                    }
                    LibPrewarmProgress::TileCompleted => {
                        if let Some(prewarm) = dashboard.prewarm_status().cloned() {
                            let mut updated = prewarm;
                            updated.tile_loaded(false);
                            dashboard.update_prewarm_progress(updated);
                        }
                    }
                    LibPrewarmProgress::TileCached => {
                        if let Some(prewarm) = dashboard.prewarm_status().cloned() {
                            let mut updated = prewarm;
                            updated.tile_loaded(true);
                            dashboard.update_prewarm_progress(updated);
                        }
                    }
                    LibPrewarmProgress::BatchProgress {
                        completed,
                        cached,
                        failed: _,
                    } => {
                        if let Some(prewarm) = dashboard.prewarm_status().cloned() {
                            let mut updated = prewarm;
                            updated.tiles_loaded_batch(completed, cached);
                            dashboard.update_prewarm_progress(updated);
                        }
                    }
                    LibPrewarmProgress::Complete {
                        tiles_completed,
                        cache_hits,
                        failed,
                    } => {
                        tracing::info!(tiles_completed, cache_hits, failed, "Prewarm complete");
                        prewarm_complete = true;
                        prewarm_active = false;
                        dashboard.clear_prewarm_status();
                    }
                    LibPrewarmProgress::Cancelled {
                        tiles_completed,
                        tiles_pending,
                    } => {
                        tracing::info!(tiles_completed, tiles_pending, "Prewarm cancelled");
                        prewarm_complete = true;
                        prewarm_active = false;
                        dashboard.clear_prewarm_status();
                    }
                }
            }
        }

        // Legacy prefetcher start block - fallback path
        if indexing_complete && prewarm_complete && !prefetcher_started {
            if prefetch_enabled {
                if scenery_index.tile_count() > 0 {
                    orchestrator.set_scenery_index(Arc::clone(&scenery_index));
                }
                if let Err(e) = orchestrator.start_prefetch() {
                    tracing::warn!(error = %e, "Failed to start prefetch system");
                } else {
                    prefetcher_started = true;
                }
            } else {
                prefetcher_started = true;
            }
            dashboard.transition_to_running();
        }

        // Update dashboard at tick rate
        if last_tick.elapsed() >= tick_rate {
            if dashboard.is_loading() {
                dashboard
                    .draw_loading()
                    .map_err(|e| CliError::Config(format!("Dashboard draw error: {}", e)))?;
            } else {
                let snapshot = orchestrator.telemetry_snapshot();
                dashboard
                    .draw(&snapshot)
                    .map_err(|e| CliError::Config(format!("Dashboard draw error: {}", e)))?;
            }
            last_tick = Instant::now();
        }

        // Small sleep to prevent busy-waiting
        std::thread::sleep(Duration::from_millis(10));
    }

    Ok(orchestrator.cancellation())
}

/// Run in headless mode (non-TTY environments).
///
/// This is a simple wait loop that displays periodic telemetry stats
/// until the shutdown signal is received.
pub fn run_headless(
    orchestrator: &mut ServiceOrchestrator,
    shutdown: Arc<AtomicBool>,
) -> Result<(), CliError> {
    println!("Start X-Plane to use XEarthLayer scenery.");
    println!("Press Ctrl+C to stop.");
    println!();

    let mut last_telemetry = Instant::now();
    let telemetry_interval = Duration::from_secs(30);

    while !shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));

        // Display telemetry every 30 seconds
        if last_telemetry.elapsed() >= telemetry_interval {
            let snapshot = orchestrator.telemetry_snapshot();
            ui::dashboard::print_simple_status(&snapshot);
            last_telemetry = Instant::now();
        }
    }

    Ok(())
}
