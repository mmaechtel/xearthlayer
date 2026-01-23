//! Application bootstrap and lifecycle management.
//!
//! This module provides the `XEarthLayerApp` type which handles proper
//! initialization sequencing and graceful shutdown of all services.
//!
//! # Problem Solved
//!
//! Prior to this module, cache services (especially disk cache GC) were
//! wired externally in CLI code, leading to bugs where GC never started
//! in TUI mode. The `XEarthLayerApp` solves this by:
//!
//! 1. Starting cache services FIRST (they own their GC daemons)
//! 2. Creating bridge adapters for backward compatibility
//! 3. Managing lifecycle in a single, testable location
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                       XEarthLayerApp                             │
//! │                                                                  │
//! │  1. CacheService (memory) ─────► MemoryCacheBridge               │
//! │     └── MemoryCacheProvider      (implements executor::MemoryCache)
//! │                                                                  │
//! │  2. CacheService (disk) ───────► DiskCacheBridge                 │
//! │     └── DiskCacheProvider        (implements executor::DiskCache)
//! │         └── Internal GC daemon (THE FIX!)                        │
//! │                                                                  │
//! │  3. XEarthLayerService ────────► FUSE, Runtime, etc.             │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::app::{XEarthLayerApp, AppConfig};
//!
//! // Start the application
//! let app = XEarthLayerApp::start(config).await?;
//!
//! // Use the service
//! let service = app.service();
//!
//! // Graceful shutdown
//! app.shutdown().await;
//! ```

mod bootstrap;
mod config;
mod error;

pub use bootstrap::XEarthLayerApp;
pub use config::AppConfig;
pub use error::AppError;
