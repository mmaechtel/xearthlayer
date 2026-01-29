//! Cruise flight prefetch strategy.
//!
//! Implements band-based prefetching for cruise flight, loading scenery
//! tiles in bands ahead of the aircraft's track direction.
//!
//! # Algorithm
//!
//! 1. Determine track quadrant (N/S/E/W or diagonal NE/SE/SW/NW)
//! 2. Calculate DSF tile bands ahead of travel direction
//! 3. For each DSF tile, enumerate contained DDS tiles
//! 4. Filter out already-cached tiles
//! 5. Order by distance (closest first)
//!
//! # X-Plane Behavior Match
//!
//! This strategy matches X-Plane's observed scenery loading behavior:
//! - Cardinal directions load a single band perpendicular to travel
//! - Diagonal directions load BOTH lat AND lon bands
//! - Lead distance of 2-3° matches X-Plane's ahead-loading

use std::collections::HashSet;
use std::sync::Arc;

use crate::coord::TileCoord;
use crate::prefetch::SceneryIndex;

use super::band_calculator::{BandCalculator, DsfTileCoord};
use super::calibration::PerformanceCalibration;
use super::config::AdaptivePrefetchConfig;
use super::phase_detector::FlightPhase;
use super::strategy::{
    AdaptivePrefetchStrategy, PrefetchPlan, PrefetchPlanMetadata, TrackQuadrant,
};

/// Cruise flight prefetch strategy.
///
/// Uses band-based prefetching to load scenery ahead of the aircraft's
/// track direction during cruise flight.
///
/// **Requires a scenery index** to determine which DDS tiles exist within
/// each DSF tile. Without an index, this strategy returns empty plans.
pub struct CruiseStrategy {
    /// Band calculator for DSF tile geometry.
    band_calculator: BandCalculator,

    /// Scenery index for looking up DDS tiles within DSF tiles.
    scenery_index: Option<Arc<SceneryIndex>>,

    /// Maximum tiles per prefetch cycle.
    max_tiles: u32,
}

impl std::fmt::Debug for CruiseStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CruiseStrategy")
            .field("band_calculator", &self.band_calculator)
            .field("has_scenery_index", &self.scenery_index.is_some())
            .field("max_tiles", &self.max_tiles)
            .finish()
    }
}

impl CruiseStrategy {
    /// Create a new cruise strategy with the given configuration.
    pub fn new(config: &AdaptivePrefetchConfig) -> Self {
        Self {
            band_calculator: BandCalculator::new(config),
            scenery_index: None,
            max_tiles: config.max_tiles_per_cycle,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(&AdaptivePrefetchConfig::default())
    }

    /// Set the scenery index for accurate tile lookup.
    ///
    /// The scenery index provides the actual DDS tiles within each DSF tile.
    /// Without an index, this strategy returns empty plans since there's
    /// no scenery to prefetch.
    pub fn with_scenery_index(mut self, index: Arc<SceneryIndex>) -> Self {
        self.scenery_index = Some(index);
        self
    }

    /// Check if a scenery index is configured.
    ///
    /// The coordinator can use this to log a warning if no index is available.
    pub fn has_scenery_index(&self) -> bool {
        self.scenery_index.is_some()
    }

    /// Get DDS tiles for a DSF tile from scenery index or fallback generation.
    ///
    /// If a scenery index is available, queries it for tiles in the DSF.
    /// Otherwise, generates a grid of tiles covering the DSF tile at zoom 14.
    fn get_dds_tiles_in_dsf(&self, dsf: &DsfTileCoord) -> Vec<TileCoord> {
        if let Some(ref index) = self.scenery_index {
            // Query scenery index for actual tiles
            let (center_lat, center_lon) = dsf.center();
            // 1° DSF tile ≈ 60nm at equator, use 45nm radius to cover the tile
            let tiles = index.tiles_near(center_lat, center_lon, 45.0);
            let result: Vec<TileCoord> = tiles.iter().map(|t| t.to_tile_coord()).collect();

            if !result.is_empty() {
                return result;
            }
            // Fall through to generate tiles if scenery index had no tiles
        }

        // Fallback: Generate tiles covering the DSF tile at zoom 14
        self.generate_tiles_for_dsf(dsf)
    }

    /// Generate DDS tile coordinates covering a DSF tile.
    ///
    /// Used as fallback when no scenery index is available or when
    /// the scenery index has no tiles for this DSF.
    fn generate_tiles_for_dsf(&self, dsf: &DsfTileCoord) -> Vec<TileCoord> {
        use crate::coord::to_tile_coords;

        const ZOOM: u8 = 14;

        // DSF tiles are 1° × 1°. Generate tiles at grid points.
        let lat_min = dsf.lat as f64;
        let lon_min = dsf.lon as f64;

        let mut tiles = Vec::new();

        // Sample a 4x4 grid within the DSF tile
        for lat_step in 0..4 {
            for lon_step in 0..4 {
                let lat = lat_min + (lat_step as f64 + 0.5) * 0.25;
                let lon = lon_min + (lon_step as f64 + 0.5) * 0.25;

                if let Ok(coord) = to_tile_coords(lat, lon, ZOOM) {
                    tiles.push(coord);
                }
            }
        }

        // Remove duplicates
        tiles.sort_by_key(|t| (t.row, t.col));
        tiles.dedup();

        tiles
    }
}

impl AdaptivePrefetchStrategy for CruiseStrategy {
    fn calculate_prefetch(
        &self,
        position: (f64, f64),
        track: f64,
        calibration: &PerformanceCalibration,
        already_cached: &HashSet<TileCoord>,
    ) -> PrefetchPlan {
        // Get DSF tiles in prefetch bands
        let dsf_tiles = self.band_calculator.calculate_bands(position, track);
        let dsf_tile_count = dsf_tiles.len();

        // Determine track quadrant for metadata
        let quadrant = TrackQuadrant::from_track(track);
        let quadrant_name = match quadrant {
            TrackQuadrant::North => "north",
            TrackQuadrant::Northeast => "northeast",
            TrackQuadrant::East => "east",
            TrackQuadrant::Southeast => "southeast",
            TrackQuadrant::South => "south",
            TrackQuadrant::Southwest => "southwest",
            TrackQuadrant::West => "west",
            TrackQuadrant::Northwest => "northwest",
        };

        if dsf_tiles.is_empty() {
            return PrefetchPlan::empty(self.name());
        }

        // Collect all DDS tiles from the DSF tiles
        let mut all_tiles: Vec<TileCoord> = Vec::new();
        for dsf in &dsf_tiles {
            let dds_tiles = self.get_dds_tiles_in_dsf(dsf);
            all_tiles.extend(dds_tiles);
        }

        let total_considered = all_tiles.len();

        // Remove duplicates
        all_tiles.sort_by_key(|t| (t.row, t.col, t.zoom));
        all_tiles.dedup();

        // Filter out already-cached tiles
        let tiles_before_filter = all_tiles.len();
        all_tiles.retain(|t| !already_cached.contains(t));
        let skipped_cached = tiles_before_filter - all_tiles.len();

        // Sort by distance from aircraft position
        let (lat, lon) = position;
        all_tiles.sort_by(|a, b| {
            let (lat_a, lon_a) = a.to_lat_lon();
            let (lat_b, lon_b) = b.to_lat_lon();
            let dist_a = (lat_a - lat).powi(2) + (lon_a - lon).powi(2);
            let dist_b = (lat_b - lat).powi(2) + (lon_b - lon).powi(2);
            dist_a
                .partial_cmp(&dist_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit to max tiles
        if all_tiles.len() > self.max_tiles as usize {
            all_tiles.truncate(self.max_tiles as usize);
        }

        // Create metadata for coordinator logging
        let metadata = PrefetchPlanMetadata::cruise(dsf_tile_count, quadrant_name);

        PrefetchPlan::with_tiles_and_metadata(
            all_tiles,
            calibration,
            self.name(),
            skipped_cached,
            total_considered,
            metadata,
        )
    }

    fn name(&self) -> &'static str {
        "cruise"
    }

    fn is_applicable(&self, phase: &FlightPhase) -> bool {
        *phase == FlightPhase::Cruise
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn test_calibration() -> PerformanceCalibration {
        PerformanceCalibration {
            throughput_tiles_per_sec: 20.0,
            avg_tile_generation_ms: 50,
            tile_generation_stddev_ms: 10,
            confidence: 0.9,
            recommended_strategy: super::super::calibration::StrategyMode::Opportunistic,
            calibrated_at: Instant::now(),
            baseline_throughput: 20.0,
            sample_count: 100,
        }
    }

    #[test]
    fn test_cruise_strategy_creation() {
        let strategy = CruiseStrategy::with_defaults();
        assert_eq!(strategy.name(), "cruise");
        assert!(strategy.is_applicable(&FlightPhase::Cruise));
        assert!(!strategy.is_applicable(&FlightPhase::Ground));
    }

    #[test]
    fn test_cruise_strategy_without_index_generates_fallback_tiles() {
        // Without a scenery index, cruise strategy generates fallback tiles
        let strategy = CruiseStrategy::with_defaults();
        let calibration = test_calibration();
        let cached = HashSet::new();

        let plan = strategy.calculate_prefetch((53.5, 9.5), 0.0, &calibration, &cached);

        // Fallback tile generation should produce tiles
        assert!(!plan.is_empty(), "Should generate fallback tiles");
        assert_eq!(plan.strategy, "cruise");
    }

    #[test]
    fn test_cruise_strategy_has_scenery_index() {
        let strategy = CruiseStrategy::with_defaults();
        assert!(!strategy.has_scenery_index());

        // Note: we can't easily test with_scenery_index() without mocking SceneryIndex
        // That will be tested in integration tests
    }

    #[test]
    fn test_cruise_strategy_limits_tiles() {
        let config = AdaptivePrefetchConfig {
            max_tiles_per_cycle: 10,
            ..Default::default()
        };
        let strategy = CruiseStrategy::new(&config);
        let calibration = test_calibration();
        let cached = HashSet::new();

        let plan = strategy.calculate_prefetch((53.5, 9.5), 0.0, &calibration, &cached);

        // Even with fallback tile generation, limit is respected
        assert!(plan.tile_count() <= 10);
    }

    #[test]
    fn test_cruise_strategy_metadata_for_cardinal_tracks() {
        let strategy = CruiseStrategy::with_defaults();
        let calibration = test_calibration();
        let cached = HashSet::new();

        // Test that metadata is populated even with empty tiles
        let plan = strategy.calculate_prefetch((53.5, 9.5), 0.0, &calibration, &cached);

        // Metadata should be present (tiles are empty but metadata tells us why)
        let metadata = plan.metadata.expect("should have metadata");
        assert_eq!(metadata.bounds_source, "track");
        assert_eq!(metadata.track_quadrant, Some("north"));
        assert!(metadata.dsf_tile_count > 0); // DSF tiles were calculated
    }

    #[test]
    fn test_cruise_strategy_metadata_quadrant_names() {
        let strategy = CruiseStrategy::with_defaults();
        let calibration = test_calibration();
        let cached = HashSet::new();

        // Test all 8 quadrants produce correct names
        let test_cases = [
            (0.0, "north"),
            (45.0, "northeast"),
            (90.0, "east"),
            (135.0, "southeast"),
            (180.0, "south"),
            (225.0, "southwest"),
            (270.0, "west"),
            (315.0, "northwest"),
        ];

        for (track, expected_quadrant) in test_cases {
            let plan = strategy.calculate_prefetch((53.5, 9.5), track, &calibration, &cached);
            let metadata = plan.metadata.expect("should have metadata");
            assert_eq!(
                metadata.track_quadrant,
                Some(expected_quadrant),
                "Track {} should produce quadrant {}",
                track,
                expected_quadrant
            );
        }
    }

    #[test]
    fn test_cruise_strategy_dsf_tile_count_varies_by_direction() {
        let strategy = CruiseStrategy::with_defaults();
        let calibration = test_calibration();
        let cached = HashSet::new();

        // Cardinal (north)
        let cardinal = strategy.calculate_prefetch((53.5, 9.5), 0.0, &calibration, &cached);

        // Diagonal (northeast) - should have more DSF tiles (both lat and lon bands)
        let diagonal = strategy.calculate_prefetch((53.5, 9.5), 45.0, &calibration, &cached);

        let cardinal_dsf = cardinal.metadata.as_ref().unwrap().dsf_tile_count;
        let diagonal_dsf = diagonal.metadata.as_ref().unwrap().dsf_tile_count;

        assert!(
            diagonal_dsf > cardinal_dsf,
            "Diagonal ({} DSF tiles) should have more DSF tiles than cardinal ({})",
            diagonal_dsf,
            cardinal_dsf
        );
    }
}
