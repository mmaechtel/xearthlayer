//! Task implementations for the executor framework.
//!
//! This module provides task implementations for the DDS generation pipeline,
//! containing the core business logic for tile processing.
//!
//! # Tasks
//!
//! - [`DownloadChunksTask`] - Downloads satellite imagery chunks (Network)
//! - [`AssembleImageTask`] - Assembles chunks into a full image (CPU)
//! - [`EncodeDdsTask`] - Encodes image to DDS format (CPU)
//! - [`CacheWriteTask`] - Writes DDS data to memory cache (CPU)
//!
//! # Data Flow
//!
//! Tasks pass data between each other via `TaskOutput`:
//!
//! ```text
//! DownloadChunks → "chunks": ChunkResults
//! AssembleImage  → "image": RgbaImage
//! EncodeDds      → "dds_data": Vec<u8>
//! CacheWrite     → (reads "dds_data")
//! ```
//!
//! # Resource Types
//!
//! Each task declares its resource type for the executor's resource pools:
//! - `DownloadChunksTask`: `ResourceType::Network`
//! - `AssembleImageTask`: `ResourceType::CPU`
//! - `EncodeDdsTask`: `ResourceType::CPU`
//! - `CacheWriteTask`: `ResourceType::CPU`

mod assemble_image;
mod cache_write;
mod download_chunks;
mod encode_dds;

// Task types
pub use assemble_image::AssembleImageTask;
pub use cache_write::CacheWriteTask;
pub use download_chunks::DownloadChunksTask;
pub use encode_dds::EncodeDdsTask;

// Output keys and helpers
pub use assemble_image::{get_image_from_output, OUTPUT_KEY_IMAGE};
pub use download_chunks::{get_chunks_from_output, OUTPUT_KEY_CHUNKS};
pub use encode_dds::{get_dds_data_from_output, OUTPUT_KEY_DDS_DATA};
