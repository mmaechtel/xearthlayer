//! Dashboard widgets for the TUI.

mod cache;
mod errors;
pub mod network;
mod pipeline;

pub use cache::{CacheConfig, CacheWidget};
pub use errors::ErrorsWidget;
pub use network::{NetworkHistory, NetworkWidget};
pub use pipeline::PipelineWidget;
