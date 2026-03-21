//! Shared state for the debug map server.
//!
//! Holds Arc references to all data sources the API needs to query.
//! All fields are thread-safe and can be read without blocking the
//! main service.

use std::sync::Arc;

use crate::aircraft_position::web_api::SharedSimState;
use crate::aircraft_position::SharedAircraftPosition;
use crate::geo_index::GeoIndex;
use crate::prefetch::SharedPrefetchStatus;

use super::activity::TileActivityTracker;

/// Shared state for the debug map server.
///
/// Passed to axum as application state. All fields are Arc-wrapped
/// and cloneable without copying data.
#[derive(Clone)]
pub struct DebugMapState {
    pub aircraft_position: SharedAircraftPosition,
    pub sim_state: SharedSimState,
    pub geo_index: Option<Arc<GeoIndex>>,
    pub prefetch_status: Arc<SharedPrefetchStatus>,
    pub tile_activity: TileActivityTracker,
}
