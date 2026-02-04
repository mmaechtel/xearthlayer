//! Coordinate conversion utilities for the Scene Tracker.
//!
//! This module provides scene-tracker-specific coordinate operations,
//! delegating to [`crate::coord`] for the core Web Mercator math.
//!
//! # Design Rationale
//!
//! Rather than duplicating coordinate conversion code, we re-export and
//! extend the existing `coord` module functionality with scene-tracker
//! specific traits and helpers.

use crate::coord::{tile_to_lat_lon_center, TileCoord};

/// Trait for types that can be converted to geographic coordinates.
///
/// This is implemented for scene tracker types that represent tile positions.
pub trait TileCoordConversion {
    /// Convert to latitude/longitude (center of tile).
    fn to_lat_lon(&self) -> (f64, f64);

    /// Convert to a 1°×1° geographic region.
    fn to_geo_region(&self) -> super::GeoRegion {
        let (lat, lon) = self.to_lat_lon();
        super::GeoRegion::from_lat_lon(lat, lon)
    }
}

impl TileCoordConversion for TileCoord {
    fn to_lat_lon(&self) -> (f64, f64) {
        tile_to_lat_lon_center(self)
    }
}

impl TileCoordConversion for super::DdsTileCoord {
    fn to_lat_lon(&self) -> (f64, f64) {
        self.to_lat_lon()
    }
}

/// Convert tile row/col/zoom to lat/lon center point.
///
/// This is a convenience function that wraps [`crate::coord::tile_to_lat_lon_center`].
///
/// # Arguments
///
/// * `row` - Web Mercator tile row (Y coordinate)
/// * `col` - Web Mercator tile column (X coordinate)
/// * `zoom` - Zoom level
///
/// # Returns
///
/// A tuple of (latitude, longitude) in degrees.
#[inline]
pub fn tile_to_lat_lon(row: u32, col: u32, zoom: u8) -> (f64, f64) {
    let tile = TileCoord { row, col, zoom };
    tile_to_lat_lon_center(&tile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_to_lat_lon_northern_europe() {
        // Tile at ZL14 in northern Europe
        // Tile (5236, 8652, 14) is approximately 54.3°N, 10.0°E
        let (lat, lon) = tile_to_lat_lon(5236, 8652, 14);

        // Verify the actual coordinates (northern Germany/Denmark area)
        assert!(lat > 54.0 && lat < 55.0, "Expected lat ~54.3, got {}", lat);
        assert!(lon > 9.0 && lon < 11.0, "Expected lon ~10, got {}", lon);
    }

    #[test]
    fn test_tile_to_lat_lon_equator() {
        // Tile at equator, prime meridian at ZL10
        // At zoom 10, equator is around row 512, prime meridian around col 512
        let (lat, lon) = tile_to_lat_lon(512, 512, 10);

        assert!(lat.abs() < 1.0, "Expected lat near 0, got {}", lat);
        assert!(lon.abs() < 1.0, "Expected lon near 0, got {}", lon);
    }

    #[test]
    fn test_tile_coord_conversion_trait() {
        let tile = TileCoord {
            row: 5236,
            col: 8652,
            zoom: 14,
        };
        let (lat, lon) = tile.to_lat_lon();

        // Northern Germany/Denmark area
        assert!(lat > 54.0 && lat < 55.0);
        assert!(lon > 9.0 && lon < 11.0);
    }

    #[test]
    fn test_dds_tile_coord_conversion_trait() {
        let dds = super::super::DdsTileCoord::new(5236, 8652, 14);
        let (lat, lon) = TileCoordConversion::to_lat_lon(&dds);

        // Northern Germany/Denmark area
        assert!(lat > 54.0 && lat < 55.0);
        assert!(lon > 9.0 && lon < 11.0);
    }

    #[test]
    fn test_to_geo_region_via_trait() {
        let tile = TileCoord {
            row: 5236,
            col: 8652,
            zoom: 14,
        };
        let region = tile.to_geo_region();

        // Region should be 54N, 10E (northern Germany/Denmark)
        assert_eq!(region.lat, 54);
        assert!(region.lon == 9 || region.lon == 10);
    }
}
