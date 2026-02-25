//! DSF boundary-aware tile prioritization for prefetch ordering.
//!
//! Re-sorts prefetch tile lists so that tiles near upcoming DSF (1x1 degree)
//! boundaries rank higher than tiles far from boundaries. This matches
//! X-Plane's scenery loading pattern, which triggers tile loads at DSF
//! boundary crossings.
//!
//! # Algorithm
//!
//! 1. Decompose the aircraft track into lat/lon velocity components.
//! 2. For each active axis (component above threshold), compute an axis rank
//!    based on how many DSF cells ahead the tile is.
//! 3. Take the minimum rank across active axes (urgent on either = urgent overall).
//! 4. Sort by rank (primary), then Euclidean distance (secondary tiebreaker).
//!
//! If neither axis is active (very slow or ambiguous track), fall back to
//! pure Euclidean distance sorting.

use crate::coord::TileCoord;

/// Minimum axis velocity component to consider that axis active.
///
/// Below this threshold the aircraft is considered stationary on that axis.
/// Value 0.15 corresponds to roughly 8-9 degrees off a cardinal direction.
const AXIS_VELOCITY_THRESHOLD: f64 = 0.15;

/// Penalty added to tiles behind the aircraft on an axis.
const BEHIND_PENALTY: f64 = 100.0;

/// Rank assigned to tiles in the same DSF cell as the aircraft.
///
/// Slightly deprioritized compared to the next cell ahead (rank 0),
/// since the current cell is presumably already loaded.
const SAME_CELL_RANK: f64 = 0.5;

/// Re-sort `tiles` in place by DSF boundary urgency relative to `position` and `track`.
///
/// # Arguments
///
/// * `position` - Aircraft position as (latitude, longitude) in degrees.
/// * `track` - Ground track in degrees, 0-360 (true north clockwise).
/// * `tiles` - Mutable slice of tiles to re-sort.
///
/// After this call, tiles nearest to the next DSF boundary crossing along the
/// track come first. Tiles at the same boundary rank are sub-sorted by
/// Euclidean distance from the aircraft.
pub fn prioritize(position: (f64, f64), track: f64, tiles: &mut [TileCoord]) {
    if tiles.is_empty() {
        return;
    }

    let (ac_lat, ac_lon) = position;
    let track_rad = track.to_radians();

    // Velocity components: sin for longitude (east+), cos for latitude (north+)
    let v_lon = track_rad.sin();
    let v_lat = track_rad.cos();

    let lon_active = v_lon.abs() >= AXIS_VELOCITY_THRESHOLD;
    let lat_active = v_lat.abs() >= AXIS_VELOCITY_THRESHOLD;

    // If neither axis is active, fall back to Euclidean distance
    if !lon_active && !lat_active {
        tiles.sort_by(|a, b| {
            let da = euclidean_distance(ac_lat, ac_lon, a);
            let db = euclidean_distance(ac_lat, ac_lon, b);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        return;
    }

    // Compute boundary rank for each tile, then sort
    tiles.sort_by(|a, b| {
        let rank_a = tile_boundary_rank(ac_lat, ac_lon, lat_active, lon_active, v_lat, v_lon, a);
        let rank_b = tile_boundary_rank(ac_lat, ac_lon, lat_active, lon_active, v_lat, v_lon, b);

        rank_a
            .partial_cmp(&rank_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let da = euclidean_distance(ac_lat, ac_lon, a);
                let db = euclidean_distance(ac_lat, ac_lon, b);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

/// Compute the boundary rank for a single tile.
///
/// Returns the minimum axis rank across all active axes.
fn tile_boundary_rank(
    ac_lat: f64,
    ac_lon: f64,
    lat_active: bool,
    lon_active: bool,
    v_lat: f64,
    v_lon: f64,
    tile: &TileCoord,
) -> f64 {
    let (tile_lat, tile_lon) = tile.to_lat_lon();

    let mut min_rank = f64::MAX;

    if lat_active {
        let rank = axis_rank(ac_lat, tile_lat, v_lat);
        min_rank = min_rank.min(rank);
    }

    if lon_active {
        let rank = axis_rank(ac_lon, tile_lon, v_lon);
        min_rank = min_rank.min(rank);
    }

    min_rank
}

/// Compute the axis rank for one coordinate axis.
///
/// - velocity > 0: moving in positive direction (north or east)
/// - velocity < 0: moving in negative direction (south or west)
///
/// Returns:
/// - 0.0 for the next DSF cell ahead
/// - 0.5 for the current (same) DSF cell
/// - N for N cells ahead beyond the next
/// - 100+ for cells behind the aircraft
fn axis_rank(aircraft_pos: f64, tile_pos: f64, velocity: f64) -> f64 {
    let aircraft_dsf = aircraft_pos.floor() as i64;
    let tile_dsf = tile_pos.floor() as i64;

    if velocity > 0.0 {
        if tile_dsf > aircraft_dsf {
            // Ahead: next cell = rank 0, each cell further adds 1
            (tile_dsf - aircraft_dsf - 1) as f64
        } else if tile_dsf == aircraft_dsf {
            SAME_CELL_RANK
        } else {
            // Behind
            BEHIND_PENALTY + (aircraft_dsf - tile_dsf) as f64
        }
    } else {
        // velocity < 0
        if tile_dsf < aircraft_dsf {
            // Ahead (in negative direction): next cell = rank 0
            (aircraft_dsf - tile_dsf - 1) as f64
        } else if tile_dsf == aircraft_dsf {
            SAME_CELL_RANK
        } else {
            // Behind
            BEHIND_PENALTY + (tile_dsf - aircraft_dsf) as f64
        }
    }
}

/// Euclidean distance in degrees between aircraft position and tile center.
fn euclidean_distance(ac_lat: f64, ac_lon: f64, tile: &TileCoord) -> f64 {
    let (tile_lat, tile_lon) = tile.to_lat_lon();
    let dlat = tile_lat - ac_lat;
    let dlon = tile_lon - ac_lon;
    (dlat * dlat + dlon * dlon).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coord::to_tile_coords;

    /// Create a tile at the given lat/lon at zoom 14.
    fn tile_at(lat: f64, lon: f64) -> TileCoord {
        to_tile_coords(lat, lon, 14).expect("valid tile coords")
    }

    /// Get the DSF longitude column for a tile.
    fn tile_dsf_lon(tile: &TileCoord) -> i32 {
        let (_, lon) = tile.to_lat_lon();
        lon.floor() as i32
    }

    /// Get the DSF latitude row for a tile.
    fn tile_dsf_lat(tile: &TileCoord) -> i32 {
        let (lat, _) = tile.to_lat_lon();
        lat.floor() as i32
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Directional priority tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_westbound_prioritizes_next_western_column() {
        // Aircraft at lon -118.3, heading west (270 degrees)
        // Tiles in DSF -119 should rank before tiles in DSF -118 (same cell)
        // and way before tiles in DSF -117 (behind)
        let position = (34.0, -118.3);
        let track = 270.0; // due west

        let tile_ahead = tile_at(34.0, -118.5); // DSF lon -119 (next west)
        let tile_same = tile_at(34.0, -118.3); // DSF lon -119 or -118 (same cell)
        let tile_behind = tile_at(34.0, -117.5); // DSF lon -118 (behind)

        let mut tiles = vec![tile_behind, tile_same, tile_ahead];
        prioritize(position, track, &mut tiles);

        // The tile in the next western DSF column should come first
        let first_dsf_lon = tile_dsf_lon(&tiles[0]);
        let last_dsf_lon = tile_dsf_lon(&tiles[tiles.len() - 1]);

        // First tile should be further west (more negative) than last
        assert!(
            first_dsf_lon <= last_dsf_lon,
            "Westbound: first tile DSF lon {} should be <= last tile DSF lon {}",
            first_dsf_lon,
            last_dsf_lon
        );
    }

    #[test]
    fn test_northbound_prioritizes_next_northern_row() {
        // Aircraft at lat 53.3, heading north (0 degrees)
        // Tiles in DSF lat 54 should rank before tiles in DSF lat 53 (same cell)
        let position = (53.3, 9.5);
        let track = 0.0; // due north

        let tile_ahead = tile_at(54.5, 9.5); // DSF lat 54 (next north)
        let tile_same = tile_at(53.5, 9.5); // DSF lat 53 (same cell)
        let tile_behind = tile_at(52.5, 9.5); // DSF lat 52 (behind)

        let mut tiles = vec![tile_behind, tile_same, tile_ahead];
        prioritize(position, track, &mut tiles);

        let first_dsf_lat = tile_dsf_lat(&tiles[0]);
        let last_dsf_lat = tile_dsf_lat(&tiles[tiles.len() - 1]);

        // First tile should be further north (higher lat) than last
        assert!(
            first_dsf_lat >= last_dsf_lat,
            "Northbound: first tile DSF lat {} should be >= last tile DSF lat {}",
            first_dsf_lat,
            last_dsf_lat
        );
    }

    #[test]
    fn test_diagonal_northwest_considers_both_axes() {
        // Heading 315 degrees (northwest)
        // Both lat and lon axes should be active
        let position = (53.3, 9.5);
        let track = 315.0; // northwest

        // Tile ahead on both axes (north and west)
        let tile_nw = tile_at(54.5, 8.5); // DSF lat 54, lon 8
        // Tile ahead on lat only
        let tile_n = tile_at(54.5, 9.5); // DSF lat 54, lon 9
        // Tile behind on both
        let tile_se = tile_at(52.5, 10.5); // DSF lat 52, lon 10

        let mut tiles = vec![tile_se, tile_n, tile_nw];
        prioritize(position, track, &mut tiles);

        // The tile that is ahead on both axes should rank high
        // The tile behind on both axes should be last
        let last_dsf_lat = tile_dsf_lat(&tiles[tiles.len() - 1]);
        let last_dsf_lon = tile_dsf_lon(&tiles[tiles.len() - 1]);

        // Last tile should be the southeast one (behind on both)
        assert!(
            last_dsf_lat <= 52 && last_dsf_lon >= 10,
            "Last tile should be behind (SE): lat={}, lon={}",
            last_dsf_lat,
            last_dsf_lon
        );
    }

    #[test]
    fn test_due_east_only_considers_longitude() {
        // Heading 90 degrees (due east)
        // Only longitude axis should be active (sin(90) = 1.0, cos(90) ~ 0)
        let position = (34.0, -118.3);
        let track = 90.0;

        // Tiles at different longitudes but same DSF latitude
        let tile_east = tile_at(34.0, -117.5); // DSF lon -118 (ahead east)
        let tile_west = tile_at(34.0, -119.5); // DSF lon -120 (behind)

        let mut tiles = vec![tile_west, tile_east];
        prioritize(position, track, &mut tiles);

        // East tile should come first
        let first_dsf_lon = tile_dsf_lon(&tiles[0]);
        let second_dsf_lon = tile_dsf_lon(&tiles[1]);

        assert!(
            first_dsf_lon > second_dsf_lon,
            "Eastbound: first tile DSF lon {} should be > second {}",
            first_dsf_lon,
            second_dsf_lon
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge case tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_tiles_is_noop() {
        let mut tiles: Vec<TileCoord> = vec![];
        prioritize((34.0, -118.3), 90.0, &mut tiles);
        assert!(tiles.is_empty());
    }

    #[test]
    fn test_tiles_within_same_rank_sorted_by_distance() {
        // Two tiles in the same DSF cell ahead should be sub-sorted by distance
        let position = (53.3, 9.5);
        let track = 0.0; // north

        // Both tiles are in DSF lat 54 (rank 0), but at different distances
        let tile_close = tile_at(54.1, 9.5); // closer to aircraft
        let tile_far = tile_at(54.9, 9.5); // farther from aircraft

        let mut tiles = vec![tile_far, tile_close];
        prioritize(position, track, &mut tiles);

        // Both have rank 0 (next DSF cell north), so distance should break tie
        let (lat0, _) = tiles[0].to_lat_lon();
        let (lat1, _) = tiles[1].to_lat_lon();

        let dist0 = (lat0 - 53.3).abs();
        let dist1 = (lat1 - 53.3).abs();

        assert!(
            dist0 <= dist1 + 0.01,
            "Within same rank, closer tile should come first: dist0={}, dist1={}",
            dist0,
            dist1
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // axis_rank unit tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_axis_rank_positive_direction() {
        // Moving in positive direction (north or east), velocity > 0

        // Next cell ahead: rank 0
        assert!((axis_rank(53.3, 54.5, 1.0) - 0.0).abs() < 0.001, "next cell ahead = rank 0");

        // Two cells ahead: rank 1
        assert!((axis_rank(53.3, 55.5, 1.0) - 1.0).abs() < 0.001, "two cells ahead = rank 1");

        // Same cell: rank 0.5
        assert!(
            (axis_rank(53.3, 53.5, 1.0) - 0.5).abs() < 0.001,
            "same cell = rank 0.5"
        );

        // Behind: rank 100+
        let behind_rank = axis_rank(53.3, 52.5, 1.0);
        assert!(behind_rank >= 100.0, "behind = rank 100+, got {}", behind_rank);
    }

    #[test]
    fn test_axis_rank_negative_direction() {
        // Moving in negative direction (south or west), velocity < 0

        // Next cell ahead (lower DSF): rank 0
        assert!(
            (axis_rank(53.3, 52.5, -1.0) - 0.0).abs() < 0.001,
            "next cell ahead (negative) = rank 0"
        );

        // Two cells ahead (negative): rank 1
        assert!(
            (axis_rank(53.3, 51.5, -1.0) - 1.0).abs() < 0.001,
            "two cells ahead (negative) = rank 1"
        );

        // Same cell: rank 0.5
        assert!(
            (axis_rank(53.3, 53.5, -1.0) - 0.5).abs() < 0.001,
            "same cell (negative) = rank 0.5"
        );

        // Behind (higher DSF when moving negative): rank 100+
        let behind_rank = axis_rank(53.3, 54.5, -1.0);
        assert!(
            behind_rank >= 100.0,
            "behind (negative) = rank 100+, got {}",
            behind_rank
        );
    }
}
