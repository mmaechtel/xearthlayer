# Job Executor Framework Design

## Status

**Proposed** - Design document for review

## Problem Statement

The current XEarthLayer pipeline is tightly coupled to DDS generation workflows. While it works well for its purpose, it has limitations:

1. **Opinionated structure** - Pipeline stages are DDS-specific (download, assemble, encode)
2. **Limited composability** - Cannot easily express "prefetch a tile" as a collection of DDS jobs
3. **No job-level tracking** - Cannot track progress of aggregate operations (e.g., "tile 80% complete")
4. **Rigid error handling** - Error policies are baked into the pipeline, not declarative per-job

As we implement tile-based prefetching, we need to express hierarchical work:
- A tile prefetch job spawns many DDS generation jobs
- Each DDS job has multiple tasks (download, assemble, encode)
- The tile job completes when enough child DDS jobs succeed

This requires a generalized job execution framework.

## Goals

1. **Generic execution** - Framework knows nothing about DDS, tiles, or imagery
2. **Trait-based extensibility** - New job/task types implement traits
3. **Hierarchical composition** - Jobs contain tasks; tasks can spawn child jobs
4. **Declarative policies** - Error handling, retries, completion criteria defined per-job
5. **Efficient execution** - Respect concurrency limits, support cancellation
6. **Observable** - Progress tracking, logging, metrics

## Non-Goals

- Distributed execution (single-process only)
- Persistence/recovery (jobs are in-memory)
- Scheduling/cron (jobs are submitted, not scheduled)

## Design

### Core Concepts

| Concept | Description |
|---------|-------------|
| **Job** | A named unit of work with tasks, error policy, and completion criteria |
| **Task** | A single async operation that may spawn child jobs |
| **JobExecutor** | Runs jobs respecting dependencies and concurrency limits |
| **JobHandle** | Reference to a submitted job for status/cancellation |
| **TaskContext** | Execution context passed to tasks for spawning children and accessing resources |

### Relationship Model

```
┌─────────────────────────────────────────────────────────────────────┐
│                       Cardinality Model                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   Job ─────── 1:n ───────► Task ─────── 1:1 ───────► Child Job      │
│    │                         │                           │          │
│    │                         │                           │          │
│    │                         │ (optional)                │          │
│    │                         │                           │          │
│    │                         └── Task may or may not     │          │
│    │                             spawn a child job       │          │
│    │                                                     │          │
│    └── Job completion depends                            │          │
│        on all tasks completing                           │          │
│        (including waiting for                            │          │
│        any spawned children)             ┌───────────────┘          │
│                                          │                          │
│                                          ▼                          │
│                                   Child Job ──── 1:n ──► Child Task │
│                                          │                          │
│                                          └── Recursive: child tasks │
│                                              can spawn grandchildren│
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Trait Definitions

#### Job Trait

```rust
/// A unit of work composed of tasks with error handling and completion criteria.
pub trait Job: Send + Sync + 'static {
    /// Unique identifier for this job instance.
    fn id(&self) -> JobId;

    /// Human-readable name for logging/display.
    fn name(&self) -> &str;

    /// Error handling policy for this job.
    fn error_policy(&self) -> ErrorPolicy {
        ErrorPolicy::FailFast
    }

    /// IDs of jobs that must complete before this job can start.
    fn dependencies(&self) -> Vec<JobId> {
        vec![]
    }

    /// Priority (higher = more important). Default: 0.
    fn priority(&self) -> i32 {
        0
    }

    /// Create the tasks for this job.
    /// Called when the job is ready to execute.
    fn create_tasks(&self) -> Vec<Box<dyn Task>>;

    /// Called when all tasks and child jobs complete.
    /// Allows job to inspect results and determine final status.
    fn on_complete(&self, result: &JobResult) -> JobStatus {
        match result.failed_tasks.is_empty() && result.failed_children.is_empty() {
            true => JobStatus::Succeeded,
            false => JobStatus::Failed,
        }
    }
}

/// Unique job identifier.
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct JobId(pub String);

impl JobId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}
```

#### Task Trait

```rust
/// A single async operation within a job.
pub trait Task: Send + Sync + 'static {
    /// Human-readable name for logging/display.
    fn name(&self) -> &str;

    /// Retry policy for this task.
    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::None
    }

    /// Execute the task.
    ///
    /// # Arguments
    /// * `ctx` - Context for spawning child jobs and accessing resources
    ///
    /// # Returns
    /// * `TaskResult` indicating success, failure, or retry
    fn execute(
        &self,
        ctx: &mut TaskContext,
    ) -> impl Future<Output = TaskResult> + Send;
}

/// Result of task execution.
pub enum TaskResult {
    /// Task completed successfully.
    Success,

    /// Task completed successfully with output data.
    SuccessWithOutput(TaskOutput),

    /// Task failed with error.
    Failed(TaskError),

    /// Task requests retry (respects retry policy).
    Retry(TaskError),

    /// Task was cancelled.
    Cancelled,
}

/// Arbitrary output data from a task.
pub struct TaskOutput {
    data: HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl TaskOutput {
    pub fn new() -> Self {
        Self { data: HashMap::new() }
    }

    pub fn set<T: Any + Send + Sync>(&mut self, key: &str, value: T) {
        self.data.insert(key.to_string(), Box::new(value));
    }

    pub fn get<T: Any + Send + Sync>(&self, key: &str) -> Option<&T> {
        self.data.get(key).and_then(|v| v.downcast_ref())
    }
}
```

#### TaskContext

```rust
/// Execution context provided to tasks.
pub struct TaskContext {
    /// Job ID of the parent job.
    job_id: JobId,

    /// Sender for spawning child jobs.
    child_job_sender: mpsc::Sender<Box<dyn Job>>,

    /// Handles to child jobs spawned by this task.
    child_handles: Vec<JobHandle>,

    /// Output from previous tasks in this job.
    previous_outputs: HashMap<String, TaskOutput>,

    /// Shared resources (caches, indices, etc.).
    resources: Arc<Resources>,

    /// Cancellation token.
    cancel_token: CancellationToken,
}

impl TaskContext {
    /// Spawn a child job. Returns immediately; child executes asynchronously.
    ///
    /// The parent job will not complete until all spawned children complete.
    pub fn spawn_child_job(&mut self, job: impl Job) -> JobHandle {
        let handle = JobHandle::new(job.id());
        self.child_job_sender
            .send(Box::new(job))
            .expect("executor channel closed");
        self.child_handles.push(handle.clone());
        handle
    }

    /// Get output from a previous task in this job.
    pub fn get_output<T: Any + Send + Sync>(&self, task_name: &str, key: &str) -> Option<&T> {
        self.previous_outputs
            .get(task_name)
            .and_then(|o| o.get(key))
    }

    /// Access shared resources.
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Get cancellation token for async operations.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}
```

### Policy Types

#### Error Policy

```rust
/// How a job handles task/child failures.
#[derive(Clone, Debug)]
pub enum ErrorPolicy {
    /// Stop job immediately on first failure.
    FailFast,

    /// Continue executing remaining tasks despite failures.
    ContinueOnError,

    /// Job succeeds if at least `threshold` fraction of work succeeds.
    /// Useful for prefetching where partial success is acceptable.
    PartialSuccess {
        /// Minimum success ratio (0.0 - 1.0).
        threshold: f64,
    },

    /// Custom completion logic (defer to `Job::on_complete`).
    Custom,
}
```

#### Retry Policy

```rust
/// How a task handles transient failures.
#[derive(Clone, Debug)]
pub enum RetryPolicy {
    /// No retries.
    None,

    /// Fixed number of retries with constant delay.
    Fixed {
        max_attempts: u32,
        delay: Duration,
    },

    /// Exponential backoff.
    ExponentialBackoff {
        max_attempts: u32,
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
    },
}

impl RetryPolicy {
    /// Convenience constructor for common case.
    pub fn exponential(max_attempts: u32) -> Self {
        Self::ExponentialBackoff {
            max_attempts,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}
```

### JobExecutor

```rust
/// Executes jobs respecting dependencies, concurrency limits, and policies.
pub struct JobExecutor {
    /// Channel for receiving new jobs (including child jobs).
    job_receiver: mpsc::Receiver<Box<dyn Job>>,

    /// Sender for submitting jobs.
    job_sender: mpsc::Sender<Box<dyn Job>>,

    /// Jobs waiting for dependencies.
    pending: HashMap<JobId, PendingJob>,

    /// Jobs currently executing.
    running: HashMap<JobId, RunningJob>,

    /// Completed jobs (kept for dependency resolution).
    completed: HashMap<JobId, JobResult>,

    /// Concurrency configuration.
    concurrency: ConcurrencyConfig,

    /// Shared resources for all jobs.
    resources: Arc<Resources>,

    /// Metrics collector.
    metrics: Arc<Metrics>,
}

/// Concurrency configuration.
pub struct ConcurrencyConfig {
    /// Maximum concurrent jobs.
    pub max_jobs: usize,

    /// Maximum concurrent tasks across all jobs.
    pub max_tasks: usize,

    /// Per-resource limits (e.g., "http" -> 256, "cpu" -> 8).
    pub resource_limits: HashMap<String, usize>,
}

impl JobExecutor {
    /// Create a new executor with the given configuration.
    pub fn new(config: ConcurrencyConfig, resources: Resources) -> Self;

    /// Submit a job for execution. Returns handle for status/cancellation.
    pub fn submit(&self, job: impl Job) -> JobHandle {
        let handle = JobHandle::new(job.id());
        self.job_sender.send(Box::new(job)).expect("executor running");
        handle
    }

    /// Run the executor (blocks until shutdown).
    pub async fn run(&mut self, shutdown: CancellationToken) -> Result<()> {
        loop {
            tokio::select! {
                // Receive new jobs (submitted or child jobs)
                Some(job) = self.job_receiver.recv() => {
                    self.enqueue_job(job);
                }

                // Check for jobs ready to run
                _ = self.poll_ready_jobs() => {}

                // Check for completed tasks
                _ = self.poll_task_completion() => {}

                // Shutdown requested
                _ = shutdown.cancelled() => {
                    self.cancel_all_jobs().await;
                    break;
                }
            }
        }
        Ok(())
    }

    /// Start jobs whose dependencies are satisfied.
    async fn poll_ready_jobs(&mut self) {
        let ready: Vec<_> = self.pending
            .iter()
            .filter(|(_, job)| self.dependencies_satisfied(job))
            .map(|(id, _)| id.clone())
            .collect();

        for job_id in ready {
            if self.running.len() < self.concurrency.max_jobs {
                if let Some(pending) = self.pending.remove(&job_id) {
                    self.start_job(pending).await;
                }
            }
        }
    }

    fn dependencies_satisfied(&self, job: &PendingJob) -> bool {
        job.dependencies
            .iter()
            .all(|dep| self.completed.contains_key(dep))
    }
}
```

### JobHandle

```rust
/// Handle to a submitted job for status queries and cancellation.
#[derive(Clone)]
pub struct JobHandle {
    job_id: JobId,
    status: watch::Receiver<JobStatus>,
    cancel_token: CancellationToken,
}

impl JobHandle {
    /// Get current job status.
    pub fn status(&self) -> JobStatus {
        *self.status.borrow()
    }

    /// Wait for job completion.
    pub async fn wait(&mut self) -> JobResult {
        loop {
            if self.status().is_terminal() {
                break;
            }
            self.status.changed().await.ok();
        }
        // Return full result...
    }

    /// Request job cancellation.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Get the job ID.
    pub fn id(&self) -> &JobId {
        &self.job_id
    }
}

/// Job execution status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    /// Waiting for dependencies.
    Pending,

    /// Currently executing.
    Running,

    /// Completed successfully.
    Succeeded,

    /// Completed with failures.
    Failed,

    /// Cancelled before completion.
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
```

### Shared Resources

```rust
/// Shared resources available to all jobs/tasks.
pub struct Resources {
    /// Ortho union index for tile lookups.
    pub ortho_index: Arc<OrthoUnionIndex>,

    /// Memory cache.
    pub memory_cache: Arc<MemoryCache>,

    /// Disk cache.
    pub disk_cache: Arc<DiskCache>,

    /// HTTP client.
    pub http_client: Arc<dyn HttpClient>,

    /// Imagery provider.
    pub imagery_provider: Arc<dyn ImageryProvider>,

    /// Extensible: additional resources.
    pub extensions: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Resources {
    /// Get a typed extension resource.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|r| r.clone().downcast().ok())
    }
}
```

## Example: Tile Prefetch Implementation

### TilePrefetchJob

```rust
/// Job to prefetch all DDS files within a 1° tile.
pub struct TilePrefetchJob {
    id: JobId,
    lat: i32,
    lon: i32,
}

impl TilePrefetchJob {
    pub fn new(lat: i32, lon: i32) -> Self {
        Self {
            id: JobId::new(format!("tile-prefetch-{}-{}", lat, lon)),
            lat,
            lon,
        }
    }
}

impl Job for TilePrefetchJob {
    fn id(&self) -> JobId {
        self.id.clone()
    }

    fn name(&self) -> &str {
        "TilePrefetch"
    }

    fn error_policy(&self) -> ErrorPolicy {
        // 80% success = tile is usable
        ErrorPolicy::PartialSuccess { threshold: 0.8 }
    }

    fn create_tasks(&self) -> Vec<Box<dyn Task>> {
        vec![
            Box::new(EnumerateDdsFilesTask::new(self.lat, self.lon)),
            Box::new(WaitForChildrenTask::new()),
        ]
    }
}
```

### EnumerateDdsFilesTask

```rust
/// Task that enumerates DDS files in a tile and spawns child jobs.
pub struct EnumerateDdsFilesTask {
    lat: i32,
    lon: i32,
}

impl EnumerateDdsFilesTask {
    pub fn new(lat: i32, lon: i32) -> Self {
        Self { lat, lon }
    }
}

impl Task for EnumerateDdsFilesTask {
    fn name(&self) -> &str {
        "EnumerateDdsFiles"
    }

    async fn execute(&self, ctx: &mut TaskContext) -> TaskResult {
        let ortho_index = &ctx.resources().ortho_index;

        // Get all DDS files in this 1° tile
        let dds_files = ortho_index.dds_files_in_tile(self.lat, self.lon);

        if dds_files.is_empty() {
            return TaskResult::Success;
        }

        // Spawn a child DdsGenerateJob for each file
        for dds_path in dds_files {
            if ctx.is_cancelled() {
                return TaskResult::Cancelled;
            }

            ctx.spawn_child_job(DdsGenerateJob::new(dds_path));
        }

        TaskResult::Success
    }
}
```

### DdsGenerateJob

```rust
/// Job to generate a single DDS file.
pub struct DdsGenerateJob {
    id: JobId,
    path: PathBuf,
}

impl DdsGenerateJob {
    pub fn new(path: PathBuf) -> Self {
        let id = JobId::new(format!("dds-{}", path.display()));
        Self { id, path }
    }
}

impl Job for DdsGenerateJob {
    fn id(&self) -> JobId {
        self.id.clone()
    }

    fn name(&self) -> &str {
        "DdsGenerate"
    }

    fn error_policy(&self) -> ErrorPolicy {
        ErrorPolicy::FailFast
    }

    fn priority(&self) -> i32 {
        // Lower priority than on-demand requests
        -10
    }

    fn create_tasks(&self) -> Vec<Box<dyn Task>> {
        vec![
            Box::new(DownloadChunksTask::new(self.path.clone())),
            Box::new(AssembleImageTask::new()),
            Box::new(EncodeDdsTask::new(self.path.clone())),
        ]
    }
}
```

### DownloadChunksTask

```rust
/// Task to download imagery chunks for a DDS file.
pub struct DownloadChunksTask {
    path: PathBuf,
}

impl Task for DownloadChunksTask {
    fn name(&self) -> &str {
        "DownloadChunks"
    }

    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::exponential(3)
    }

    async fn execute(&self, ctx: &mut TaskContext) -> TaskResult {
        let provider = &ctx.resources().imagery_provider;
        let coords = parse_dds_coords(&self.path)?;

        // Download all 256 chunks (16x16)
        let chunks = provider
            .download_chunks(coords, ctx.cancel_token())
            .await;

        match chunks {
            Ok(data) => {
                let mut output = TaskOutput::new();
                output.set("chunks", data);
                TaskResult::SuccessWithOutput(output)
            }
            Err(e) if e.is_transient() => TaskResult::Retry(e.into()),
            Err(e) => TaskResult::Failed(e.into()),
        }
    }
}
```

### AssembleImageTask

```rust
/// Task to assemble chunks into a full image.
pub struct AssembleImageTask;

impl Task for AssembleImageTask {
    fn name(&self) -> &str {
        "AssembleImage"
    }

    async fn execute(&self, ctx: &mut TaskContext) -> TaskResult {
        // Get chunks from previous task
        let chunks: &Vec<Bytes> = ctx
            .get_output("DownloadChunks", "chunks")
            .ok_or_else(|| TaskError::missing_input("chunks"))?;

        // Assemble into 4096x4096 image
        let image = assemble_chunks(chunks)?;

        let mut output = TaskOutput::new();
        output.set("image", image);
        TaskResult::SuccessWithOutput(output)
    }
}
```

### EncodeDdsTask

```rust
/// Task to encode image as DDS and cache it.
pub struct EncodeDdsTask {
    path: PathBuf,
}

impl Task for EncodeDdsTask {
    fn name(&self) -> &str {
        "EncodeDds"
    }

    async fn execute(&self, ctx: &mut TaskContext) -> TaskResult {
        let image: &DynamicImage = ctx
            .get_output("AssembleImage", "image")
            .ok_or_else(|| TaskError::missing_input("image"))?;

        // Encode to DDS
        let dds_data = encode_bc1_dds(image)?;

        // Store in caches
        let cache = &ctx.resources().memory_cache;
        cache.put(&self.path, dds_data.clone()).await;

        let disk_cache = &ctx.resources().disk_cache;
        disk_cache.put(&self.path, &dds_data).await?;

        TaskResult::Success
    }
}
```

## Execution Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Execution Flow Example                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. Submit TilePrefetchJob(lat=60, lon=-146)                       │
│     │                                                               │
│     ▼                                                               │
│  2. Executor starts TilePrefetchJob                                │
│     │                                                               │
│     ▼                                                               │
│  3. EnumerateDdsFilesTask executes                                 │
│     │  - Queries OrthoUnionIndex                                   │
│     │  - Finds 847 DDS files                                       │
│     │  - Spawns 847 DdsGenerateJob children                        │
│     │                                                               │
│     ▼                                                               │
│  4. Executor receives 847 child jobs                               │
│     │  - Queues them (respects max_jobs limit)                     │
│     │                                                               │
│     ▼                                                               │
│  5. DdsGenerateJobs execute in parallel                            │
│     │  - Each runs: Download → Assemble → Encode                   │
│     │  - Some may fail, retry, or succeed                          │
│     │                                                               │
│     ▼                                                               │
│  6. WaitForChildrenTask waits for all children                     │
│     │                                                               │
│     ▼                                                               │
│  7. TilePrefetchJob::on_complete() evaluates results               │
│     │  - 812/847 succeeded (95.9%)                                 │
│     │  - Threshold 80% met → JobStatus::Succeeded                  │
│     │                                                               │
│     ▼                                                               │
│  8. Job complete, handle notified                                  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Integration with Tile-Based Prefetcher

The `TileBasedPrefetcher` uses the job executor:

```rust
impl TileBasedPrefetcher {
    async fn prefetch_tile(&self, tile: TileCoord) {
        // Submit tile prefetch job
        let job = TilePrefetchJob::new(tile.lat, tile.lon);
        let handle = self.executor.submit(job);

        // Track active prefetch
        self.active_tiles.insert(tile, handle.clone());

        // Optionally wait or let it run in background
        // handle.wait().await;
    }

    fn cancel_active_prefetch(&self) {
        for (_, handle) in self.active_tiles.drain() {
            handle.cancel();
        }
    }
}
```

## Module Structure

```
xearthlayer/src/
├── executor/
│   ├── mod.rs              # Module exports
│   ├── job.rs              # Job trait and JobId
│   ├── task.rs             # Task trait and TaskResult
│   ├── context.rs          # TaskContext
│   ├── executor.rs         # JobExecutor implementation
│   ├── handle.rs           # JobHandle and JobStatus
│   ├── policy.rs           # ErrorPolicy, RetryPolicy
│   └── resources.rs        # Shared Resources
│
├── jobs/                   # Job implementations
│   ├── mod.rs
│   ├── tile_prefetch.rs    # TilePrefetchJob
│   ├── dds_generate.rs     # DdsGenerateJob
│   └── index_rebuild.rs    # IndexRebuildJob (future)
│
├── tasks/                  # Task implementations
│   ├── mod.rs
│   ├── enumerate_dds.rs    # EnumerateDdsFilesTask
│   ├── download_chunks.rs  # DownloadChunksTask
│   ├── assemble_image.rs   # AssembleImageTask
│   ├── encode_dds.rs       # EncodeDdsTask
│   └── wait_children.rs    # WaitForChildrenTask
```

## Migration Path

### Phase 1: Core Framework

1. Implement `executor/` module with traits and executor
2. Add basic job/task implementations for testing
3. Unit tests for executor logic

### Phase 2: DDS Pipeline Migration

1. Implement `DdsGenerateJob` and its tasks
2. Wire up to existing pipeline infrastructure
3. Ensure feature parity with current DDS generation

### Phase 3: Tile Prefetch Integration

1. Implement `TilePrefetchJob`
2. Update `TileBasedPrefetcher` to use executor
3. Integration tests

### Phase 4: Deprecate Old Pipeline

1. Remove old pipeline stages
2. Update all callers to use job executor
3. Clean up dead code

## Success Criteria

- [ ] Jobs and Tasks are trait-based, allowing new implementations
- [ ] Child job spawning works correctly (1:1 task→child relationship)
- [ ] Error policies (FailFast, ContinueOnError, PartialSuccess) work correctly
- [ ] Retry policies work with exponential backoff
- [ ] Cancellation propagates from parent to children
- [ ] Concurrency limits are respected
- [ ] Progress can be tracked via JobHandle
- [ ] Existing DDS generation works via new framework
- [ ] Tile prefetch jobs work end-to-end

## Future Considerations

1. **Metrics/Observability** - Prometheus metrics for job/task durations, success rates
2. **Priority queues** - Higher priority jobs preempt lower priority
3. **Resource tagging** - Tasks declare resource requirements (http, cpu, disk)
4. **Job persistence** - Optional persistence for crash recovery (not in scope)
5. **Visualization** - Debug UI showing job graph and status

## References

- [Tile-Based Prefetch Design](tile-based-prefetch-design.md) - Consumer of this framework
- [Async Pipeline Architecture](async-pipeline-architecture.md) - Current pipeline (to be migrated)
- [Predictive Caching](predictive-caching.md) - Prefetcher integration
