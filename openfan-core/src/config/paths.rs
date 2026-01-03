//! Default path resolution for configuration files
//!
//! Uses XDG Base Directory specification when available, with sensible fallbacks.
//!
//! On macOS, the `dirs` crate returns `~/Library/Application Support` by default,
//! but we explicitly check XDG environment variables first to allow users to
//! override this behavior.

use std::path::PathBuf;

/// Returns the XDG config directory, checking environment variables first.
///
/// Priority:
/// 1. `XDG_CONFIG_HOME` environment variable (if set)
/// 2. `dirs::config_dir()` (platform default)
/// 3. Fallback to `/etc`
fn xdg_config_dir() -> PathBuf {
    if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg_config)
    } else {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("/etc"))
    }
}

/// Returns the XDG data directory, checking environment variables first.
///
/// Priority:
/// 1. `XDG_DATA_HOME` environment variable (if set)
/// 2. `dirs::data_dir()` (platform default)
/// 3. Fallback to `/var/lib`
fn xdg_data_dir() -> PathBuf {
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg_data)
    } else {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from("/var/lib"))
    }
}

/// Returns the default path for the static configuration file.
///
/// Uses XDG config directory if available:
/// - `$XDG_CONFIG_HOME/openfan/config.toml` (if XDG_CONFIG_HOME is set)
/// - Linux: `~/.config/openfan/config.toml`
/// - macOS: `~/Library/Application Support/openfan/config.toml`
/// - Fallback: `/etc/openfan/config.toml`
pub fn default_config_path() -> PathBuf {
    xdg_config_dir().join("openfan").join("config.toml")
}

/// Returns the default data directory for mutable configuration files.
///
/// Uses XDG data directory if available:
/// - `$XDG_DATA_HOME/openfan` (if XDG_DATA_HOME is set)
/// - Linux: `~/.local/share/openfan`
/// - macOS: `~/Library/Application Support/openfan`
/// - Fallback: `/var/lib/openfan`
pub fn default_data_dir() -> PathBuf {
    xdg_data_dir().join("openfan")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_path_is_toml() {
        let path = default_config_path();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("toml"));
        assert!(path.ends_with("openfan/config.toml"));
    }

    #[test]
    fn test_default_data_dir_ends_with_openfan() {
        let path = default_data_dir();
        assert!(path.ends_with("openfan"));
    }
}
