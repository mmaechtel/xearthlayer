//! Configuration management CLI commands.
//!
//! Provides `config get`, `config set`, `config list`, and `config path` commands
//! for viewing and modifying configuration settings from the command line.

use clap::Subcommand;
use xearthlayer::config::{config_file_path, ConfigFile, ConfigKey};

use crate::error::CliError;

/// Config subcommands.
#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Get a configuration value
    Get {
        /// Configuration key in format section.key (e.g., packages.library_url)
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key in format section.key (e.g., packages.library_url)
        key: String,

        /// Value to set
        value: String,
    },

    /// List all configuration settings
    List,

    /// Show the configuration file path
    Path,
}

/// Run a config subcommand.
pub fn run(command: ConfigCommands) -> Result<(), CliError> {
    match command {
        ConfigCommands::Get { key } => run_get(&key),
        ConfigCommands::Set { key, value } => run_set(&key, &value),
        ConfigCommands::List => run_list(),
        ConfigCommands::Path => run_path(),
    }
}

/// Get a configuration value.
fn run_get(key: &str) -> Result<(), CliError> {
    let config_key: ConfigKey = key.parse().map_err(|_| {
        CliError::Config(format!(
            "Unknown configuration key '{}'. Use 'xearthlayer config list' to see available keys.",
            key
        ))
    })?;

    let config = ConfigFile::load().unwrap_or_default();
    let value = config_key.get(&config);

    if value.is_empty() {
        println!("(not set)");
    } else {
        println!("{}", value);
    }

    Ok(())
}

/// Set a configuration value.
fn run_set(key: &str, value: &str) -> Result<(), CliError> {
    let config_key: ConfigKey = key.parse().map_err(|_| {
        CliError::Config(format!(
            "Unknown configuration key '{}'. Use 'xearthlayer config list' to see available keys.",
            key
        ))
    })?;

    let mut config = ConfigFile::load().unwrap_or_default();
    config_key
        .set(&mut config, value)
        .map_err(|e| CliError::Config(e.to_string()))?;
    config.save()?;

    println!("Set {} = {}", config_key.name(), value);

    Ok(())
}

/// List all configuration settings.
fn run_list() -> Result<(), CliError> {
    let config = ConfigFile::load().unwrap_or_default();

    println!("Configuration Settings");
    println!("======================");
    println!();

    let mut current_section = "";

    for key in ConfigKey::all() {
        let section = key.section();

        // Print section header when section changes
        if section != current_section {
            if !current_section.is_empty() {
                println!();
            }
            println!("[{}]", section);
            current_section = section;
        }

        let value = key.get(&config);
        let key_name = key.key_name();

        if value.is_empty() {
            println!("  {} = (not set)", key_name);
        } else {
            println!("  {} = {}", key_name, value);
        }
    }

    Ok(())
}

/// Show the configuration file path.
fn run_path() -> Result<(), CliError> {
    println!("{}", config_file_path().display());
    Ok(())
}
