//! Types for the fuse3 filesystem implementation.

use fuse3::raw::MountHandle as Fuse3MountHandle;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Command;
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Result type for fuse3 operations.
pub type Fuse3Result<T> = Result<T, Fuse3Error>;

/// Errors that can occur in the fuse3 filesystem.
#[derive(Debug, Error)]
pub enum Fuse3Error {
    /// I/O error during filesystem operations
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Mount operation failed
    #[error("Mount failed: {0}")]
    MountFailed(String),

    /// Invalid path
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Handle to a mounted fuse3 filesystem.
///
/// When dropped, the filesystem is automatically unmounted.
/// This is a wrapper around fuse3's MountHandle that provides
/// a cleaner API for XEarthLayer.
///
/// The handle can be awaited - it will resolve when the filesystem
/// is unmounted (e.g., via Ctrl+C or `fusermount -u`).
pub struct MountHandle {
    inner: Fuse3MountHandle,
}

impl MountHandle {
    /// Create a new mount handle from a fuse3 mount handle.
    pub(crate) fn new(inner: Fuse3MountHandle) -> Self {
        Self { inner }
    }

    /// Unmount the filesystem.
    ///
    /// This is called automatically when the handle is dropped,
    /// but can be called explicitly for more control.
    pub async fn unmount(self) -> io::Result<()> {
        self.inner.unmount().await
    }
}

/// Implement Future so the handle can be awaited.
/// Resolves when the filesystem is unmounted.
impl Future for MountHandle {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Delegate to the inner MountHandle's Future implementation
        Pin::new(&mut self.inner).poll(cx)
    }
}

/// Handle to a spawned fuse3 filesystem task.
///
/// This wraps a `JoinHandle` for the fuse3 mount task, allowing the mount
/// to run in the background while providing control over unmounting.
///
/// Unlike `MountHandle`, this can be safely stored and dropped outside
/// of an async context because the actual fuse3 handle is managed by
/// the spawned task.
pub struct SpawnedMountHandle {
    /// The spawned task handle
    task: Option<JoinHandle<io::Result<()>>>,
    /// Channel to signal unmount
    unmount_tx: Option<oneshot::Sender<()>>,
    /// Mountpoint for fallback unmount via fusermount
    mountpoint: PathBuf,
}

impl SpawnedMountHandle {
    /// Create a new spawned mount handle.
    pub(crate) fn new(
        task: JoinHandle<io::Result<()>>,
        unmount_tx: oneshot::Sender<()>,
        mountpoint: PathBuf,
    ) -> Self {
        Self {
            task: Some(task),
            unmount_tx: Some(unmount_tx),
            mountpoint,
        }
    }

    /// Unmount the filesystem asynchronously.
    ///
    /// Signals the mount task to unmount and waits for it to complete.
    pub async fn unmount(mut self) -> io::Result<()> {
        // Signal the task to unmount
        if let Some(tx) = self.unmount_tx.take() {
            let _ = tx.send(());
        }

        // Wait for the task to complete
        if let Some(task) = self.task.take() {
            match task.await {
                Ok(result) => result,
                Err(e) => Err(io::Error::other(format!("Mount task panicked: {}", e))),
            }
        } else {
            Ok(())
        }
    }

    /// Unmount the filesystem synchronously using fusermount.
    ///
    /// This is a fallback for when we can't use async unmount.
    pub fn unmount_sync(&mut self) {
        // Signal the task to stop (if channel still exists)
        if let Some(tx) = self.unmount_tx.take() {
            let _ = tx.send(());
        }

        // Use fusermount to unmount
        let mountpoint_str = self.mountpoint.to_string_lossy();
        debug!(mountpoint = %mountpoint_str, "Unmounting via fusermount");

        match Command::new("fusermount")
            .args(["-u", &mountpoint_str])
            .output()
        {
            Ok(output) => {
                if !output.status.success() {
                    warn!(
                        mountpoint = %mountpoint_str,
                        stderr = %String::from_utf8_lossy(&output.stderr),
                        "fusermount -u failed"
                    );
                }
            }
            Err(e) => {
                warn!(
                    mountpoint = %mountpoint_str,
                    error = %e,
                    "Failed to run fusermount"
                );
            }
        }

        // Cancel the task
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for SpawnedMountHandle {
    fn drop(&mut self) {
        // If we're being dropped without explicit unmount, try fusermount
        if self.task.is_some() {
            self.unmount_sync();
        }
    }
}
