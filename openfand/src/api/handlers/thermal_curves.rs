//! Thermal curve handlers for CRUD operations and temperature interpolation

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::{
    api::{
        AddCurveRequest, ApiResponse, InterpolateResponse, SingleCurveResponse,
        ThermalCurveResponse, UpdateCurveRequest,
    },
    CurvePoint, ThermalCurve,
};
use serde::Deserialize;
use tracing::{debug, info};

/// Query parameters for interpolation operation.
#[derive(Deserialize)]
pub struct InterpolateQuery {
    /// Temperature in Celsius
    pub temp: f32,
}

/// Validates a curve name.
///
/// Valid names contain only alphanumeric characters, hyphens, and underscores.
fn is_valid_curve_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Validates curve points.
///
/// - At least 2 points required
/// - Points must be in ascending temperature order
/// - PWM values must be 0-100
/// - Temperature must be in valid range
fn validate_points(points: &[CurvePoint]) -> Result<(), String> {
    if points.len() < 2 {
        return Err("At least 2 points are required".to_string());
    }

    // Check temperature ordering
    for window in points.windows(2) {
        if window[0].temp_c >= window[1].temp_c {
            return Err(format!(
                "Points must be in ascending temperature order: {} >= {}",
                window[0].temp_c, window[1].temp_c
            ));
        }
    }

    // Check PWM values and temperature range
    for point in points {
        if point.pwm > 100 {
            return Err(format!(
                "PWM value {} exceeds maximum of 100 at temperature {}",
                point.pwm, point.temp_c
            ));
        }
        if point.temp_c < -50.0 || point.temp_c > 150.0 {
            return Err(format!(
                "Temperature {} is outside valid range (-50 to 150)",
                point.temp_c
            ));
        }
    }

    Ok(())
}

/// Lists all configured thermal curves.
///
/// # Endpoint
///
/// `GET /api/v0/curves/list`
pub async fn list_curves(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ThermalCurveResponse>>, ApiError> {
    debug!("Request: GET /api/v0/curves/list");

    let curves = state.config.thermal_curves().await;
    let curve_map = curves.curves.clone();

    let response = ThermalCurveResponse { curves: curve_map };

    info!("Listed {} thermal curves", response.curves.len());
    api_ok!(response)
}

/// Gets a single thermal curve by name.
///
/// # Endpoint
///
/// `GET /api/v0/curve/:name/get`
pub async fn get_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<SingleCurveResponse>>, ApiError> {
    debug!("Request: GET /api/v0/curve/{}/get", name);

    let curves = state.config.thermal_curves().await;

    match curves.get(&name) {
        Some(curve) => {
            let response = SingleCurveResponse {
                curve: curve.clone(),
            };
            api_ok!(response)
        }
        None => api_fail!(format!(
            "Thermal curve '{}' does not exist! (Names are case-sensitive!)",
            name
        )),
    }
}

/// Adds a new thermal curve.
///
/// # Validation Rules
///
/// - Curve name must be non-empty and contain only alphanumeric characters, hyphens, and underscores
/// - At least 2 points are required
/// - Points must be in ascending temperature order
/// - PWM values must be 0-100
///
/// # Endpoint
///
/// `POST /api/v0/curves/add`
///
/// # Request Body
///
/// ```json
/// {
///   "name": "Custom",
///   "points": [
///     {"temp_c": 30.0, "pwm": 25},
///     {"temp_c": 50.0, "pwm": 50},
///     {"temp_c": 80.0, "pwm": 100}
///   ],
///   "description": "Custom thermal curve"
/// }
/// ```
pub async fn add_curve(
    State(state): State<AppState>,
    Json(request): Json<AddCurveRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/curves/add");

    let curve_name = request.name.trim();

    if !is_valid_curve_name(curve_name) {
        return api_fail!(
            "Curve name must be non-empty and contain only alphanumeric characters, hyphens, and underscores!"
        );
    }

    // Validate points
    if let Err(e) = validate_points(&request.points) {
        return api_fail!(e);
    }

    // Add curve
    {
        let mut curves = state.config.thermal_curves_mut().await;

        // Check if curve name already exists
        if curves.contains(curve_name) {
            return api_fail!(format!("Thermal curve '{}' already exists!", curve_name));
        }

        let curve = match &request.description {
            Some(desc) => {
                ThermalCurve::with_description(curve_name, request.points.clone(), desc)
            }
            None => ThermalCurve::new(curve_name, request.points.clone()),
        };

        curves.insert(curve_name.to_string(), curve);
    }

    // Save to disk
    if let Err(e) = state.config.save_thermal_curves().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save thermal curves: {}",
            e
        )));
    }

    info!("Added thermal curve: {}", curve_name);
    api_ok!(())
}

/// Updates an existing thermal curve.
///
/// # Endpoint
///
/// `POST /api/v0/curve/:name/update`
///
/// # Request Body
///
/// ```json
/// {
///   "points": [
///     {"temp_c": 30.0, "pwm": 30},
///     {"temp_c": 60.0, "pwm": 60},
///     {"temp_c": 85.0, "pwm": 100}
///   ],
///   "description": "Updated description"
/// }
/// ```
pub async fn update_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<UpdateCurveRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: POST /api/v0/curve/{}/update", name);

    // Validate points
    if let Err(e) = validate_points(&request.points) {
        return api_fail!(e);
    }

    // Update curve
    {
        let mut curves = state.config.thermal_curves_mut().await;

        // Check if curve exists
        if !curves.contains(&name) {
            return api_fail!(format!(
                "Thermal curve '{}' does not exist! (Names are case-sensitive!)",
                name
            ));
        }

        let curve = match &request.description {
            Some(desc) => ThermalCurve::with_description(&name, request.points.clone(), desc),
            None => {
                // Preserve existing description if not provided
                let existing_desc = curves
                    .get(&name)
                    .and_then(|c| c.description.clone());
                let mut curve = ThermalCurve::new(&name, request.points.clone());
                curve.description = existing_desc;
                curve
            }
        };

        curves.insert(name.clone(), curve);
    }

    // Save to disk
    if let Err(e) = state.config.save_thermal_curves().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save thermal curves: {}",
            e
        )));
    }

    info!("Updated thermal curve: {}", name);
    api_ok!(())
}

/// Deletes a thermal curve.
///
/// # Endpoint
///
/// `DELETE /api/v0/curve/:name`
pub async fn delete_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    debug!("Request: DELETE /api/v0/curve/{}", name);

    // Remove curve
    {
        let mut curves = state.config.thermal_curves_mut().await;

        if curves.remove(&name).is_none() {
            return api_fail!(format!(
                "Thermal curve '{}' does not exist! (Names are case-sensitive!)",
                name
            ));
        }
    }

    // Save to disk
    if let Err(e) = state.config.save_thermal_curves().await {
        return Err(ApiError::internal_error(format!(
            "Failed to save thermal curves: {}",
            e
        )));
    }

    info!("Deleted thermal curve: {}", name);
    api_ok!(())
}

/// Interpolates PWM value for a given temperature using the specified curve.
///
/// # Endpoint
///
/// `GET /api/v0/curve/:name/interpolate?temp=X`
pub async fn interpolate_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<InterpolateQuery>,
) -> Result<Json<ApiResponse<InterpolateResponse>>, ApiError> {
    debug!(
        "Request: GET /api/v0/curve/{}/interpolate?temp={}",
        name, params.temp
    );

    let curves = state.config.thermal_curves().await;

    match curves.get(&name) {
        Some(curve) => {
            let pwm = curve.interpolate(params.temp);
            let response = InterpolateResponse {
                temperature: params.temp,
                pwm,
            };
            info!(
                "Interpolated curve '{}' at {}°C = {}% PWM",
                name, params.temp, pwm
            );
            api_ok!(response)
        }
        None => api_fail!(format!(
            "Thermal curve '{}' does not exist! (Names are case-sensitive!)",
            name
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_curve_names() {
        assert!(is_valid_curve_name("Balanced"));
        assert!(is_valid_curve_name("Silent-Mode"));
        assert!(is_valid_curve_name("aggressive_v2"));
        assert!(is_valid_curve_name("curve123"));
    }

    #[test]
    fn test_invalid_curve_names() {
        assert!(!is_valid_curve_name(""));
        assert!(!is_valid_curve_name("has space"));
        assert!(!is_valid_curve_name("has.dot"));
        assert!(!is_valid_curve_name("special@char"));
    }

    #[test]
    fn test_validate_points_valid() {
        let points = vec![
            CurvePoint::new(30.0, 25),
            CurvePoint::new(50.0, 50),
            CurvePoint::new(80.0, 100),
        ];
        assert!(validate_points(&points).is_ok());
    }

    #[test]
    fn test_validate_points_too_few() {
        let points = vec![CurvePoint::new(50.0, 50)];
        assert!(validate_points(&points).is_err());
    }

    #[test]
    fn test_validate_points_wrong_order() {
        let points = vec![
            CurvePoint::new(80.0, 100),
            CurvePoint::new(30.0, 25),
        ];
        let result = validate_points(&points);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ascending"));
    }

    #[test]
    fn test_validate_points_pwm_out_of_range() {
        let points = vec![
            CurvePoint::new(30.0, 25),
            CurvePoint::new(80.0, 150),
        ];
        let result = validate_points(&points);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_points_temp_out_of_range() {
        let points = vec![
            CurvePoint::new(-100.0, 25),
            CurvePoint::new(80.0, 100),
        ];
        let result = validate_points(&points);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside valid range"));
    }
}
