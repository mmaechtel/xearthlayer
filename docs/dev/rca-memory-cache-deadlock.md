# Root Cause Analysis: Memory Cache Deadlock

**Date:** 2025-12-25
**Severity:** Critical
**Status:** Root cause identified, fix in progress
**Branch:** `feature/concurrency-hardening`

## Executive Summary

XEL experienced a complete process freeze during flight operations. All 133 threads became blocked on mutex synchronization, rendering the process unresponsive while FUSE mounts remained active. The root cause is the use of `std::sync::Mutex` (a blocking mutex) in the `MemoryCache` implementation, which starves the Tokio async runtime when LRU eviction is triggered under load.

## Incident Timeline

| Time (UTC) | Event |
|------------|-------|
| 04:02:12 | Normal operation - cache hits responding in 0-20ms |
| 04:02:13 | Cache misses trigger tile generation (200-300ms) |
| 04:02:13 | Last log entry - 6 tiles generated successfully |
| ~04:02:13 | Process freezes - no further log output |
| +57 min | Issue detected - process unresponsive |

## Symptoms

- Process PID 2435796 running but completely frozen
- 133 threads total, all on `futex_wait` (mutex waiting)
- Only 3 threads on `epoll_wait` (event loop)
- FUSE mounts still active (process not crashed)
- No panic messages or errors in logs
- 30% CPU usage before freeze (normal), 0% after
- File descriptors (162) and memory (1.5GB) within normal limits

## Root Cause

### The Problem

The `MemoryCache` implementation in `xearthlayer/src/cache/memory.rs` uses `std::sync::Mutex` for thread synchronization:

```rust
pub struct MemoryCache {
    cache: Arc<Mutex<HashMap<CacheKey, CacheEntry>>>,  // std::sync::Mutex
    current_size_bytes: Arc<Mutex<usize>>,              // std::sync::Mutex
    // ...
}
```

When called from async context, `std::sync::Mutex::lock()` **blocks the OS thread**, not just the async task. This prevents Tokio from scheduling other work on that thread.

### The Trigger: LRU Eviction

The `evict_lru_until_size()` function (lines 175-213) performs expensive operations while holding the blocking mutex:

```rust
fn evict_lru_until_size(&self, required_size: usize) -> Result<(), CacheError> {
    let mut cache = self.cache.lock().unwrap();           // Acquire lock 1
    let mut current_size = self.current_size_bytes.lock().unwrap();  // Acquire lock 2

    // Collect ALL entries into a vector
    let mut entries: Vec<...> = cache.iter().map(...).collect();

    // O(n log n) sort of all entries
    entries.sort_by_key(|(_, accessed, _)| *accessed);

    // Iterate through entries to evict
    for (key, _, size) in entries { ... }
}
```

With a 2GB cache containing ~180+ DDS tiles (~11MB each), this operation can take significant time.

### The Deadlock Sequence

```
1. FUSE requests arrive (X-Plane burst loading)
   ↓
2. Tokio workers process requests via memory_cache.get()/put()
   ↓
3. Worker A triggers eviction → holds blocking mutex during O(n log n) sort
   ↓
4. Workers B, C, D... call cache.lock().unwrap() → BLOCK (not yield)
   ↓
5. Blocked workers can't run other Tokio tasks
   ↓
6. All 64 tokio workers eventually blocked on the same mutex
   ↓
7. Health monitor (also a Tokio task) can't run to detect/recover
   ↓
8. Complete system freeze
```

### Code Path

```
runner.rs (async context)
  → process_dds_request_with_control_plane (async)
    → control_plane.submit() (async)
      → process_tile_with_observer (async)
        → cache_stage (async)
          → memory_cache.put()           // Synchronous call from async
            → MemoryCache::put()         // memory.rs:102
              → evict_lru_until_size()   // BLOCKING mutex + O(n log n) sort
```

The cache stage is marked as `async fn` but calls synchronous blocking code:

```rust
// cache.rs:39-40
// Memory cache is synchronous (fast in-memory operation)  // <-- INCORRECT ASSUMPTION
memory_cache.put(tile.row, tile.col, tile.zoom, dds_data.to_vec());
```

## Why the Health Monitor Couldn't Help

The control plane includes a health monitor that detects stalled jobs (60s threshold) and initiates recovery. However:

1. The health monitor runs as a Tokio task
2. It needs a Tokio worker to execute its `interval.tick()` future
3. All workers were blocked on the memory cache mutex
4. Therefore, the health monitor never got CPU time to detect the stall

This is a fundamental limitation: the recovery mechanism shares the same thread pool as the blocked work.

## Evidence

| Evidence | Interpretation |
|----------|----------------|
| All threads on `futex_wait` | Classic mutex contention pattern |
| 3 threads on `epoll_wait` | Event loop threads can't wake blocked workers |
| Last logs show cache misses | New tiles being generated → cache puts → potential eviction |
| 6 concurrent cache misses | Burst of puts likely triggered eviction |
| No errors before freeze | Clean deadlock, not a crash or panic |
| FUSE mounts still active | Kernel FUSE driver waiting on frozen userspace |

## Solution Options

### Option 1: spawn_blocking (Not Recommended)

Wrap `memory_cache.put()` in `tokio::task::spawn_blocking()`.

**Pros:**
- Quick to implement
- Moves blocking work off Tokio workers

**Cons:**
- Band-aid that moves the problem to blocking thread pool
- Blocking pool can also become saturated
- O(n log n) eviction still exists
- Adds scheduling overhead

**Verdict:** Not recommended - treats symptoms, not cause.

### Option 2: tokio::sync::Mutex (Partial Fix)

Replace `std::sync::Mutex` with `tokio::sync::Mutex`.

**Pros:**
- Tasks `await` instead of block (Tokio can schedule other work)
- Relatively simple code change
- Health monitor could run during contention

**Cons:**
- Eviction still holds lock during expensive sort
- Waiting tasks queue up, causing cascading FUSE timeouts
- O(n log n) eviction complexity remains
- Still serializes all cache access

**Verdict:** Better, but doesn't fully solve the problem.

### Option 3: Async-Native Cache (Recommended)

Replace custom `MemoryCache` with an async-native cache like `moka`.

**Pros:**
- Designed for async/concurrent contexts from the ground up
- Fine-grained or lock-free internal synchronization
- Eviction is amortized and non-blocking
- Battle-tested for high-concurrency scenarios
- Automatic entry expiration and size-based eviction
- Excellent async support (`moka::future::Cache`)

**Cons:**
- More significant code change
- New dependency
- API differences require adapter updates

**Verdict:** Recommended - addresses root cause with production-grade solution.

## Recommended Fix

Replace the custom `MemoryCache` implementation with `moka::future::Cache`:

```rust
// Before: Custom implementation with std::sync::Mutex
pub struct MemoryCache {
    cache: Arc<Mutex<HashMap<CacheKey, CacheEntry>>>,
    // ...
}

// After: moka async cache
use moka::future::Cache;

pub struct MemoryCache {
    cache: Cache<CacheKey, Vec<u8>>,
}

impl MemoryCache {
    pub fn new(max_size_bytes: u64) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(max_size_bytes)
                .build(),
        }
    }

    pub async fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.cache.get(key).await
    }

    pub async fn put(&self, key: CacheKey, data: Vec<u8>) {
        self.cache.insert(key, data).await;
    }
}
```

Key benefits of `moka`:
- Lock-free reads (most common operation)
- Concurrent writes without blocking
- Automatic LRU eviction without explicit locking
- Memory-bounded with configurable limits
- Async-first API design

## Impact Assessment

| Area | Impact |
|------|--------|
| API Changes | `MemoryCache` trait methods become async |
| Dependencies | Add `moka` crate |
| Performance | Expected improvement due to lock-free reads |
| Stability | Eliminates deadlock risk |
| Testing | Existing tests need async adaptation |

## Prevention Measures

1. **Code Review Focus:** Flag `std::sync::Mutex` usage in async contexts
2. **Linting:** Consider clippy lint for blocking in async
3. **Documentation:** Update CLAUDE.md with async-safety guidelines
4. **Testing:** Add stress tests that simulate burst loading patterns

## References

- [Tokio: Bridging with sync code](https://tokio.rs/tokio/topics/bridging)
- [moka: A fast, concurrent cache library](https://github.com/moka-rs/moka)
- [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/)

## Appendix: Thread State at Time of Freeze

```
Thread Distribution:
- 64 xearthlayer-tok (Tokio workers) - all on futex_wait
- 32 tile-worker-* threads - all on futex_wait
- 3 threads on epoll_wait (event loop)
- Remaining: runtime infrastructure

Key Observation:
All application threads blocked on synchronization primitives,
indicating classic mutex starvation pattern.
```
