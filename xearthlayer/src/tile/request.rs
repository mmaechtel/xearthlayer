//! Tile generation request types.
//!
//! Provides the `TileRequest` type that encapsulates all information
//! needed to generate a tile texture, abstracting away the specific
//! filename parsing details.

/// Request to generate a tile texture.
///
/// Contains the geographic coordinates and zoom level needed to
/// generate a satellite imagery tile.
///
/// # Example
///
/// ```
/// use xearthlayer::tile::TileRequest;
///
/// let request = TileRequest::new(37, -123, 16);
/// assert_eq!(request.lat(), 37);
/// assert_eq!(request.lon(), -123);
/// assert_eq!(request.zoom(), 16);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileRequest {
    /// Latitude tile coordinate (degrees)
    lat: i32,
    /// Longitude tile coordinate (degrees)
    lon: i32,
    /// Zoom level
    zoom: u8,
}

impl TileRequest {
    /// Create a new tile request.
    ///
    /// # Arguments
    ///
    /// * `lat` - Latitude tile coordinate in degrees
    /// * `lon` - Longitude tile coordinate in degrees
    /// * `zoom` - Zoom level (typically 12-19)
    pub fn new(lat: i32, lon: i32, zoom: u8) -> Self {
        Self { lat, lon, zoom }
    }

    /// Get the latitude coordinate.
    pub fn lat(&self) -> i32 {
        self.lat
    }

    /// Get the longitude coordinate.
    pub fn lon(&self) -> i32 {
        self.lon
    }

    /// Get the zoom level.
    pub fn zoom(&self) -> u8 {
        self.zoom
    }

    /// Get latitude as floating point for coordinate conversion.
    pub fn lat_f64(&self) -> f64 {
        self.lat as f64
    }

    /// Get longitude as floating point for coordinate conversion.
    pub fn lon_f64(&self) -> f64 {
        self.lon as f64
    }
}

impl From<crate::fuse::DdsFilename> for TileRequest {
    fn from(filename: crate::fuse::DdsFilename) -> Self {
        Self {
            lat: filename.row,
            lon: filename.col,
            zoom: filename.zoom,
        }
    }
}

impl From<&crate::fuse::DdsFilename> for TileRequest {
    fn from(filename: &crate::fuse::DdsFilename) -> Self {
        Self {
            lat: filename.row,
            lon: filename.col,
            zoom: filename.zoom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuse::DdsFilename;

    #[test]
    fn test_new() {
        let request = TileRequest::new(37, -123, 16);
        assert_eq!(request.lat(), 37);
        assert_eq!(request.lon(), -123);
        assert_eq!(request.zoom(), 16);
    }

    #[test]
    fn test_negative_coords() {
        let request = TileRequest::new(-40, -74, 15);
        assert_eq!(request.lat(), -40);
        assert_eq!(request.lon(), -74);
        assert_eq!(request.zoom(), 15);
    }

    #[test]
    fn test_lat_f64() {
        let request = TileRequest::new(37, -123, 16);
        assert!((request.lat_f64() - 37.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_lon_f64() {
        let request = TileRequest::new(37, -123, 16);
        assert!((request.lon_f64() - (-123.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_clone() {
        let request = TileRequest::new(37, -123, 16);
        let cloned = request;
        assert_eq!(request, cloned);
    }

    #[test]
    fn test_copy() {
        let request = TileRequest::new(37, -123, 16);
        let copied = request;
        assert_eq!(request.lat(), copied.lat());
    }

    #[test]
    fn test_equality() {
        let request1 = TileRequest::new(37, -123, 16);
        let request2 = TileRequest::new(37, -123, 16);
        let request3 = TileRequest::new(38, -123, 16);

        assert_eq!(request1, request2);
        assert_ne!(request1, request3);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TileRequest::new(37, -123, 16));
        set.insert(TileRequest::new(37, -123, 16));
        set.insert(TileRequest::new(38, -123, 16));

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_debug() {
        let request = TileRequest::new(37, -123, 16);
        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("37"));
        assert!(debug_str.contains("-123"));
        assert!(debug_str.contains("16"));
    }

    #[test]
    fn test_from_dds_filename() {
        let filename = DdsFilename {
            row: 37,
            col: -123,
            zoom: 16,
        };
        let request: TileRequest = filename.into();
        assert_eq!(request.lat(), 37);
        assert_eq!(request.lon(), -123);
        assert_eq!(request.zoom(), 16);
    }

    #[test]
    fn test_from_dds_filename_ref() {
        let filename = DdsFilename {
            row: 37,
            col: -123,
            zoom: 16,
        };
        let request: TileRequest = (&filename).into();
        assert_eq!(request.lat(), 37);
        assert_eq!(request.lon(), -123);
        assert_eq!(request.zoom(), 16);
    }
}
