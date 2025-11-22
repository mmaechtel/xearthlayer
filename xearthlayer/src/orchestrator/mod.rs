//! Tile download orchestration
//!
//! Coordinates parallel downloading of 256 chunks per tile and assembles
//! them into complete 4096×4096 pixel images.

mod download;
mod types;

pub use download::TileOrchestrator;
pub use types::{ChunkResult, DownloadStats, OrchestratorError};
