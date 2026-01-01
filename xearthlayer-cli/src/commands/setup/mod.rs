//! Setup wizard command for first-time configuration.
//!
//! Provides an interactive TUI-based wizard that guides users through
//! initial XEarthLayer configuration. Detects system hardware and
//! X-Plane installation to recommend optimal settings.

mod detection;
mod wizard;

use crate::error::CliError;

/// Run the setup wizard.
pub fn run() -> Result<(), CliError> {
    wizard::run_wizard()
}
