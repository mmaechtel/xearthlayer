//! JSON API for the debug map.
//!
//! Collects state from all data sources into a single JSON snapshot.
//! Called by the Leaflet.js map page every 2 seconds.

use serde::Serialize;

use crate::aircraft_position::AircraftPositionProvider;
use crate::geo_index::{PatchCoverage, PrefetchedRegion, RetainedRegion};
use crate::prefetch::adaptive::PrefetchBox;

use super::state::DebugMapState;

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot types (serialised to JSON)
// ─────────────────────────────────────────────────────────────────────────────

/// Complete state snapshot returned by `/api/state`.
#[derive(Serialize, Default)]
pub struct DebugStateSnapshot {
    pub aircraft: Option<AircraftInfo>,
    pub sim_state: SimStateInfo,
    pub prefetch_box: Option<BoxBounds>,
    pub regions: Vec<RegionInfo>,
    pub stats: StatsInfo,
}

/// Aircraft position and vectors.
#[derive(Serialize)]
pub struct AircraftInfo {
    pub latitude: f64,
    pub longitude: f64,
    pub heading: f32,
    pub track: Option<f32>,
    pub ground_speed: f32,
    pub altitude: f32,
}

/// Sim state from X-Plane Web API.
#[derive(Serialize, Default)]
pub struct SimStateInfo {
    pub paused: bool,
    pub on_ground: bool,
    pub scenery_loading: bool,
    pub replay: bool,
    pub sim_speed: i32,
}

/// Prefetch box geographic bounds.
#[derive(Serialize)]
pub struct BoxBounds {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

/// A single DSF region with its current state.
#[derive(Serialize)]
pub struct RegionInfo {
    pub lat: i32,
    pub lon: i32,
    pub state: RegionState,
}

/// Region state for map colour coding.
#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegionState {
    InProgress,
    Prefetched,
    NoCoverage,
    Retained,
    Patched,
}

/// Pipeline statistics.
#[derive(Serialize, Default)]
pub struct StatsInfo {
    pub memory_cache_hit_rate: f64,
    pub tiles_submitted: u64,
    pub deferred_cycles: u64,
    pub prefetch_mode: String,
    pub active_strategy: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// State collection
// ─────────────────────────────────────────────────────────────────────────────

/// Default forward/behind margins for prefetch box visualisation.
const DEFAULT_FORWARD_MARGIN: f64 = 3.0;
const DEFAULT_BEHIND_MARGIN: f64 = 1.0;

/// Collect current state from all sources into a JSON-serialisable snapshot.
pub fn collect_snapshot(state: &DebugMapState) -> DebugStateSnapshot {
    let aircraft = collect_aircraft(state);
    let sim_state = collect_sim_state(state);
    let prefetch_box = compute_prefetch_box(&aircraft);
    let regions = collect_regions(state);
    let stats = collect_stats(state);

    DebugStateSnapshot {
        aircraft,
        sim_state,
        prefetch_box,
        regions,
        stats,
    }
}

fn collect_aircraft(state: &DebugMapState) -> Option<AircraftInfo> {
    let status = state.aircraft_position.status();
    let aircraft_state = status.state?;

    Some(AircraftInfo {
        latitude: aircraft_state.latitude,
        longitude: aircraft_state.longitude,
        heading: aircraft_state.heading,
        track: aircraft_state.track,
        ground_speed: aircraft_state.ground_speed,
        altitude: aircraft_state.altitude,
    })
}

fn collect_sim_state(state: &DebugMapState) -> SimStateInfo {
    match state.sim_state.read() {
        Ok(sim) => SimStateInfo {
            paused: sim.paused,
            on_ground: sim.on_ground,
            scenery_loading: sim.scenery_loading,
            replay: sim.replay,
            sim_speed: sim.sim_speed,
        },
        Err(_) => SimStateInfo::default(),
    }
}

fn compute_prefetch_box(aircraft: &Option<AircraftInfo>) -> Option<BoxBounds> {
    let aircraft = aircraft.as_ref()?;
    let track = aircraft.track.unwrap_or(aircraft.heading) as f64;

    let pbox = PrefetchBox::new(DEFAULT_FORWARD_MARGIN, DEFAULT_BEHIND_MARGIN);
    let (lat_min, lat_max, lon_min, lon_max) = pbox.bounds(
        aircraft.latitude,
        aircraft.longitude,
        track,
    );

    Some(BoxBounds {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    })
}

fn collect_regions(state: &DebugMapState) -> Vec<RegionInfo> {
    let Some(ref geo_index) = state.geo_index else {
        return Vec::new();
    };

    let mut regions = Vec::new();

    // Prefetch states (most informative — show these first)
    for (region, prefetched) in geo_index.iter::<PrefetchedRegion>() {
        let state = if prefetched.is_in_progress() {
            RegionState::InProgress
        } else if prefetched.is_prefetched() {
            RegionState::Prefetched
        } else {
            RegionState::NoCoverage
        };
        regions.push(RegionInfo {
            lat: region.lat,
            lon: region.lon,
            state,
        });
    }

    // Patch coverage
    for region in geo_index.regions::<PatchCoverage>() {
        // Only add if not already covered by prefetch state
        if !regions.iter().any(|r| r.lat == region.lat && r.lon == region.lon) {
            regions.push(RegionInfo {
                lat: region.lat,
                lon: region.lon,
                state: RegionState::Patched,
            });
        }
    }

    // Retained regions (only add if not already present)
    for region in geo_index.regions::<RetainedRegion>() {
        if !regions.iter().any(|r| r.lat == region.lat && r.lon == region.lon) {
            regions.push(RegionInfo {
                lat: region.lat,
                lon: region.lon,
                state: RegionState::Retained,
            });
        }
    }

    regions
}

fn collect_stats(state: &DebugMapState) -> StatsInfo {
    let snapshot = state.prefetch_status.snapshot();

    let (tiles_submitted, deferred_cycles, active_strategy) =
        if let Some(ref detailed) = snapshot.detailed_stats {
            (
                detailed.tiles_submitted_total,
                detailed.deferred_cycles,
                String::new(), // Active strategy not in detailed stats
            )
        } else {
            (snapshot.stats.tiles_submitted, 0, String::new())
        };

    StatsInfo {
        memory_cache_hit_rate: 0.0, // TODO: wire from TelemetrySnapshot when available
        tiles_submitted,
        deferred_cycles,
        prefetch_mode: format!("{:?}", snapshot.prefetch_mode),
        active_strategy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_state_snapshot_serialises() {
        let snapshot = DebugStateSnapshot {
            aircraft: Some(AircraftInfo {
                latitude: 48.0,
                longitude: 15.0,
                heading: 270.0,
                track: Some(265.0),
                ground_speed: 450.0,
                altitude: 35000.0,
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("48"));
        assert!(json.contains("270"));
    }

    #[test]
    fn test_region_info_serialises_state() {
        let region = RegionInfo {
            lat: 48,
            lon: 15,
            state: RegionState::Prefetched,
        };
        let json = serde_json::to_string(&region).unwrap();
        assert!(json.contains("\"prefetched\""));
    }

    #[test]
    fn test_region_info_serialises_in_progress() {
        let region = RegionInfo {
            lat: 48,
            lon: 15,
            state: RegionState::InProgress,
        };
        let json = serde_json::to_string(&region).unwrap();
        assert!(json.contains("\"in_progress\""));
    }

    #[test]
    fn test_box_bounds_serialises() {
        let bounds = BoxBounds {
            lat_min: 45.0,
            lat_max: 49.0,
            lon_min: 12.0,
            lon_max: 16.0,
        };
        let json = serde_json::to_string(&bounds).unwrap();
        assert!(json.contains("45"));
        assert!(json.contains("49"));
    }

    #[test]
    fn test_compute_prefetch_box_heading_west() {
        let aircraft = Some(AircraftInfo {
            latitude: 48.0,
            longitude: 15.0,
            heading: 270.0,
            track: Some(270.0),
            ground_speed: 450.0,
            altitude: 35000.0,
        });
        let bounds = compute_prefetch_box(&aircraft).unwrap();
        // Heading west: lon biased west (3° ahead), east (1° behind)
        assert!(bounds.lon_min < 13.0, "West edge should be ~12°");
        assert!(bounds.lon_max < 17.0, "East edge should be ~16°");
    }

    #[test]
    fn test_compute_prefetch_box_none_without_aircraft() {
        let bounds = compute_prefetch_box(&None);
        assert!(bounds.is_none());
    }

    #[test]
    fn test_empty_snapshot_serialises() {
        let snapshot = DebugStateSnapshot::default();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"aircraft\":null"));
        assert!(json.contains("\"regions\":[]"));
    }
}
