//! Output formatting utilities for publish commands.
//!
//! This module provides helper functions for formatting output consistently
//! across all publish command handlers.

use super::traits::Output;
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
