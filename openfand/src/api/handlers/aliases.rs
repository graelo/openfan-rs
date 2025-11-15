//! Alias handlers for fan alias management

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::api::{AliasResponse, ApiResponse};
use openfan_core::MAX_FANS;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Query parameters for alias operations
#[derive(Deserialize)]
pub struct AliasQuery {
    /// Alias value to set
    pub value: Option<String>,
}

/// Get all fan aliases
/// GET /api/v0/alias/all/get
pub async fn get_all_aliases(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<AliasResponse>>, ApiError> {
    debug!("Request: GET /api/v0/alias/all/get");

    let config = state.config.read().await;
    let aliases = config.config().fan_aliases.clone();

    let response = AliasResponse { aliases };

    info!("Retrieved all fan aliases");
    api_ok!(response)
}

/// Get alias for a specific fan
/// GET /api/v0/alias/:id/get
pub async fn get_alias(
    State(state): State<AppState>,
    Path(fan_id): Path<String>,
) -> Result<Json<ApiResponse<AliasResponse>>, ApiError> {
    debug!("Request: GET /api/v0/alias/{}/get", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    if fan_index as usize >= MAX_FANS {
        return api_fail!(format!("Invalid fan index (0<={fan_index}<{})", MAX_FANS));
    }

    let config = state.config.read().await;
    let alias = config
        .config()
        .fan_aliases
        .get(&fan_index)
        .cloned()
        .unwrap_or_else(|| format!("Fan #{}", fan_index + 1));

    let mut aliases = HashMap::new();
    aliases.insert(fan_index, alias.clone());

    let response = AliasResponse { aliases };

    debug!("Retrieved alias for fan {}: {}", fan_index, alias);
    api_ok!(response)
}

/// Set alias for a specific fan
/// GET /api/v0/alias/:id/set?value=CPU Fan
pub async fn set_alias(
    State(state): State<AppState>,
    Path(fan_id): Path<String>,
    Query(params): Query<AliasQuery>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: GET /api/v0/alias/{}/set", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    if fan_index as usize >= MAX_FANS {
        return api_fail!(format!("Invalid fan index (0<={fan_index}<{})", MAX_FANS));
    }

    let Some(alias_value) = params.value else {
        return api_fail!("Fan alias cannot be none!");
    };

    // Validate alias format
    if !is_valid_alias(&alias_value) {
        return api_fail!(
            "Fan alias can only contain 'A-Z', '0-9', '-', '_', '#' and <space> characters!"
        );
    }

    // Update configuration
    let mut config = state.config.write().await;
    config
        .config_mut()
        .fan_aliases
        .insert(fan_index, alias_value.clone());

    // Save configuration
    if let Err(e) = config.save().await {
        warn!("Failed to save configuration: {}", e);
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Set alias for fan {} to '{}'", fan_index, alias_value);
    api_ok!(())
}

/// Validate alias string format
///
/// Allows: A-Z, a-z, 0-9, -, _, #, and space characters
fn is_valid_alias(alias: &str) -> bool {
    if alias.is_empty() {
        return false;
    }

    alias
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '#' || c == ' ' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fan_id_validation() {
        assert_eq!("0".parse::<u8>().unwrap(), 0);
        assert_eq!("9".parse::<u8>().unwrap(), 9);
        assert!("10".parse::<u8>().unwrap() > 9);
        assert!("abc".parse::<u8>().is_err());
    }

    #[test]
    fn test_alias_validation() {
        // Valid aliases
        assert!(is_valid_alias("CPU Fan"));
        assert!(is_valid_alias("Fan-1"));
        assert!(is_valid_alias("GPU_FAN"));
        assert!(is_valid_alias("Case#1"));
        assert!(is_valid_alias("Intake.Fan"));
        assert!(is_valid_alias("ABC123"));

        // Invalid aliases
        assert!(!is_valid_alias(""));
        assert!(!is_valid_alias("Fan@1"));
        assert!(!is_valid_alias("Fan$1"));
        assert!(!is_valid_alias("Fan%1"));
        assert!(!is_valid_alias("Fan!"));
    }

    #[test]
    fn test_default_alias_format() {
        for i in 0..MAX_FANS as u8 {
            let default_alias = format!("Fan #{}", i + 1);
            assert!(is_valid_alias(&default_alias));
            assert!(default_alias.starts_with("Fan #"));
        }
    }

    #[test]
    fn test_alias_character_limits() {
        // Test maximum reasonable length
        let long_alias = "A".repeat(50);
        assert!(is_valid_alias(&long_alias));

        // Test with all allowed special characters
        assert!(is_valid_alias("Test-Fan_1#Main.Intake"));
    }
}
