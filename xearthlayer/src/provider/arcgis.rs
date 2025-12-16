//! ArcGIS World Imagery provider.
//!
//! Provides access to Esri's World Imagery basemap, which offers high-resolution
//! satellite and aerial imagery with global coverage.
//!
//! # URL Pattern
//!
//! `https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}`
//!
//! - Uses standard XYZ tile coordinates (TMS-style with y=row, x=col)
//! - No authentication required for the public tier
//! - Free for non-commercial and limited commercial use
//!
//! # Coverage
//!
//! - Global coverage with varying resolution
//! - Higher resolution in populated areas and points of interest
//! - Continuously updated with newer imagery
//!
//! # Coordinate System
//!
//! Uses standard Web Mercator XYZ tile coordinates:
//! - X: Column (0 to 2^zoom - 1, west to east)
//! - Y: Row (0 to 2^zoom - 1, north to south)
//! - Z: Zoom level (0 to 19)
//!
//! # Terms of Use
//!
//! The World Imagery basemap is provided by Esri and is subject to their
//! terms of use. See: <https://www.esri.com/en-us/legal/terms/full-master-agreement>

use crate::provider::{AsyncHttpClient, AsyncProvider, HttpClient, Provider, ProviderError};

/// Base URL for ArcGIS World Imagery tiles.
const ARCGIS_BASE_URL: &str =
    "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile";

/// Minimum zoom level supported by ArcGIS World Imagery.
const MIN_ZOOM: u8 = 0;

/// Maximum zoom level supported by ArcGIS World Imagery.
/// ArcGIS provides imagery up to zoom level 19 in most areas.
const MAX_ZOOM: u8 = 19;

/// ArcGIS World Imagery satellite provider.
///
/// Provides access to Esri's global satellite and aerial imagery basemap.
/// No API key or authentication required for the public tier.
///
/// # Example
///
/// ```ignore
/// use xearthlayer::provider::{ArcGisProvider, ReqwestClient};
///
/// let client = ReqwestClient::new().unwrap();
/// let provider = ArcGisProvider::new(client);
/// // Use provider with TileOrchestrator...
/// ```
///
/// # Coverage
///
/// Global coverage with varying resolution. Higher zoom levels (17-19) may
/// not be available in all regions.
pub struct ArcGisProvider<C: HttpClient> {
    http_client: C,
}

impl<C: HttpClient> ArcGisProvider<C> {
    /// Creates a new ArcGIS World Imagery provider.
    ///
    /// No API key or authentication is required for the public tier.
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client for making requests
    pub fn new(http_client: C) -> Self {
        Self { http_client }
    }

    /// Builds the tile URL for the given coordinates.
    ///
    /// ArcGIS uses the pattern: `{base}/tile/{z}/{y}/{x}`
    fn build_url(&self, row: u32, col: u32, zoom: u8) -> String {
        format!("{}/{}/{}/{}", ARCGIS_BASE_URL, zoom, row, col)
    }
}

impl<C: HttpClient> Provider for ArcGisProvider<C> {
    fn download_chunk(&self, row: u32, col: u32, zoom: u8) -> Result<Vec<u8>, ProviderError> {
        if !self.supports_zoom(zoom) {
            return Err(ProviderError::UnsupportedZoom(zoom));
        }

        let url = self.build_url(row, col, zoom);
        self.http_client.get(&url)
    }

    fn name(&self) -> &str {
        "ArcGIS"
    }

    fn min_zoom(&self) -> u8 {
        MIN_ZOOM
    }

    fn max_zoom(&self) -> u8 {
        MAX_ZOOM
    }
}

/// Async ArcGIS World Imagery satellite provider.
///
/// Provides access to Esri's global satellite and aerial imagery basemap,
/// with non-blocking I/O. This is the preferred provider for high-throughput
/// scenarios.
pub struct AsyncArcGisProvider<C: AsyncHttpClient> {
    http_client: C,
}

impl<C: AsyncHttpClient> AsyncArcGisProvider<C> {
    /// Creates a new async ArcGIS World Imagery provider.
    ///
    /// No API key or authentication is required for the public tier.
    pub fn new(http_client: C) -> Self {
        Self { http_client }
    }

    /// Builds the tile URL for the given coordinates.
    fn build_url(&self, row: u32, col: u32, zoom: u8) -> String {
        format!("{}/{}/{}/{}", ARCGIS_BASE_URL, zoom, row, col)
    }
}

impl<C: AsyncHttpClient> AsyncProvider for AsyncArcGisProvider<C> {
    async fn download_chunk(&self, row: u32, col: u32, zoom: u8) -> Result<Vec<u8>, ProviderError> {
        if !self.supports_zoom(zoom) {
            return Err(ProviderError::UnsupportedZoom(zoom));
        }

        let url = self.build_url(row, col, zoom);
        self.http_client.get(&url).await
    }

    fn name(&self) -> &str {
        "ArcGIS"
    }

    fn min_zoom(&self) -> u8 {
        MIN_ZOOM
    }

    fn max_zoom(&self) -> u8 {
        MAX_ZOOM
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{MockAsyncHttpClient, MockHttpClient};

    fn sample_jpeg_response() -> Vec<u8> {
        // Minimal valid JPEG header
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46]
    }

    #[test]
    fn test_provider_name() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);
        assert_eq!(provider.name(), "ArcGIS");
    }

    #[test]
    fn test_zoom_range() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);
        assert_eq!(provider.min_zoom(), 0);
        assert_eq!(provider.max_zoom(), 19);
    }

    #[test]
    fn test_supports_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);
        assert!(provider.supports_zoom(0));
        assert!(provider.supports_zoom(10));
        assert!(provider.supports_zoom(19));
        assert!(!provider.supports_zoom(20));
        assert!(!provider.supports_zoom(22));
    }

    #[test]
    fn test_url_construction() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);

        let url = provider.build_url(100, 200, 15);
        assert_eq!(
            url,
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/15/100/200"
        );
    }

    #[test]
    fn test_url_construction_zoom_0() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);

        let url = provider.build_url(0, 0, 0);
        assert_eq!(
            url,
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/0/0/0"
        );
    }

    #[test]
    fn test_url_construction_max_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);

        let url = provider.build_url(262143, 524287, 19);
        assert_eq!(
            url,
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/19/262143/524287"
        );
    }

    #[test]
    fn test_download_chunk_unsupported_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 20); // Beyond max zoom
        assert!(result.is_err());
        match result {
            Err(ProviderError::UnsupportedZoom(zoom)) => assert_eq!(zoom, 20),
            _ => panic!("Expected UnsupportedZoom error"),
        }
    }

    #[test]
    fn test_download_chunk_success() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = ArcGisProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 15);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), sample_jpeg_response());
    }

    #[test]
    fn test_download_chunk_network_error() {
        let mock_client = MockHttpClient {
            response: Err(ProviderError::HttpError("Connection refused".to_string())),
        };
        let provider = ArcGisProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 15);
        assert!(result.is_err());
        match result {
            Err(ProviderError::HttpError(msg)) => {
                assert!(msg.contains("Connection refused"));
            }
            _ => panic!("Expected HttpError"),
        }
    }

    // Async provider tests

    #[tokio::test]
    async fn test_async_provider_name() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncArcGisProvider::new(mock_client);
        assert_eq!(provider.name(), "ArcGIS");
    }

    #[tokio::test]
    async fn test_async_zoom_range() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncArcGisProvider::new(mock_client);
        assert_eq!(provider.min_zoom(), 0);
        assert_eq!(provider.max_zoom(), 19);
    }

    #[tokio::test]
    async fn test_async_supports_zoom() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncArcGisProvider::new(mock_client);
        assert!(provider.supports_zoom(0));
        assert!(provider.supports_zoom(10));
        assert!(provider.supports_zoom(19));
        assert!(!provider.supports_zoom(20));
    }

    #[tokio::test]
    async fn test_async_download_chunk_unsupported_zoom() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncArcGisProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 20).await;
        assert!(result.is_err());
        match result {
            Err(ProviderError::UnsupportedZoom(zoom)) => assert_eq!(zoom, 20),
            _ => panic!("Expected UnsupportedZoom error"),
        }
    }

    #[tokio::test]
    async fn test_async_download_chunk_success() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncArcGisProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 15).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), sample_jpeg_response());
    }

    #[tokio::test]
    async fn test_async_download_chunk_network_error() {
        let mock_client = MockAsyncHttpClient {
            response: Err(ProviderError::HttpError("Connection refused".to_string())),
        };
        let provider = AsyncArcGisProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 15).await;
        assert!(result.is_err());
        match result {
            Err(ProviderError::HttpError(msg)) => {
                assert!(msg.contains("Connection refused"));
            }
            _ => panic!("Expected HttpError"),
        }
    }
}
