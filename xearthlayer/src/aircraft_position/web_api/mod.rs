//! X-Plane Web API adapter for aircraft telemetry and sim state.
//!
//! Connects to X-Plane's built-in Web API (available since 12.1.1)
//! via REST for dataref ID lookup and WebSocket for 10Hz position
//! and sim state subscriptions. Requires no user configuration.

mod adapter;
pub mod client;
pub mod config;
pub mod datarefs;
pub mod sim_state;

use std::sync::{Arc, RwLock};

use self::sim_state::SimState;

pub use adapter::WebApiAdapter;

/// Thread-safe shared sim state for cross-component access.
///
/// Updated by the [`WebApiAdapter`], read by the prefetch coordinator.
pub type SharedSimState = Arc<RwLock<SimState>>;

/// Create a new [`SharedSimState`] with default values.
pub fn shared_sim_state() -> SharedSimState {
    Arc::new(RwLock::new(SimState::default()))
}
