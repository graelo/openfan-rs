//! CLI configuration management
//!
//! Handles loading and saving CLI-specific configuration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Default server URL
    pub server_url: String,

    /// Default output format
    pub output_format: String,

    /// Enable verbose logging by default
    pub verbose: bool,

    /// Request timeout in seconds
    pub timeout: u64,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:3000".to_string(),
            output_format: "table".to_string(),
            verbose: false,
            timeout: 10,
        }
    }
}

impl CliConfig {
    /// Load configuration from file or create default
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read CLI config file")?;

            toml::from_str(&content).context("Failed to parse CLI config file")
        } else {
            // Create default config and save it
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize CLI config")?;

        std::fs::write(&config_path, content).context("Failed to write CLI config file")?;

        Ok(())
    }

    /// Get the configuration file path
    fn config_path() -> Result<PathBuf> {
        let config_dir = if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg_config)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config")
        } else {
            return Err(anyhow::anyhow!("Cannot determine config directory"));
        };

        Ok(config_dir.join("openfan").join("cli.toml"))
    }

    /// Update configuration with environment variables
    pub fn apply_env_overrides(&mut self) {
        if let Ok(server_url) = std::env::var("OPENFAN_SERVER") {
            self.server_url = server_url;
        }

        if let Ok(format) = std::env::var("OPENFAN_FORMAT") {
            self.output_format = format;
        }

        if let Ok(verbose) = std::env::var("OPENFAN_VERBOSE") {
            self.verbose = verbose.to_lowercase() == "true" || verbose == "1";
        }

        if let Ok(timeout) = std::env::var("OPENFAN_TIMEOUT") {
            if let Ok(timeout) = timeout.parse() {
                self.timeout = timeout;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert_eq!(config.server_url, "http://localhost:3000");
        assert_eq!(config.output_format, "table");
        assert!(!config.verbose);
        assert_eq!(config.timeout, 10);
    }

    #[test]
    fn test_config_serialization() {
        let config = CliConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: CliConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.server_url, parsed.server_url);
        assert_eq!(config.output_format, parsed.output_format);
        assert_eq!(config.verbose, parsed.verbose);
        assert_eq!(config.timeout, parsed.timeout);
    }

    #[test]
    fn test_env_overrides() {
        std::env::set_var("OPENFAN_SERVER", "http://example.com:8080");
        std::env::set_var("OPENFAN_FORMAT", "json");
        std::env::set_var("OPENFAN_VERBOSE", "true");
        std::env::set_var("OPENFAN_TIMEOUT", "30");

        let mut config = CliConfig::default();
        config.apply_env_overrides();

        assert_eq!(config.server_url, "http://example.com:8080");
        assert_eq!(config.output_format, "json");
        assert!(config.verbose);
        assert_eq!(config.timeout, 30);

        // Clean up
        std::env::remove_var("OPENFAN_SERVER");
        std::env::remove_var("OPENFAN_FORMAT");
        std::env::remove_var("OPENFAN_VERBOSE");
        std::env::remove_var("OPENFAN_TIMEOUT");
    }
}
