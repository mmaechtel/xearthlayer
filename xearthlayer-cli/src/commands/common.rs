//! Common types and utilities shared across CLI commands.

use clap::ValueEnum;
use xearthlayer::config::ConfigFile;
use xearthlayer::dds::DdsFormat;
use xearthlayer::provider::ProviderConfig;

use crate::error::CliError;

/// Imagery provider selection for CLI arguments.
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum ProviderType {
    /// Bing Maps aerial imagery (no API key required)
    Bing,
    /// Google Maps via public tile servers (no API key required, same as Ortho4XP GO2)
    Go2,
    /// Google Maps official API (requires API key, has usage limits)
    Google,
}

impl ProviderType {
    /// Convert to a ProviderConfig, requiring API key for Google provider.
    pub fn to_config(&self, api_key: Option<String>) -> Result<ProviderConfig, CliError> {
        match self {
            ProviderType::Bing => Ok(ProviderConfig::bing()),
            ProviderType::Go2 => Ok(ProviderConfig::go2()),
            ProviderType::Google => {
                let key = api_key.ok_or_else(|| {
                    CliError::Config(
                        "Google Maps provider requires an API key. \
                         Set google_api_key in config.ini or use --google-api-key"
                            .to_string(),
                    )
                })?;
                Ok(ProviderConfig::google(key))
            }
        }
    }

    /// Parse from config file string.
    pub fn from_config_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bing" => Some(ProviderType::Bing),
            "go2" => Some(ProviderType::Go2),
            "google" => Some(ProviderType::Google),
            _ => None,
        }
    }
}

/// DDS compression format selection for CLI arguments.
#[derive(Debug, Clone, ValueEnum)]
pub enum DdsCompression {
    /// BC1/DXT1 compression (4:1, best for opaque textures)
    Bc1,
    /// BC3/DXT5 compression (4:1, with full alpha channel)
    Bc3,
}

impl From<DdsCompression> for DdsFormat {
    fn from(compression: DdsCompression) -> Self {
        match compression {
            DdsCompression::Bc1 => DdsFormat::BC1,
            DdsCompression::Bc3 => DdsFormat::BC3,
        }
    }
}

/// Resolve provider settings from CLI args and config.
pub fn resolve_provider(
    cli_provider: Option<ProviderType>,
    cli_api_key: Option<String>,
    config: &ConfigFile,
) -> Result<ProviderConfig, CliError> {
    // CLI takes precedence, then config
    let provider = cli_provider
        .or_else(|| ProviderType::from_config_str(&config.provider.provider_type))
        .unwrap_or(ProviderType::Bing);

    let api_key = cli_api_key.or_else(|| config.provider.google_api_key.clone());

    provider.to_config(api_key)
}

/// Resolve DDS format from CLI args and config.
pub fn resolve_dds_format(cli_format: Option<DdsCompression>, config: &ConfigFile) -> DdsFormat {
    cli_format
        .map(DdsFormat::from)
        .unwrap_or(config.texture.format)
}
