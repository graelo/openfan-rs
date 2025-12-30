//! Default path resolution for configuration files
//!
//! Uses XDG Base Directory specification when available, with sensible fallbacks.

use std::path::PathBuf;

/// Returns the default path for the static configuration file.
///
/// Uses XDG config directory if available:
/// - Linux/macOS: `~/.config/openfan/config.toml`
/// - Fallback: `/etc/openfan/config.toml`
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/etc"))
        .join("openfan")
        .join("config.toml")
}

/// Returns the default data directory for mutable configuration files.
///
/// Uses XDG data directory if available:
/// - Linux/macOS: `~/.local/share/openfan`
/// - Fallback: `/var/lib/openfan`
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/var/lib"))
        .join("openfan")
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
