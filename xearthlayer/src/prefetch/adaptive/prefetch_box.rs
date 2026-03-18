//! Sliding prefetch box for cruise-phase tile prefetching.
//!
//! Computes a heading-biased rectangle around the aircraft position,
//! enumerates the DSF regions (1°×1°) it covers, and filters out
//! regions already tracked in the GeoIndex.

use crate::geo_index::{DsfRegion, GeoIndex, PrefetchedRegion};

/// A heading-aware prefetch region around the aircraft.
///
/// The box biases forward in the direction of travel:
/// - Axes with forward motion: `forward_margin` ahead, `behind_margin` behind
/// - Axes with no motion (near-cardinal perpendicular): symmetric at
///   `(forward_margin + behind_margin) / 2` each side
///
/// Any heading component above [`COMPONENT_THRESHOLD`] on an axis triggers
/// the forward bias. Only near-exact cardinal headings produce symmetric
/// perpendicular axes.
#[derive(Debug, Clone)]
pub struct PrefetchBox {
    /// Degrees ahead of aircraft in direction of travel per axis.
    forward_margin: f64,
    /// Degrees behind aircraft per axis.
    behind_margin: f64,
}

/// Threshold below which a heading component is treated as zero.
/// Prevents floating-point noise from biasing the perpendicular axis
/// on near-cardinal headings (e.g., cos(90°) ≈ 6e-17 in f64).
const COMPONENT_THRESHOLD: f64 = 1e-6;

impl PrefetchBox {
    /// Create a new prefetch box with the given margins.
    pub fn new(forward_margin: f64, behind_margin: f64) -> Self {
        Self {
            forward_margin,
            behind_margin,
        }
    }

    /// Compute all DSF regions within the heading-biased box.
    pub fn regions(&self, lat: f64, lon: f64, track: f64) -> Vec<DsfRegion> {
        let (lat_min, lat_max, lon_min, lon_max) = self.bounds(lat, lon, track);

        let dsf_lat_min = lat_min.floor() as i32;
        let dsf_lat_max = (lat_max - 1e-9).floor() as i32;
        let dsf_lon_min = lon_min.floor() as i32;
        let dsf_lon_max = (lon_max - 1e-9).floor() as i32;

        let capacity =
            ((dsf_lat_max - dsf_lat_min + 1) * (dsf_lon_max - dsf_lon_min + 1)).max(0) as usize;
        let mut result = Vec::with_capacity(capacity);

        for lat_i in dsf_lat_min..=dsf_lat_max {
            for lon_i in dsf_lon_min..=dsf_lon_max {
                result.push(DsfRegion::new(lat_i, lon_i));
            }
        }

        result
    }

    /// Compute DSF regions in the box that are NOT already tracked in GeoIndex.
    ///
    /// Filters out regions with any `PrefetchedRegion` state (InProgress,
    /// Prefetched, or NoCoverage).
    pub fn new_regions(
        &self,
        lat: f64,
        lon: f64,
        track: f64,
        geo_index: &GeoIndex,
    ) -> Vec<DsfRegion> {
        self.regions(lat, lon, track)
            .into_iter()
            .filter(|r| PrefetchedRegion::should_prefetch(geo_index, r))
            .collect()
    }

    /// Compute the geographic bounds of the box.
    ///
    /// Returns `(lat_min, lat_max, lon_min, lon_max)`.
    pub fn bounds(&self, lat: f64, lon: f64, track: f64) -> (f64, f64, f64, f64) {
        let track_rad = track.to_radians();
        let lat_component = track_rad.cos(); // positive = north
        let lon_component = track_rad.sin(); // positive = east

        let symmetric = (self.forward_margin + self.behind_margin) / 2.0;

        // Latitude axis
        let (lat_min, lat_max) = if lat_component > COMPONENT_THRESHOLD {
            // Moving north: ahead = north
            (lat - self.behind_margin, lat + self.forward_margin)
        } else if lat_component < -COMPONENT_THRESHOLD {
            // Moving south: ahead = south
            (lat - self.forward_margin, lat + self.behind_margin)
        } else {
            // Near-cardinal east/west: symmetric
            (lat - symmetric, lat + symmetric)
        };

        // Longitude axis
        let (lon_min, lon_max) = if lon_component > COMPONENT_THRESHOLD {
            // Moving east: ahead = east
            (lon - self.behind_margin, lon + self.forward_margin)
        } else if lon_component < -COMPONENT_THRESHOLD {
            // Moving west: ahead = west
            (lon - self.forward_margin, lon + self.behind_margin)
        } else {
            // Near-cardinal north/south: symmetric
            (lon - symmetric, lon + symmetric)
        };

        (lat_min, lat_max, lon_min, lon_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_due_west_biases_lon_west() {
        let pbox = PrefetchBox::new(3.0, 1.0);
        let regions = pbox.regions(48.0, 15.0, 270.0);

        assert!(!regions.is_empty());

        // Must include regions west of aircraft (ahead)
        let has_west = regions.iter().any(|r| r.lon == 12);
        assert!(has_west, "Should include lon=12 (3° west of aircraft at 15°)");

        // Must include region east of aircraft (behind, 1°)
        let has_east = regions.iter().any(|r| r.lon == 15);
        assert!(has_east, "Should include lon=15 (behind aircraft)");

        // Should NOT include lon=17 (2° behind — beyond 1° behind margin)
        let has_far_east = regions.iter().any(|r| r.lon == 17);
        assert!(!has_far_east, "Should not include lon=17 (beyond behind margin)");
    }

    #[test]
    fn test_due_south_biases_lat_south() {
        let pbox = PrefetchBox::new(3.0, 1.0);
        let regions = pbox.regions(48.0, 15.0, 180.0);

        let has_south = regions.iter().any(|r| r.lat == 45);
        assert!(has_south, "Should include lat=45 (3° south)");

        let has_north = regions.iter().any(|r| r.lat == 48);
        assert!(has_north, "Should include lat=48 (behind)");

        let has_far_north = regions.iter().any(|r| r.lat == 50);
        assert!(!has_far_north, "Should not include lat=50 (beyond behind margin)");
    }

    #[test]
    fn test_southwest_biases_both_axes() {
        let pbox = PrefetchBox::new(3.0, 1.0);
        let regions = pbox.regions(48.0, 15.0, 225.0);

        // Must include far SW corner
        let has_sw = regions.iter().any(|r| r.lat == 45 && r.lon == 12);
        assert!(has_sw, "Should include SW corner (45, 12)");

        // Must NOT include far NE (beyond behind on both axes)
        let has_ne = regions.iter().any(|r| r.lat == 50 && r.lon == 17);
        assert!(!has_ne, "Should not include far NE corner");
    }

    #[test]
    fn test_exact_cardinal_symmetric_perpendicular() {
        let pbox = PrefetchBox::new(3.0, 1.0);

        // Due north (0°): lat biased north, lon symmetric
        let regions = pbox.regions(48.0, 15.0, 0.0);

        let lat_min = regions.iter().map(|r| r.lat).min().unwrap();
        let lat_max = regions.iter().map(|r| r.lat).max().unwrap();
        assert_eq!(lat_min, 47, "South edge should be 47 (1° behind)");
        assert_eq!(lat_max, 50, "North edge should be 50 (3° ahead)");

        let lon_min = regions.iter().map(|r| r.lon).min().unwrap();
        let lon_max = regions.iter().map(|r| r.lon).max().unwrap();
        assert_eq!(lon_min, 13, "West should be symmetric 2°");
        assert_eq!(lon_max, 16, "East should be symmetric 2°");
    }

    #[test]
    fn test_due_east_lat_symmetric_despite_float() {
        let pbox = PrefetchBox::new(3.0, 1.0);

        // Due east (90°): cos(90°) ≈ 6.12e-17 in f64, NOT exact zero.
        // The threshold (1e-6) should treat this as zero → symmetric lat.
        let regions = pbox.regions(48.0, 15.0, 90.0);

        let lat_min = regions.iter().map(|r| r.lat).min().unwrap();
        let lat_max = regions.iter().map(|r| r.lat).max().unwrap();
        assert_eq!(lat_min, 46, "Lat should be symmetric 2° south");
        assert_eq!(lat_max, 49, "Lat should be symmetric 2° north");
    }

    #[test]
    fn test_southern_hemisphere_negative_lat() {
        let pbox = PrefetchBox::new(3.0, 1.0);

        // Sydney area, heading west
        let regions = pbox.regions(-34.0, 151.0, 270.0);

        let has_south = regions.iter().any(|r| r.lat == -36);
        assert!(has_south, "Should include lat=-36 (negative floor)");

        let has_west = regions.iter().any(|r| r.lon == 148);
        assert!(has_west, "Should include lon=148 (3° west ahead)");

        assert!(
            regions.iter().all(|r| r.lat < 0),
            "All regions should have negative lat in southern hemisphere"
        );
    }

    #[test]
    fn test_new_regions_filters_already_tracked() {
        use crate::geo_index::GeoIndex;

        let pbox = PrefetchBox::new(3.0, 1.0);
        let geo_index = GeoIndex::new();

        // Mark one region as already in progress
        let tracked = DsfRegion::new(48, 14);
        geo_index.insert::<PrefetchedRegion>(tracked, PrefetchedRegion::in_progress());

        let new = pbox.new_regions(48.0, 15.0, 270.0, &geo_index);

        assert!(!new.contains(&tracked), "Should exclude already-tracked region");
        assert!(!new.is_empty(), "Should have untracked regions");
    }
}
