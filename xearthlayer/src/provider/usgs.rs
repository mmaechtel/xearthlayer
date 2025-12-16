//! USGS (United States Geological Survey) imagery provider.
//!
//! Provides free access to USGS orthoimagery via the National Map tile services.
//! Coverage is limited to the United States.
//!
//! # URL Pattern
//!
//! `https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}`
//!
//! - Uses standard XYZ tile coordinates (TMS-style with y=row, x=col)
//! - No authentication required
//! - Free for all uses
//!
//! # Coverage
//!
//! - United States only (continental US, Alaska, Hawaii, territories)
//! - Tiles outside US coverage will return errors or blank tiles
//!
//! # Coordinate System
//!
//! Uses standard Web Mercator XYZ tile coordinates:
//! - X: Column (0 to 2^zoom - 1, west to east)
//! - Y: Row (0 to 2^zoom - 1, north to south)
//! - Z: Zoom level (0 to 16)

use crate::provider::{AsyncHttpClient, AsyncProvider, HttpClient, Provider, ProviderError};

/// Base URL for USGS imagery tiles.
const USGS_BASE_URL: &str =
    "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile";

/// Minimum zoom level supported by USGS.
const MIN_ZOOM: u8 = 0;

/// Maximum zoom level supported by USGS.
/// USGS provides imagery up to zoom level 16.
const MAX_ZOOM: u8 = 16;

/// USGS satellite imagery provider.
///
/// Provides free access to USGS orthoimagery for the United States.
/// No API key or authentication required.
///
/// # Example
///
/// ```ignore
/// use xearthlayer::provider::{UsgsProvider, ReqwestClient};
///
/// let client = ReqwestClient::new().unwrap();
/// let provider = UsgsProvider::new(client);
/// // Use provider with TileOrchestrator...
/// ```
///
/// # Note
///
/// Coverage is limited to the United States. Requests for tiles outside
/// US territory may return errors or blank/placeholder tiles.
pub struct UsgsProvider<C: HttpClient> {
    http_client: C,
}

impl<C: HttpClient> UsgsProvider<C> {
    /// Creates a new USGS provider.
    ///
    /// No API key or authentication is required.
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client for making requests
    pub fn new(http_client: C) -> Self {
        Self { http_client }
    }

    /// Builds the tile URL for the given coordinates.
    ///
    /// USGS uses the pattern: `{base}/tile/{z}/{y}/{x}`
    fn build_url(&self, row: u32, col: u32, zoom: u8) -> String {
        format!("{}/{}/{}/{}", USGS_BASE_URL, zoom, row, col)
    }
}

impl<C: HttpClient> Provider for UsgsProvider<C> {
    fn download_chunk(&self, row: u32, col: u32, zoom: u8) -> Result<Vec<u8>, ProviderError> {
        if !self.supports_zoom(zoom) {
            return Err(ProviderError::UnsupportedZoom(zoom));
        }

        let url = self.build_url(row, col, zoom);
        self.http_client.get(&url)
    }

    fn name(&self) -> &str {
        "USGS"
    }

    fn min_zoom(&self) -> u8 {
        MIN_ZOOM
    }

    fn max_zoom(&self) -> u8 {
        MAX_ZOOM
    }
}

/// Async USGS satellite imagery provider.
///
/// Provides free access to USGS orthoimagery for the United States,
/// with non-blocking I/O. This is the preferred provider for high-throughput
/// scenarios.
pub struct AsyncUsgsProvider<C: AsyncHttpClient> {
    http_client: C,
}

impl<C: AsyncHttpClient> AsyncUsgsProvider<C> {
    /// Creates a new async USGS provider.
    ///
    /// No API key or authentication is required.
    pub fn new(http_client: C) -> Self {
        Self { http_client }
    }

    /// Builds the tile URL for the given coordinates.
    fn build_url(&self, row: u32, col: u32, zoom: u8) -> String {
        format!("{}/{}/{}/{}", USGS_BASE_URL, zoom, row, col)
    }
}

impl<C: AsyncHttpClient> AsyncProvider for AsyncUsgsProvider<C> {
    async fn download_chunk(&self, row: u32, col: u32, zoom: u8) -> Result<Vec<u8>, ProviderError> {
        if !self.supports_zoom(zoom) {
            return Err(ProviderError::UnsupportedZoom(zoom));
        }

        let url = self.build_url(row, col, zoom);
        self.http_client.get(&url).await
    }

    fn name(&self) -> &str {
        "USGS"
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
        let provider = UsgsProvider::new(mock_client);
        assert_eq!(provider.name(), "USGS");
    }

    #[test]
    fn test_zoom_range() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = UsgsProvider::new(mock_client);
        assert_eq!(provider.min_zoom(), 0);
        assert_eq!(provider.max_zoom(), 16);
    }

    #[test]
    fn test_supports_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = UsgsProvider::new(mock_client);
        assert!(provider.supports_zoom(0));
        assert!(provider.supports_zoom(10));
        assert!(provider.supports_zoom(16));
        assert!(!provider.supports_zoom(17));
        assert!(!provider.supports_zoom(22));
    }

    #[test]
    fn test_url_construction() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = UsgsProvider::new(mock_client);

        let url = provider.build_url(100, 200, 15);
        assert_eq!(
            url,
            "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/15/100/200"
        );
    }

    #[test]
    fn test_url_construction_zoom_0() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = UsgsProvider::new(mock_client);

        let url = provider.build_url(0, 0, 0);
        assert_eq!(
            url,
            "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/0/0/0"
        );
    }

    #[test]
    fn test_download_chunk_unsupported_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = UsgsProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 17); // Beyond max zoom
        assert!(result.is_err());
        match result {
            Err(ProviderError::UnsupportedZoom(zoom)) => assert_eq!(zoom, 17),
            _ => panic!("Expected UnsupportedZoom error"),
        }
    }

    #[test]
    fn test_download_chunk_success() {
        let mock_client = MockHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = UsgsProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 15);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), sample_jpeg_response());
    }

    #[test]
    fn test_download_chunk_network_error() {
        let mock_client = MockHttpClient {
            response: Err(ProviderError::HttpError("Connection refused".to_string())),
        };
        let provider = UsgsProvider::new(mock_client);

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
        let provider = AsyncUsgsProvider::new(mock_client);
        assert_eq!(provider.name(), "USGS");
    }

    #[tokio::test]
    async fn test_async_zoom_range() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncUsgsProvider::new(mock_client);
        assert_eq!(provider.min_zoom(), 0);
        assert_eq!(provider.max_zoom(), 16);
    }

    #[tokio::test]
    async fn test_async_supports_zoom() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncUsgsProvider::new(mock_client);
        assert!(provider.supports_zoom(0));
        assert!(provider.supports_zoom(10));
        assert!(provider.supports_zoom(16));
        assert!(!provider.supports_zoom(17));
    }

    #[tokio::test]
    async fn test_async_download_chunk_unsupported_zoom() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncUsgsProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 17).await;
        assert!(result.is_err());
        match result {
            Err(ProviderError::UnsupportedZoom(zoom)) => assert_eq!(zoom, 17),
            _ => panic!("Expected UnsupportedZoom error"),
        }
    }

    #[tokio::test]
    async fn test_async_download_chunk_success() {
        let mock_client = MockAsyncHttpClient {
            response: Ok(sample_jpeg_response()),
        };
        let provider = AsyncUsgsProvider::new(mock_client);

        let result = provider.download_chunk(100, 200, 15).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), sample_jpeg_response());
    }

    #[tokio::test]
    async fn test_async_download_chunk_network_error() {
        let mock_client = MockAsyncHttpClient {
            response: Err(ProviderError::HttpError("Connection refused".to_string())),
        };
        let provider = AsyncUsgsProvider::new(mock_client);

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
