//! Adapters for connecting pipeline traits to existing implementations.
//!
//! This module provides adapter types that bridge the pipeline's abstract traits
//! to the concrete implementations in other modules. This follows the Adapter
//! pattern, allowing the pipeline to work with existing code without modification.
//!
//! # Adapters
//!
//! - [`ProviderAdapter`] - Adapts sync `Provider` to `ChunkProvider` (legacy)
//! - [`AsyncProviderAdapter`] - Adapts `AsyncProvider` to `ChunkProvider` (preferred)
//! - [`TextureEncoderAdapter`] - Adapts `TextureEncoder` to `TextureEncoderAsync`
//! - [`MemoryCacheAdapter`] - Adapts `cache::MemoryCache` to pipeline `MemoryCache`
//! - [`DiskCacheAdapter`] - Adapts disk cache operations to pipeline `DiskCache`
//! - [`NullDiskCache`] - No-op disk cache for testing
//! - [`ParallelDiskCache`] - High-performance parallel disk cache for SSDs
//! - [`BatchedDiskCache`] - Batched disk cache with existence pre-checking

mod disk_cache;
mod disk_cache_parallel;
mod memory_cache;
mod provider;
mod texture_encoder;

pub use disk_cache::{DiskCacheAdapter, NullDiskCache};
pub use disk_cache_parallel::{BatchedDiskCache, ParallelDiskCache};
pub use memory_cache::MemoryCacheAdapter;
pub use provider::{AsyncProviderAdapter, ProviderAdapter};
pub use texture_encoder::TextureEncoderAdapter;
