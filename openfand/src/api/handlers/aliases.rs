//! Alias handlers for fan alias management

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::api::{AliasResponse, ApiResponse};
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, info};

/// Query parameters for alias operations.
#[derive(Deserialize)]
pub struct AliasQuery {
    /// Alias value to set (must contain only allowed characters)
    pub value: Option<String>,
}

/// Retrieve all configured fan aliases.
///
/// Return a map of fan IDs to their human-readable alias names.
/// Fans without configured aliases will not appear in the response.
///
/// # Endpoint
///
/// `GET /api/v0/alias/all/get`
pub async fn get_all_aliases(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<AliasResponse>>, ApiError> {
    debug!("Request: GET /api/v0/alias/all/get");

    let alias_data = state.config.aliases().await;
    let aliases = alias_data.aliases.clone();

    let response = AliasResponse { aliases };

    info!("Retrieved all fan aliases");
    api_ok!(response)
}

/// Retrieve the alias for a specific fan.
///
/// If no alias is configured, return the default alias "Fan #N" where N is fan_id + 1.
///
/// # Endpoint
///
/// `GET /api/v0/alias/:id/get`
///
/// # Path Parameters
///
/// - `id` - Fan identifier (0-9)
pub async fn get_alias(
    State(state): State<AppState>,
    Path(fan_id): Path<String>,
) -> Result<Json<ApiResponse<AliasResponse>>, ApiError> {
    debug!("Request: GET /api/v0/alias/{}/get", fan_id);

    // Parse and validate fan ID
    let fan_index = fan_id
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request(format!("Invalid fan ID: {}", fan_id)))?;

    // Validate fan ID against board configuration
    state.board_info.validate_fan_id(fan_index)?;

    let alias_data = state.config.aliases().await;
    let alias = alias_data.get(fan_index);

    let mut aliases = HashMap::new();
    aliases.insert(fan_index, alias.clone());

    let response = AliasResponse { aliases };

    debug!("Retrieved alias for fan {}: {}", fan_index, alias);
    api_ok!(response)
}

/// Set a human-readable alias for a specific fan.
///
/// The alias is validated and saved to the configuration file.
///
/// # Validation
///
/// Aliases must contain only:
/// - Alphanumeric characters (A-Z, a-z, 0-9)
/// - Hyphens (-)
/// - Underscores (_)
/// - Hash symbols (#)
/// - Periods (.)
/// - Spaces
///
/// Empty aliases are not allowed.
///
/// # Endpoint
///
/// `GET /api/v0/alias/:id/set?value=CPU Fan`
///
/// # Path Parameters
///
/// - `id` - Fan identifier (0-9)
///
/// # Query Parameters
///
/// - `value` - Alias to set (must match allowed character set)
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

    // Validate fan ID against board configuration
    state.board_info.validate_fan_id(fan_index)?;

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
    {
        let mut aliases = state.config.aliases_mut().await;
        aliases.set(fan_index, alias_value.clone());
    }

    // Save configuration
    if let Err(e) = state.config.save_aliases().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save configuration: {}",
            e
        )));
    }

    info!("Set alias for fan {} to '{}'", fan_index, alias_value);
    api_ok!(())
}

/// Validate alias string format.
///
/// # Allowed Characters
///
/// - Alphanumeric: A-Z, a-z, 0-9
/// - Special characters: `-`, `_`, `#`, `.`, ` ` (space)
///
/// # Returns
///
/// - `true` if the alias is non-empty and contains only allowed characters
/// - `false` if the alias is empty or contains disallowed characters
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
    use openfan_core::{BoardConfig, DefaultBoard};

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
        for i in 0..DefaultBoard::FAN_COUNT as u8 {
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
