//! Live debug map for prefetch observability.
//!
//! Feature-gated behind `debug-map`. Serves a browser-based Leaflet.js map
//! showing aircraft position, sliding prefetch box bounds, DSF region states
//! from GeoIndex, and pipeline metrics. Polls via JSON API at 2-second intervals.
//!
//! # Usage
//!
//! Build with `--features debug-map`, then open `http://localhost:8087` in a browser.

pub mod api;
mod html;
mod server;
pub mod state;

pub use server::{DebugMapServer, DEFAULT_DEBUG_MAP_PORT};
pub use state::DebugMapState;
