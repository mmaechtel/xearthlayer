//! DDS filename parsing for X-Plane texture coordinates.
//!
//! Parses filenames in AutoOrtho/Ortho4XP Web Mercator format:
//! `{row}_{col}_{maptype}{zoom}.dds`
//!
//! Examples:
//! - `100000_125184_BI18.dds` (Europe: lat ~39.19°, lon ~-8.07°)
//! - `169840_253472_BI18.dds` (Australia: lat ~-46.91°, lon ~168.10°)
//!
//! The coordinates are unsigned Web Mercator tile indices at the specified zoom level.
//! - Row increases southward (equator ≈ 2^(zoom-1))
//! - Col increases eastward (prime meridian ≈ 2^(zoom-1))
//!
//! The map type identifier (e.g., "BI" for Bing, "GO" for Google) is captured
//! but currently ignored as we use the configured provider.

use regex::Regex;
use std::sync::OnceLock;

/// Parsed DDS filename containing Web Mercator tile coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsFilename {
    /// Tile row (Y coordinate in Web Mercator, increases southward)
    pub row: u32,
    /// Tile column (X coordinate in Web Mercator, increases eastward)
    pub col: u32,
    /// Zoom level (chunk zoom, typically 12-22)
    pub zoom: u8,
    /// Map type identifier (e.g., "BI" for Bing, "GO" for Google)
    pub map_type: String,
}

/// Error parsing DDS filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Filename doesn't match expected pattern
    InvalidPattern,
    /// Row coordinate is invalid
    InvalidRow(String),
    /// Column coordinate is invalid
    InvalidColumn(String),
    /// Zoom level is invalid
    InvalidZoom(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidPattern => write!(f, "Filename doesn't match DDS pattern"),
            ParseError::InvalidRow(s) => write!(f, "Invalid row coordinate: {}", s),
            ParseError::InvalidColumn(s) => write!(f, "Invalid column coordinate: {}", s),
            ParseError::InvalidZoom(s) => write!(f, "Invalid zoom level: {}", s),
        }
    }
}

impl std::error::Error for ParseError {}

/// Get the DDS filename regex pattern for AutoOrtho/Ortho4XP format.
///
/// Pattern: `<row>_<col>_<maptype><zoom>.dds`
/// Example: `100000_125184_BI18.dds`
///
/// We capture:
/// - Group 1: row (unsigned integer, e.g., "100000")
/// - Group 2: col (unsigned integer, e.g., "125184")
/// - Group 3: map type (letters, e.g., "BI")
/// - Group 4: zoom level (exactly 2 digits, e.g., "18")
fn dds_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        // Pattern breakdown:
        // (\d+)     - row (unsigned integer)
        // _         - separator
        // (\d+)     - column (unsigned integer)
        // _         - separator
        // ([A-Za-z]+) - map type (letters only)
        // (\d{2})   - zoom level (exactly 2 digits)
        // \.dds     - extension (case insensitive)
        Regex::new(r"(\d+)_(\d+)_([A-Za-z]+)(\d{2})\.dds$").unwrap()
    })
}

/// Parse a DDS filename to extract Web Mercator tile coordinates.
///
/// # Arguments
///
/// * `filename` - Filename to parse (e.g., "100000_125184_BI18.dds")
///
/// # Returns
///
/// Parsed coordinates or error if filename doesn't match pattern
///
/// # Examples
///
/// ```
/// use xearthlayer::fuse::parse_dds_filename;
///
/// // Europe tile
/// let coords = parse_dds_filename("100000_125184_BI18.dds").unwrap();
/// assert_eq!(coords.row, 100000);
/// assert_eq!(coords.col, 125184);
/// assert_eq!(coords.zoom, 18);
/// assert_eq!(coords.map_type, "BI");
///
/// // Australia tile
/// let coords = parse_dds_filename("169840_253472_BI18.dds").unwrap();
/// assert_eq!(coords.row, 169840);
/// assert_eq!(coords.col, 253472);
/// assert_eq!(coords.zoom, 18);
/// ```
pub fn parse_dds_filename(filename: &str) -> Result<DdsFilename, ParseError> {
    let pattern = dds_pattern();

    let captures = pattern
        .captures(filename)
        .ok_or(ParseError::InvalidPattern)?;

    // Parse row (unsigned)
    let row_str = captures.get(1).unwrap().as_str();
    let row = row_str
        .parse::<u32>()
        .map_err(|_| ParseError::InvalidRow(row_str.to_string()))?;

    // Parse column (unsigned)
    let col_str = captures.get(2).unwrap().as_str();
    let col = col_str
        .parse::<u32>()
        .map_err(|_| ParseError::InvalidColumn(col_str.to_string()))?;

    // Parse map type
    let map_type = captures.get(3).unwrap().as_str().to_uppercase();

    // Parse zoom
    let zoom_str = captures.get(4).unwrap().as_str();
    let zoom = zoom_str
        .parse::<u8>()
        .map_err(|_| ParseError::InvalidZoom(zoom_str.to_string()))?;

    Ok(DdsFilename {
        row,
        col,
        zoom,
        map_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // AutoOrtho format tests (primary format)
    // ========================================================================

    #[test]
    fn test_parse_europe_tile() {
        // From AutoOrtho Europe pack: 100000_125184_BI18.ter -> ../textures/100000_125184_BI18.dds
        let result = parse_dds_filename("100000_125184_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 100000);
        assert_eq!(coords.col, 125184);
        assert_eq!(coords.zoom, 18);
        assert_eq!(coords.map_type, "BI");
    }

    #[test]
    fn test_parse_australia_tile() {
        // From AutoOrtho Australia pack: 169840_253472_BI18.dds
        let result = parse_dds_filename("169840_253472_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 169840);
        assert_eq!(coords.col, 253472);
        assert_eq!(coords.zoom, 18);
        assert_eq!(coords.map_type, "BI");
    }

    #[test]
    fn test_parse_asia_tile() {
        // From AutoOrtho Asia pack: 100000_222560_BI18.dds
        let result = parse_dds_filename("100000_222560_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 100000);
        assert_eq!(coords.col, 222560);
        assert_eq!(coords.zoom, 18);
        assert_eq!(coords.map_type, "BI");
    }

    #[test]
    fn test_parse_south_america_tile() {
        // From AutoOrtho South America pack: 116208_75824_BI18.dds
        let result = parse_dds_filename("116208_75824_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 116208);
        assert_eq!(coords.col, 75824);
        assert_eq!(coords.zoom, 18);
        assert_eq!(coords.map_type, "BI");
    }

    #[test]
    fn test_parse_lower_zoom() {
        // Lower zoom level (16)
        let result = parse_dds_filename("24832_12416_BI16.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 24832);
        assert_eq!(coords.col, 12416);
        assert_eq!(coords.zoom, 16);
        assert_eq!(coords.map_type, "BI");
    }

    #[test]
    fn test_parse_google_provider() {
        // Google Maps provider
        let result = parse_dds_filename("100000_125184_GO18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 100000);
        assert_eq!(coords.col, 125184);
        assert_eq!(coords.zoom, 18);
        assert_eq!(coords.map_type, "GO");
    }

    #[test]
    fn test_parse_with_path() {
        // Should work with path prefix (FUSE will call with just filename typically)
        let result = parse_dds_filename("/path/to/textures/100000_125184_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 100000);
        assert_eq!(coords.col, 125184);
        assert_eq!(coords.zoom, 18);
    }

    #[test]
    fn test_parse_relative_path() {
        // Relative path as in .ter files: ../textures/100000_125184_BI18.dds
        let result = parse_dds_filename("../textures/100000_125184_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 100000);
        assert_eq!(coords.col, 125184);
        assert_eq!(coords.zoom, 18);
    }

    #[test]
    fn test_parse_various_zoom_levels() {
        // Test zoom levels 12-22
        for zoom in 12..=22 {
            let filename = format!("100000_125184_BI{:02}.dds", zoom);
            let result = parse_dds_filename(&filename);
            assert!(result.is_ok(), "Failed to parse zoom {}", zoom);
            assert_eq!(result.unwrap().zoom, zoom);
        }
    }

    #[test]
    fn test_parse_lowercase_map_type() {
        // Map type should be normalized to uppercase
        let result = parse_dds_filename("100000_125184_bi18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.map_type, "BI");
    }

    #[test]
    fn test_parse_mixed_case_map_type() {
        let result = parse_dds_filename("100000_125184_Bi18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.map_type, "BI");
    }

    // ========================================================================
    // Edge cases and boundary tests
    // ========================================================================

    #[test]
    fn test_parse_zero_coordinates() {
        let result = parse_dds_filename("0_0_BI12.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 0);
        assert_eq!(coords.col, 0);
        assert_eq!(coords.zoom, 12);
    }

    #[test]
    fn test_parse_max_zoom_18_coordinates() {
        // At zoom 18, max coordinate is 2^18 - 1 = 262143
        let result = parse_dds_filename("262143_262143_BI18.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 262143);
        assert_eq!(coords.col, 262143);
        assert_eq!(coords.zoom, 18);
    }

    #[test]
    fn test_parse_large_coordinates() {
        // At zoom 22, max coordinate is 2^22 - 1 = 4194303
        let result = parse_dds_filename("4194303_4194303_GO22.dds");
        assert!(result.is_ok());
        let coords = result.unwrap();
        assert_eq!(coords.row, 4194303);
        assert_eq!(coords.col, 4194303);
        assert_eq!(coords.zoom, 22);
    }

    // ========================================================================
    // Invalid pattern tests
    // ========================================================================

    #[test]
    fn test_parse_invalid_signed_coordinates() {
        // Old format with signed coordinates should not match
        let result = parse_dds_filename("+37-123_BI16.dds");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_missing_underscore() {
        let result = parse_dds_filename("100000125184_BI18.dds");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_missing_map_type() {
        let result = parse_dds_filename("100000_125184_18.dds");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_wrong_extension() {
        let result = parse_dds_filename("100000_125184_BI18.jpg");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_single_digit_zoom() {
        let result = parse_dds_filename("100000_125184_BI8.dds");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_three_digit_zoom() {
        let result = parse_dds_filename("100000_125184_BI180.dds");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_numeric_map_type() {
        let result = parse_dds_filename("100000_125184_1218.dds");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_empty_filename() {
        let result = parse_dds_filename("");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    #[test]
    fn test_parse_invalid_non_dds_file() {
        let result = parse_dds_filename("readme.txt");
        assert!(matches!(result, Err(ParseError::InvalidPattern)));
    }

    // ========================================================================
    // Overflow tests
    // ========================================================================

    #[test]
    fn test_parse_row_overflow() {
        // Test row that exceeds u32 range
        let result = parse_dds_filename("9999999999999_125184_BI18.dds");
        assert!(matches!(result, Err(ParseError::InvalidRow(_))));
    }

    #[test]
    fn test_parse_col_overflow() {
        // Test column that exceeds u32 range
        let result = parse_dds_filename("100000_9999999999999_BI18.dds");
        assert!(matches!(result, Err(ParseError::InvalidColumn(_))));
    }

    // ========================================================================
    // Error display tests
    // ========================================================================

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::InvalidPattern;
        assert_eq!(err.to_string(), "Filename doesn't match DDS pattern");

        let err = ParseError::InvalidRow("9999999999999".to_string());
        assert_eq!(err.to_string(), "Invalid row coordinate: 9999999999999");

        let err = ParseError::InvalidColumn("9999999999999".to_string());
        assert_eq!(err.to_string(), "Invalid column coordinate: 9999999999999");

        let err = ParseError::InvalidZoom("99".to_string());
        assert_eq!(err.to_string(), "Invalid zoom level: 99");
    }

    // ========================================================================
    // DdsFilename struct tests
    // ========================================================================

    #[test]
    fn test_dds_filename_equality() {
        let coords1 = DdsFilename {
            row: 100000,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };
        let coords2 = DdsFilename {
            row: 100000,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };
        let coords3 = DdsFilename {
            row: 100001,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };

        assert_eq!(coords1, coords2);
        assert_ne!(coords1, coords3);
    }

    #[test]
    fn test_dds_filename_clone() {
        let coords = DdsFilename {
            row: 100000,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };
        let cloned = coords.clone();
        assert_eq!(coords, cloned);
    }

    #[test]
    fn test_dds_filename_debug() {
        let coords = DdsFilename {
            row: 100000,
            col: 125184,
            zoom: 18,
            map_type: "BI".to_string(),
        };
        let debug_str = format!("{:?}", coords);
        assert!(debug_str.contains("100000"));
        assert!(debug_str.contains("125184"));
        assert!(debug_str.contains("18"));
        assert!(debug_str.contains("BI"));
    }
}
