//! Priority queue for task scheduling.
//!
//! Tasks are ordered by priority (higher values first), then by enqueue time
//! (FIFO within the same priority level). This ensures:
//!
//! 1. ON_DEMAND requests preempt PREFETCH work
//! 2. Tasks at the same priority are processed in order
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::executor::{PriorityQueue, QueuedTask, Priority};
//!
//! let mut queue = PriorityQueue::new();
//!
//! queue.push(QueuedTask::new(prefetch_task, Priority::PREFETCH));
//! queue.push(QueuedTask::new(on_demand_task, Priority::ON_DEMAND));
//!
//! // ON_DEMAND task comes out first despite being pushed second
//! let next = queue.pop();
//! assert_eq!(next.unwrap().priority, Priority::ON_DEMAND);
//! ```

use super::job::JobId;
use super::policy::Priority;
use super::resource_pool::ResourceType;
use super::task::Task;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Instant;

// =============================================================================
// Sequence Number Generator
// =============================================================================

/// Global sequence counter for FIFO ordering within priority levels.
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates a unique sequence number for queue ordering.
fn next_sequence() -> u64 {
    SEQUENCE_COUNTER.fetch_add(1, AtomicOrdering::Relaxed)
}

// =============================================================================
// Queued Task
// =============================================================================

/// A task waiting to be executed.
///
/// This wrapper holds a task along with metadata needed for scheduling:
/// - Priority for ordering
/// - Sequence number for FIFO within same priority
/// - Enqueue time for wait time calculation
/// - Job context (ID, resource type)
pub struct QueuedTask {
    /// The task to execute.
    pub task: Box<dyn Task>,

    /// The job this task belongs to.
    pub job_id: JobId,

    /// Task priority (higher = more important).
    pub priority: Priority,

    /// Resource type required by this task.
    pub resource_type: ResourceType,

    /// Sequence number for FIFO ordering within priority level.
    sequence: u64,

    /// When the task was enqueued (for wait time telemetry).
    pub enqueued_at: Instant,
}

impl QueuedTask {
    /// Creates a new queued task.
    ///
    /// The sequence number is automatically assigned for FIFO ordering.
    pub fn new(task: Box<dyn Task>, job_id: JobId, priority: Priority) -> Self {
        let resource_type = task.resource_type();
        Self {
            task,
            job_id,
            priority,
            resource_type,
            sequence: next_sequence(),
            enqueued_at: Instant::now(),
        }
    }

    /// Returns how long this task has been waiting in the queue.
    pub fn wait_time(&self) -> std::time::Duration {
        self.enqueued_at.elapsed()
    }

    /// Returns the task name.
    pub fn task_name(&self) -> &str {
        self.task.name()
    }
}

impl std::fmt::Debug for QueuedTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedTask")
            .field("task_name", &self.task.name())
            .field("job_id", &self.job_id)
            .field("priority", &self.priority)
            .field("resource_type", &self.resource_type)
            .field("sequence", &self.sequence)
            .finish()
    }
}

// Ordering for BinaryHeap: higher priority first, then lower sequence (older) first
impl PartialEq for QueuedTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for QueuedTask {}

impl PartialOrd for QueuedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap, so we want:
        // 1. Higher priority first (natural ordering)
        // 2. Lower sequence first (reverse ordering) for FIFO within priority
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                // Reverse sequence ordering: older (lower sequence) should come first
                other.sequence.cmp(&self.sequence)
            }
            other_ordering => other_ordering,
        }
    }
}

// =============================================================================
// Priority Queue
// =============================================================================

/// Priority queue for scheduling tasks.
///
/// Tasks are ordered by:
/// 1. Priority (descending) - ON_DEMAND before PREFETCH
/// 2. Enqueue order (ascending) - FIFO within same priority
///
/// The queue is not thread-safe; use external synchronization if needed
/// (the executor wraps it in a Mutex).
pub struct PriorityQueue {
    heap: BinaryHeap<QueuedTask>,
}

impl PriorityQueue {
    /// Creates a new empty priority queue.
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    /// Creates a priority queue with the specified initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(capacity),
        }
    }

    /// Adds a task to the queue.
    pub fn push(&mut self, task: QueuedTask) {
        self.heap.push(task);
    }

    /// Removes and returns the highest-priority task.
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<QueuedTask> {
        self.heap.pop()
    }

    /// Returns a reference to the highest-priority task without removing it.
    pub fn peek(&self) -> Option<&QueuedTask> {
        self.heap.peek()
    }

    /// Returns the number of tasks in the queue.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Removes all tasks from the queue.
    pub fn clear(&mut self) {
        self.heap.clear();
    }

    /// Removes all tasks belonging to a specific job.
    ///
    /// Returns the number of tasks removed.
    pub fn remove_job(&mut self, job_id: &JobId) -> usize {
        let before = self.heap.len();
        let remaining: Vec<_> = self.heap.drain().filter(|t| t.job_id != *job_id).collect();
        let removed = before - remaining.len();
        self.heap = BinaryHeap::from(remaining);
        removed
    }

    /// Returns an iterator over tasks (in arbitrary order, not priority order).
    ///
    /// For priority-ordered iteration, repeatedly call `pop()`.
    pub fn iter(&self) -> impl Iterator<Item = &QueuedTask> {
        self.heap.iter()
    }

    /// Returns the number of tasks at each priority level.
    pub fn priority_counts(&self) -> std::collections::HashMap<Priority, usize> {
        let mut counts = std::collections::HashMap::new();
        for task in self.heap.iter() {
            *counts.entry(task.priority).or_insert(0) += 1;
        }
        counts
    }

    /// Returns the number of tasks for each resource type.
    pub fn resource_type_counts(&self) -> std::collections::HashMap<ResourceType, usize> {
        let mut counts = std::collections::HashMap::new();
        for task in self.heap.iter() {
            *counts.entry(task.resource_type).or_insert(0) += 1;
        }
        counts
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PriorityQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PriorityQueue")
            .field("len", &self.heap.len())
            .field("priority_counts", &self.priority_counts())
            .finish()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::policy::RetryPolicy;
    use std::future::Future;
    use std::pin::Pin;

    /// A minimal test task.
    struct TestTask {
        name: String,
        resource_type: ResourceType,
    }

    impl TestTask {
        fn new(name: &str, resource_type: ResourceType) -> Self {
            Self {
                name: name.to_string(),
                resource_type,
            }
        }
    }

    impl Task for TestTask {
        fn name(&self) -> &str {
            &self.name
        }

        fn resource_type(&self) -> ResourceType {
            self.resource_type
        }

        fn retry_policy(&self) -> RetryPolicy {
            RetryPolicy::None
        }

        fn execute<'a>(
            &'a self,
            _ctx: &'a mut super::super::context::TaskContext,
        ) -> Pin<Box<dyn Future<Output = super::super::task::TaskResult> + Send + 'a>> {
            Box::pin(async { super::super::task::TaskResult::Success })
        }
    }

    fn make_task(name: &str, priority: Priority) -> QueuedTask {
        QueuedTask::new(
            Box::new(TestTask::new(name, ResourceType::CPU)),
            JobId::new("test-job"),
            priority,
        )
    }

    #[test]
    fn test_priority_ordering() {
        let mut queue = PriorityQueue::new();

        // Push in arbitrary order
        queue.push(make_task("housekeeping", Priority::HOUSEKEEPING));
        queue.push(make_task("on_demand", Priority::ON_DEMAND));
        queue.push(make_task("prefetch", Priority::PREFETCH));

        // Should come out in priority order
        assert_eq!(queue.pop().unwrap().task_name(), "on_demand");
        assert_eq!(queue.pop().unwrap().task_name(), "prefetch");
        assert_eq!(queue.pop().unwrap().task_name(), "housekeeping");
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_fifo_within_priority() {
        let mut queue = PriorityQueue::new();

        // Push three tasks at same priority
        queue.push(make_task("first", Priority::PREFETCH));
        queue.push(make_task("second", Priority::PREFETCH));
        queue.push(make_task("third", Priority::PREFETCH));

        // Should come out in FIFO order
        assert_eq!(queue.pop().unwrap().task_name(), "first");
        assert_eq!(queue.pop().unwrap().task_name(), "second");
        assert_eq!(queue.pop().unwrap().task_name(), "third");
    }

    #[test]
    fn test_mixed_priority_and_fifo() {
        let mut queue = PriorityQueue::new();

        // Interleave different priorities
        queue.push(make_task("prefetch1", Priority::PREFETCH));
        queue.push(make_task("on_demand1", Priority::ON_DEMAND));
        queue.push(make_task("prefetch2", Priority::PREFETCH));
        queue.push(make_task("on_demand2", Priority::ON_DEMAND));
        queue.push(make_task("housekeeping1", Priority::HOUSEKEEPING));

        // ON_DEMAND first (FIFO), then PREFETCH (FIFO), then HOUSEKEEPING
        assert_eq!(queue.pop().unwrap().task_name(), "on_demand1");
        assert_eq!(queue.pop().unwrap().task_name(), "on_demand2");
        assert_eq!(queue.pop().unwrap().task_name(), "prefetch1");
        assert_eq!(queue.pop().unwrap().task_name(), "prefetch2");
        assert_eq!(queue.pop().unwrap().task_name(), "housekeeping1");
    }

    #[test]
    fn test_queue_operations() {
        let mut queue = PriorityQueue::new();

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);

        queue.push(make_task("task1", Priority::PREFETCH));
        queue.push(make_task("task2", Priority::ON_DEMAND));

        assert!(!queue.is_empty());
        assert_eq!(queue.len(), 2);

        // Peek doesn't remove
        assert_eq!(queue.peek().unwrap().task_name(), "task2");
        assert_eq!(queue.len(), 2);

        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_remove_job() {
        let mut queue = PriorityQueue::new();

        // Add tasks from different jobs
        queue.push(QueuedTask::new(
            Box::new(TestTask::new("job1_task1", ResourceType::CPU)),
            JobId::new("job1"),
            Priority::PREFETCH,
        ));
        queue.push(QueuedTask::new(
            Box::new(TestTask::new("job2_task1", ResourceType::CPU)),
            JobId::new("job2"),
            Priority::PREFETCH,
        ));
        queue.push(QueuedTask::new(
            Box::new(TestTask::new("job1_task2", ResourceType::CPU)),
            JobId::new("job1"),
            Priority::PREFETCH,
        ));

        assert_eq!(queue.len(), 3);

        // Remove job1's tasks
        let removed = queue.remove_job(&JobId::new("job1"));
        assert_eq!(removed, 2);
        assert_eq!(queue.len(), 1);

        // Only job2's task remains
        assert_eq!(queue.pop().unwrap().task_name(), "job2_task1");
    }

    #[test]
    fn test_priority_counts() {
        let mut queue = PriorityQueue::new();

        queue.push(make_task("t1", Priority::ON_DEMAND));
        queue.push(make_task("t2", Priority::ON_DEMAND));
        queue.push(make_task("t3", Priority::PREFETCH));
        queue.push(make_task("t4", Priority::HOUSEKEEPING));

        let counts = queue.priority_counts();
        assert_eq!(counts.get(&Priority::ON_DEMAND), Some(&2));
        assert_eq!(counts.get(&Priority::PREFETCH), Some(&1));
        assert_eq!(counts.get(&Priority::HOUSEKEEPING), Some(&1));
    }

    #[test]
    fn test_resource_type_counts() {
        let mut queue = PriorityQueue::new();

        queue.push(QueuedTask::new(
            Box::new(TestTask::new("net1", ResourceType::Network)),
            JobId::new("job"),
            Priority::PREFETCH,
        ));
        queue.push(QueuedTask::new(
            Box::new(TestTask::new("cpu1", ResourceType::CPU)),
            JobId::new("job"),
            Priority::PREFETCH,
        ));
        queue.push(QueuedTask::new(
            Box::new(TestTask::new("net2", ResourceType::Network)),
            JobId::new("job"),
            Priority::PREFETCH,
        ));

        let counts = queue.resource_type_counts();
        assert_eq!(counts.get(&ResourceType::Network), Some(&2));
        assert_eq!(counts.get(&ResourceType::CPU), Some(&1));
        assert_eq!(counts.get(&ResourceType::DiskIO), None);
    }

    #[test]
    fn test_queued_task_wait_time() {
        let task = make_task("test", Priority::PREFETCH);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(task.wait_time() >= std::time::Duration::from_millis(10));
    }

    #[test]
    fn test_custom_priority() {
        let mut queue = PriorityQueue::new();

        // Custom priority between ON_DEMAND and PREFETCH
        let custom = Priority::new(50);

        queue.push(make_task("prefetch", Priority::PREFETCH));
        queue.push(make_task("custom", custom));
        queue.push(make_task("on_demand", Priority::ON_DEMAND));

        assert_eq!(queue.pop().unwrap().task_name(), "on_demand");
        assert_eq!(queue.pop().unwrap().task_name(), "custom");
        assert_eq!(queue.pop().unwrap().task_name(), "prefetch");
    }
}
