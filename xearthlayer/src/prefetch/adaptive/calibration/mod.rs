//! Performance calibration for adaptive prefetch.
//!
//! This module measures tile generation throughput during X-Plane's initial
//! 12° × 12° scenery load to determine the optimal prefetch strategy.
//!
//! # Module Structure
//!
//! - [`types`] - Core types: `StrategyMode`, `PerformanceCalibration`, `RecalibrationResult`
//! - [`observer`] - `ThroughputObserver` trait for recording completions
//! - [`calibrator`] - `PerformanceCalibrator` for initial throughput measurement
//! - [`rolling`] - `RollingCalibrator` for ongoing throughput monitoring
//!
//! # Calibration Flow
//!
//! ```text
//! X-Plane spawns → Initial 12° load begins → ThroughputObserver records completions
//!                                                     ↓
//!                                          FuseLoadMonitor detects high→low transition
//!                                                     ↓
//!                                          PerformanceCalibrator finalizes calibration
//!                                                     ↓
//!                                          StrategyMode selected based on throughput
//!                                                     ↓
//!                                          RollingCalibrator monitors for changes
//! ```
//!
//! # Strategy Mode Selection
//!
//! | Throughput | Mode | Trigger |
//! |------------|------|---------|
//! | > 30 tiles/sec | Aggressive | Position-based (0.3° into DSF) |
//! | 10-30 tiles/sec | Opportunistic | Circuit breaker close |
//! | < 10 tiles/sec | Disabled | Skip prefetch |

mod calibrator;
mod observer;
mod rolling;
mod types;

// Re-export all public types
pub use calibrator::PerformanceCalibrator;
pub use observer::{
    create_throughput_observer, create_throughput_observer_with_config, SharedThroughputObserver,
    ThroughputObserver,
};
pub use rolling::RollingCalibrator;
pub use types::{PerformanceCalibration, RecalibrationResult, StrategyMode};
