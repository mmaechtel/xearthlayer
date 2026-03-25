//! Counting semaphore for concurrency-limited parallel downloads.
//!
//! Uses `Condvar` + `Mutex` to implement a counting semaphore with RAII permit.
//! This avoids external dependencies while providing thread-safe concurrency limiting.

use std::sync::{Condvar, Mutex};

/// A counting semaphore that limits concurrent access to a resource.
///
/// Threads acquire permits before proceeding. When all permits are held,
/// subsequent acquire calls block until a permit is released (via RAII drop).
pub struct CountingSemaphore {
    count: Mutex<usize>,
    condvar: Condvar,
}

/// RAII guard that releases a semaphore permit when dropped.
pub struct SemaphorePermit<'a> {
    semaphore: &'a CountingSemaphore,
}

impl CountingSemaphore {
    /// Create a new semaphore with the given number of permits.
    pub fn new(permits: usize) -> Self {
        Self {
            count: Mutex::new(permits),
            condvar: Condvar::new(),
        }
    }

    /// Acquire a permit, blocking if none are available.
    ///
    /// Returns a `SemaphorePermit` that releases the permit when dropped.
    pub fn acquire(&self) -> SemaphorePermit<'_> {
        let mut count = self.count.lock().unwrap();
        while *count == 0 {
            count = self.condvar.wait(count).unwrap();
        }
        *count -= 1;
        SemaphorePermit { semaphore: self }
    }

    fn release(&self) {
        let mut count = self.count.lock().unwrap();
        *count += 1;
        self.condvar.notify_one();
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.semaphore.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn test_semaphore_limits_concurrency() {
        let sem = Arc::new(CountingSemaphore::new(2));
        let max_active = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..6)
            .map(|_| {
                let sem = sem.clone();
                let active = active.clone();
                let max_active = max_active.clone();
                thread::spawn(move || {
                    let _permit = sem.acquire();
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(current, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(50));
                    active.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert!(max_active.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn test_semaphore_drop_releases_permit() {
        let sem = Arc::new(CountingSemaphore::new(1));
        {
            let _permit = sem.acquire();
            // permit held
        }
        // permit dropped — should be able to acquire again immediately
        let start = Instant::now();
        let _permit = sem.acquire();
        assert!(start.elapsed() < Duration::from_millis(100)); // generous tolerance for CI
    }

    #[test]
    fn test_semaphore_all_permits_used() {
        let sem = CountingSemaphore::new(3);
        let _p1 = sem.acquire();
        let _p2 = sem.acquire();
        let _p3 = sem.acquire();
        // All 3 permits consumed — a 4th would block
        // (can't easily test blocking without a timeout, so just verify we got 3)
        assert_eq!(*sem.count.lock().unwrap(), 0);
    }
}
