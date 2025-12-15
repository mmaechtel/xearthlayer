//! Async multi-threaded FUSE filesystem using fuse3.
//!
//! This module provides a fully async FUSE implementation that leverages
//! the Tokio runtime for concurrent filesystem operations. Unlike the
//! single-threaded `fuser` implementation, all operations run as async
//! tasks, enabling true parallel processing of X-Plane's DDS requests.
//!
//! # Architecture
//!
//! ```text
//! X-Plane                    Tokio Runtime (multi-threaded)
//!    │                              │
//!    ├── read(file1.dds) ──────────►├── spawn task ──► generate_dds()
//!    ├── read(file2.dds) ──────────►├── spawn task ──► generate_dds()
//!    ├── read(file3.dds) ──────────►├── spawn task ──► generate_dds()
//!    │   [All run concurrently]     │   [All process in parallel]
//!    │◄── responses ────────────────┤
//! ```
//!
//! # Key Differences from fuser Implementation
//!
//! | Aspect | fuser | fuse3 |
//! |--------|-------|-------|
//! | Threading | Single-threaded | Multi-threaded via Tokio |
//! | Async | `block_on()` | Native async/await |
//! | Concurrency | Sequential | Parallel |
//! | Self reference | `&mut self` | `&self` (immutable) |

mod filesystem;
mod inode;
mod types;

pub use filesystem::Fuse3PassthroughFS;
pub use types::{Fuse3Error, Fuse3Result, MountHandle, SpawnedMountHandle};
