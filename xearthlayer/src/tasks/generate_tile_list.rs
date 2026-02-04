//! Generate tile list task implementation.
//!
//! [`GenerateTileListTask`] converts geographic coordinates (lat/lon) and a radius
//! into a grid of tile coordinates, spawning child `DdsGenerateJob` for each tile.
//!
//! # Resource Type
//!
//! This task uses `ResourceType::CPU` since tile calculation is lightweight
//! and doesn't require network or disk I/O.
//!
//! # Child Job Spawning
//!
//! This task spawns child jobs via the factory pattern. For a radius of N tiles,
//! it will spawn (2N+1)² child jobs (e.g., radius=5 → 121 children).
//!
//! # Output
//!
//! Produces `TaskOutput` with key "tiles_spawned" containing the count of
//! child jobs spawned (useful for progress tracking).

use crate::coord::{to_tile_coords, TileCoord};
use crate::executor::{Priority, ResourceType, Task, TaskContext, TaskOutput, TaskResult};
use crate::jobs::DdsJobFactory;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, warn};

/// Task that generates tile coordinates from lat/lon and spawns child jobs.
///
/// This task converts a geographic center point and radius into a grid of
/// tile coordinates, then spawns a `DdsGenerateJob` for each tile via the
/// injected factory.
///
/// # Type Parameters
///
/// * `F` - Factory type implementing `DdsJobFactory` for creating child jobs
///
/// # Example
///
/// For `radius_tiles = 2`, the task generates a 5×5 grid (25 tiles):
///
/// ```text
/// ┌───┬───┬───┬───┬───┐
/// │ * │ * │ * │ * │ * │
/// ├───┼───┼───┼───┼───┤
/// │ * │ * │ * │ * │ * │
/// ├───┼───┼───┼───┼───┤
/// │ * │ * │ C │ * │ * │  C = center tile
/// ├───┼───┼───┼───┼───┤
/// │ * │ * │ * │ * │ * │
/// ├───┼───┼───┼───┼───┤
/// │ * │ * │ * │ * │ * │
/// └───┴───┴───┴───┴───┘
/// ```
pub struct GenerateTileListTask<F>
where
    F: DdsJobFactory,
{
    /// Center latitude in degrees
    lat: f64,

    /// Center longitude in degrees
    lon: f64,

    /// Target zoom level for tiles
    zoom: u8,

    /// Number of tiles to include in each direction from center
    radius_tiles: u32,

    /// Factory for creating child DdsGenerateJob instances
    factory: Arc<F>,

    /// Priority for spawned child jobs
    child_priority: Priority,
}

impl<F> GenerateTileListTask<F>
where
    F: DdsJobFactory,
{
    /// Creates a new generate tile list task.
    ///
    /// # Arguments
    ///
    /// * `lat` - Center latitude in degrees (-85.05 to 85.05)
    /// * `lon` - Center longitude in degrees (-180.0 to 180.0)
    /// * `zoom` - Target zoom level (0-18)
    /// * `radius_tiles` - Number of tiles in each direction from center
    /// * `factory` - Factory for creating child jobs
    /// * `child_priority` - Priority to assign to child jobs
    pub fn new(
        lat: f64,
        lon: f64,
        zoom: u8,
        radius_tiles: u32,
        factory: Arc<F>,
        child_priority: Priority,
    ) -> Self {
        Self {
            lat,
            lon,
            zoom,
            radius_tiles,
            factory,
            child_priority,
        }
    }

    /// Returns the expected number of tiles that will be generated.
    ///
    /// This is (2 * radius + 1)² - useful for progress estimation.
    pub fn expected_tile_count(&self) -> u32 {
        let side = 2 * self.radius_tiles + 1;
        side * side
    }
}

impl<F> Task for GenerateTileListTask<F>
where
    F: DdsJobFactory,
{
    fn name(&self) -> &str {
        "GenerateTileList"
    }

    fn resource_type(&self) -> ResourceType {
        // Tile calculation is lightweight CPU work
        ResourceType::CPU
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a mut TaskContext,
    ) -> Pin<Box<dyn Future<Output = TaskResult> + Send + 'a>> {
        Box::pin(async move {
            // Check for cancellation before starting
            if ctx.is_cancelled() {
                return TaskResult::Cancelled;
            }

            // Clone job_id early to avoid borrow conflicts with ctx
            let job_id = ctx.job_id().clone();

            debug!(
                job_id = %job_id,
                lat = self.lat,
                lon = self.lon,
                zoom = self.zoom,
                radius = self.radius_tiles,
                "Generating tile list"
            );

            // Convert lat/lon to center tile coordinates
            let center = match to_tile_coords(self.lat, self.lon, self.zoom) {
                Ok(tile) => tile,
                Err(e) => {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        lat = self.lat,
                        lon = self.lon,
                        "Failed to convert coordinates to tile"
                    );
                    return TaskResult::Failed(crate::executor::TaskError::new(format!(
                        "Invalid coordinates: {}",
                        e
                    )));
                }
            };

            debug!(
                job_id = %job_id,
                center_row = center.row,
                center_col = center.col,
                "Center tile calculated"
            );

            // Calculate maximum tile coordinate at this zoom level
            let max_tile = 2u32.pow(self.zoom as u32);

            // Generate tile grid and spawn child jobs
            let mut spawned = 0u32;
            let radius = self.radius_tiles as i64;

            for dr in -radius..=radius {
                for dc in -radius..=radius {
                    // Check for cancellation periodically
                    if ctx.is_cancelled() {
                        debug!(
                            job_id = %job_id,
                            spawned = spawned,
                            "Tile generation cancelled"
                        );
                        return TaskResult::Cancelled;
                    }

                    // Calculate tile coordinates, clamping to valid range
                    let row = (center.row as i64 + dr).clamp(0, max_tile as i64 - 1) as u32;
                    let col = (center.col as i64 + dc).clamp(0, max_tile as i64 - 1) as u32;

                    let tile = TileCoord {
                        row,
                        col,
                        zoom: self.zoom,
                    };

                    // Create child job via factory
                    let child_job = self.factory.create_job(tile, self.child_priority);

                    // Spawn child job (use boxed variant since factory returns Box<dyn Job>)
                    if ctx.spawn_child_job_boxed(child_job, "GenerateTileList") {
                        spawned += 1;
                    } else {
                        warn!(
                            job_id = %job_id,
                            tile_row = row,
                            tile_col = col,
                            "Failed to spawn child job - child spawning may not be available"
                        );
                    }
                }
            }

            debug!(
                job_id = %job_id,
                spawned = spawned,
                expected = self.expected_tile_count(),
                "Tile list generation complete"
            );

            // Return success with spawn count
            let mut output = TaskOutput::new();
            output.set("tiles_spawned", spawned);
            TaskResult::SuccessWithOutput(output)
        })
    }
}

/// Output key for tiles spawned count.
pub const OUTPUT_KEY_TILES_SPAWNED: &str = "tiles_spawned";

/// Helper function to extract tiles spawned count from task output.
pub fn get_tiles_spawned_from_output(output: &TaskOutput) -> Option<&u32> {
    output.get::<u32>(OUTPUT_KEY_TILES_SPAWNED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ErrorPolicy, Job, JobId, JobResult, JobStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock factory that counts job creations
    struct CountingMockFactory {
        jobs_created: AtomicUsize,
    }

    impl CountingMockFactory {
        fn new() -> Self {
            Self {
                jobs_created: AtomicUsize::new(0),
            }
        }

        #[allow(dead_code)] // Infrastructure for future integration tests
        fn jobs_created(&self) -> usize {
            self.jobs_created.load(Ordering::SeqCst)
        }
    }

    /// Minimal mock job for testing
    struct MockJob {
        id: JobId,
        priority: Priority,
    }

    impl Job for MockJob {
        fn id(&self) -> JobId {
            self.id.clone()
        }
        fn name(&self) -> &str {
            "MockJob"
        }
        fn error_policy(&self) -> ErrorPolicy {
            ErrorPolicy::FailFast
        }
        fn priority(&self) -> Priority {
            self.priority
        }
        fn create_tasks(&self) -> Vec<Box<dyn Task>> {
            vec![]
        }
        fn on_complete(&self, _: &JobResult) -> JobStatus {
            JobStatus::Succeeded
        }
    }

    impl DdsJobFactory for CountingMockFactory {
        fn create_job(&self, tile: TileCoord, priority: Priority) -> Box<dyn Job> {
            self.jobs_created.fetch_add(1, Ordering::SeqCst);
            Box::new(MockJob {
                id: JobId::new(format!("mock-{}_{}_ZL{}", tile.row, tile.col, tile.zoom)),
                priority,
            })
        }
    }

    #[test]
    fn test_expected_tile_count() {
        let factory = Arc::new(CountingMockFactory::new());

        // radius=0 → 1 tile (just center)
        let task =
            GenerateTileListTask::new(0.0, 0.0, 10, 0, Arc::clone(&factory), Priority::PREFETCH);
        assert_eq!(task.expected_tile_count(), 1);

        // radius=1 → 9 tiles (3×3)
        let task =
            GenerateTileListTask::new(0.0, 0.0, 10, 1, Arc::clone(&factory), Priority::PREFETCH);
        assert_eq!(task.expected_tile_count(), 9);

        // radius=2 → 25 tiles (5×5)
        let task =
            GenerateTileListTask::new(0.0, 0.0, 10, 2, Arc::clone(&factory), Priority::PREFETCH);
        assert_eq!(task.expected_tile_count(), 25);

        // radius=5 → 121 tiles (11×11)
        let task =
            GenerateTileListTask::new(0.0, 0.0, 10, 5, Arc::clone(&factory), Priority::PREFETCH);
        assert_eq!(task.expected_tile_count(), 121);
    }

    #[test]
    fn test_task_name() {
        let factory = Arc::new(CountingMockFactory::new());
        let task = GenerateTileListTask::new(0.0, 0.0, 10, 1, factory, Priority::PREFETCH);
        assert_eq!(task.name(), "GenerateTileList");
    }

    #[test]
    fn test_resource_type() {
        let factory = Arc::new(CountingMockFactory::new());
        let task = GenerateTileListTask::new(0.0, 0.0, 10, 1, factory, Priority::PREFETCH);
        assert_eq!(task.resource_type(), ResourceType::CPU);
    }

    #[test]
    fn test_output_key_constant() {
        assert_eq!(OUTPUT_KEY_TILES_SPAWNED, "tiles_spawned");
    }

    #[test]
    fn test_get_tiles_spawned_from_output() {
        let mut output = TaskOutput::new();
        output.set("tiles_spawned", 42u32);

        assert_eq!(get_tiles_spawned_from_output(&output), Some(&42u32));
    }

    #[test]
    fn test_get_tiles_spawned_missing() {
        let output = TaskOutput::new();
        assert_eq!(get_tiles_spawned_from_output(&output), None);
    }
}
