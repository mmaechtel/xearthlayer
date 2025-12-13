//! FUSE filesystem for on-demand DDS texture generation.
//!
//! Provides a virtual filesystem that intercepts X-Plane texture reads
//! and generates satellite imagery DDS files on demand.
//!
//! # Implementations
//!
//! - [`fuse3::Fuse3PassthroughFS`] - Async multi-threaded passthrough (recommended)
//! - [`AsyncPassthroughFS`] - Legacy single-threaded passthrough (fuser-based)
//! - [`XEarthLayerFS`] - Standalone virtual-only filesystem

pub mod async_passthrough;
mod filename;
mod filesystem;
pub mod fuse3;
mod placeholder;

pub use async_passthrough::{AsyncPassthroughFS, DdsHandler, DdsRequest, DdsResponse};
pub use filename::{parse_dds_filename, DdsFilename, ParseError};
pub use filesystem::XEarthLayerFS;
pub use fuse3::{Fuse3Error, Fuse3PassthroughFS, Fuse3Result, MountHandle, SpawnedMountHandle};
pub use placeholder::{generate_default_placeholder, generate_magenta_placeholder};
