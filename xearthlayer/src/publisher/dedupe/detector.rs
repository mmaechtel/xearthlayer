//! Overlap detection for zoom level tiles.
//!
//! Scans package terrain directories and detects overlapping tiles
//! across different zoom levels.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

use tracing::{debug, trace};

use super::types::{DedupeError, DedupeFilter, OverlapCoverage, TileReference, ZoomOverlap};

/// Detects overlapping zoom level tiles in scenery packages.
#[derive(Debug, Default)]
pub struct OverlapDetector {
    /// Optional filter for targeted operations.
    filter: DedupeFilter,
}

impl OverlapDetector {
    /// Create a new overlap detector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a detector with a filter for targeted operations.
    pub fn with_filter(filter: DedupeFilter) -> Self {
        Self { filter }
    }

    /// Scan a package directory for tile references.
    ///
    /// Parses all `.ter` files in the package's `terrain/` subdirectory.
    pub fn scan_package(&self, package_path: &Path) -> Result<Vec<TileReference>, DedupeError> {
        let terrain_path = package_path.join("terrain");
        if !terrain_path.exists() {
            return Err(DedupeError::TerrainDirNotFound(terrain_path));
        }

        debug!(
            terrain_path = %terrain_path.display(),
            "Scanning terrain directory for tiles"
        );

        let mut tiles = Vec::new();
        let entries =
            fs::read_dir(&terrain_path).map_err(|e| DedupeError::IoError(e.to_string()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "ter") {
                match self.parse_ter_file(&path) {
                    Ok(tile) => {
                        if self.filter.matches(&tile) {
                            tiles.push(tile);
                        }
                    }
                    Err(e) => {
                        trace!(path = %path.display(), error = %e, "Failed to parse .ter file");
                    }
                }
            }
        }

        debug!(tiles = tiles.len(), "Scanned tiles from package");
        Ok(tiles)
    }

    /// Detect all overlaps in a collection of tiles.
    ///
    /// Returns a list of `ZoomOverlap` structs describing each overlap.
    pub fn detect_overlaps(&self, tiles: &[TileReference]) -> Vec<ZoomOverlap> {
        let mut overlaps = Vec::new();

        // Group tiles by zoom level
        let by_zoom: HashMap<u8, Vec<&TileReference>> =
            tiles.iter().fold(HashMap::new(), |mut acc, tile| {
                acc.entry(tile.zoom).or_default().push(tile);
                acc
            });

        // Get sorted zoom levels (highest first)
        let mut zoom_levels: Vec<u8> = by_zoom.keys().copied().collect();
        zoom_levels.sort_by(|a, b| b.cmp(a));

        debug!(
            zoom_levels = ?zoom_levels,
            "Detecting overlaps across zoom levels"
        );

        // For each higher ZL, check against all lower ZLs
        for (i, &high_zl) in zoom_levels.iter().enumerate() {
            for &low_zl in &zoom_levels[i + 1..] {
                // Skip if zoom difference is odd (tiles don't align)
                if (high_zl - low_zl) % 2 != 0 {
                    continue;
                }

                let detected = self.detect_between_levels(&by_zoom, high_zl, low_zl);
                debug!(
                    high_zl = high_zl,
                    low_zl = low_zl,
                    overlaps = detected.len(),
                    "Detected overlaps between zoom levels"
                );
                overlaps.extend(detected);
            }
        }

        overlaps
    }

    /// Detect overlaps between two specific zoom levels.
    ///
    /// This uses a "parent-centric" approach: for each low ZL tile, we check
    /// if ALL of its children at the high ZL exist. This ensures we only
    /// mark overlaps as Complete when removing the low ZL tile won't create gaps.
    fn detect_between_levels(
        &self,
        by_zoom: &HashMap<u8, Vec<&TileReference>>,
        high_zl: u8,
        low_zl: u8,
    ) -> Vec<ZoomOverlap> {
        let Some(high_tiles) = by_zoom.get(&high_zl) else {
            return Vec::new();
        };
        let Some(low_tiles) = by_zoom.get(&low_zl) else {
            return Vec::new();
        };

        // Build lookup for high ZL tiles: (row, col) → &tile
        let high_lookup: HashMap<(u32, u32), &TileReference> =
            high_tiles.iter().map(|t| ((t.row, t.col), *t)).collect();

        // Calculate how many children each low ZL tile should have
        let zl_diff = high_zl - low_zl;
        let scale = 4u32.pow((zl_diff / 2) as u32); // 4 for 2 levels, 16 for 4 levels
        let expected_children = scale * scale; // 16 for 2 levels, 256 for 4 levels

        let mut overlaps = Vec::new();

        // For each low ZL tile, check how many high ZL children exist
        for low in low_tiles {
            // Calculate the range of child coordinates
            let child_row_start = low.row * scale;
            let child_col_start = low.col * scale;

            // Collect all existing children
            let mut existing_children: Vec<TileReference> = Vec::new();

            for row_offset in 0..scale {
                for col_offset in 0..scale {
                    let child_row = child_row_start + row_offset;
                    let child_col = child_col_start + col_offset;
                    if let Some(child) = high_lookup.get(&(child_row, child_col)) {
                        existing_children.push((*child).clone());
                    }
                }
            }

            // Only create an overlap if at least one child exists
            if !existing_children.is_empty() {
                let coverage = if existing_children.len() as u32 == expected_children {
                    OverlapCoverage::Complete
                } else {
                    OverlapCoverage::Partial
                };

                trace!(
                    low_row = low.row,
                    low_col = low.col,
                    low_zl = low_zl,
                    high_zl = high_zl,
                    existing = existing_children.len(),
                    expected = expected_children,
                    coverage = ?coverage,
                    "Detected overlap"
                );

                overlaps.push(ZoomOverlap {
                    higher_zl: existing_children[0].clone(), // First child as representative
                    lower_zl: (*low).clone(),
                    all_higher_zl: existing_children,
                    zl_diff,
                    coverage,
                });
            }
        }

        overlaps
    }

    /// Parse a .ter file to extract tile information.
    ///
    /// Expected format:
    /// ```text
    /// A
    /// 800
    /// TERRAIN
    ///
    /// LOAD_CENTER <lat> <lon> <elevation> <size>
    /// BASE_TEX_NOWRAP ../textures/<row>_<col>_<provider><zoom>.dds
    /// NO_ALPHA
    /// ```
    fn parse_ter_file(&self, path: &Path) -> Result<TileReference, DedupeError> {
        let file = File::open(path).map_err(|e| DedupeError::IoError(e.to_string()))?;
        let reader = BufReader::new(file);

        let mut lat: Option<f32> = None;
        let mut lon: Option<f32> = None;
        let mut row: Option<u32> = None;
        let mut col: Option<u32> = None;
        let mut zoom: Option<u8> = None;
        let mut provider: Option<String> = None;

        // Check filename for sea indicator
        let is_sea = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.contains("_sea"));

        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();

            if line.starts_with("LOAD_CENTER") {
                // Parse: LOAD_CENTER <lat> <lon> <elevation> <size>
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    lat = parts[1].parse().ok();
                    lon = parts[2].parse().ok();
                }
            } else if line.starts_with("BASE_TEX_NOWRAP") || line.starts_with("BASE_TEX") {
                // Parse: BASE_TEX_NOWRAP ../textures/<row>_<col>_<provider><zoom>.dds
                if let Some(dds_part) = line.split('/').next_back() {
                    if let Some((r, c, z, p)) = parse_dds_filename(dds_part) {
                        row = Some(r);
                        col = Some(c);
                        zoom = Some(z);
                        provider = Some(p);
                    }
                }
            }
        }

        // Validate we got all required fields
        let lat = lat.ok_or_else(|| DedupeError::ParseError("Missing LOAD_CENTER".to_string()))?;
        let lon = lon.ok_or_else(|| DedupeError::ParseError("Missing LOAD_CENTER".to_string()))?;
        let row = row.ok_or_else(|| DedupeError::ParseError("Missing BASE_TEX".to_string()))?;
        let col = col.ok_or_else(|| DedupeError::ParseError("Missing BASE_TEX".to_string()))?;
        let zoom = zoom.ok_or_else(|| DedupeError::ParseError("Missing zoom level".to_string()))?;
        let provider =
            provider.ok_or_else(|| DedupeError::ParseError("Missing provider".to_string()))?;

        Ok(TileReference {
            row,
            col,
            zoom,
            provider,
            lat,
            lon,
            ter_path: path.to_path_buf(),
            is_sea,
        })
    }
}

/// Parse a DDS filename to extract row, col, zoom, and provider.
///
/// Format: `<row>_<col>_<provider><zoom>.dds`
/// Examples:
/// - `94800_47888_BI18.dds` → (94800, 47888, 18, "BI")
/// - `25264_10368_GO216.dds` → (25264, 10368, 16, "GO2")
fn parse_dds_filename(filename: &str) -> Option<(u32, u32, u8, String)> {
    // Remove .dds extension
    let name = filename.strip_suffix(".dds")?;

    // Split by underscore
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() < 3 {
        return None;
    }

    // Parse row and col
    let row: u32 = parts[0].parse().ok()?;
    let col: u32 = parts[1].parse().ok()?;

    // Parse provider+zoom (e.g., "BI18", "GO216")
    let provider_zoom = parts[2];

    // Extract zoom from the last 2 characters (zoom levels are 12-20, always 2 digits)
    // Provider is everything before the last 2 characters
    if provider_zoom.len() < 3 {
        return None;
    }
    let zoom_str = &provider_zoom[provider_zoom.len() - 2..];
    let zoom: u8 = zoom_str.parse().ok()?;
    let provider = provider_zoom[..provider_zoom.len() - 2].to_uppercase();

    Some((row, col, zoom, provider))
}

/// Get zoom levels present in a collection of tiles.
#[allow(dead_code)]
pub fn get_zoom_levels(tiles: &[TileReference]) -> Vec<u8> {
    let levels: HashSet<u8> = tiles.iter().map(|t| t.zoom).collect();
    let mut sorted: Vec<u8> = levels.into_iter().collect();
    sorted.sort();
    sorted
}

/// Count tiles by zoom level.
#[allow(dead_code)]
pub fn count_by_zoom(tiles: &[TileReference]) -> HashMap<u8, usize> {
    tiles.iter().fold(HashMap::new(), |mut acc, tile| {
        *acc.entry(tile.zoom).or_insert(0) += 1;
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_ter_file(
        dir: &Path,
        name: &str,
        lat: f32,
        lon: f32,
        row: u32,
        col: u32,
        zoom: u8,
    ) {
        let content = format!(
            "A\n800\nTERRAIN\n\nLOAD_CENTER {} {} 1000 4096\nBASE_TEX_NOWRAP ../textures/{}_{}_{}{}.dds\nNO_ALPHA\n",
            lat, lon, row, col, "BI", zoom
        );
        let path = dir.join(name);
        let mut file = File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_parse_dds_filename() {
        assert_eq!(
            parse_dds_filename("94800_47888_BI18.dds"),
            Some((94800, 47888, 18, "BI".to_string()))
        );
        assert_eq!(
            parse_dds_filename("25264_10368_GO216.dds"),
            Some((25264, 10368, 16, "GO2".to_string()))
        );
        assert_eq!(
            parse_dds_filename("100000_125184_GO18.dds"),
            Some((100000, 125184, 18, "GO".to_string()))
        );
        assert_eq!(parse_dds_filename("invalid.dds"), None);
        assert_eq!(parse_dds_filename("no_extension"), None);
    }

    #[test]
    fn test_scan_package() {
        let temp = TempDir::new().unwrap();
        let terrain_dir = temp.path().join("terrain");
        fs::create_dir_all(&terrain_dir).unwrap();

        // Create test .ter files
        create_test_ter_file(
            &terrain_dir,
            "100032_42688_BI18.ter",
            39.15,
            -121.36,
            100032,
            42688,
            18,
        );
        create_test_ter_file(
            &terrain_dir,
            "25008_10672_BI16.ter",
            39.13,
            -121.33,
            25008,
            10672,
            16,
        );

        let detector = OverlapDetector::new();
        let tiles = detector.scan_package(temp.path()).unwrap();

        assert_eq!(tiles.len(), 2);
    }

    #[test]
    fn test_detect_overlaps() {
        // Create tiles that overlap
        let tiles = vec![
            TileReference {
                row: 100032,
                col: 42688,
                zoom: 18,
                provider: "BI".to_string(),
                lat: 39.15,
                lon: -121.36,
                ter_path: PathBuf::from("100032_42688_BI18.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25008, // 100032 / 4
                col: 10672, // 42688 / 4
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.13,
                lon: -121.33,
                ter_path: PathBuf::from("25008_10672_BI16.ter"),
                is_sea: false,
            },
        ];

        let detector = OverlapDetector::new();
        let overlaps = detector.detect_overlaps(&tiles);

        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].higher_zl.zoom, 18);
        assert_eq!(overlaps[0].lower_zl.zoom, 16);
        assert_eq!(overlaps[0].zl_diff, 2);
    }

    #[test]
    fn test_detect_no_overlaps() {
        // Create tiles that don't overlap
        let tiles = vec![
            TileReference {
                row: 100032,
                col: 42688,
                zoom: 18,
                provider: "BI".to_string(),
                lat: 39.15,
                lon: -121.36,
                ter_path: PathBuf::from("100032_42688_BI18.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25009, // Not a parent of 100032 (100032 / 4 = 25008)
                col: 10672,
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.2,
                lon: -121.33,
                ter_path: PathBuf::from("25009_10672_BI16.ter"),
                is_sea: false,
            },
        ];

        let detector = OverlapDetector::new();
        let overlaps = detector.detect_overlaps(&tiles);

        assert!(overlaps.is_empty());
    }

    #[test]
    fn test_detect_multi_level_overlaps() {
        // Create tiles at ZL18, ZL16, and ZL14 that all overlap
        let tiles = vec![
            TileReference {
                row: 100032,
                col: 42688,
                zoom: 18,
                provider: "BI".to_string(),
                lat: 39.15,
                lon: -121.36,
                ter_path: PathBuf::from("100032_42688_BI18.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25008, // 100032 / 4
                col: 10672, // 42688 / 4
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.13,
                lon: -121.33,
                ter_path: PathBuf::from("25008_10672_BI16.ter"),
                is_sea: false,
            },
            TileReference {
                row: 6252, // 25008 / 4 = 6252
                col: 2668, // 10672 / 4 = 2668
                zoom: 14,
                provider: "BI".to_string(),
                lat: 39.1,
                lon: -121.3,
                ter_path: PathBuf::from("6252_2668_BI14.ter"),
                is_sea: false,
            },
        ];

        let detector = OverlapDetector::new();
        let overlaps = detector.detect_overlaps(&tiles);

        // Should detect: ZL18→ZL16, ZL18→ZL14, ZL16→ZL14
        assert_eq!(overlaps.len(), 3);
    }

    #[test]
    fn test_get_zoom_levels() {
        let tiles = vec![
            TileReference {
                row: 100032,
                col: 42688,
                zoom: 18,
                provider: "BI".to_string(),
                lat: 39.15,
                lon: -121.36,
                ter_path: PathBuf::from("test.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25008,
                col: 10672,
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.13,
                lon: -121.33,
                ter_path: PathBuf::from("test.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25009,
                col: 10673,
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.14,
                lon: -121.34,
                ter_path: PathBuf::from("test.ter"),
                is_sea: false,
            },
        ];

        let levels = get_zoom_levels(&tiles);
        assert_eq!(levels, vec![16, 18]);
    }

    #[test]
    fn test_count_by_zoom() {
        let tiles = vec![
            TileReference {
                row: 100032,
                col: 42688,
                zoom: 18,
                provider: "BI".to_string(),
                lat: 39.15,
                lon: -121.36,
                ter_path: PathBuf::from("test.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25008,
                col: 10672,
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.13,
                lon: -121.33,
                ter_path: PathBuf::from("test.ter"),
                is_sea: false,
            },
            TileReference {
                row: 25009,
                col: 10673,
                zoom: 16,
                provider: "BI".to_string(),
                lat: 39.14,
                lon: -121.34,
                ter_path: PathBuf::from("test.ter"),
                is_sea: false,
            },
        ];

        let counts = count_by_zoom(&tiles);
        assert_eq!(counts.get(&16), Some(&2));
        assert_eq!(counts.get(&18), Some(&1));
    }
}
