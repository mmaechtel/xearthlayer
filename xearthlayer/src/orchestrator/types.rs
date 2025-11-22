//! Orchestrator types and errors

use crate::coord::TileCoord;
use crate::provider::ProviderError;
use std::fmt;

/// Errors that can occur during tile orchestration.
#[derive(Debug)]
pub enum OrchestratorError {
    /// Timeout exceeded while downloading tile
    Timeout {
        tile: TileCoord,
        elapsed_secs: u64,
        chunks_downloaded: usize,
        chunks_total: usize,
    },
    /// Too many chunks failed to download
    TooManyFailures {
        tile: TileCoord,
        successful: usize,
        failed: usize,
        min_required: usize,
    },
    /// Image assembly failed
    ImageError(String),
    /// Provider error
    ProviderError(ProviderError),
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrchestratorError::Timeout {
                tile,
                elapsed_secs,
                chunks_downloaded,
                chunks_total,
            } => write!(
                f,
                "Timeout after {}s downloading tile {:?}: {}/{} chunks completed",
                elapsed_secs, tile, chunks_downloaded, chunks_total
            ),
            OrchestratorError::TooManyFailures {
                tile,
                successful,
                failed,
                min_required,
            } => write!(
                f,
                "Too many failures for tile {:?}: {} successful, {} failed (min required: {})",
                tile, successful, failed, min_required
            ),
            OrchestratorError::ImageError(msg) => write!(f, "Image processing error: {}", msg),
            OrchestratorError::ProviderError(e) => write!(f, "Provider error: {}", e),
        }
    }
}

impl std::error::Error for OrchestratorError {}

impl From<ProviderError> for OrchestratorError {
    fn from(e: ProviderError) -> Self {
        OrchestratorError::ProviderError(e)
    }
}

impl From<image::ImageError> for OrchestratorError {
    fn from(e: image::ImageError) -> Self {
        OrchestratorError::ImageError(format!("{}", e))
    }
}

/// Result of downloading a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkResult {
    /// Chunk row within tile (0-15)
    pub chunk_row: u8,
    /// Chunk column within tile (0-15)
    pub chunk_col: u8,
    /// Downloaded image data (JPEG bytes)
    pub data: Vec<u8>,
}

/// Statistics about a tile download operation.
#[derive(Debug, Clone)]
pub struct DownloadStats {
    /// Total number of chunks in tile (always 256)
    pub total_chunks: usize,
    /// Number of chunks successfully downloaded
    pub successful: usize,
    /// Number of chunks that failed
    pub failed: usize,
    /// Total time elapsed in seconds
    pub elapsed_secs: f64,
    /// Number of retry attempts made
    pub retries: usize,
}
