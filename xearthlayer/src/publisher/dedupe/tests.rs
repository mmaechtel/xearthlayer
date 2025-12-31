//! Integration tests for the dedupe module.
//!
//! These tests verify that the detector and resolver work together correctly.

use super::*;
use std::path::PathBuf;

/// Create a test tile reference.
fn make_tile(row: u32, col: u32, zoom: u8, lat: f32, lon: f32) -> TileReference {
    TileReference {
        row,
        col,
        zoom,
        provider: "BI".to_string(),
        lat,
        lon,
        ter_path: PathBuf::from(format!("{}_{}_{}{}.ter", row, col, "BI", zoom)),
        is_sea: false,
    }
}

/// Create a ZoomOverlap for two tiles.
fn make_overlap(higher: &TileReference, lower: &TileReference) -> ZoomOverlap {
    ZoomOverlap {
        higher_zl: higher.clone(),
        lower_zl: lower.clone(),
        zl_diff: higher.zoom - lower.zoom,
        coverage: OverlapCoverage::Complete,
    }
}

#[test]
fn test_full_workflow_highest_priority() {
    // Simulate a package with overlapping ZL18 and ZL16 tiles
    // ZL18 (100032, 42688) → ZL16 (25008, 10672)
    let zl18 = make_tile(100032, 42688, 18, 39.15, -121.36);
    let zl16 = make_tile(25008, 10672, 16, 39.15, -121.36);

    let tiles = vec![zl18.clone(), zl16.clone()];

    // Detect overlaps
    let detector = OverlapDetector::new();
    let overlaps = detector.detect_overlaps(&tiles);

    assert_eq!(overlaps.len(), 1);
    assert_eq!(overlaps[0].higher_zl.zoom, 18);
    assert_eq!(overlaps[0].lower_zl.zoom, 16);

    // Resolve with highest priority (default)
    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Highest);

    assert_eq!(result.tiles_analyzed, 2);
    assert_eq!(result.tiles_removed.len(), 1);
    assert_eq!(result.tiles_removed[0].zoom, 16); // Lower removed
    assert_eq!(result.tiles_preserved.len(), 1);
    assert_eq!(result.tiles_preserved[0].zoom, 18); // Higher kept
}

#[test]
fn test_full_workflow_lowest_priority() {
    let zl18 = make_tile(100032, 42688, 18, 39.15, -121.36);
    let zl16 = make_tile(25008, 10672, 16, 39.15, -121.36);

    let tiles = vec![zl18.clone(), zl16.clone()];
    let detector = OverlapDetector::new();
    let overlaps = detector.detect_overlaps(&tiles);

    // Resolve with lowest priority (smaller package)
    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Lowest);

    assert_eq!(result.tiles_removed.len(), 1);
    assert_eq!(result.tiles_removed[0].zoom, 18); // Higher removed
    assert_eq!(result.tiles_preserved.len(), 1);
    assert_eq!(result.tiles_preserved[0].zoom, 16); // Lower kept
}

#[test]
fn test_three_level_cascade() {
    // ZL18 → ZL16 → ZL14 overlaps
    // Coordinates: ZL18(100032, 42688) → ZL16(25008, 10672) → ZL14(6252, 2668)
    let zl18 = make_tile(100032, 42688, 18, 39.15, -121.36);
    let zl16 = make_tile(25008, 10672, 16, 39.15, -121.36);
    let zl14 = make_tile(6252, 2668, 14, 39.15, -121.36);

    let tiles = vec![zl18.clone(), zl16.clone(), zl14.clone()];

    let detector = OverlapDetector::new();
    let overlaps = detector.detect_overlaps(&tiles);

    // Should detect 3 overlaps: 18→16, 18→14, 16→14
    assert_eq!(overlaps.len(), 3);

    // With highest priority, ZL14 and ZL16 should be removed
    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Highest);

    assert_eq!(result.tiles_removed.len(), 2);
    let removed_zooms: Vec<u8> = result.tiles_removed.iter().map(|t| t.zoom).collect();
    assert!(removed_zooms.contains(&14));
    assert!(removed_zooms.contains(&16));
    assert_eq!(result.tiles_preserved.len(), 1);
    assert_eq!(result.tiles_preserved[0].zoom, 18);
}

#[test]
fn test_specific_zoom_priority() {
    let zl18 = make_tile(100032, 42688, 18, 39.15, -121.36);
    let zl16 = make_tile(25008, 10672, 16, 39.15, -121.36);

    let tiles = vec![zl18.clone(), zl16.clone()];
    let detector = OverlapDetector::new();
    let overlaps = detector.detect_overlaps(&tiles);

    // Keep ZL16 specifically
    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Specific(16));

    assert_eq!(result.tiles_removed.len(), 1);
    assert_eq!(result.tiles_removed[0].zoom, 18); // ZL18 removed
    assert_eq!(result.tiles_preserved[0].zoom, 16); // ZL16 kept
}

#[test]
fn test_no_overlap_different_areas() {
    // Two ZL18 tiles in different areas (not parent/child)
    let tile1 = make_tile(100032, 42688, 18, 39.15, -121.36);
    let tile2 = make_tile(100000, 42000, 18, 38.50, -120.50);

    let tiles = vec![tile1, tile2];
    let detector = OverlapDetector::new();
    let overlaps = detector.detect_overlaps(&tiles);

    assert!(overlaps.is_empty());

    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Highest);
    assert!(result.tiles_removed.is_empty());
    assert_eq!(result.tiles_preserved.len(), 2);
}

#[test]
fn test_filter_by_tile_coord() {
    let tile_in_cell = make_tile(100032, 42688, 18, 39.15, -121.36);
    let tile_out_of_cell = make_tile(100000, 42000, 18, 38.50, -120.50);

    // Filter for tiles in cell (39, -122)
    let filter = DedupeFilter::for_tile(39, -122);

    assert!(filter.matches(&tile_in_cell));
    assert!(!filter.matches(&tile_out_of_cell));
}

#[test]
fn test_overlaps_by_pair_stats() {
    // Multiple overlaps of the same type
    let zl18_a = make_tile(100032, 42688, 18, 39.15, -121.36);
    let zl18_b = make_tile(100048, 42704, 18, 39.20, -121.30);
    let zl16 = make_tile(25008, 10672, 16, 39.15, -121.36);

    let tiles = vec![zl18_a.clone(), zl18_b.clone(), zl16.clone()];

    // Manually create overlaps (both ZL18 tiles overlap the same ZL16)
    let overlaps = vec![make_overlap(&zl18_a, &zl16), make_overlap(&zl18_b, &zl16)];

    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Highest);

    // Should report 2 overlaps for the (18, 16) pair
    assert_eq!(result.overlaps_by_pair.get(&(18, 16)), Some(&2));
    assert_eq!(result.total_overlaps(), 2);
    assert!(result.has_overlaps());
}

#[test]
fn test_removal_set_deduplication() {
    // Same tile appears in multiple overlaps
    let zl18 = make_tile(100032, 42688, 18, 39.15, -121.36);
    let zl16 = make_tile(25008, 10672, 16, 39.15, -121.36);
    let zl14 = make_tile(6252, 2668, 14, 39.15, -121.36);

    let tiles = vec![zl18.clone(), zl16.clone(), zl14.clone()];

    // ZL16 appears in two overlaps but should only be removed once
    let overlaps = vec![make_overlap(&zl18, &zl16), make_overlap(&zl16, &zl14)];

    let result = resolve_overlaps(&tiles, &overlaps, ZoomPriority::Highest);

    // ZL16 and ZL14 removed (ZL16 once, not twice)
    assert_eq!(result.tiles_removed.len(), 2);
}
