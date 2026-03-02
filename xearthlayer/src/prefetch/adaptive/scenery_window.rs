//! Scenery window model for the adaptive prefetch system.
//!
//! The `SceneryWindow` derives X-Plane's scenery loading window dimensions
//! from observed FUSE requests (via `SceneTracker`), manages a state machine
//! for window derivation, and provides boundary crossing predictions.
//!
//! # State Machine
//!
//! ```text
//! Uninitialized ──→ Measuring ──→ Ready
//!       │                           ↑
//!       └──→ Assumed ───────────────┘
//! ```
//!
//! - **Uninitialized**: No data yet, waiting for first `SceneTracker` bounds.
//! - **Assumed**: Using default dimensions for ocean/sparse starts where
//!   X-Plane may not request enough tiles to derive real dimensions.
//! - **Measuring**: First real bounds observed from `SceneTracker`, waiting
//!   for two consecutive stable checks before committing.
//! - **Ready**: Window dimensions derived and stable, ready for prefetch use.

use tracing::debug;

use crate::scene_tracker::SceneTracker;

/// Configuration for the SceneryWindow.
#[derive(Debug, Clone)]
pub struct SceneryWindowConfig {
    /// Default window rows (latitude) for assumed state.
    pub default_rows: usize,
    /// Default window columns (longitude) for assumed state.
    pub default_cols: usize,
    /// Buffer in DSF tiles around the window for retention.
    pub buffer: u8,
    /// Trigger distance for boundary monitors (degrees).
    pub trigger_distance: f64,
    /// Number of DSF tiles deep to load per boundary crossing.
    pub load_depth: u8,
}

impl Default for SceneryWindowConfig {
    fn default() -> Self {
        Self {
            default_rows: 6,
            default_cols: 8,
            buffer: 1,
            trigger_distance: 1.5,
            load_depth: 3,
        }
    }
}

/// State machine for the scenery window derivation.
#[derive(Debug)]
pub enum WindowState {
    /// No data yet -- waiting for first SceneTracker bounds.
    Uninitialized,
    /// Using assumed (default) dimensions -- no real data.
    Assumed,
    /// First bounds observed, waiting for stability.
    Measuring {
        /// Last observed row count (latitude span in degrees).
        last_rows: usize,
        /// Last observed column count (longitude span in degrees).
        last_cols: usize,
        /// Number of consecutive stable checks so far.
        stable_checks: u8,
    },
    /// Window dimensions derived and stable.
    Ready,
}

/// Central model that derives X-Plane's scenery loading window.
///
/// Observes the `SceneTracker` to determine window dimensions, then
/// provides boundary crossing predictions via dual monitors (integrated
/// in later tasks).
pub struct SceneryWindow {
    state: WindowState,
    window_size: Option<(usize, usize)>,
    config: SceneryWindowConfig,
}

impl SceneryWindow {
    /// Create a new `SceneryWindow` in `Uninitialized` state.
    pub fn new(config: SceneryWindowConfig) -> Self {
        Self {
            state: WindowState::Uninitialized,
            window_size: None,
            config,
        }
    }

    /// Set assumed window dimensions for ocean/sparse starts.
    ///
    /// Transitions to `Assumed` state with default dimensions. This allows
    /// prefetch to begin immediately with reasonable defaults while waiting
    /// for real data from the `SceneTracker`.
    pub fn set_assumed_dimensions(&mut self, rows: usize, cols: usize) {
        self.state = WindowState::Assumed;
        self.window_size = Some((rows, cols));
        debug!(rows, cols, "scenery window: assumed dimensions set");
    }

    /// Update the window model from `SceneTracker` observations.
    ///
    /// Drives the state machine:
    /// - `Uninitialized`/`Assumed` + bounds -> `Measuring`
    /// - `Measuring` + stable bounds -> `Ready`
    /// - `Measuring` + changed bounds -> reset stability counter
    pub fn update_from_tracker(&mut self, tracker: &dyn SceneTracker) {
        let bounds = match tracker.loaded_bounds() {
            Some(b) => b,
            None => return, // No data yet
        };

        let rows = bounds.height().round() as usize;
        let cols = bounds.width().round() as usize;

        match &self.state {
            WindowState::Uninitialized | WindowState::Assumed => {
                debug!(rows, cols, "scenery window: first bounds observed, measuring");
                self.state = WindowState::Measuring {
                    last_rows: rows,
                    last_cols: cols,
                    stable_checks: 1,
                };
            }
            WindowState::Measuring {
                last_rows,
                last_cols,
                stable_checks,
            } => {
                if rows == *last_rows && cols == *last_cols {
                    let new_count = stable_checks + 1;
                    if new_count >= 2 {
                        debug!(rows, cols, "scenery window: bounds stable, ready");
                        self.state = WindowState::Ready;
                        self.window_size = Some((rows, cols));
                    } else {
                        self.state = WindowState::Measuring {
                            last_rows: rows,
                            last_cols: cols,
                            stable_checks: new_count,
                        };
                    }
                } else {
                    debug!(
                        old_rows = *last_rows,
                        old_cols = *last_cols,
                        new_rows = rows,
                        new_cols = cols,
                        "scenery window: bounds changed, resetting stability"
                    );
                    self.state = WindowState::Measuring {
                        last_rows: rows,
                        last_cols: cols,
                        stable_checks: 1,
                    };
                }
            }
            WindowState::Ready => {
                // Already ready -- no-op for now (rebuild detection in Task 8)
            }
        }
    }

    /// Returns the derived window size as `(rows, cols)` in DSF tiles.
    pub fn window_size(&self) -> Option<(usize, usize)> {
        self.window_size
    }

    /// Returns `true` if the window is in `Ready` or `Assumed` state.
    pub fn is_ready(&self) -> bool {
        matches!(self.state, WindowState::Ready | WindowState::Assumed)
    }

    /// Returns the current state.
    pub fn state(&self) -> &WindowState {
        &self.state
    }

    /// Returns the configuration.
    pub fn config(&self) -> &SceneryWindowConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_tracker::{DdsTileCoord, GeoBounds, GeoRegion, SceneTracker};
    use std::collections::HashSet;

    /// Mock SceneTracker that returns controlled bounds.
    struct MockSceneTracker {
        bounds: std::sync::Mutex<Option<GeoBounds>>,
        burst_active: std::sync::atomic::AtomicBool,
    }

    impl MockSceneTracker {
        fn new() -> Self {
            Self {
                bounds: std::sync::Mutex::new(None),
                burst_active: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn set_bounds(&self, bounds: GeoBounds) {
            *self.bounds.lock().unwrap() = Some(bounds);
        }

        #[allow(dead_code)]
        fn clear_bounds(&self) {
            *self.bounds.lock().unwrap() = None;
        }
    }

    impl SceneTracker for MockSceneTracker {
        fn requested_tiles(&self) -> HashSet<DdsTileCoord> {
            HashSet::new()
        }
        fn is_tile_requested(&self, _tile: &DdsTileCoord) -> bool {
            false
        }
        fn is_burst_active(&self) -> bool {
            self.burst_active
                .load(std::sync::atomic::Ordering::Relaxed)
        }
        fn current_burst_tiles(&self) -> Vec<DdsTileCoord> {
            vec![]
        }
        fn total_requests(&self) -> u64 {
            0
        }
        fn loaded_regions(&self) -> HashSet<GeoRegion> {
            HashSet::new()
        }
        fn is_region_loaded(&self, _region: &GeoRegion) -> bool {
            false
        }
        fn loaded_bounds(&self) -> Option<GeoBounds> {
            *self.bounds.lock().unwrap()
        }
    }

    fn make_bounds(min_lat: f64, max_lat: f64, min_lon: f64, max_lon: f64) -> GeoBounds {
        GeoBounds {
            min_lat,
            max_lat,
            min_lon,
            max_lon,
        }
    }

    #[test]
    fn test_new_starts_uninitialized() {
        let window = SceneryWindow::new(SceneryWindowConfig::default());
        assert!(matches!(window.state(), WindowState::Uninitialized));
        assert!(window.window_size().is_none());
        assert!(!window.is_ready());
    }

    #[test]
    fn test_set_assumed_dimensions() {
        let mut window = SceneryWindow::new(SceneryWindowConfig::default());
        window.set_assumed_dimensions(6, 8);
        assert!(matches!(window.state(), WindowState::Assumed));
        assert_eq!(window.window_size(), Some((6, 8)));
        assert!(window.is_ready()); // Assumed counts as ready for prefetch
    }

    #[test]
    fn test_transition_to_measuring_on_first_bounds() {
        let tracker = std::sync::Arc::new(MockSceneTracker::new());
        tracker.set_bounds(make_bounds(47.0, 53.0, 3.0, 11.0));

        let mut window = SceneryWindow::new(SceneryWindowConfig::default());
        window.update_from_tracker(tracker.as_ref());

        assert!(matches!(window.state(), WindowState::Measuring { .. }));
        assert!(window.window_size().is_none()); // Not ready yet
    }

    #[test]
    fn test_transition_to_ready_after_stable_bounds() {
        let tracker = std::sync::Arc::new(MockSceneTracker::new());
        // Same bounds (6x8 degrees) for two consecutive checks
        tracker.set_bounds(make_bounds(47.0, 53.0, 3.0, 11.0));

        let mut window = SceneryWindow::new(SceneryWindowConfig::default());
        window.update_from_tracker(tracker.as_ref()); // -> Measuring
        window.update_from_tracker(tracker.as_ref()); // -> Ready (stable)

        assert!(matches!(window.state(), WindowState::Ready));
        assert_eq!(window.window_size(), Some((6, 8))); // 53-47=6 rows, 11-3=8 cols
        assert!(window.is_ready());
    }

    #[test]
    fn test_measuring_resets_on_changed_bounds() {
        let tracker = std::sync::Arc::new(MockSceneTracker::new());

        let mut window = SceneryWindow::new(SceneryWindowConfig::default());

        // First bounds
        tracker.set_bounds(make_bounds(47.0, 53.0, 3.0, 11.0));
        window.update_from_tracker(tracker.as_ref()); // -> Measuring

        // Different bounds (window expanded)
        tracker.set_bounds(make_bounds(46.0, 53.0, 3.0, 12.0));
        window.update_from_tracker(tracker.as_ref()); // -> still Measuring (reset counter)

        // Same as last check
        window.update_from_tracker(tracker.as_ref()); // -> Ready
        assert!(matches!(window.state(), WindowState::Ready));
        assert_eq!(window.window_size(), Some((7, 9))); // 53-46=7, 12-3=9
    }

    #[test]
    fn test_no_bounds_stays_uninitialized() {
        let tracker = std::sync::Arc::new(MockSceneTracker::new());
        // No bounds set (no tiles requested)

        let mut window = SceneryWindow::new(SceneryWindowConfig::default());
        window.update_from_tracker(tracker.as_ref());

        assert!(matches!(window.state(), WindowState::Uninitialized));
    }

    #[test]
    fn test_assumed_transitions_to_measuring_on_real_bounds() {
        let tracker = std::sync::Arc::new(MockSceneTracker::new());
        tracker.set_bounds(make_bounds(47.0, 53.0, 3.0, 11.0));

        let mut window = SceneryWindow::new(SceneryWindowConfig::default());
        window.set_assumed_dimensions(6, 8); // -> Assumed
        window.update_from_tracker(tracker.as_ref()); // -> Measuring (real data available)

        assert!(matches!(window.state(), WindowState::Measuring { .. }));
    }

    #[test]
    fn test_default_config() {
        let config = SceneryWindowConfig::default();
        assert_eq!(config.default_rows, 6);
        assert_eq!(config.default_cols, 8);
        assert_eq!(config.buffer, 1);
    }
}
