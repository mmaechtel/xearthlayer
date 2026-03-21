//! Embedded HTML page for the debug map.
//!
//! Single-page Leaflet.js application that polls `/api/state` and renders
//! aircraft position, prefetch box, and DSF region states.
//!
//! Note: This is a debug-only tool (feature-gated behind `debug-map`).
//! The HTML uses innerHTML for stats display with data sourced exclusively
//! from the local XEL process — no untrusted external content.

/// The complete HTML/CSS/JS for the debug map, served at `GET /`.
pub const MAP_HTML: &str = include_str!("map.html");
