//! MetricsSystem — top-level factory for creating and managing metrics components.

use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::client::MetricsClient;
use super::daemon::{MetricsDaemon, MetricsStateSnapshot, SharedMetricsState};
use super::reporter::MetricsReporter;

/// The complete metrics system.
///
/// This is the top-level factory that creates and manages all metrics
/// components. It provides:
///
/// - A [`MetricsClient`] for emitting events
/// - Access to state snapshots for reporting
/// - Graceful shutdown coordination
pub struct MetricsSystem {
    /// Client for emitting events.
    client: MetricsClient,

    /// Handle to the shared state for reporters.
    state_handle: SharedMetricsState,

    /// Handle to the daemon task.
    daemon_handle: Option<JoinHandle<()>>,

    /// Shutdown signal for the daemon.
    shutdown: CancellationToken,
}

impl MetricsSystem {
    /// Creates a new metrics system and starts the daemon.
    pub fn new(runtime_handle: &tokio::runtime::Handle) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let client = MetricsClient::new(tx);

        let daemon = MetricsDaemon::new(rx);
        let state_handle = daemon.state_handle();
        let shutdown = CancellationToken::new();

        let daemon_shutdown = shutdown.clone();
        let daemon_handle = Some(runtime_handle.spawn(async move {
            daemon.run(daemon_shutdown).await;
        }));

        Self {
            client,
            state_handle,
            daemon_handle,
            shutdown,
        }
    }

    /// Returns a clone of the metrics client.
    pub fn client(&self) -> MetricsClient {
        self.client.clone()
    }

    /// Returns a handle to the shared metrics state.
    pub fn state_handle(&self) -> SharedMetricsState {
        Arc::clone(&self.state_handle)
    }

    /// Generates a snapshot using the provided reporter.
    pub fn snapshot<R, O>(&self, reporter: &R) -> O
    where
        R: MetricsReporter<Output = O>,
    {
        let guard = self.state_handle.read().unwrap();
        reporter.report(&guard.state, &guard.history)
    }

    /// Returns a snapshot of the current state without a reporter.
    pub fn state_snapshot(&self) -> MetricsStateSnapshot {
        self.state_handle.read().unwrap().clone()
    }

    /// Shuts down the metrics system gracefully.
    pub async fn shutdown(mut self) {
        self.shutdown.cancel();
        if let Some(handle) = self.daemon_handle.take() {
            let _ = handle.await;
        }
    }

    /// Returns true if the daemon is still running.
    pub fn is_running(&self) -> bool {
        self.daemon_handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for MetricsSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsSystem")
            .field("running", &self.is_running())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::reporter::TuiReporter;
    use std::time::Duration;

    #[tokio::test]
    async fn test_metrics_system_lifecycle() {
        let runtime = tokio::runtime::Handle::current();
        let system = MetricsSystem::new(&runtime);

        assert!(system.is_running());

        let client = system.client();
        client.download_started();
        client.download_completed(1024, 5000);

        tokio::time::sleep(Duration::from_millis(150)).await;

        let reporter = TuiReporter::new();
        let snapshot = system.snapshot(&reporter);
        assert_eq!(snapshot.chunks_downloaded, 1);
        assert_eq!(snapshot.bytes_downloaded, 1024);

        system.shutdown().await;
    }

    #[tokio::test]
    async fn test_state_snapshot() {
        let runtime = tokio::runtime::Handle::current();
        let system = MetricsSystem::new(&runtime);

        let client = system.client();
        client.job_submitted(true);
        client.job_started();

        tokio::time::sleep(Duration::from_millis(150)).await;

        let snapshot = system.state_snapshot();
        assert_eq!(snapshot.state.jobs_submitted, 1);
        assert_eq!(snapshot.state.fuse_jobs_submitted, 1);
        assert_eq!(snapshot.state.jobs_active, 1);

        system.shutdown().await;
    }

    #[tokio::test]
    async fn test_state_handle_access() {
        let runtime = tokio::runtime::Handle::current();
        let system = MetricsSystem::new(&runtime);

        let handle = system.state_handle();

        {
            let _guard1 = handle.read().unwrap();
        }
        {
            let _guard2 = handle.read().unwrap();
        }

        system.shutdown().await;
    }

    #[tokio::test]
    async fn test_debug_output() {
        let runtime = tokio::runtime::Handle::current();
        let system = MetricsSystem::new(&runtime);

        let debug = format!("{:?}", system);
        assert!(debug.contains("MetricsSystem"));
        assert!(debug.contains("running"));

        system.shutdown().await;
    }
}
