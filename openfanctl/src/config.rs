//! CLI configuration management
//!
//! Handles loading and saving CLI-specific configuration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

    /// Create a new builder for constructing configuration
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }
}

/// Builder for CLI configuration with validation and priority chain support
///
/// Priority chain (lowest to highest):
/// 1. Defaults
/// 2. Config file
/// 3. Environment variables
/// 4. CLI arguments
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    server_url: Option<String>,
    output_format: Option<String>,
    verbose: Option<bool>,
    timeout: Option<u64>,
}

impl ConfigBuilder {
    /// Create a new configuration builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set server URL (with validation)
    pub fn with_server_url(mut self, url: impl Into<String>) -> Result<Self> {
        let url = url.into();
        Self::validate_url(&url)?;
        self.server_url = Some(url);
        Ok(self)
    }

    /// Set output format (with validation)
    pub fn with_output_format(mut self, format: impl Into<String>) -> Result<Self> {
        let format = format.into();
        Self::validate_output_format(&format)?;
        self.output_format = Some(format);
        Ok(self)
    }

    /// Set verbose flag
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = Some(verbose);
        self
    }

    /// Set timeout (with validation)
    pub fn with_timeout(mut self, timeout: u64) -> Result<Self> {
        Self::validate_timeout(timeout)?;
        self.timeout = Some(timeout);
        Ok(self)
    }

    /// Load configuration from file
    pub fn with_config_file(self, load_file: bool) -> Result<Self> {
        if !load_file {
            return Ok(self);
        }

        match CliConfig::load() {
            Ok(config) => {
                let builder = self;
                // Only use file values if they weren't already set (preserving priority)
                Ok(Self {
                    server_url: builder.server_url.or(Some(config.server_url)),
                    output_format: builder.output_format.or(Some(config.output_format)),
                    verbose: builder.verbose.or(Some(config.verbose)),
                    timeout: builder.timeout.or(Some(config.timeout)),
                })
            }
            Err(_) => {
                // If file doesn't exist or can't be loaded, continue with current builder
                Ok(self)
            }
        }
    }

    /// Apply environment variable overrides
    pub fn with_env_overrides(mut self) -> Self {
        // Only apply env vars if values weren't already set (preserving priority)
        if self.server_url.is_none() {
            if let Ok(server_url) = std::env::var("OPENFAN_SERVER") {
                // Validate before applying
                if Self::validate_url(&server_url).is_ok() {
                    self.server_url = Some(server_url);
                }
            }
        }

        if self.output_format.is_none() {
            if let Ok(format) = std::env::var("OPENFAN_FORMAT") {
                // Validate before applying
                if Self::validate_output_format(&format).is_ok() {
                    self.output_format = Some(format);
                }
            }
        }

        if self.verbose.is_none() {
            if let Ok(verbose) = std::env::var("OPENFAN_VERBOSE") {
                self.verbose = Some(verbose.to_lowercase() == "true" || verbose == "1");
            }
        }

        if self.timeout.is_none() {
            if let Ok(timeout) = std::env::var("OPENFAN_TIMEOUT") {
                if let Ok(timeout) = timeout.parse() {
                    // Validate before applying
                    if Self::validate_timeout(timeout).is_ok() {
                        self.timeout = Some(timeout);
                    }
                }
            }
        }

        self
    }

    /// Build the final configuration with validation
    pub fn build(self) -> Result<CliConfig> {
        let defaults = CliConfig::default();

        let server_url = self.server_url.unwrap_or(defaults.server_url);
        let output_format = self.output_format.unwrap_or(defaults.output_format);
        let timeout = self.timeout.unwrap_or(defaults.timeout);

        // Validate final values
        Self::validate_url(&server_url)?;
        Self::validate_output_format(&output_format)?;
        Self::validate_timeout(timeout)?;

        Ok(CliConfig {
            server_url,
            output_format,
            verbose: self.verbose.unwrap_or(defaults.verbose),
            timeout,
        })
    }

    /// Validate URL format
    fn validate_url(url: &str) -> Result<()> {
        if url.is_empty() {
            return Err(anyhow::anyhow!("Server URL cannot be empty"));
        }

        // Basic URL validation - must start with http:// or https://
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(anyhow::anyhow!(
                "Server URL must start with http:// or https://"
            ));
        }

        Ok(())
    }

    /// Validate output format
    fn validate_output_format(format: &str) -> Result<()> {
        match format {
            "table" | "json" => Ok(()),
            _ => Err(anyhow::anyhow!(
                "Invalid output format '{}'. Must be 'table' or 'json'",
                format
            )),
        }
    }

    /// Validate timeout value
    fn validate_timeout(timeout: u64) -> Result<()> {
        if timeout == 0 {
            return Err(anyhow::anyhow!("Timeout must be greater than 0"));
        }

        if timeout > 300 {
            return Err(anyhow::anyhow!(
                "Timeout must be less than or equal to 300 seconds"
            ));
        }

        Ok(())
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

    // ConfigBuilder tests

    #[test]
    fn test_builder_with_defaults() {
        let config = ConfigBuilder::new().build().unwrap();
        let defaults = CliConfig::default();
        assert_eq!(config, defaults);
    }

    #[test]
    fn test_builder_with_custom_values() {
        let config = ConfigBuilder::new()
            .with_server_url("http://example.com:8080")
            .unwrap()
            .with_output_format("json")
            .unwrap()
            .with_verbose(true)
            .with_timeout(30)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(config.server_url, "http://example.com:8080");
        assert_eq!(config.output_format, "json");
        assert!(config.verbose);
        assert_eq!(config.timeout, 30);
    }

    #[test]
    fn test_builder_url_validation() {
        // Empty URL
        assert!(ConfigBuilder::new().with_server_url("").is_err());

        // Invalid protocol
        assert!(ConfigBuilder::new()
            .with_server_url("ftp://example.com")
            .is_err());

        // Valid URLs
        assert!(ConfigBuilder::new()
            .with_server_url("http://localhost:3000")
            .is_ok());
        assert!(ConfigBuilder::new()
            .with_server_url("https://example.com")
            .is_ok());
    }

    #[test]
    fn test_builder_format_validation() {
        // Invalid formats
        assert!(ConfigBuilder::new().with_output_format("xml").is_err());
        assert!(ConfigBuilder::new().with_output_format("csv").is_err());

        // Valid formats
        assert!(ConfigBuilder::new().with_output_format("table").is_ok());
        assert!(ConfigBuilder::new().with_output_format("json").is_ok());
    }

    #[test]
    fn test_builder_timeout_validation() {
        // Zero timeout
        assert!(ConfigBuilder::new().with_timeout(0).is_err());

        // Timeout too large
        assert!(ConfigBuilder::new().with_timeout(301).is_err());

        // Valid timeouts
        assert!(ConfigBuilder::new().with_timeout(1).is_ok());
        assert!(ConfigBuilder::new().with_timeout(300).is_ok());
    }

    #[test]
    fn test_builder_with_env_overrides() {
        // Clean environment first
        std::env::remove_var("OPENFAN_SERVER");
        std::env::remove_var("OPENFAN_FORMAT");
        std::env::remove_var("OPENFAN_VERBOSE");
        std::env::remove_var("OPENFAN_TIMEOUT");

        // Set env vars
        std::env::set_var("OPENFAN_SERVER", "http://env.example.com:9000");
        std::env::set_var("OPENFAN_FORMAT", "json");
        std::env::set_var("OPENFAN_VERBOSE", "true");
        std::env::set_var("OPENFAN_TIMEOUT", "25");

        let config = ConfigBuilder::new().with_env_overrides().build().unwrap();

        assert_eq!(config.server_url, "http://env.example.com:9000");
        assert_eq!(config.output_format, "json");
        assert!(config.verbose);
        assert_eq!(config.timeout, 25);

        // Clean up
        std::env::remove_var("OPENFAN_SERVER");
        std::env::remove_var("OPENFAN_FORMAT");
        std::env::remove_var("OPENFAN_VERBOSE");
        std::env::remove_var("OPENFAN_TIMEOUT");
    }

    #[test]
    fn test_builder_priority_chain() {
        // Clean environment
        std::env::remove_var("OPENFAN_SERVER");
        std::env::remove_var("OPENFAN_TIMEOUT");

        // Set env vars
        std::env::set_var("OPENFAN_SERVER", "http://env.example.com:9000");
        std::env::set_var("OPENFAN_TIMEOUT", "25");

        // CLI args should override env vars
        let config = ConfigBuilder::new()
            .with_env_overrides()
            .with_server_url("http://cli.example.com:7000")
            .unwrap()
            .build()
            .unwrap();

        // CLI arg wins
        assert_eq!(config.server_url, "http://cli.example.com:7000");
        // Env var applies for timeout
        assert_eq!(config.timeout, 25);

        // Clean up
        std::env::remove_var("OPENFAN_SERVER");
        std::env::remove_var("OPENFAN_TIMEOUT");
    }

    #[test]
    fn test_builder_env_priority_over_defaults() {
        // Clean environment
        std::env::remove_var("OPENFAN_VERBOSE");

        std::env::set_var("OPENFAN_VERBOSE", "true");

        let config = ConfigBuilder::new().with_env_overrides().build().unwrap();

        // Env var overrides default (false)
        assert!(config.verbose);

        std::env::remove_var("OPENFAN_VERBOSE");
    }

    #[test]
    fn test_builder_invalid_env_values_ignored() {
        // Clean environment first to avoid interference from other tests
        std::env::remove_var("OPENFAN_SERVER");
        std::env::remove_var("OPENFAN_FORMAT");
        std::env::remove_var("OPENFAN_VERBOSE");
        std::env::remove_var("OPENFAN_TIMEOUT");

        // Set invalid values
        std::env::set_var("OPENFAN_TIMEOUT", "invalid");
        std::env::set_var("OPENFAN_FORMAT", "xml"); // Invalid format

        let config = ConfigBuilder::new().with_env_overrides().build().unwrap();

        // Should fall back to defaults
        assert_eq!(config.timeout, 10);
        assert_eq!(config.output_format, "table");

        // Clean up
        std::env::remove_var("OPENFAN_TIMEOUT");
        std::env::remove_var("OPENFAN_FORMAT");
    }
}
