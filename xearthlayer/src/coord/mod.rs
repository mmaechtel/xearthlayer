//! Coordinate conversion module
//!
//! Provides conversions between geographic coordinates (latitude/longitude)
//! and Web Mercator tile/chunk coordinates used by satellite imagery providers.

mod types;

pub use types::{
    ChunkCoord, CoordError, TileChunksIterator, TileCoord, MAX_LAT, MAX_ZOOM, MIN_LAT, MIN_LON,
    MIN_ZOOM,
};

use std::f64::consts::PI;

/// Converts geographic coordinates to tile coordinates.
///
/// # Arguments
///
/// * `lat` - Latitude in degrees (-85.05112878 to 85.05112878)
/// * `lon` - Longitude in degrees (-180.0 to 180.0)
/// * `zoom` - Zoom level (0 to 18)
///
/// # Returns
///
/// A `Result` containing the tile coordinates or an error if inputs are invalid.
#[inline]
pub fn to_tile_coords(lat: f64, lon: f64, zoom: u8) -> Result<TileCoord, CoordError> {
    // Validate inputs
    if !(MIN_LAT..=MAX_LAT).contains(&lat) {
        return Err(CoordError::InvalidLatitude(lat));
    }
    if !(MIN_LON..=180.0).contains(&lon) {
        return Err(CoordError::InvalidLongitude(lon));
    }
    if zoom > MAX_ZOOM {
        return Err(CoordError::InvalidZoom(zoom));
    }

    // Calculate number of tiles at this zoom level
    let n = 2.0_f64.powi(zoom as i32);

    // Convert longitude to tile X coordinate
    let col = ((lon + 180.0) / 360.0 * n) as u32;

    // Convert latitude to tile Y coordinate using Web Mercator projection
    let lat_rad = lat * PI / 180.0;
    let row = ((1.0 - lat_rad.tan().asinh() / PI) / 2.0 * n) as u32;

    Ok(TileCoord { row, col, zoom })
}

/// Converts geographic coordinates to chunk coordinates.
///
/// This directly converts lat/lon to a chunk within a tile, useful for
/// determining which specific 256×256 pixel chunk to download.
///
/// # Arguments
///
/// * `lat` - Latitude in degrees (-85.05112878 to 85.05112878)
/// * `lon` - Longitude in degrees (-180.0 to 180.0)
/// * `zoom` - Zoom level (0 to 18)
///
/// # Returns
///
/// A `Result` containing the chunk coordinates or an error if inputs are invalid.
#[inline]
pub fn to_chunk_coords(lat: f64, lon: f64, zoom: u8) -> Result<ChunkCoord, CoordError> {
    // First get the tile coordinates
    let tile = to_tile_coords(lat, lon, zoom)?;

    // Now we need to find the position within the tile
    // Each tile is divided into 16×16 chunks
    // We calculate at chunk resolution (zoom + 4 for 2^4 = 16)
    let chunk_zoom_offset = 4; // log2(16) = 4
    let n_chunks = 2.0_f64.powi((zoom + chunk_zoom_offset) as i32);

    // Calculate chunk-level coordinates
    let chunk_col_global = ((lon + 180.0) / 360.0 * n_chunks) as u32;
    let lat_rad = lat * PI / 180.0;
    let chunk_row_global = ((1.0 - lat_rad.tan().asinh() / PI) / 2.0 * n_chunks) as u32;

    // Extract the chunk position within the tile (0-15)
    let chunk_row = (chunk_row_global % 16) as u8;
    let chunk_col = (chunk_col_global % 16) as u8;

    Ok(ChunkCoord {
        tile_row: tile.row,
        tile_col: tile.col,
        chunk_row,
        chunk_col,
        zoom,
    })
}

/// Converts tile coordinates back to geographic coordinates.
///
/// Returns the latitude/longitude of the tile's northwest corner.
#[inline]
pub fn tile_to_lat_lon(tile: &TileCoord) -> (f64, f64) {
    let n = 2.0_f64.powi(tile.zoom as i32);

    // Convert tile X coordinate to longitude
    let lon = tile.col as f64 / n * 360.0 - 180.0;

    // Convert tile Y coordinate to latitude using inverse Web Mercator
    let y = tile.row as f64 / n;
    let lat_rad = (PI * (1.0 - 2.0 * y)).sinh().atan();
    let lat = lat_rad * 180.0 / PI;

    (lat, lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_york_city_at_zoom_16() {
        // New York City: 40.7128°N, 74.0060°W
        let result = to_tile_coords(40.7128, -74.0060, 16);
        assert!(result.is_ok(), "Valid coordinates should not error");

        let tile = result.unwrap();
        assert_eq!(tile.row, 24640);
        assert_eq!(tile.col, 19295);
        assert_eq!(tile.zoom, 16);
    }

    #[test]
    fn test_invalid_latitude() {
        let result = to_tile_coords(90.0, 0.0, 10);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CoordError::InvalidLatitude(_)
        ));
    }

    #[test]
    fn test_tile_to_lat_lon_northwest_corner() {
        // Tile should return its northwest corner coordinates
        let tile = TileCoord {
            row: 24640,
            col: 19295,
            zoom: 16,
        };

        let (lat, lon) = tile_to_lat_lon(&tile);

        // Should be close to NYC but not exact (northwest corner of tile)
        assert!(
            (lat - 40.713).abs() < 0.01,
            "Latitude should be close to 40.713"
        );
        assert!(
            (lon - (-74.007)).abs() < 0.01,
            "Longitude should be close to -74.007"
        );
    }

    #[test]
    fn test_tile_to_lat_lon_at_equator() {
        // Tile at equator, prime meridian
        let tile = TileCoord {
            row: 512,
            col: 512,
            zoom: 10,
        };

        let (lat, lon) = tile_to_lat_lon(&tile);

        // At zoom 10, tile 512,512 should be near 0,0
        assert!(lat.abs() < 1.0, "Should be near equator");
        assert!(lon.abs() < 1.0, "Should be near prime meridian");
    }

    #[test]
    fn test_roundtrip_conversion() {
        // Convert lat/lon → tile → lat/lon should give similar coordinates
        let original_lat = 40.7128;
        let original_lon = -74.0060;
        let zoom = 16;

        // Forward conversion
        let tile = to_tile_coords(original_lat, original_lon, zoom).unwrap();

        // Reverse conversion
        let (converted_lat, converted_lon) = tile_to_lat_lon(&tile);

        // Should be close (within tile precision)
        // At zoom 16, each tile is ~1.2km, so tolerance should be small
        assert!(
            (converted_lat - original_lat).abs() < 0.01,
            "Latitude should roundtrip within 0.01 degrees"
        );
        assert!(
            (converted_lon - original_lon).abs() < 0.01,
            "Longitude should roundtrip within 0.01 degrees"
        );
    }

    #[test]
    fn test_roundtrip_at_different_zooms() {
        let lat = 51.5074; // London
        let lon = -0.1278;

        for zoom in [0, 5, 10, 15, 18] {
            let tile = to_tile_coords(lat, lon, zoom).unwrap();
            let (converted_lat, converted_lon) = tile_to_lat_lon(&tile);

            // Tolerance is the size of one tile at this zoom level
            // Since tile_to_lat_lon returns northwest corner, we need full tile tolerance
            let tile_size_degrees = 360.0 / (2.0_f64.powi(zoom as i32));

            assert!(
                (converted_lat - lat).abs() < tile_size_degrees,
                "Zoom {}: lat diff {} exceeds tile size {}",
                zoom,
                (converted_lat - lat).abs(),
                tile_size_degrees
            );
            assert!(
                (converted_lon - lon).abs() < tile_size_degrees,
                "Zoom {}: lon diff {} exceeds tile size {}",
                zoom,
                (converted_lon - lon).abs(),
                tile_size_degrees
            );
        }
    }

    // Chunk coordinate tests
    #[test]
    fn test_to_chunk_coords_basic() {
        // NYC should map to a specific chunk
        let chunk = to_chunk_coords(40.7128, -74.0060, 16).unwrap();

        // Verify it's the correct tile
        assert_eq!(chunk.tile_row, 24640);
        assert_eq!(chunk.tile_col, 19295);
        assert_eq!(chunk.zoom, 16);

        // Chunk coords should be 0-15
        assert!(chunk.chunk_row < 16);
        assert!(chunk.chunk_col < 16);
    }

    #[test]
    fn test_chunk_to_global_coords() {
        // A chunk in tile (100, 200) at position (5, 7) within the tile
        let chunk = ChunkCoord {
            tile_row: 100,
            tile_col: 200,
            chunk_row: 5,
            chunk_col: 7,
            zoom: 10,
        };

        let (global_row, global_col, zoom) = chunk.to_global_coords();

        // Global coords should be: tile * 16 + chunk_offset
        assert_eq!(global_row, 100 * 16 + 5); // 1605
        assert_eq!(global_col, 200 * 16 + 7); // 3207
        assert_eq!(zoom, 10);
    }

    #[test]
    fn test_chunk_at_tile_origin() {
        // Chunk at (0,0) within tile should have same global coords as tile*16
        let chunk = ChunkCoord {
            tile_row: 50,
            tile_col: 75,
            chunk_row: 0,
            chunk_col: 0,
            zoom: 12,
        };

        let (global_row, global_col, _) = chunk.to_global_coords();
        assert_eq!(global_row, 50 * 16);
        assert_eq!(global_col, 75 * 16);
    }

    #[test]
    fn test_chunk_at_tile_max() {
        // Chunk at (15,15) within tile (last chunk)
        let chunk = ChunkCoord {
            tile_row: 10,
            tile_col: 20,
            chunk_row: 15,
            chunk_col: 15,
            zoom: 8,
        };

        let (global_row, global_col, _) = chunk.to_global_coords();
        assert_eq!(global_row, 10 * 16 + 15); // 175
        assert_eq!(global_col, 20 * 16 + 15); // 335
    }

    #[test]
    fn test_tile_and_chunk_coords_consistent() {
        // Converting to tile and to chunk should give consistent results
        let lat = 51.5074;
        let lon = -0.1278;
        let zoom = 12;

        let tile = to_tile_coords(lat, lon, zoom).unwrap();
        let chunk = to_chunk_coords(lat, lon, zoom).unwrap();

        // Chunk's tile coords should match direct tile coords
        assert_eq!(chunk.tile_row, tile.row);
        assert_eq!(chunk.tile_col, tile.col);
        assert_eq!(chunk.zoom, tile.zoom);
    }

    // Tile chunks iterator tests
    #[test]
    fn test_tile_chunks_iterator_count() {
        // A tile should yield exactly 256 chunks (16×16)
        let tile = TileCoord {
            row: 100,
            col: 200,
            zoom: 12,
        };

        let chunks: Vec<_> = tile.chunks().collect();
        assert_eq!(chunks.len(), 256, "Tile should contain exactly 256 chunks");
    }

    #[test]
    fn test_tile_chunks_iterator_order() {
        // Chunks should be yielded in row-major order
        let tile = TileCoord {
            row: 50,
            col: 75,
            zoom: 10,
        };

        let mut chunks = tile.chunks();

        // First chunk should be (0, 0)
        let first = chunks.next().unwrap();
        assert_eq!(first.chunk_row, 0);
        assert_eq!(first.chunk_col, 0);

        // Second chunk should be (0, 1)
        let second = chunks.next().unwrap();
        assert_eq!(second.chunk_row, 0);
        assert_eq!(second.chunk_col, 1);

        // Skip to end of first row (chunk 15)
        for _ in 2..16 {
            chunks.next();
        }

        // 17th chunk should be (1, 0) - start of second row
        let row2_start = chunks.next().unwrap();
        assert_eq!(row2_start.chunk_row, 1);
        assert_eq!(row2_start.chunk_col, 0);
    }

    #[test]
    fn test_tile_chunks_all_belong_to_same_tile() {
        // All chunks should reference the same tile coordinates
        let tile = TileCoord {
            row: 123,
            col: 456,
            zoom: 15,
        };

        for chunk in tile.chunks() {
            assert_eq!(chunk.tile_row, tile.row);
            assert_eq!(chunk.tile_col, tile.col);
            assert_eq!(chunk.zoom, tile.zoom);
        }
    }

    #[test]
    fn test_tile_chunks_coordinates_in_range() {
        // All chunk coordinates should be 0-15
        let tile = TileCoord {
            row: 10,
            col: 20,
            zoom: 8,
        };

        for chunk in tile.chunks() {
            assert!(
                chunk.chunk_row < 16,
                "Chunk row {} should be less than 16",
                chunk.chunk_row
            );
            assert!(
                chunk.chunk_col < 16,
                "Chunk col {} should be less than 16",
                chunk.chunk_col
            );
        }
    }

    #[test]
    fn test_tile_chunks_no_duplicates() {
        // Each chunk position should appear exactly once
        let tile = TileCoord {
            row: 42,
            col: 84,
            zoom: 14,
        };

        let mut seen = std::collections::HashSet::new();
        for chunk in tile.chunks() {
            let key = (chunk.chunk_row, chunk.chunk_col);
            assert!(
                seen.insert(key),
                "Duplicate chunk at ({}, {})",
                chunk.chunk_row,
                chunk.chunk_col
            );
        }

        assert_eq!(seen.len(), 256, "Should have 256 unique chunks");
    }

    // Property-based tests using proptest
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn test_roundtrip_property(
                lat in -85.05..85.05_f64,
                lon in -180.0..180.0_f64,
                zoom in 0u8..=18
            ) {
                // Convert to tile and back
                let tile = to_tile_coords(lat, lon, zoom)?;
                let (converted_lat, converted_lon) = tile_to_lat_lon(&tile);

                // Calculate expected precision at this zoom level
                let tile_size = 360.0 / (2.0_f64.powi(zoom as i32));

                // Converted coordinates should be within one tile of original
                prop_assert!(
                    (converted_lat - lat).abs() < tile_size,
                    "Latitude roundtrip failed: {} -> {} (diff: {}, tile_size: {})",
                    lat, converted_lat, (converted_lat - lat).abs(), tile_size
                );
                prop_assert!(
                    (converted_lon - lon).abs() < tile_size,
                    "Longitude roundtrip failed: {} -> {} (diff: {}, tile_size: {})",
                    lon, converted_lon, (converted_lon - lon).abs(), tile_size
                );
            }

            #[test]
            fn test_tile_coords_in_bounds(
                lat in -85.05..85.05_f64,
                lon in -180.0..180.0_f64,
                zoom in 0u8..=18
            ) {
                let tile = to_tile_coords(lat, lon, zoom)?;

                // Tile coordinates should be within valid range
                let max_tile = 2u32.pow(zoom as u32);
                prop_assert!(
                    tile.row < max_tile,
                    "Row {} exceeds maximum {} at zoom {}",
                    tile.row, max_tile, zoom
                );
                prop_assert!(
                    tile.col < max_tile,
                    "Col {} exceeds maximum {} at zoom {}",
                    tile.col, max_tile, zoom
                );
                prop_assert_eq!(tile.zoom, zoom);
            }

            #[test]
            fn test_longitude_monotonic(
                lat in 0.0..1.0_f64,
                lon1 in -180.0..-90.0_f64,
                lon2 in -90.0..0.0_f64,
                zoom in 10u8..=15
            ) {
                // For fixed latitude, increasing longitude should increase column
                let tile1 = to_tile_coords(lat, lon1, zoom)?;
                let tile2 = to_tile_coords(lat, lon2, zoom)?;

                prop_assert!(
                    tile1.col < tile2.col,
                    "Longitude not monotonic: lon {} (col {}) >= lon {} (col {})",
                    lon1, tile1.col, lon2, tile2.col
                );
            }

            #[test]
            fn test_tile_to_lat_lon_in_bounds(
                row_raw in 0u32..65536,
                col_raw in 0u32..65536,
                zoom in 0u8..=16
            ) {
                let max_coord = 2u32.pow(zoom as u32);
                // Constrain row/col to valid range for this zoom
                let row = row_raw % max_coord;
                let col = col_raw % max_coord;

                let tile = TileCoord { row, col, zoom };
                let (lat, lon) = tile_to_lat_lon(&tile);

                // Results should be in valid geographic bounds
                prop_assert!(
                    lat >= MIN_LAT && lat <= MAX_LAT,
                    "Latitude {} out of bounds [{}, {}]",
                    lat, MIN_LAT, MAX_LAT
                );
                prop_assert!(
                    lon >= -180.0 && lon <= 180.0,
                    "Longitude {} out of bounds [-180, 180]",
                    lon
                );
            }

            #[test]
            fn test_reject_invalid_latitude(
                lat in -90.0..-85.06_f64,
                lon in -180.0..180.0_f64,
                zoom in 0u8..=18
            ) {
                // Latitudes outside Web Mercator range should error
                let result = to_tile_coords(lat, lon, zoom);
                prop_assert!(result.is_err());
                prop_assert!(matches!(result.unwrap_err(), CoordError::InvalidLatitude(_)));
            }

            #[test]
            fn test_reject_invalid_longitude(
                lat in -85.0..85.0_f64,
                lon in 180.01..360.0_f64,
                zoom in 0u8..=18
            ) {
                // Longitudes outside valid range should error
                let result = to_tile_coords(lat, lon, zoom);
                prop_assert!(result.is_err());
                prop_assert!(matches!(result.unwrap_err(), CoordError::InvalidLongitude(_)));
            }

            // Chunk coordinate property tests
            #[test]
            fn test_chunk_coords_in_valid_range(
                lat in -85.05..85.05_f64,
                lon in -180.0..180.0_f64,
                zoom in 0u8..=18
            ) {
                // Chunk coordinates should always be 0-15
                let chunk = to_chunk_coords(lat, lon, zoom)?;

                prop_assert!(
                    chunk.chunk_row < 16,
                    "Chunk row {} should be < 16",
                    chunk.chunk_row
                );
                prop_assert!(
                    chunk.chunk_col < 16,
                    "Chunk col {} should be < 16",
                    chunk.chunk_col
                );
            }

            #[test]
            fn test_chunk_tile_coords_match_tile_conversion(
                lat in -85.05..85.05_f64,
                lon in -180.0..180.0_f64,
                zoom in 0u8..=18
            ) {
                // Chunk's tile coords should match direct tile conversion
                let tile = to_tile_coords(lat, lon, zoom)?;
                let chunk = to_chunk_coords(lat, lon, zoom)?;

                prop_assert_eq!(chunk.tile_row, tile.row);
                prop_assert_eq!(chunk.tile_col, tile.col);
                prop_assert_eq!(chunk.zoom, tile.zoom);
            }

            #[test]
            fn test_chunk_global_coords_calculation(
                tile_row in 0u32..1000,
                tile_col in 0u32..1000,
                chunk_row in 0u8..16,
                chunk_col in 0u8..16,
                zoom in 0u8..=18
            ) {
                // Global coords should be tile * 16 + chunk_offset
                let chunk = ChunkCoord {
                    tile_row,
                    tile_col,
                    chunk_row,
                    chunk_col,
                    zoom,
                };

                let (global_row, global_col, global_zoom) = chunk.to_global_coords();

                prop_assert_eq!(global_row, tile_row * 16 + chunk_row as u32);
                prop_assert_eq!(global_col, tile_col * 16 + chunk_col as u32);
                prop_assert_eq!(global_zoom, zoom);
            }

            #[test]
            fn test_tile_chunks_iterator_yields_256(
                row in 0u32..1000,
                col in 0u32..1000,
                zoom in 0u8..=18
            ) {
                // Iterator should always yield exactly 256 chunks
                let tile = TileCoord { row, col, zoom };
                let count = tile.chunks().count();
                prop_assert_eq!(count, 256, "Tile should yield 256 chunks");
            }

            #[test]
            fn test_tile_chunks_iterator_all_valid(
                row in 0u32..1000,
                col in 0u32..1000,
                zoom in 0u8..=18
            ) {
                // All chunks from iterator should have valid coordinates
                let tile = TileCoord { row, col, zoom };

                for chunk in tile.chunks() {
                    prop_assert_eq!(chunk.tile_row, tile.row);
                    prop_assert_eq!(chunk.tile_col, tile.col);
                    prop_assert_eq!(chunk.zoom, tile.zoom);
                    prop_assert!(chunk.chunk_row < 16);
                    prop_assert!(chunk.chunk_col < 16);
                }
            }

            #[test]
            fn test_tile_chunks_iterator_no_duplicates(
                row in 0u32..100,
                col in 0u32..100,
                zoom in 0u8..=18
            ) {
                // Iterator should yield no duplicate chunk positions
                let tile = TileCoord { row, col, zoom };
                let mut seen = std::collections::HashSet::new();

                for chunk in tile.chunks() {
                    let key = (chunk.chunk_row, chunk.chunk_col);
                    prop_assert!(
                        seen.insert(key),
                        "Duplicate chunk at ({}, {})",
                        chunk.chunk_row,
                        chunk.chunk_col
                    );
                }

                prop_assert_eq!(seen.len(), 256);
            }
        }
    }
}
