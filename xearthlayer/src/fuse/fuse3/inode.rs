//! Inode management for fuse3 filesystem.
//!
//! This module re-exports the InodeManager from async_passthrough
//! as it's already thread-safe and works with both implementations.

pub use crate::fuse::async_passthrough::inode::InodeManager;
