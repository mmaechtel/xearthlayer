//! Audit report generation for dedupe operations.
//!
//! Provides comprehensive reporting of deduplication results in multiple formats.

use std::collections::HashMap;

use super::types::TileReference;

/// A comprehensive audit report of deduplication results.
#[derive(Debug, Clone)]
pub struct DedupeAuditReport {
    /// Total number of tiles analyzed
    pub tiles_analyzed: usize,
    /// Zoom levels present in the analyzed tiles
    pub zoom_levels_present: Vec<u8>,
    /// Overlaps found, keyed by (higher_zl, lower_zl)
    pub overlaps_by_pair: HashMap<(u8, u8), usize>,
    /// Tiles that were removed
    pub tiles_removed: Vec<TileReference>,
    /// Tiles that were preserved
    pub tiles_preserved: Vec<TileReference>,
    /// Whether this was a dry run
    pub dry_run: bool,
    /// Priority mode used for resolution
    pub priority_mode: String,
    /// Optional tile filter applied
    pub tile_filter: Option<(i32, i32)>,
}

impl DedupeAuditReport {
    /// Generate a JSON-formatted report string.
    pub fn to_json(&self) -> String {
        let mut json_parts = Vec::new();

        // Basic statistics
        json_parts.push(format!("  \"tiles_analyzed\": {}", self.tiles_analyzed));

        let zl_arr: Vec<String> = self
            .zoom_levels_present
            .iter()
            .map(|z| z.to_string())
            .collect();
        json_parts.push(format!(
            "  \"zoom_levels_present\": [{}]",
            zl_arr.join(", ")
        ));

        // Overlaps by pair
        let mut overlap_parts = Vec::new();
        let mut sorted_pairs: Vec<_> = self.overlaps_by_pair.iter().collect();
        sorted_pairs.sort_by_key(|((h, l), _)| (*h, *l));
        for ((h, l), count) in sorted_pairs {
            overlap_parts.push(format!("    \"{}-{}\": {}", h, l, count));
        }
        json_parts.push(format!(
            "  \"overlaps_by_pair\": {{\n{}\n  }}",
            overlap_parts.join(",\n")
        ));

        // Counts
        json_parts.push(format!(
            "  \"tiles_removed_count\": {}",
            self.tiles_removed.len()
        ));
        json_parts.push(format!(
            "  \"tiles_preserved_count\": {}",
            self.tiles_preserved.len()
        ));

        // Metadata
        json_parts.push(format!("  \"dry_run\": {}", self.dry_run));
        json_parts.push(format!("  \"priority_mode\": \"{}\"", self.priority_mode));

        if let Some((lat, lon)) = self.tile_filter {
            json_parts.push(format!("  \"tile_filter\": \"{},{}\"", lat, lon));
        } else {
            json_parts.push("  \"tile_filter\": null".to_string());
        }

        // Removed tiles by zoom level
        let mut removed_by_zoom: HashMap<u8, Vec<&TileReference>> = HashMap::new();
        for tile in &self.tiles_removed {
            removed_by_zoom.entry(tile.zoom).or_default().push(tile);
        }

        let mut removed_zoom_parts = Vec::new();
        let mut sorted_zooms: Vec<_> = removed_by_zoom.keys().collect();
        sorted_zooms.sort();
        for zoom in sorted_zooms {
            let tiles = &removed_by_zoom[zoom];
            let paths: Vec<String> = tiles
                .iter()
                .map(|t| format!("        \"{}\"", t.ter_path.display()))
                .collect();
            removed_zoom_parts.push(format!(
                "    \"ZL{}\": [\n{}\n    ]",
                zoom,
                paths.join(",\n")
            ));
        }
        json_parts.push(format!(
            "  \"removed_by_zoom\": {{\n{}\n  }}",
            removed_zoom_parts.join(",\n")
        ));

        format!("{{\n{}\n}}", json_parts.join(",\n"))
    }

    /// Generate a text-formatted report string.
    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();

        // Header
        lines.push("Deduplication Audit Report".to_string());
        lines.push("==========================".to_string());
        lines.push(String::new());

        // Configuration
        lines.push("Configuration".to_string());
        lines.push("-------------".to_string());
        lines.push(format!("Priority mode: {}", self.priority_mode));
        lines.push(format!("Dry run: {}", self.dry_run));
        if let Some((lat, lon)) = self.tile_filter {
            lines.push(format!("Tile filter: {},{}", lat, lon));
        } else {
            lines.push("Tile filter: None (all tiles)".to_string());
        }
        lines.push(String::new());

        // Summary statistics
        lines.push("Summary".to_string());
        lines.push("-------".to_string());
        lines.push(format!("Tiles analyzed: {}", self.tiles_analyzed));
        lines.push(format!("Zoom levels: {:?}", self.zoom_levels_present));
        lines.push(format!("Tiles removed: {}", self.tiles_removed.len()));
        lines.push(format!("Tiles preserved: {}", self.tiles_preserved.len()));
        lines.push(String::new());

        // Overlaps by pair
        lines.push("Overlaps Detected".to_string());
        lines.push("-----------------".to_string());
        if self.overlaps_by_pair.is_empty() {
            lines.push("  No overlapping tiles found.".to_string());
        } else {
            let mut pairs: Vec<_> = self.overlaps_by_pair.iter().collect();
            pairs.sort_by_key(|((h, l), _)| (*h, *l));
            for ((higher, lower), count) in pairs {
                lines.push(format!(
                    "  ZL{} overlaps ZL{}: {} tiles",
                    higher, lower, count
                ));
            }
        }
        lines.push(String::new());

        // Removed tiles grouped by zoom level
        if !self.tiles_removed.is_empty() {
            lines.push("Removed Tiles by Zoom Level".to_string());
            lines.push("---------------------------".to_string());

            let mut by_zoom: HashMap<u8, Vec<&TileReference>> = HashMap::new();
            for tile in &self.tiles_removed {
                by_zoom.entry(tile.zoom).or_default().push(tile);
            }

            let mut sorted_zooms: Vec<_> = by_zoom.keys().collect();
            sorted_zooms.sort();

            for zoom in sorted_zooms {
                let tiles = &by_zoom[zoom];
                lines.push(format!("\n  ZL{} ({} tiles):", zoom, tiles.len()));
                for tile in tiles.iter().take(50) {
                    lines.push(format!(
                        "    ({}, {}) - {}",
                        tile.row,
                        tile.col,
                        tile.ter_path.display()
                    ));
                }
                if tiles.len() > 50 {
                    lines.push(format!("    ... and {} more", tiles.len() - 50));
                }
            }
            lines.push(String::new());
        }

        // Space savings estimate
        if !self.tiles_removed.is_empty() {
            let estimated_savings = estimate_space_savings(&self.tiles_removed);
            lines.push("Estimated Space Savings".to_string());
            lines.push("-----------------------".to_string());
            lines.push(format!("  ~{} (estimated)", format_size(estimated_savings)));
            lines.push(String::new());
        }

        lines.join("\n")
    }
}

/// Estimate space savings from removed tiles.
///
/// This is a rough estimate based on typical tile sizes at different zoom levels.
fn estimate_space_savings(removed: &[TileReference]) -> u64 {
    // Rough estimates of average .ter file sizes by zoom level
    // These are approximations based on typical scenery data
    removed
        .iter()
        .map(|tile| match tile.zoom {
            12..=14 => 100 * 1024, // ~100KB for low zoom
            15..=16 => 300 * 1024, // ~300KB for medium zoom
            17..=18 => 800 * 1024, // ~800KB for high zoom
            19 => 1500 * 1024,     // ~1.5MB for highest zoom
            _ => 200 * 1024,       // Default estimate
        })
        .sum()
}

/// Format a size in bytes as a human-readable string.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_tile(zoom: u8, row: u32, col: u32) -> TileReference {
        TileReference {
            row,
            col,
            zoom,
            provider: "BI".to_string(),
            lat: 37.0,
            lon: -122.0,
            ter_path: PathBuf::from(format!("terrain/{}_{}_{}.ter", row, col, zoom)),
            is_sea: false,
        }
    }

    #[test]
    fn test_json_report_generation() {
        let mut overlaps = HashMap::new();
        overlaps.insert((18, 16), 5);

        let report = DedupeAuditReport {
            tiles_analyzed: 100,
            zoom_levels_present: vec![16, 18],
            overlaps_by_pair: overlaps,
            tiles_removed: vec![sample_tile(18, 100, 200)],
            tiles_preserved: vec![sample_tile(16, 25, 50)],
            dry_run: false,
            priority_mode: "highest".to_string(),
            tile_filter: None,
        };

        let json = report.to_json();

        assert!(json.contains("\"tiles_analyzed\": 100"));
        assert!(json.contains("\"zoom_levels_present\": [16, 18]"));
        assert!(json.contains("\"18-16\": 5"));
        assert!(json.contains("\"tiles_removed_count\": 1"));
        assert!(json.contains("\"dry_run\": false"));
        assert!(json.contains("\"priority_mode\": \"highest\""));
    }

    #[test]
    fn test_text_report_generation() {
        let mut overlaps = HashMap::new();
        overlaps.insert((18, 16), 5);

        let report = DedupeAuditReport {
            tiles_analyzed: 100,
            zoom_levels_present: vec![16, 18],
            overlaps_by_pair: overlaps,
            tiles_removed: vec![sample_tile(18, 100, 200)],
            tiles_preserved: vec![sample_tile(16, 25, 50)],
            dry_run: true,
            priority_mode: "zl18".to_string(),
            tile_filter: Some((37, -122)),
        };

        let text = report.to_text();

        assert!(text.contains("Deduplication Audit Report"));
        assert!(text.contains("Priority mode: zl18"));
        assert!(text.contains("Dry run: true"));
        assert!(text.contains("Tile filter: 37,-122"));
        assert!(text.contains("Tiles analyzed: 100"));
        assert!(text.contains("ZL18 overlaps ZL16: 5 tiles"));
        assert!(text.contains("Estimated Space Savings"));
    }

    #[test]
    fn test_space_savings_estimate() {
        let tiles = vec![
            sample_tile(16, 100, 100),
            sample_tile(16, 101, 100),
            sample_tile(18, 200, 200),
        ];

        let savings = estimate_space_savings(&tiles);

        // 2 * 300KB + 1 * 800KB = 1400KB
        assert!(savings > 0);
        assert_eq!(savings, 2 * 300 * 1024 + 800 * 1024);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 bytes");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }
}
