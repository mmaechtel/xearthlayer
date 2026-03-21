//! X-Plane 12 environment utilities.
//!
//! This module provides utilities for interacting with X-Plane 12 installations,
//! including path detection, resource location, and environment queries.

mod detection;
mod environment;
mod paths;

pub use detection::{
    derive_mountpoint, detect_custom_scenery, detect_scenery_dir, detect_xplane_install,
    detect_xplane_installs, SceneryDetectionResult, XPlanePathError,
};
pub use environment::XPlaneEnvironment;
