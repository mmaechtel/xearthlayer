//! DSF-aligned band geometry for cruise prefetch.
//!
//! Calculates which DSF tiles (1°×1°) fall within prefetch bands ahead
//! of the aircraft's track. Handles both cardinal and diagonal directions.
//!
//! # X-Plane DSF Tiles
//!
//! X-Plane organizes scenery into 1°×1° DSF (Distribution Scenery Format) tiles.
//! Each DSF tile contains multiple DDS texture tiles at various zoom levels.
//!
//! ```text
//! DSF Tile: +53+009 (53°N, 9°E)
//!   └── Contains ~100-500 DDS tiles depending on zoom level
//! ```
//!
//! # Band Geometry
//!
//! For cardinal directions (N/S/E/W), a single band perpendicular to travel:
//!
//! ```text
//! Northbound flight at 53.5°N:
//!
//!         Band (2° ahead)
//!     ┌─────────────────────┐
//!     │ +55+007 │ +55+008 │ +55+009 │ +55+010 │ +55+011 │
//!     │ +56+007 │ +56+008 │ +56+009 │ +56+010 │ +56+011 │
//!     └─────────────────────┘
//!                   ↑
//!               Aircraft
//! ```
//!
//! For diagonal directions (NE/SE/SW/NW), BOTH lat AND lon bands:
//!
//! ```text
//! Northeast flight: loads tiles in BOTH directions
//! ```

use super::config::AdaptivePrefetchConfig;
use super::strategy::TrackQuadrant;

/// DSF tile coordinate (1°×1° tile).
///
/// Uses X-Plane's DSF naming convention where coordinates represent
/// the southwest corner of the tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DsfTileCoord {
    /// Latitude of southwest corner (integer degrees).
    pub lat: i16,
    /// Longitude of southwest corner (integer degrees).
    pub lon: i16,
}

impl DsfTileCoord {
    /// Create a new DSF tile coordinate.
    pub fn new(lat: i16, lon: i16) -> Self {
        Self { lat, lon }
    }

    /// Get DSF tile containing a given position.
    pub fn from_position(lat: f64, lon: f64) -> Self {
        Self {
            lat: lat.floor() as i16,
            lon: lon.floor() as i16,
        }
    }

    /// Get the X-Plane DSF tile name (e.g., "+53+009").
    pub fn to_name(&self) -> String {
        let lat_sign = if self.lat >= 0 { '+' } else { '-' };
        let lon_sign = if self.lon >= 0 { '+' } else { '-' };
        format!(
            "{}{:02}{}{:03}",
            lat_sign,
            self.lat.abs(),
            lon_sign,
            self.lon.abs()
        )
    }

    /// Get the center position of this DSF tile.
    pub fn center(&self) -> (f64, f64) {
        (self.lat as f64 + 0.5, self.lon as f64 + 0.5)
    }

    /// Calculate approximate distance from a position (degrees).
    ///
    /// Uses simple Euclidean distance, which is good enough for
    /// nearby tiles at mid-latitudes.
    pub fn distance_from(&self, lat: f64, lon: f64) -> f64 {
        let (center_lat, center_lon) = self.center();
        let dlat = center_lat - lat;
        let dlon = center_lon - lon;
        (dlat * dlat + dlon * dlon).sqrt()
    }
}

impl std::fmt::Display for DsfTileCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_name())
    }
}

/// Calculator for DSF tile bands ahead of aircraft.
///
/// Generates the list of DSF tiles that should be prefetched based on
/// aircraft position and track direction.
#[derive(Debug, Clone)]
pub struct BandCalculator {
    /// Lead distance in degrees (how far ahead to prefetch).
    lead_distance_deg: f64,

    /// Band width in DSF tiles (tiles perpendicular to travel on each side).
    band_width_dsf: u8,
}

impl BandCalculator {
    /// Create a new band calculator with the given configuration.
    pub fn new(config: &AdaptivePrefetchConfig) -> Self {
        Self {
            lead_distance_deg: config.lead_distance as f64,
            band_width_dsf: config.band_width,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(&AdaptivePrefetchConfig::default())
    }

    /// Create with explicit parameters.
    pub fn with_params(lead_distance_deg: f64, band_width_dsf: u8) -> Self {
        Self {
            lead_distance_deg,
            band_width_dsf,
        }
    }

    /// Calculate DSF tiles in prefetch bands for the given position and track.
    ///
    /// # Arguments
    ///
    /// * `position` - Aircraft position (lat, lon) in degrees
    /// * `track` - Ground track in degrees (0-360, true north)
    ///
    /// # Returns
    ///
    /// Vector of DSF tile coordinates, ordered by distance from aircraft.
    pub fn calculate_bands(&self, position: (f64, f64), track: f64) -> Vec<DsfTileCoord> {
        let (lat, lon) = position;
        let quadrant = TrackQuadrant::from_track(track);

        let mut tiles = Vec::new();

        if quadrant.is_diagonal() {
            // Diagonal: load BOTH lat and lon bands
            self.add_latitude_band(&mut tiles, lat, lon, quadrant.is_northbound());
            self.add_longitude_band(&mut tiles, lat, lon, quadrant.is_eastbound());
        } else {
            // Cardinal: load single band based on direction
            match quadrant {
                TrackQuadrant::North | TrackQuadrant::South => {
                    self.add_latitude_band(&mut tiles, lat, lon, quadrant.is_northbound());
                }
                TrackQuadrant::East | TrackQuadrant::West => {
                    self.add_longitude_band(&mut tiles, lat, lon, quadrant.is_eastbound());
                }
                _ => unreachable!("Cardinal check failed"),
            }
        }

        // Remove duplicates (can occur at corners)
        tiles.sort_by_key(|t| (t.lat, t.lon));
        tiles.dedup();

        // Sort by distance from aircraft
        tiles.sort_by(|a, b| {
            let dist_a = a.distance_from(lat, lon);
            let dist_b = b.distance_from(lat, lon);
            dist_a
                .partial_cmp(&dist_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        tiles
    }

    /// Add latitude band (for north/south movement).
    fn add_latitude_band(
        &self,
        tiles: &mut Vec<DsfTileCoord>,
        lat: f64,
        lon: f64,
        is_northbound: bool,
    ) {
        let current_dsf_lat = lat.floor() as i16;
        let current_dsf_lon = lon.floor() as i16;

        // Calculate band latitude range
        let (lat_start, lat_end) = if is_northbound {
            // North: start from next DSF tile, extend lead_distance ahead
            let start = current_dsf_lat + 1;
            let end = current_dsf_lat + 1 + self.lead_distance_deg as i16;
            (start, end)
        } else {
            // South: start from previous DSF tile, extend lead_distance ahead
            let start = current_dsf_lat - self.lead_distance_deg as i16;
            let end = current_dsf_lat;
            (start, end)
        };

        // Calculate longitude width (band_width on each side)
        let lon_start = current_dsf_lon - self.band_width_dsf as i16;
        let lon_end = current_dsf_lon + self.band_width_dsf as i16;

        // Add all tiles in the band
        for tile_lat in lat_start..=lat_end {
            for tile_lon in lon_start..=lon_end {
                tiles.push(DsfTileCoord::new(tile_lat, tile_lon));
            }
        }
    }

    /// Add longitude band (for east/west movement).
    fn add_longitude_band(
        &self,
        tiles: &mut Vec<DsfTileCoord>,
        lat: f64,
        lon: f64,
        is_eastbound: bool,
    ) {
        let current_dsf_lat = lat.floor() as i16;
        let current_dsf_lon = lon.floor() as i16;

        // Calculate band longitude range
        let (lon_start, lon_end) = if is_eastbound {
            // East: start from next DSF tile, extend lead_distance ahead
            let start = current_dsf_lon + 1;
            let end = current_dsf_lon + 1 + self.lead_distance_deg as i16;
            (start, end)
        } else {
            // West: start from previous DSF tile, extend lead_distance ahead
            let start = current_dsf_lon - self.lead_distance_deg as i16;
            let end = current_dsf_lon;
            (start, end)
        };

        // Calculate latitude width (band_width on each side)
        let lat_start = current_dsf_lat - self.band_width_dsf as i16;
        let lat_end = current_dsf_lat + self.band_width_dsf as i16;

        // Add all tiles in the band
        for tile_lat in lat_start..=lat_end {
            for tile_lon in lon_start..=lon_end {
                tiles.push(DsfTileCoord::new(tile_lat, tile_lon));
            }
        }
    }

    /// Get lead distance in degrees.
    pub fn lead_distance(&self) -> f64 {
        self.lead_distance_deg
    }

    /// Get band width in DSF tiles.
    pub fn band_width(&self) -> u8 {
        self.band_width_dsf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ─────────────────────────────────────────────────────────────────────────
    // DsfTileCoord tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_dsf_tile_from_position() {
        // Positive coordinates
        let tile = DsfTileCoord::from_position(53.5, 9.8);
        assert_eq!(tile.lat, 53);
        assert_eq!(tile.lon, 9);

        // Negative coordinates
        let tile = DsfTileCoord::from_position(-33.9, -118.4);
        assert_eq!(tile.lat, -34);
        assert_eq!(tile.lon, -119);

        // Edge cases
        let tile = DsfTileCoord::from_position(53.0, 9.0);
        assert_eq!(tile.lat, 53);
        assert_eq!(tile.lon, 9);
    }

    #[test]
    fn test_dsf_tile_name() {
        assert_eq!(DsfTileCoord::new(53, 9).to_name(), "+53+009");
        assert_eq!(DsfTileCoord::new(-34, -119).to_name(), "-34-119");
        assert_eq!(DsfTileCoord::new(0, 0).to_name(), "+00+000");
        assert_eq!(DsfTileCoord::new(-1, 180).to_name(), "-01+180");
    }

    #[test]
    fn test_dsf_tile_center() {
        let tile = DsfTileCoord::new(53, 9);
        let (lat, lon) = tile.center();
        assert!((lat - 53.5).abs() < 0.001);
        assert!((lon - 9.5).abs() < 0.001);
    }

    #[test]
    fn test_dsf_tile_distance() {
        let tile = DsfTileCoord::new(53, 9);

        // Distance from tile center should be ~0
        let dist = tile.distance_from(53.5, 9.5);
        assert!(dist < 0.01);

        // Distance from corner
        let dist = tile.distance_from(53.0, 9.0);
        assert!((dist - 0.707).abs() < 0.01); // sqrt(0.5^2 + 0.5^2)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // BandCalculator tests - Cardinal directions
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_band_calculator_northbound() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((53.5, 9.5), 0.0); // Northbound

        // Should have tiles at lat 54, 55, 56 (current+1 through current+1+lead)
        // With band_width=1: lon 8, 9, 10
        // Total: 3 lat × 3 lon = 9 tiles
        assert!(!tiles.is_empty());

        // All tiles should be north of current position
        for tile in &tiles {
            assert!(tile.lat >= 54, "Tile {:?} should be north of 53", tile);
        }

        // Check band width
        let lons: std::collections::HashSet<_> = tiles.iter().map(|t| t.lon).collect();
        assert!(lons.contains(&8));
        assert!(lons.contains(&9));
        assert!(lons.contains(&10));
    }

    #[test]
    fn test_band_calculator_southbound() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((53.5, 9.5), 180.0); // Southbound

        // All tiles should be south of or at current DSF row
        for tile in &tiles {
            assert!(tile.lat <= 53, "Tile {:?} should be south of 54", tile);
        }
    }

    #[test]
    fn test_band_calculator_eastbound() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((53.5, 9.5), 90.0); // Eastbound

        // All tiles should be east of current position
        for tile in &tiles {
            assert!(tile.lon >= 10, "Tile {:?} should be east of 9", tile);
        }

        // Check band width in latitude
        let lats: std::collections::HashSet<_> = tiles.iter().map(|t| t.lat).collect();
        assert!(lats.contains(&52));
        assert!(lats.contains(&53));
        assert!(lats.contains(&54));
    }

    #[test]
    fn test_band_calculator_westbound() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((53.5, 9.5), 270.0); // Westbound

        // All tiles should be west of or at current DSF column
        for tile in &tiles {
            assert!(tile.lon <= 9, "Tile {:?} should be west of 10", tile);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // BandCalculator tests - Diagonal directions
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_band_calculator_northeast_has_both_bands() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((53.5, 9.5), 45.0); // Northeast

        // Should have BOTH lat band (north) AND lon band (east)
        let has_north = tiles.iter().any(|t| t.lat > 53);
        let has_east = tiles.iter().any(|t| t.lon > 9);

        assert!(has_north, "Northeast should have northern tiles");
        assert!(has_east, "Northeast should have eastern tiles");

        // Should have more tiles than cardinal direction
        let cardinal_tiles = calc.calculate_bands((53.5, 9.5), 0.0);
        assert!(
            tiles.len() > cardinal_tiles.len(),
            "Diagonal should have more tiles than cardinal"
        );
    }

    #[test]
    fn test_band_calculator_southwest_has_both_bands() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((53.5, 9.5), 225.0); // Southwest

        // Should have BOTH lat band (south) AND lon band (west)
        let has_south = tiles.iter().any(|t| t.lat < 53);
        let has_west = tiles.iter().any(|t| t.lon < 9);

        assert!(has_south, "Southwest should have southern tiles");
        assert!(has_west, "Southwest should have western tiles");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // BandCalculator tests - Edge cases
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_band_calculator_no_duplicates() {
        let calc = BandCalculator::with_params(2.0, 2);
        let tiles = calc.calculate_bands((53.5, 9.5), 45.0); // Diagonal

        let mut coords: Vec<_> = tiles.iter().map(|t| (t.lat, t.lon)).collect();
        let original_len = coords.len();
        coords.sort();
        coords.dedup();
        assert_eq!(coords.len(), original_len, "Should have no duplicates");
    }

    #[test]
    fn test_band_calculator_sorted_by_distance() {
        let calc = BandCalculator::with_params(3.0, 2);
        let tiles = calc.calculate_bands((53.5, 9.5), 0.0);

        let mut prev_dist = 0.0;
        for tile in &tiles {
            let dist = tile.distance_from(53.5, 9.5);
            assert!(
                dist >= prev_dist - 0.001, // Allow small float errors
                "Tiles should be sorted by distance"
            );
            prev_dist = dist;
        }
    }

    #[test]
    fn test_band_calculator_negative_coordinates() {
        let calc = BandCalculator::with_params(2.0, 1);
        let tiles = calc.calculate_bands((-33.5, -118.5), 0.0); // Northbound in southern hemisphere

        // Should work correctly with negative coordinates
        assert!(!tiles.is_empty());

        // All tiles should be north (less negative) than current
        for tile in &tiles {
            assert!(tile.lat >= -33, "Tile {:?} should be north of -34", tile);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Property-based tests
    // ─────────────────────────────────────────────────────────────────────────

    proptest! {
        /// Band calculation always returns tiles
        #[test]
        fn prop_bands_never_empty(
            lat in -85.0f64..85.0f64,
            lon in -180.0f64..180.0f64,
            track in 0.0f64..360.0f64
        ) {
            let calc = BandCalculator::with_params(2.0, 1);
            let tiles = calc.calculate_bands((lat, lon), track);
            prop_assert!(!tiles.is_empty(), "Bands should never be empty");
        }

        /// Band calculation returns unique tiles
        #[test]
        fn prop_bands_unique(
            lat in -85.0f64..85.0f64,
            lon in -180.0f64..180.0f64,
            track in 0.0f64..360.0f64
        ) {
            let calc = BandCalculator::with_params(2.0, 2);
            let tiles = calc.calculate_bands((lat, lon), track);

            let mut coords: Vec<_> = tiles.iter().map(|t| (t.lat, t.lon)).collect();
            let original_len = coords.len();
            coords.sort();
            coords.dedup();

            prop_assert_eq!(coords.len(), original_len, "All tiles should be unique");
        }

        /// Diagonal tracks produce more tiles than cardinal
        #[test]
        fn prop_diagonal_more_tiles(
            lat in -85.0f64..85.0f64,
            lon in -180.0f64..180.0f64,
        ) {
            let calc = BandCalculator::with_params(2.0, 1);

            // Cardinal (north)
            let cardinal = calc.calculate_bands((lat, lon), 0.0);

            // Diagonal (northeast)
            let diagonal = calc.calculate_bands((lat, lon), 45.0);

            prop_assert!(
                diagonal.len() >= cardinal.len(),
                "Diagonal should have >= tiles: {} vs {}",
                diagonal.len(),
                cardinal.len()
            );
        }
    }
}
