//! HTTP download manager for package archives.
//!
//! This module provides functionality for downloading package archive parts,
//! including:
//! - Single file downloads with resume support (`http`)
//! - SHA-256 checksum verification (`checksum`)
//! - Multi-part download state tracking (`state`)
//! - Real-time progress reporting (`progress`)
//! - Sequential and parallel download strategies (`strategy`)
//! - High-level download orchestration (`orchestrator`)
//!
//! # Architecture
//!
//! The download system is organized using the Strategy pattern:
//!
//! ```text
//! MultiPartDownloader (orchestrator)
//!         │
//!         ├── DownloadStrategy (trait)
//!         │       ├── SequentialStrategy
//!         │       └── ParallelStrategy
//!         │
//!         ├── HttpDownloader (single file downloads)
//!         │
//!         ├── DownloadState (tracks progress)
//!         │
//!         └── ProgressReporter (real-time updates)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use xearthlayer::manager::download::{MultiPartDownloader, DownloadState};
//!
//! let downloader = MultiPartDownloader::new();
//!
//! let mut state = DownloadState::new(
//!     vec!["http://example.com/part1.bin".to_string()],
//!     vec!["abc123...".to_string()],
//!     vec![PathBuf::from("/tmp/part1.bin")],
//! );
//!
//! // Query file sizes for accurate progress
//! downloader.query_sizes(&mut state);
//!
//! // Download with progress callback
//! downloader.download_all(&mut state, Some(Box::new(|bytes, total, parts, total_parts| {
//!     println!("Downloaded {} of {} bytes", bytes, total);
//! })))?;
//! ```

mod checksum;
mod http;
mod orchestrator;
mod progress;
mod state;
mod strategy;

// Public API - types used by installer and other modules
pub use orchestrator::MultiPartDownloader;
pub use progress::MultiPartProgressCallback;
pub use state::DownloadState;
