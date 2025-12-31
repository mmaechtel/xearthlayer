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

        // Build lookup for low ZL tiles: (row, col) → tile
        let low_lookup: HashMap<(u32, u32), &TileReference> =
            low_tiles.iter().map(|t| ((t.row, t.col), *t)).collect();

        // Check each high ZL tile against low ZL lookup
        high_tiles
            .iter()
            .filter_map(|high| {
                let (parent_row, parent_col) = high.parent_at(low_zl)?;
                let low = low_lookup.get(&(parent_row, parent_col))?;

                Some(ZoomOverlap {
                    higher_zl: (*high).clone(),
                    lower_zl: (*low).clone(),
                    zl_diff: high_zl - low_zl,
                    coverage: self.determine_coverage(high, low, high_zl, low_zl),
                })
            })
            .collect()
    }

    /// Determine if a higher ZL tile completely covers a lower ZL tile.
    ///
    /// For complete coverage, all child tiles at the higher ZL must exist
    /// within the lower ZL tile's area. Currently returns Complete for
    /// direct parent-child relationships.
    fn determine_coverage(
        &self,
        _high: &TileReference,
        _low: &TileReference,
        _high_zl: u8,
        _low_zl: u8,
    ) -> OverlapCoverage {
        // For now, assume complete coverage if the parent relationship exists.
        // A more sophisticated check would verify that ALL child tiles exist.
        OverlapCoverage::Complete
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
