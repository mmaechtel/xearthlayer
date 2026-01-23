//! Domain-specific cache clients.
//!
//! These clients wrap the generic `Cache` trait with domain-specific
//! key translation and optional metrics injection.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────┐     ┌─────────────────────┐
//! │  TileCacheClient    │     │  ChunkCacheClient   │
//! │                     │     │                     │
//! │ TileCoord → key     │     │ ChunkCoord → key    │
//! │ Metrics injection   │     │ Metrics injection   │
//! └──────────┬──────────┘     └──────────┬──────────┘
//!            │                           │
//!            ▼                           ▼
//! ┌─────────────────────────────────────────────────┐
//! │              Arc<dyn Cache>                     │
//! │                                                 │
//! │  Generic key-value store (string → Vec<u8>)    │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! # Key Formats
//!
//! - Tiles: `"tile:{zoom}:{row}:{col}"` (e.g., `"tile:15:12754:5279"`)
//! - Chunks: `"chunk:{zoom}:{tile_row}:{tile_col}:{chunk_row}:{chunk_col}"`
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::cache::clients::TileCacheClient;
//! use xearthlayer::coord::TileCoord;
//!
//! let tile_client = TileCacheClient::with_metrics(cache, metrics);
//!
//! let tile = TileCoord { row: 12754, col: 5279, zoom: 15 };
//! tile_client.set(&tile, dds_data).await;
//!
//! if let Some(data) = tile_client.get(&tile).await {
//!     // Cache hit
//! }
//! ```

mod chunk;
mod tile;

pub use chunk::ChunkCacheClient;
pub use tile::TileCacheClient;
