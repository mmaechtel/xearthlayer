//! Prewarm orchestrator for background cache warming.
//!
//! This module provides `PrewarmOrchestrator` which handles the startup
//! and lifecycle of the prewarm system that pre-caches tiles around airports.
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::service::PrewarmOrchestrator;
//!
//! let handle = PrewarmOrchestrator::start(
//!     orchestrator,
//!     "KSFO",
//!     xplane_env,
//! )?;
//!
//! // Receive progress updates
//! while let Some(progress) = handle.try_recv_progress() {
//!     println!("Progress: {:?}", progress);
//! }
//!
//! // Cancel if needed
//! handle.cancel();
//! ```

use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::aircraft_position::SharedAircraftPosition;
use crate::airport::AirportIndex;
use crate::prefetch::{PrewarmConfig as PrefetchPrewarmConfig, PrewarmPrefetcher, PrewarmProgress};
use crate::xplane::XPlaneEnvironment;

use super::orchestrator_config::PrewarmConfig;
use super::ServiceOrchestrator;

/// Error returned when prewarm fails to start.
#[derive(Debug, Clone)]
pub struct PrewarmStartError {
    message: String,
}

impl PrewarmStartError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for PrewarmStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Prewarm start error: {}", self.message)
    }
}

impl std::error::Error for PrewarmStartError {}

/// Result of a successful prewarm start.
pub struct PrewarmStartResult {
    /// Handle to the running prewarm task.
    pub handle: PrewarmHandle,
    /// Name of the airport being prewarmed.
    pub airport_name: String,
    /// Estimated number of tiles to prewarm.
    pub estimated_tiles: usize,
}

/// Handle to a running prewarm task.
pub struct PrewarmHandle {
    /// Progress receiver for UI updates.
    progress_rx: mpsc::Receiver<PrewarmProgress>,
    /// Cancellation token to stop the prewarm.
    cancellation: CancellationToken,
}

impl PrewarmHandle {
    /// Create a new prewarm handle.
    fn new(progress_rx: mpsc::Receiver<PrewarmProgress>, cancellation: CancellationToken) -> Self {
        Self {
            progress_rx,
            cancellation,
        }
    }

    /// Try to receive progress (non-blocking).
    pub fn try_recv_progress(&mut self) -> Result<PrewarmProgress, mpsc::error::TryRecvError> {
        self.progress_rx.try_recv()
    }

    /// Get mutable access to the progress receiver.
    pub fn progress_receiver(&mut self) -> &mut mpsc::Receiver<PrewarmProgress> {
        &mut self.progress_rx
    }

    /// Cancel the prewarm task.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Check if the prewarm has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Get the cancellation token.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

/// Orchestrates background cache pre-warming around airports.
///
/// This struct provides a clean API for starting prewarm operations, which
/// was previously scattered in the CLI layer. It handles:
///
/// 1. Airport lookup from X-Plane's apt.dat database
/// 2. APT position seeding with airport coordinates
/// 3. OrthoUnionIndex and DDS client/cache wiring
/// 4. Spawning the prewarm task with progress channel
pub struct PrewarmOrchestrator;

impl PrewarmOrchestrator {
    /// Start prewarm for a given airport.
    ///
    /// Uses tile-based (DSF grid) enumeration to find all DDS textures within
    /// an N×N grid of 1°×1° tiles centered on the target airport.
    ///
    /// # Arguments
    ///
    /// * `orchestrator` - Service orchestrator with mounted services
    /// * `icao` - Airport ICAO code (e.g., "KSFO")
    /// * `xplane_env` - X-Plane environment for apt.dat lookup
    /// * `aircraft_position` - Aircraft position for APT seeding
    /// * `config` - Prewarm configuration (grid size, batch size)
    /// * `runtime_handle` - Tokio runtime handle for spawning
    ///
    /// # Returns
    ///
    /// Returns a `PrewarmStartResult` containing:
    /// - `handle` - Handle to manage and receive progress from the prewarm task
    /// - `airport_name` - Name of the airport being prewarmed
    /// - `estimated_tiles` - Rough estimate of tiles to be prewarmed
    pub fn start(
        orchestrator: &mut ServiceOrchestrator,
        icao: &str,
        xplane_env: Option<&XPlaneEnvironment>,
        aircraft_position: &SharedAircraftPosition,
        config: &PrewarmConfig,
        runtime_handle: &Handle,
    ) -> Result<PrewarmStartResult, PrewarmStartError> {
        // Get X-Plane environment for apt.dat lookup
        let xplane_env = xplane_env
            .ok_or_else(|| PrewarmStartError::new("X-Plane installation not detected"))?;

        // Get apt.dat path
        let apt_dat_path = xplane_env.apt_dat_path().ok_or_else(|| {
            PrewarmStartError::new(format!(
                "Airport database not found at {}",
                xplane_env.earth_nav_data_path().display()
            ))
        })?;

        // Load airport index from apt.dat
        let airport_index = AirportIndex::from_apt_dat(&apt_dat_path).map_err(|e| {
            PrewarmStartError::new(format!("Failed to load airport database: {}", e))
        })?;

        // Look up the airport
        let airport = airport_index.get(icao).ok_or_else(|| {
            PrewarmStartError::new(format!("Airport '{}' not found in apt.dat", icao))
        })?;

        // Get OrthoUnionIndex from mount manager
        let ortho_index = orchestrator
            .ortho_union_index()
            .ok_or_else(|| PrewarmStartError::new("OrthoUnionIndex not available for prewarm"))?;

        // Seed APT with airport position (manual reference source)
        // This provides initial position for prefetch and dashboard before telemetry connects
        if aircraft_position.receive_manual_reference(airport.latitude, airport.longitude) {
            tracing::info!(
                icao = %icao,
                lat = airport.latitude,
                lon = airport.longitude,
                "APT seeded with airport position"
            );
        }

        // Log airport coordinates and grid info for debugging
        tracing::debug!(
            airport_lat = airport.latitude,
            airport_lon = airport.longitude,
            airport_name = %airport.name,
            grid_size = config.grid_size,
            "Starting tile-based prewarm"
        );

        // Get service for DDS handler and memory cache
        let service = orchestrator
            .service()
            .ok_or_else(|| PrewarmStartError::new("No services available for prewarm"))?;

        let memory_cache = service
            .memory_cache_adapter()
            .ok_or_else(|| PrewarmStartError::new("Memory cache not available for prewarm"))?;

        let dds_client = service
            .dds_client()
            .expect("DDS client should be available");

        // Create prefetch layer prewarm config
        let prewarm_config = PrefetchPrewarmConfig {
            grid_size: config.grid_size,
            batch_size: config.batch_size,
        };

        // Estimate tile count for UI progress (actual count determined at runtime)
        // Rough estimate: grid_size² DSF tiles × ~50 DDS tiles per DSF tile on average
        let estimated_tiles = (config.grid_size * config.grid_size * 50) as usize;

        // Create the prewarm prefetcher
        let prewarm = PrewarmPrefetcher::new(ortho_index, dds_client, memory_cache, prewarm_config);

        // Create progress channel and cancellation token
        let (progress_tx, progress_rx) = mpsc::channel(32);
        let cancellation = CancellationToken::new();
        let cancel_token = cancellation.clone();
        let airport_lat = airport.latitude;
        let airport_lon = airport.longitude;
        let airport_name = airport.name.clone();

        // Spawn the prewarm task
        runtime_handle.spawn(async move {
            prewarm
                .run(airport_lat, airport_lon, progress_tx, cancel_token)
                .await;
        });

        Ok(PrewarmStartResult {
            handle: PrewarmHandle::new(progress_rx, cancellation),
            airport_name,
            estimated_tiles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prewarm_start_error_display() {
        let error = PrewarmStartError::new("test error");
        assert!(error.to_string().contains("test error"));
    }

    #[test]
    fn test_prewarm_handle_cancellation() {
        let (_, rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let handle = PrewarmHandle::new(rx, cancellation);

        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
    }
}
