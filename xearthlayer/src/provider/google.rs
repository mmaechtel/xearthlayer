//! Google Maps satellite imagery provider.
//!
//! Uses Google Maps Platform API with proper authentication via API key.
//! Requires users to have their own Google Cloud Platform account and
//! Maps API key with Maps Static API or Map Tiles API enabled.
//!
//! # API Endpoints
//!
//! Google Maps provides tiles via:
//! - Map Tiles API: `https://tile.googleapis.com/v1/2dtiles/{z}/{x}/{y}?key={API_KEY}`
//! - Legacy endpoint: `https://mt{server}.googleapis.com/vt?lyrs=s&x={x}&y={y}&z={z}&key={API_KEY}`
//!
//! # Coordinate System
//!
//! Google Maps uses standard Web Mercator XYZ tile coordinates:
//! - X: Column (0 to 2^zoom - 1, west to east)
//! - Y: Row (0 to 2^zoom - 1, north to south)
//! - Z: Zoom level (0 to 22)
//!
//! This differs from Bing's quadkey system but maps directly to our
//! tile coordinates.

use crate::provider::{HttpClient, Provider, ProviderError};

/// Google Maps satellite imagery provider.
///
/// Requires a valid Google Maps Platform API key. Users must:
/// 1. Create a Google Cloud Platform project
/// 2. Enable Maps JavaScript API or Map Tiles API
/// 3. Create an API key with appropriate restrictions
/// 4. Provide the API key to this provider
///
/// # Pricing
///
/// Google Maps Platform is a paid service. Check current pricing at:
/// https://cloud.google.com/maps-platform/pricing
///
/// # Example
///
/// ```no_run
/// use xearthlayer::provider::{GoogleMapsProvider, ReqwestClient};
///
/// let client = ReqwestClient::new().unwrap();
/// let provider = GoogleMapsProvider::new(client, "YOUR_API_KEY".to_string());
/// // Use provider with TileOrchestrator...
/// ```
pub struct GoogleMapsProvider<C: HttpClient> {
    http_client: C,
    api_key: String,
    style: String,
    use_legacy_endpoint: bool,
}

impl<C: HttpClient> GoogleMapsProvider<C> {
    /// Creates a new Google Maps provider with the given API key.
    ///
    /// Uses the modern Map Tiles API endpoint by default.
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client for making requests
    /// * `api_key` - Valid Google Maps Platform API key
    pub fn new(http_client: C, api_key: String) -> Self {
        Self {
            http_client,
            api_key,
            style: "satellite".to_string(),
            use_legacy_endpoint: false,
        }
    }

    /// Creates a provider using the legacy tile endpoint.
    ///
    /// The legacy endpoint may have better compatibility but might
    /// be deprecated in the future.
    pub fn with_legacy_endpoint(http_client: C, api_key: String) -> Self {
        Self {
            http_client,
            api_key,
            style: "s".to_string(), // "s" = satellite in legacy API
            use_legacy_endpoint: true,
        }
    }

    /// Builds the tile URL for the given coordinates.
    ///
    /// Google Maps uses standard XYZ coordinates (not quadkeys like Bing).
    /// The row/col from our coordinate system map directly to y/x in Google's API.
    fn build_url(&self, row: u32, col: u32, zoom: u8) -> String {
        if self.use_legacy_endpoint {
            // Legacy endpoint with load balancing across servers
            let server = (row + col) % 4; // Distribute across mt0-mt3
            format!(
                "https://mt{}.googleapis.com/vt?lyrs={}&x={}&y={}&z={}&key={}",
                server, self.style, col, row, zoom, self.api_key
            )
        } else {
            // Modern Map Tiles API
            format!(
                "https://tile.googleapis.com/v1/2dtiles/{}/{}/{}?key={}",
                zoom, col, row, self.api_key
            )
        }
    }
}

impl<C: HttpClient> Provider for GoogleMapsProvider<C> {
    fn download_chunk(&self, row: u32, col: u32, zoom: u8) -> Result<Vec<u8>, ProviderError> {
        if !self.supports_zoom(zoom) {
            return Err(ProviderError::UnsupportedZoom(zoom));
        }

        let url = self.build_url(row, col, zoom);
        self.http_client.get(&url)
    }

    fn name(&self) -> &str {
        "Google Maps"
    }

    fn min_zoom(&self) -> u8 {
        0
    }

    fn max_zoom(&self) -> u8 {
        22
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockHttpClient;

    #[test]
    fn test_provider_name() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_key".to_string());
        assert_eq!(provider.name(), "Google Maps");
    }

    #[test]
    fn test_zoom_range() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_key".to_string());
        assert_eq!(provider.min_zoom(), 0);
        assert_eq!(provider.max_zoom(), 22);
    }

    #[test]
    fn test_supports_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_key".to_string());
        assert!(provider.supports_zoom(0));
        assert!(provider.supports_zoom(15));
        assert!(provider.supports_zoom(22));
        assert!(!provider.supports_zoom(23));
    }

    #[test]
    fn test_modern_url_construction() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_api_key".to_string());

        let url = provider.build_url(100, 200, 10);
        assert_eq!(
            url,
            "https://tile.googleapis.com/v1/2dtiles/10/200/100?key=test_api_key"
        );
    }

    #[test]
    fn test_legacy_url_construction() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider =
            GoogleMapsProvider::with_legacy_endpoint(mock_client, "test_api_key".to_string());

        let url = provider.build_url(100, 200, 10);
        // Server should be (100 + 200) % 4 = 0
        assert_eq!(
            url,
            "https://mt0.googleapis.com/vt?lyrs=s&x=200&y=100&z=10&key=test_api_key"
        );
    }

    #[test]
    fn test_legacy_url_server_distribution() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider =
            GoogleMapsProvider::with_legacy_endpoint(mock_client, "test_api_key".to_string());

        // Test different server selections
        assert!(provider.build_url(0, 0, 10).contains("mt0."));
        assert!(provider.build_url(0, 1, 10).contains("mt1."));
        assert!(provider.build_url(1, 1, 10).contains("mt2."));
        assert!(provider.build_url(1, 2, 10).contains("mt3."));
        assert!(provider.build_url(0, 4, 10).contains("mt0.")); // Wraps around
    }

    #[test]
    fn test_download_chunk_success() {
        let mock_data = vec![1, 2, 3, 4];
        let mock_client = MockHttpClient {
            response: Ok(mock_data.clone()),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_key".to_string());

        let result = provider.download_chunk(100, 200, 10);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), mock_data);
    }

    #[test]
    fn test_download_chunk_http_error() {
        let mock_client = MockHttpClient {
            response: Err(ProviderError::HttpError("Network error".to_string())),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_key".to_string());

        let result = provider.download_chunk(100, 200, 10);
        assert!(result.is_err());
        match result {
            Err(ProviderError::HttpError(msg)) => assert_eq!(msg, "Network error"),
            _ => panic!("Expected HttpError"),
        }
    }

    #[test]
    fn test_download_chunk_unsupported_zoom() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider = GoogleMapsProvider::new(mock_client, "test_key".to_string());

        let result = provider.download_chunk(100, 200, 23); // Beyond max zoom
        assert!(result.is_err());
        match result {
            Err(ProviderError::UnsupportedZoom(zoom)) => assert_eq!(zoom, 23),
            _ => panic!("Expected UnsupportedZoom error"),
        }
    }

    #[test]
    fn test_api_key_included_in_url() {
        let mock_client = MockHttpClient {
            response: Ok(vec![]),
        };
        let provider = GoogleMapsProvider::new(mock_client, "secret_key_123".to_string());

        let url = provider.build_url(10, 20, 5);
        assert!(url.contains("key=secret_key_123"));
    }
}
