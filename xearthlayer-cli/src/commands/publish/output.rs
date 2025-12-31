//! Output formatting utilities for publish commands.
//!
//! This module provides helper functions for formatting output consistently
//! across all publish command handlers.

use super::traits::{DedupeReport, Output, OverlapSummary};
use xearthlayer::config::format_size;
use xearthlayer::publisher::{ProcessSummary, RegionSuggestion, ReleaseStatus, SceneryScanResult};

/// Print scan results to the output.
pub fn print_scan_result(out: &dyn Output, scan: &SceneryScanResult) {
    out.header("Scan Results");
    out.newline();
    out.println(&format!("Tiles:  {}", scan.tiles.len()));

    if !scan.tiles.is_empty() {
        out.newline();
        out.println("Tiles found:");
        for tile in &scan.tiles {
            out.indented(&format!(
                "{:+03}{:+04} - {} DSF, {} TER, {} masks",
                tile.latitude,
                tile.longitude,
                tile.dsf_files.len(),
                tile.ter_files.len(),
                tile.mask_files.len()
            ));
        }
    }

    if !scan.warnings.is_empty() {
        out.newline();
        out.println(&format!("Warnings ({}):", scan.warnings.len()));
        for warning in &scan.warnings {
            out.indented(&format!("- {:?}", warning));
        }
    }
}

/// Print region suggestion to the output.
pub fn print_region_suggestion(out: &dyn Output, suggestion: &RegionSuggestion) {
    out.subheader("Region Suggestion");
    if let Some(ref region) = suggestion.region {
        out.println(&format!(
            "Suggested region: {} ({})",
            region.code().to_uppercase(),
            region.name()
        ));
    } else if suggestion.regions_found.is_empty() {
        out.println("Could not determine region from tile coordinates.");
    } else {
        out.println("Tiles span multiple regions:");
        for region in &suggestion.regions_found {
            out.indented(&format!(
                "{} ({})",
                region.code().to_uppercase(),
                region.name()
            ));
        }
        out.newline();
        out.println("Consider processing tiles separately by region.");
    }
}

/// Print process summary to the output.
pub fn print_process_summary(out: &dyn Output, summary: &ProcessSummary) {
    out.subheader("Processing Summary");
    out.println(&format!("Tiles processed: {}", summary.tile_count));
    out.println(&format!("DSF files:       {}", summary.dsf_count));
    out.println(&format!("TER files:       {}", summary.ter_count));
    out.println(&format!("Mask files:      {}", summary.mask_count));
    out.println(&format!("DDS skipped:     {}", summary.dds_skipped));
}

/// Print a short status indicator.
pub fn print_status_short(out: &dyn Output, status: &ReleaseStatus) {
    match status {
        ReleaseStatus::NotBuilt => out.print("Not Built"),
        ReleaseStatus::AwaitingUrls { part_count, .. } => {
            out.print(&format!("Awaiting URLs ({} parts)", part_count));
        }
        ReleaseStatus::Ready => out.print("Ready"),
        ReleaseStatus::Released => out.print("Released"),
    }
}

/// Format a release status as a descriptive string.
pub fn format_status(status: &ReleaseStatus) -> String {
    match status {
        ReleaseStatus::NotBuilt => "Not Built - run 'publish build'".to_string(),
        ReleaseStatus::AwaitingUrls {
            archive_name,
            part_count,
        } => format!(
            "Awaiting URLs - {} ({} parts) - run 'publish urls'",
            archive_name, part_count
        ),
        ReleaseStatus::Ready => "Ready - run 'publish release'".to_string(),
        ReleaseStatus::Released => "Released".to_string(),
    }
}

/// Format a size in bytes as a human-readable string.
pub fn format_size_display(size: u64) -> String {
    format_size(size as usize)
}

/// Print dedupe results to the output.
pub fn print_dedupe_result(out: &dyn Output, report: &DedupeReport) {
    out.header("Deduplication Results");
    out.newline();

    // Summary
    out.println(&format!("Tiles analyzed: {}", report.tiles_analyzed));
    out.println(&format!("Zoom levels:    {:?}", report.zoom_levels_present));
    out.newline();

    // Overlaps detected
    if report.overlaps_by_pair.is_empty() {
        out.println("No overlapping tiles detected.");
    } else {
        out.subheader("Overlaps Detected");
        let mut pairs: Vec<_> = report.overlaps_by_pair.iter().collect();
        pairs.sort_by_key(|((h, l), _)| (*h, *l));
        for ((higher, lower), count) in pairs {
            out.println(&format!(
                "  ZL{} overlaps ZL{}: {} tiles",
                higher, lower, count
            ));
        }
        out.newline();

        // Action taken
        if report.dry_run {
            out.println(&format!(
                "Would remove {} tiles (dry run - no files modified)",
                report.tiles_removed.len()
            ));
        } else {
            out.println(&format!("Removed {} tiles", report.tiles_removed.len()));
        }

        out.println(&format!("Preserved {} tiles", report.tiles_preserved.len()));
    }
}

/// Print overlap summary from scan.
pub fn print_overlap_summary(out: &dyn Output, summary: &OverlapSummary) {
    if summary.tiles_scanned == 0 {
        return;
    }

    out.subheader("Zoom Level Analysis");

    // Print tiles by zoom level
    let mut zooms: Vec<_> = summary.tiles_by_zoom.iter().collect();
    zooms.sort_by_key(|(z, _)| *z);
    for (zoom, count) in zooms {
        out.println(&format!("  ZL{} tiles: {}", zoom, count));
    }
    out.newline();

    // Print overlaps
    if summary.total_overlaps > 0 {
        out.println("Overlaps Detected:");
        let mut pairs: Vec<_> = summary.overlaps_by_pair.iter().collect();
        pairs.sort_by_key(|((h, l), _)| (*h, *l));
        for ((higher, lower), count) in pairs {
            out.println(&format!(
                "  ZL{} overlaps ZL{}: {} tiles",
                higher, lower, count
            ));
        }
        out.println(&format!(
            "  Total redundant tiles: {}",
            summary.total_overlaps
        ));
        out.newline();
        out.println("Recommendation:");
        out.indented("Use 'publish dedupe' to remove redundant tiles before building.");
    } else {
        out.println("No overlapping tiles detected.");
    }
}

/// Print dedupe result as JSON.
#[allow(dead_code)]
pub fn print_dedupe_json(out: &dyn Output, report: &DedupeReport) {
    // Build a simple JSON structure
    let mut json_parts = Vec::new();

    // tiles_analyzed
    json_parts.push(format!("  \"tiles_analyzed\": {}", report.tiles_analyzed));

    // zoom_levels_present
    let zl_arr: Vec<String> = report
        .zoom_levels_present
        .iter()
        .map(|z| z.to_string())
        .collect();
    json_parts.push(format!(
        "  \"zoom_levels_present\": [{}]",
        zl_arr.join(", ")
    ));

    // overlaps_by_pair
    let mut overlap_parts = Vec::new();
    for ((h, l), count) in &report.overlaps_by_pair {
        overlap_parts.push(format!("    \"{}-{}\": {}", h, l, count));
    }
    json_parts.push(format!(
        "  \"overlaps_by_pair\": {{\n{}\n  }}",
        overlap_parts.join(",\n")
    ));

    // tiles_removed count
    json_parts.push(format!(
        "  \"tiles_removed_count\": {}",
        report.tiles_removed.len()
    ));

    // tiles_preserved count
    json_parts.push(format!(
        "  \"tiles_preserved_count\": {}",
        report.tiles_preserved.len()
    ));

    // dry_run
    json_parts.push(format!("  \"dry_run\": {}", report.dry_run));

    // removed tile paths
    let removed_paths: Vec<String> = report
        .tiles_removed
        .iter()
        .map(|t| format!("    \"{}\"", t.ter_path.display()))
        .collect();
    json_parts.push(format!(
        "  \"tiles_removed\": [\n{}\n  ]",
        removed_paths.join(",\n")
    ));

    out.println("{");
    out.println(&json_parts.join(",\n"));
    out.println("}");
}
