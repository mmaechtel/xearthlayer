//! Pre-warm prefetcher for cold-start cache warming.
//!
//! This module provides a one-shot prefetcher that loads tiles around a
//! specific airport before the flight starts, using terrain file scanning.
//!
//! # Architecture
//!
//! The prewarm system is decomposed into focused sub-modules:
//!
//! ```text
//! PrewarmPrefetcher (orchestrator)
//!     │
//!     ├─► grid.rs: DSF grid generation
//!     │     └─ generate_dsf_grid(), DsfGridBounds
//!     │
//!     ├─► scanner.rs: Terrain file discovery
//!     │     └─ TerrainScanner trait, FileTerrainScanner
//!     │
//!     ├─► submitter.rs: Job submission with backpressure
//!     │     └─ TileSubmitter, SubmissionResult
//!     │
//!     └─► config.rs: Configuration types
//!           └─ PrewarmConfig, PrewarmProgress
//! ```
//!
//! # Workflow
//!
//! 1. Compute an N×N grid of DSF (1°×1°) tiles centered on the target airport
//! 2. Scan `terrain/` folders to find tiles within the grid bounds
//! 3. Filter out tiles already in memory cache
//! 4. Submit tiles to executor with backpressure and track completions
//! 5. Report progress as tiles complete
//!
//! # Example
//!
//! ```ignore
//! let prewarm = PrewarmPrefetcher::new(
//!     ortho_index,
//!     dds_client,
//!     memory_cache,
//!     PrewarmConfig::default(),
//! );
//!
//! let (progress_tx, mut progress_rx) = mpsc::channel(32);
//! let result = prewarm.run(43.6294, 1.3678, progress_tx, cancellation).await;
//! ```

mod config;
mod grid;
mod scanner;
mod submitter;

// Public API
pub use config::{PrewarmConfig, PrewarmProgress};
pub use grid::{generate_dsf_grid, DsfGridBounds};
pub use scanner::{FileTerrainScanner, TerrainScanner};
pub use submitter::TileSubmitter;

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::executor::{DdsClient, MemoryCache};
use crate::ortho_union::OrthoUnionIndex;

/// Pre-warm prefetcher for loading tiles around an airport.
///
/// Orchestrates DSF grid generation, terrain scanning, cache filtering,
/// and tile submission to warm the cache before flight.
///
/// # Type Parameters
///
/// * `M` - Memory cache implementation for checking cached tiles
pub struct PrewarmPrefetcher<M: MemoryCache> {
    scanner: FileTerrainScanner,
    submitter: TileSubmitter,
    memory_cache: Arc<M>,
    config: PrewarmConfig,
}

impl<M: MemoryCache + Send + Sync + 'static> PrewarmPrefetcher<M> {
    /// Create a new prewarm prefetcher.
    pub fn new(
        ortho_index: Arc<OrthoUnionIndex>,
        dds_client: Arc<dyn DdsClient>,
        memory_cache: Arc<M>,
        config: PrewarmConfig,
    ) -> Self {
        Self {
            scanner: FileTerrainScanner::new(ortho_index),
            submitter: TileSubmitter::new(dds_client),
            memory_cache,
            config,
        }
    }

    /// Create a new prewarm prefetcher with custom scanner and submitter configs.
    pub fn with_components(
        scanner: FileTerrainScanner,
        submitter: TileSubmitter,
        memory_cache: Arc<M>,
        config: PrewarmConfig,
    ) -> Self {
        Self {
            scanner,
            submitter,
            memory_cache,
            config,
        }
    }

    /// Run the prewarm prefetcher.
    ///
    /// Generates an N×N grid of DSF tiles centered on the airport coordinates,
    /// scans terrain files to find DDS textures, filters cached tiles, and
    /// submits the rest for generation.
    ///
    /// Progress updates are sent through the channel as tiles complete.
    ///
    /// # Arguments
    ///
    /// * `lat` - Airport latitude
    /// * `lon` - Airport longitude
    /// * `progress_tx` - Channel for progress updates
    /// * `cancellation` - Token for cancellation
    ///
    /// # Returns
    ///
    /// Number of tiles that completed successfully.
    pub async fn run(
        &self,
        lat: f64,
        lon: f64,
        progress_tx: mpsc::Sender<PrewarmProgress>,
        cancellation: CancellationToken,
    ) -> usize {
        // Step 1: Generate DSF grid centered on airport
        let dsf_tiles = generate_dsf_grid(lat, lon, self.config.grid_size);
        let bounds = DsfGridBounds::from_tiles(&dsf_tiles);

        info!(
            lat = lat,
            lon = lon,
            grid_size = self.config.grid_size,
            dsf_tiles = dsf_tiles.len(),
            bounds = ?bounds,
            "Starting prewarm terrain scan"
        );

        // Check for early cancellation
        if cancellation.is_cancelled() {
            let _ = progress_tx
                .send(PrewarmProgress::Cancelled {
                    tiles_completed: 0,
                    tiles_pending: 0,
                })
                .await;
            return 0;
        }

        // Step 2: Scan terrain files to find tiles within bounds
        let unique_tiles = self.scanner.scan(&bounds);

        if unique_tiles.is_empty() {
            info!(
                lat = lat,
                lon = lon,
                grid_size = self.config.grid_size,
                "No DDS tiles found in prewarm area"
            );
            let _ = progress_tx
                .send(PrewarmProgress::Complete {
                    tiles_completed: 0,
                    cache_hits: 0,
                    failed: 0,
                })
                .await;
            return 0;
        }

        info!(
            total = unique_tiles.len(),
            grid_size = self.config.grid_size,
            "Found tiles for prewarm"
        );

        let _ = progress_tx
            .send(PrewarmProgress::Starting {
                total_tiles: unique_tiles.len(),
            })
            .await;

        // Step 3: Filter out cached tiles
        let mut tiles_to_generate = Vec::new();
        let mut cache_hits = 0usize;

        for tile in unique_tiles.iter() {
            if self
                .memory_cache
                .get(tile.row, tile.col, tile.zoom)
                .await
                .is_some()
            {
                cache_hits += 1;
            } else {
                tiles_to_generate.push(*tile);
            }
        }

        info!(
            tiles_to_generate = tiles_to_generate.len(),
            cache_hits, "Cache check complete, starting tile generation"
        );

        // Step 4: Submit and track completions
        let result = self
            .submitter
            .submit_and_track(tiles_to_generate, cache_hits, progress_tx, cancellation)
            .await;

        result.tiles_completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify core types are exported
        let _config = PrewarmConfig::default();
        let _progress = PrewarmProgress::Starting { total_tiles: 0 };
    }

    #[test]
    fn test_generate_dsf_grid_exported() {
        let tiles = generate_dsf_grid(43.0, 1.0, 4);
        assert_eq!(tiles.len(), 16);
    }
}
