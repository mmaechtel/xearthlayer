//! Tile filtering pipeline for prefetch plans.
//!
//! Three sequential filtering stages remove tiles that don't need prefetching:
//! 1. **Memory cache** — tiles already in the volatile memory cache
//! 2. **Patch regions** — tiles in DSF regions owned by scenery patches
//! 3. **Disk existence** — tiles already present as DDS files on disk
//!
//! Each stage returns the filtered list and a count of skipped tiles.

use std::collections::HashSet;
use std::sync::Arc;

use crate::coord::TileCoord;
use crate::executor::DaemonMemoryCache;
use crate::geo_index::{DsfRegion, GeoIndex, PatchCoverage};
use crate::ortho_union::OrthoUnionIndex;
use crate::prefetch::tile_based::DsfTileCoord;

// ─────────────────────────────────────────────────────────────────────────────
// Result type
// ─────────────────────────────────────────────────────────────────────────────

/// Counts from the filtering pipeline.
#[derive(Debug, Default)]
pub(crate) struct FilterCounts {
    /// Tiles skipped because they were in the local tracking set or memory cache.
    pub cache_hits: usize,
    /// Tiles skipped because they are in patch-owned DSF regions.
    pub patch_skipped: usize,
    /// Tiles skipped because a DDS file already exists on disk.
    pub disk_skipped: usize,
}

impl FilterCounts {
    /// Total tiles filtered across all stages.
    pub fn total(&self) -> usize {
        self.cache_hits + self.patch_skipped + self.disk_skipped
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline stages
// ─────────────────────────────────────────────────────────────────────────────

/// Filter tiles already present in the memory cache or local tracking set.
///
/// Returns the surviving tiles and the number of cache hits.
/// Tiles found in the actual cache are added to `cached_tiles` for future fast-path.
pub(crate) async fn filter_memory_cache(
    tiles: Vec<TileCoord>,
    cache: &dyn DaemonMemoryCache,
    cached_tiles: &mut HashSet<TileCoord>,
) -> (Vec<TileCoord>, usize) {
    let mut filtered = Vec::with_capacity(tiles.len());
    let mut hits = 0usize;

    for tile in &tiles {
        // Check local tracking first (fast path)
        if cached_tiles.contains(tile) {
            hits += 1;
            continue;
        }

        // Query the actual memory cache
        if cache.contains(tile.row, tile.col, tile.zoom).await {
            hits += 1;
            cached_tiles.insert(*tile);
            continue;
        }

        filtered.push(*tile);
    }

    if hits > 0 {
        tracing::debug!(
            cache_hits = hits,
            remaining = filtered.len(),
            "Filtered cached tiles from prefetch plan"
        );
    }

    (filtered, hits)
}

/// Filter tiles in DSF regions owned by scenery patches.
///
/// Returns the surviving tiles and the number of patch-filtered tiles.
pub(crate) fn filter_patched_regions(
    tiles: Vec<TileCoord>,
    geo_index: &GeoIndex,
) -> (Vec<TileCoord>, usize) {
    let before = tiles.len();
    let filtered: Vec<TileCoord> = tiles
        .into_iter()
        .filter(|tile| {
            let (lat, lon) = tile.to_lat_lon();
            let dsf = DsfTileCoord::from_lat_lon(lat, lon);
            !geo_index.contains::<PatchCoverage>(&DsfRegion::new(dsf.lat, dsf.lon))
        })
        .collect();
    let skipped = before - filtered.len();

    if skipped > 0 {
        tracing::debug!(
            patch_skipped = skipped,
            remaining = filtered.len(),
            "Filtered tiles in patched regions"
        );
    }

    (filtered, skipped)
}

/// Filter tiles that already exist as DDS files on disk.
///
/// Returns the surviving tiles and the number of disk-filtered tiles.
pub(crate) fn filter_disk_tiles(
    tiles: Vec<TileCoord>,
    ortho_index: &OrthoUnionIndex,
) -> (Vec<TileCoord>, usize) {
    let before = tiles.len();
    let filtered: Vec<TileCoord> = tiles
        .into_iter()
        .filter(|tile| {
            let (chunk_row, chunk_col, chunk_zoom) = tile.chunk_origin();
            !ortho_index.dds_tile_exists(chunk_row, chunk_col, chunk_zoom)
        })
        .collect();
    let skipped = before - filtered.len();

    if skipped > 0 {
        tracing::debug!(
            skipped,
            remaining = filtered.len(),
            "Filtered tiles already on disk"
        );
    }

    (filtered, skipped)
}

/// Run all filtering stages in sequence.
///
/// Returns the surviving tiles and aggregate filter counts.
pub(crate) async fn run_filter_pipeline(
    mut tiles: Vec<TileCoord>,
    memory_cache: Option<&dyn DaemonMemoryCache>,
    cached_tiles: &mut HashSet<TileCoord>,
    geo_index: Option<&Arc<GeoIndex>>,
    ortho_union_index: Option<&Arc<OrthoUnionIndex>>,
) -> (Vec<TileCoord>, FilterCounts) {
    let mut counts = FilterCounts::default();

    // Stage 1: Memory cache filter
    if let Some(cache) = memory_cache {
        let (filtered, hits) = filter_memory_cache(tiles, cache, cached_tiles).await;
        counts.cache_hits = hits;
        tiles = filtered;
    }

    // Stage 2: Patch region filter
    if let Some(gi) = geo_index {
        let (filtered, skipped) = filter_patched_regions(tiles, gi);
        counts.patch_skipped = skipped;
        tiles = filtered;
    }

    // Stage 3: Disk existence filter
    if let Some(index) = ortho_union_index {
        let (filtered, skipped) = filter_disk_tiles(tiles, index);
        counts.disk_skipped = skipped;
        tiles = filtered;
    }

    (tiles, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn test_tiles(count: usize) -> Vec<TileCoord> {
        (0..count)
            .map(|i| TileCoord {
                row: 100 + i as u32,
                col: 200,
                zoom: 14,
            })
            .collect()
    }

    #[test]
    fn test_filter_patched_regions_empty_input() {
        let geo_index = GeoIndex::new();
        let (result, skipped) = filter_patched_regions(vec![], &geo_index);
        assert!(result.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_patched_regions_no_patches() {
        let geo_index = GeoIndex::new();
        let tiles = test_tiles(5);
        let (result, skipped) = filter_patched_regions(tiles, &geo_index);
        assert_eq!(result.len(), 5);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_patched_regions_all_patched() {
        let geo_index = GeoIndex::new();
        let tiles = test_tiles(3);

        // Patch every possible DSF region these tiles fall in
        for tile in &tiles {
            let (lat, lon) = tile.to_lat_lon();
            let dsf = DsfTileCoord::from_lat_lon(lat, lon);
            geo_index.insert::<PatchCoverage>(
                DsfRegion::new(dsf.lat, dsf.lon),
                PatchCoverage {
                    patch_name: "test".to_string(),
                },
            );
        }

        let (result, skipped) = filter_patched_regions(tiles, &geo_index);
        assert!(result.is_empty());
        assert_eq!(skipped, 3);
    }

    #[test]
    fn test_filter_patched_regions_preserves_order() {
        let geo_index = GeoIndex::new();
        let tiles = test_tiles(5);

        // Only patch the middle tile's region
        let mid = &tiles[2];
        let (lat, lon) = mid.to_lat_lon();
        let dsf = DsfTileCoord::from_lat_lon(lat, lon);
        geo_index.insert::<PatchCoverage>(
            DsfRegion::new(dsf.lat, dsf.lon),
            PatchCoverage {
                patch_name: "test".to_string(),
            },
        );

        let (result, skipped) = filter_patched_regions(tiles.clone(), &geo_index);
        // Order of non-patched tiles should be preserved
        assert!(skipped >= 1);
        for (i, tile) in result.iter().enumerate() {
            if i > 0 {
                // Rows should be monotonically increasing (preserved order)
                assert!(tile.row >= result[i - 1].row);
            }
        }
    }

    #[test]
    fn test_filter_disk_tiles_empty_index() {
        let index = OrthoUnionIndex::new();
        let tiles = test_tiles(5);
        let (result, skipped) = filter_disk_tiles(tiles, &index);
        assert_eq!(result.len(), 5);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_counts_total() {
        let counts = FilterCounts {
            cache_hits: 3,
            patch_skipped: 2,
            disk_skipped: 1,
        };
        assert_eq!(counts.total(), 6);
    }

    #[tokio::test]
    async fn test_run_filter_pipeline_no_filters() {
        let tiles = test_tiles(5);
        let mut tracked = HashSet::new();

        let (result, counts) = run_filter_pipeline(tiles, None, &mut tracked, None, None).await;
        assert_eq!(result.len(), 5);
        assert_eq!(counts.total(), 0);
    }
}
