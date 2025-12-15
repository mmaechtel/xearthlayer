//! Pipeline telemetry for observability and user feedback.
//!
//! This module provides metrics collection and reporting for the tile generation
//! pipeline. It uses lock-free atomic counters for high-performance instrumentation
//! with minimal overhead.
//!
//! # Architecture
//!
//! ```text
//! Pipeline Stages ─────► PipelineMetrics ─────► TelemetrySnapshot ─────► Views
//!                        (atomic counters)     (point-in-time copy)      (CLI, etc.)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::telemetry::{PipelineMetrics, TelemetrySnapshot};
//! use std::sync::Arc;
//!
//! let metrics = Arc::new(PipelineMetrics::new());
//!
//! // Record events from pipeline stages
//! metrics.job_started();
//! metrics.chunks_downloaded(256, 768_000); // 256 chunks, 768KB
//! metrics.job_completed();
//!
//! // Take snapshot for display
//! let snapshot = metrics.snapshot();
//! println!("Jobs completed: {}", snapshot.jobs_completed);
//! println!("Download throughput: {:.1} MB/s", snapshot.bytes_per_second / 1_000_000.0);
//! ```

mod metrics;
mod snapshot;

pub use metrics::PipelineMetrics;
pub use snapshot::TelemetrySnapshot;
