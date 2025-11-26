//! TileGenerator trait for abstracting tile generation strategies.
//!
//! This module defines the `TileGenerator` trait which allows different
//! tile generation implementations to be used interchangeably, following
//! the Liskov Substitution Principle.
//!
//! # Example
//!
//! ```
//! use xearthlayer::tile::{TileGenerator, TileRequest};
//! use std::sync::Arc;
//!
//! fn process_tile(generator: &dyn TileGenerator, request: &TileRequest) {
//!     let expected_size = generator.expected_size();
//!     println!("Expected size: {} bytes", expected_size);
//!     // In real code: let data = generator.generate(request)?;
//! }
//! ```

use crate::tile::{TileGeneratorError, TileRequest};

/// Trait for tile generation strategies.
///
/// This trait abstracts the generation of texture tiles from geographic
/// coordinates. Implementations must be thread-safe (`Send + Sync`) to
/// support concurrent FUSE operations.
///
/// # Implementors
///
/// - [`DefaultTileGenerator`] - Downloads imagery and encodes to texture format
///
/// # Future Implementors
///
/// - `CachingTileGenerator` - Adds caching around another generator
/// - `MockTileGenerator` - For testing without real downloads
pub trait TileGenerator: Send + Sync {
    /// Generate a tile texture from the given request.
    ///
    /// # Arguments
    ///
    /// * `request` - Tile request containing coordinates and zoom level
    ///
    /// # Returns
    ///
    /// Complete texture file as bytes (including headers), or an error
    /// if generation fails.
    ///
    /// # Errors
    ///
    /// Returns `TileGeneratorError` if:
    /// - Coordinates are invalid
    /// - Download fails
    /// - Texture encoding fails
    fn generate(&self, request: &TileRequest) -> Result<Vec<u8>, TileGeneratorError>;

    /// Return the expected output size for a standard tile.
    ///
    /// This is used for FUSE file attribute reporting before the actual
    /// file is generated. Returns the expected size for a 4096×4096 tile.
    ///
    /// # Returns
    ///
    /// Expected file size in bytes.
    fn expected_size(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Mock generator for testing trait object behavior.
    struct MockTileGenerator {
        size: usize,
        data: Vec<u8>,
        should_fail: bool,
    }

    impl MockTileGenerator {
        fn new() -> Self {
            Self {
                size: 1024,
                data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                should_fail: false,
            }
        }

        fn with_failure() -> Self {
            Self {
                size: 1024,
                data: vec![],
                should_fail: true,
            }
        }
    }

    impl TileGenerator for MockTileGenerator {
        fn generate(&self, _request: &TileRequest) -> Result<Vec<u8>, TileGeneratorError> {
            if self.should_fail {
                Err(TileGeneratorError::DownloadFailed(
                    "mock failure".to_string(),
                ))
            } else {
                Ok(self.data.clone())
            }
        }

        fn expected_size(&self) -> usize {
            self.size
        }
    }

    #[test]
    fn test_trait_object_creation() {
        let generator: Arc<dyn TileGenerator> = Arc::new(MockTileGenerator::new());
        assert_eq!(generator.expected_size(), 1024);
    }

    #[test]
    fn test_trait_object_generate() {
        let generator: Arc<dyn TileGenerator> = Arc::new(MockTileGenerator::new());
        let request = TileRequest::new(100000, 125184, 18);

        let result = generator.generate(&request);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_trait_object_generate_failure() {
        let generator: Arc<dyn TileGenerator> = Arc::new(MockTileGenerator::with_failure());
        let request = TileRequest::new(100000, 125184, 18);

        let result = generator.generate(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn TileGenerator>();
    }

    #[test]
    fn test_multiple_trait_objects() {
        let generators: Vec<Arc<dyn TileGenerator>> = vec![
            Arc::new(MockTileGenerator {
                size: 100,
                data: vec![1, 2, 3],
                should_fail: false,
            }),
            Arc::new(MockTileGenerator {
                size: 200,
                data: vec![4, 5, 6],
                should_fail: false,
            }),
        ];

        assert_eq!(generators[0].expected_size(), 100);
        assert_eq!(generators[1].expected_size(), 200);
    }
}
