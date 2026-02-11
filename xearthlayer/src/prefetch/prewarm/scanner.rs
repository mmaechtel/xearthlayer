//! Terrain file scanning for tile discovery.
//!
//! This module provides functionality to scan X-Plane terrain (.ter) files
//! and discover DDS texture tiles within geographic bounds.
//!
//! # Architecture
//!
//! The scanner uses rayon for parallel file I/O:
//! 1. Collect all terrain directories from ortho sources
//! 2. Scan directories in parallel using `par_iter`
//! 3. Read LOAD_CENTER from each .ter file for lat/lon filtering
//! 4. Parse tile coordinates from filenames
//!
//! # TER File Format
//!
//! X-Plane terrain files contain a LOAD_CENTER directive:
//! ```text
//! A
//! 800
//! TERRAIN
//!
//! LOAD_CENTER 43.48481 1.09863 7098 4096
//! ```
//!
//! The LOAD_CENTER provides the geographic center of the terrain mesh,
//! used for efficient bounds filtering without parsing the full file.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use tracing::{debug, warn};

use crate::coord::TileCoord;
use crate::ortho_union::OrthoUnionIndex;

use super::grid::DsfGridBounds;

/// Trait for scanning terrain files to discover DDS tiles.
///
/// This abstraction enables testing with mock scanners and potentially
/// supporting different scanning strategies (file-based, index-based).
pub trait TerrainScanner: Send + Sync {
    /// Scan for tiles within the given geographic bounds.
    ///
    /// Returns a deduplicated list of tile coordinates found within the bounds.
    fn scan(&self, bounds: &DsfGridBounds) -> Vec<TileCoord>;
}

/// File-based terrain scanner using the OrthoUnionIndex.
///
/// Scans actual terrain directories from all ortho sources, reading
/// .ter files to discover tiles within the requested bounds.
pub struct FileTerrainScanner {
    ortho_index: Arc<OrthoUnionIndex>,
}

impl FileTerrainScanner {
    /// Create a new file-based terrain scanner.
    pub fn new(ortho_index: Arc<OrthoUnionIndex>) -> Self {
        Self { ortho_index }
    }
}

impl TerrainScanner for FileTerrainScanner {
    fn scan(&self, bounds: &DsfGridBounds) -> Vec<TileCoord> {
        let sources = self.ortho_index.sources();

        if sources.is_empty() {
            warn!("No ortho sources available for terrain scanning");
            return Vec::new();
        }

        debug!(
            source_count = sources.len(),
            "Scanning terrain files in sources"
        );

        // Collect all terrain directories
        let terrain_dirs: Vec<_> = sources
            .iter()
            .map(|s| s.source_path.join("terrain"))
            .filter(|p| p.exists())
            .collect();

        if terrain_dirs.is_empty() {
            warn!("No terrain directories found in any source");
            return Vec::new();
        }

        debug!(
            terrain_dirs = terrain_dirs.len(),
            "Found terrain directories"
        );

        // Scan all terrain directories in parallel
        let tiles: Vec<TileCoord> = terrain_dirs
            .par_iter()
            .flat_map(|terrain_dir| scan_terrain_directory(terrain_dir, bounds))
            .collect();

        // Deduplicate tiles (same tile might exist in multiple sources)
        let mut seen = HashSet::new();
        let unique_tiles: Vec<TileCoord> = tiles
            .into_iter()
            .filter(|tile| {
                let key = (tile.row, tile.col, tile.zoom);
                seen.insert(key)
            })
            .collect();

        debug!(
            total_scanned = seen.len(),
            unique = unique_tiles.len(),
            "Terrain scan complete"
        );

        unique_tiles
    }
}

/// Scan a terrain directory for tiles within the given bounds.
///
/// Reads each `.ter` file's LOAD_CENTER to get lat/lon and filters by bounds.
/// Uses rayon for parallel file processing within the directory.
fn scan_terrain_directory(terrain_dir: &Path, bounds: &DsfGridBounds) -> Vec<TileCoord> {
    let entries = match std::fs::read_dir(terrain_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(
                path = %terrain_dir.display(),
                error = %e,
                "Failed to read terrain directory"
            );
            return Vec::new();
        }
    };

    // Collect .ter files first
    let ter_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "ter")
                .unwrap_or(false)
        })
        .collect();

    // Process files in parallel
    ter_files
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let filename = path.file_name()?.to_string_lossy();

            // Read LOAD_CENTER from the file
            let (lat, lon) = read_load_center(&path)?;

            // Check if within bounds
            if !bounds.contains(lat, lon) {
                return None;
            }

            // Parse tile coordinates from filename
            parse_ter_filename(&filename)
        })
        .collect()
}

/// Read the LOAD_CENTER lat/lon from a .ter file.
///
/// The .ter file format has LOAD_CENTER on an early line:
/// ```text
/// A
/// 800
/// TERRAIN
///
/// LOAD_CENTER 43.48481 1.09863 7098 4096
/// ```
///
/// Only reads the first 10 lines for efficiency.
fn read_load_center(path: &Path) -> Option<(f64, f64)> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    // LOAD_CENTER is typically in the first 10 lines
    for line in reader.lines().take(10) {
        let line = line.ok()?;
        if line.starts_with("LOAD_CENTER") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let lat: f64 = parts[1].parse().ok()?;
                let lon: f64 = parts[2].parse().ok()?;
                return Some((lat, lon));
            }
        }
    }

    None
}

/// Parse a terrain filename to extract tile coordinates.
///
/// Filename format: `row_col_provider+zoom.ter` or `row_col_provider+zoom_sea.ter`
/// Examples: `23952_32960_BI16.ter`, `23952_32960_BI16_sea.ter`
///
/// **IMPORTANT**: The row/col in TER filenames are CHUNK coordinates (same as DDS filenames).
/// We must convert to TILE coordinates:
/// - tile_row = chunk_row / 16
/// - tile_col = chunk_col / 16
/// - tile_zoom = chunk_zoom - 4
pub fn parse_ter_filename(filename: &str) -> Option<TileCoord> {
    // Remove .ter extension
    let name = filename.strip_suffix(".ter")?;

    // Handle _sea suffix and other suffixes like _sea_overlay
    let name = name
        .strip_suffix("_sea_overlay")
        .or_else(|| name.strip_suffix("_sea"))
        .unwrap_or(name);

    // Split by underscore: row_col_type+zoom
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() != 3 {
        return None;
    }

    // Parse CHUNK row and column (these are 16× tile coordinates)
    let chunk_row: u32 = parts[0].parse().ok()?;
    let chunk_col: u32 = parts[1].parse().ok()?;

    // Parse CHUNK zoom from provider+zoom (e.g., "BI16" -> 16, "GO218" -> 18)
    let provider_zoom = parts[2];
    if provider_zoom.len() < 2 {
        return None;
    }

    // Zoom is last 2 digits (this is CHUNK zoom, not tile zoom)
    let zoom_str = &provider_zoom[provider_zoom.len() - 2..];
    let chunk_zoom: u8 = zoom_str.parse().ok()?;

    // Convert CHUNK coordinates to TILE coordinates (inverse of TileCoord::chunk_origin)
    let tile_row = chunk_row / crate::coord::CHUNKS_PER_TILE_SIDE;
    let tile_col = chunk_col / crate::coord::CHUNKS_PER_TILE_SIDE;
    let tile_zoom = chunk_zoom.saturating_sub(crate::coord::CHUNK_ZOOM_OFFSET);

    Some(TileCoord {
        row: tile_row,
        col: tile_col,
        zoom: tile_zoom,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ter_filename() {
        // Standard format: chunk coords (23952, 32960) at chunk zoom 16
        // → tile coords (1497, 2060) at tile zoom 12
        let tile = parse_ter_filename("23952_32960_BI16.ter").unwrap();
        assert_eq!(tile.row, 23952 / 16); // 1497
        assert_eq!(tile.col, 32960 / 16); // 2060
        assert_eq!(tile.zoom, 16 - 4); // 12

        // With _sea suffix
        let tile = parse_ter_filename("23952_32960_BI16_sea.ter").unwrap();
        assert_eq!(tile.row, 1497);
        assert_eq!(tile.col, 2060);
        assert_eq!(tile.zoom, 12);

        // GO2 provider at chunk zoom 16
        let tile = parse_ter_filename("10000_19808_GO216.ter").unwrap();
        assert_eq!(tile.row, 10000 / 16); // 625
        assert_eq!(tile.col, 19808 / 16); // 1238
        assert_eq!(tile.zoom, 12);

        // With _sea_overlay suffix
        let tile = parse_ter_filename("10000_19808_GO216_sea_overlay.ter").unwrap();
        assert_eq!(tile.row, 625);
        assert_eq!(tile.col, 1238);
        assert_eq!(tile.zoom, 12);

        // Higher zoom: chunk zoom 18 → tile zoom 14
        let tile = parse_ter_filename("93248_139168_BI18.ter").unwrap();
        assert_eq!(tile.row, 93248 / 16); // 5828
        assert_eq!(tile.col, 139168 / 16); // 8698
        assert_eq!(tile.zoom, 18 - 4); // 14

        // Invalid format
        assert!(parse_ter_filename("invalid.ter").is_none());
        assert!(parse_ter_filename("23952_32960.ter").is_none()); // Missing zoom part
    }

    #[test]
    fn test_parse_ter_filename_edge_cases() {
        // Very short provider code
        assert!(parse_ter_filename("100_200_X1.ter").is_none()); // zoom "1" is only 1 digit

        // Minimum valid
        let tile = parse_ter_filename("16_32_AB10.ter").unwrap();
        assert_eq!(tile.row, 1);
        assert_eq!(tile.col, 2);
        assert_eq!(tile.zoom, 6); // 10 - 4

        // Zero coordinates
        let tile = parse_ter_filename("0_0_BI16.ter").unwrap();
        assert_eq!(tile.row, 0);
        assert_eq!(tile.col, 0);
        assert_eq!(tile.zoom, 12);
    }
}
