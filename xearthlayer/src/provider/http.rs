//! HTTP client abstraction for testability

use super::types::ProviderError;

/// Trait for HTTP client operations.
///
/// This abstraction allows for dependency injection and easier testing
/// by enabling mock HTTP clients in tests.
pub trait HttpClient: Send + Sync {
    /// Performs an HTTP GET request.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to request
    ///
    /// # Returns
    ///
    /// The response body as bytes or an error.
    fn get(&self, url: &str) -> Result<Vec<u8>, ProviderError>;

    /// Performs an HTTP POST request with JSON body.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to request
    /// * `json_body` - JSON body as a string
    ///
    /// # Returns
    ///
    /// The response body as bytes or an error.
    fn post_json(&self, url: &str, json_body: &str) -> Result<Vec<u8>, ProviderError>;
}

/// Real HTTP client implementation using reqwest.
pub struct ReqwestClient {
    client: reqwest::blocking::Client,
}

/// Default User-Agent string for HTTP requests.
/// Required by some tile servers (e.g., Google) that reject requests without a User-Agent.
const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

impl ReqwestClient {
    /// Creates a new ReqwestClient with default configuration.
    pub fn new() -> Result<Self, ProviderError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .map_err(|e| {
                ProviderError::HttpError(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client })
    }

    /// Creates a new ReqwestClient with custom timeout.
    pub fn with_timeout(timeout_secs: u64) -> Result<Self, ProviderError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .map_err(|e| {
                ProviderError::HttpError(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client })
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default HTTP client")
    }
}

impl HttpClient for ReqwestClient {
    fn get(&self, url: &str) -> Result<Vec<u8>, ProviderError> {
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|e| ProviderError::HttpError(format!("Request failed: {}", e)))?;

        // Check HTTP status
        if !response.status().is_success() {
            return Err(ProviderError::HttpError(format!(
                "HTTP {} from {}",
                response.status(),
                url
            )));
        }

        // Read response body
        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| ProviderError::HttpError(format!("Failed to read response: {}", e)))
    }

    fn post_json(&self, url: &str, json_body: &str) -> Result<Vec<u8>, ProviderError> {
        let response = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(json_body.to_string())
            .send()
            .map_err(|e| ProviderError::HttpError(format!("POST request failed: {}", e)))?;

        // Check HTTP status
        if !response.status().is_success() {
            return Err(ProviderError::HttpError(format!(
                "HTTP {} from POST {}",
                response.status(),
                url
            )));
        }

        // Read response body
        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| ProviderError::HttpError(format!("Failed to read response: {}", e)))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Mock HTTP client for testing
    pub struct MockHttpClient {
        pub response: Result<Vec<u8>, ProviderError>,
    }

    impl HttpClient for MockHttpClient {
        fn get(&self, _url: &str) -> Result<Vec<u8>, ProviderError> {
            self.response.clone()
        }

        fn post_json(&self, _url: &str, _json_body: &str) -> Result<Vec<u8>, ProviderError> {
            self.response.clone()
        }
    }

    #[test]
    fn test_mock_client_success() {
        let mock = MockHttpClient {
            response: Ok(vec![1, 2, 3, 4]),
        };

        let result = mock.get("http://example.com");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_mock_client_error() {
        let mock = MockHttpClient {
            response: Err(ProviderError::HttpError("Test error".to_string())),
        };

        let result = mock.get("http://example.com");
        assert!(result.is_err());
    }
}
