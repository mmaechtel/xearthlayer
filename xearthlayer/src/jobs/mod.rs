//! DDS job implementations for the executor framework.
//!
//! This module provides DDS-specific job implementations that integrate with
//! the generic job executor from [`crate::executor`].
//!
//! # Jobs
//!
//! - [`DdsGenerateJob`] - Generates a single DDS texture from satellite imagery
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::jobs::DdsGenerateJob;
//! use xearthlayer::executor::{JobExecutor, Priority};
//!
//! let job = DdsGenerateJob::new(tile, priority, provider, encoder, memory_cache, disk_cache, executor);
//! let handle = job_executor.submit(job);
//! ```

mod dds_generate;

pub use dds_generate::DdsGenerateJob;
