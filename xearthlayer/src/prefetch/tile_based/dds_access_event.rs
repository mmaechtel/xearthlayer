//! DDS access event from FUSE layer.

use std::time::Instant;

use super::dsf_coord::DsfTileCoord;

/// Event sent from FUSE when a DDS file is accessed.
///
/// The FUSE layer sends these events via a channel whenever X-Plane requests
/// a DDS texture. This enables tracking which DSF tiles are actively being
/// loaded for monitoring purposes.
#[derive(Debug, Clone)]
pub struct DdsAccessEvent {
    /// The 1° DSF tile containing the requested DDS texture.
    pub dsf_tile: DsfTileCoord,

    /// When the access occurred.
    pub timestamp: Instant,
}

impl DdsAccessEvent {
    /// Create a new DDS access event.
    pub fn new(dsf_tile: DsfTileCoord) -> Self {
        Self {
            dsf_tile,
            timestamp: Instant::now(),
        }
    }

    /// Create from raw coordinates.
    pub fn from_coords(lat: f64, lon: f64) -> Self {
        Self::new(DsfTileCoord::from_lat_lon(lat, lon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dds_access_event_creation() {
        let tile = DsfTileCoord::new(60, -146);
        let event = DdsAccessEvent::new(tile);

        assert_eq!(event.dsf_tile.lat, 60);
        assert_eq!(event.dsf_tile.lon, -146);
    }

    #[test]
    fn test_dds_access_event_from_coords() {
        let event = DdsAccessEvent::from_coords(60.5, -145.3);

        assert_eq!(event.dsf_tile.lat, 60);
        assert_eq!(event.dsf_tile.lon, -146);
    }
}
