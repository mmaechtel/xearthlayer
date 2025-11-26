//! Tile generation request types.
//!
//! Provides the `TileRequest` type that encapsulates all information
//! needed to generate a tile texture, abstracting away the specific
//! filename parsing details.

/// Request to generate a tile texture.
///
/// Contains the tile coordinates (row/col in Web Mercator projection)
/// and zoom level needed to download and encode a satellite imagery tile.
///
/// # Note
///
/// The row/col values are unsigned tile indices in the Web Mercator grid:
/// - Row increases southward (equator ≈ 2^(zoom-1))
/// - Col increases eastward (prime meridian ≈ 2^(zoom-1))
///
/// These come from parsed FUSE filenames like `100000_125184_BI18.dds`.
///
/// # Example
///
/// ```
/// use xearthlayer::tile::TileRequest;
///
/// let request = TileRequest::new(100000, 125184, 18);
/// assert_eq!(request.row(), 100000);
/// assert_eq!(request.col(), 125184);
/// assert_eq!(request.zoom(), 18);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileRequest {
    /// Tile row (Y coordinate in Web Mercator grid, unsigned)
    row: u32,
    /// Tile column (X coordinate in Web Mercator grid, unsigned)
    col: u32,
    /// Zoom level
    zoom: u8,
}

impl TileRequest {
    /// Create a new tile request.
    ///
    /// # Arguments
    ///
    /// * `row` - Tile row (Y coordinate in Web Mercator grid)
    /// * `col` - Tile column (X coordinate in Web Mercator grid)
    /// * `zoom` - Zoom level (typically 12-22)
    pub fn new(row: u32, col: u32, zoom: u8) -> Self {
        Self { row, col, zoom }
    }

    /// Get the tile row.
    pub fn row(&self) -> u32 {
        self.row
    }

    /// Get the tile column.
    pub fn col(&self) -> u32 {
        self.col
    }

    /// Get the zoom level.
    pub fn zoom(&self) -> u8 {
        self.zoom
    }
}

impl From<crate::fuse::DdsFilename> for TileRequest {
    fn from(filename: crate::fuse::DdsFilename) -> Self {
        Self {
            row: filename.row,
            col: filename.col,
            zoom: filename.zoom,
        }
    }
}

impl From<&crate::fuse::DdsFilename> for TileRequest {
    fn from(filename: &crate::fuse::DdsFilename) -> Self {
        Self {
            row: filename.row,
            col: filename.col,
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
        let request = TileRequest::new(100000, 125184, 18);
        assert_eq!(request.row(), 100000);
        assert_eq!(request.col(), 125184);
        assert_eq!(request.zoom(), 18);
    }

    #[test]
    fn test_new_zero_coords() {
        let request = TileRequest::new(0, 0, 12);
        assert_eq!(request.row(), 0);
        assert_eq!(request.col(), 0);
        assert_eq!(request.zoom(), 12);
    }

    #[test]
    fn test_new_max_coords_zoom_18() {
        // At zoom 18, max coordinate is 2^18 - 1 = 262143
        let request = TileRequest::new(262143, 262143, 18);
        assert_eq!(request.row(), 262143);
        assert_eq!(request.col(), 262143);
        assert_eq!(request.zoom(), 18);
    }

    #[test]
    fn test_clone() {
        let request = TileRequest::new(100000, 125184, 18);
        let cloned = request;
        assert_eq!(request, cloned);
    }

    #[test]
    fn test_copy() {
        let request = TileRequest::new(100000, 125184, 18);
        let copied = request;
        assert_eq!(request.row(), copied.row());
    }

    #[test]
    fn test_equality() {
        let request1 = TileRequest::new(100000, 125184, 18);
        let request2 = TileRequest::new(100000, 125184, 18);
        let request3 = TileRequest::new(100001, 125184, 18);

        assert_eq!(request1, request2);
        assert_ne!(request1, request3);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TileRequest::new(100000, 125184, 18));
        set.insert(TileRequest::new(100000, 125184, 18));
        set.insert(TileRequest::new(100001, 125184, 18));

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_debug() {
        let request = TileRequest::new(100000, 125184, 18);
        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("100000"));
        assert!(debug_str.contains("125184"));
        assert!(debug_str.contains("18"));
    }

    #[test]
    fn test_from_dds_filename() {
        let filename = DdsFilename {
            row: 100000,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };
        let request: TileRequest = filename.into();
        assert_eq!(request.row(), 100000);
        assert_eq!(request.col(), 125184);
        assert_eq!(request.zoom(), 18);
    }

    #[test]
    fn test_from_dds_filename_ref() {
        let filename = DdsFilename {
            row: 100000,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };
        let request: TileRequest = (&filename).into();
        assert_eq!(request.row(), 100000);
        assert_eq!(request.col(), 125184);
        assert_eq!(request.zoom(), 18);
    }
}
