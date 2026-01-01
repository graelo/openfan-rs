//! Thermal curve handlers for CRUD operations and temperature interpolation

use crate::api::error::ApiError;
use crate::api::AppState;
use crate::{api_fail, api_ok};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use openfan_core::{api, CurvePoint, ThermalCurve};
use serde::Deserialize;
use tracing::{debug, info};

/// Query parameters for interpolation operation.
#[derive(Deserialize)]
pub(crate) struct InterpolateQuery {
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
pub(crate) async fn list_curves(
    State(state): State<AppState>,
) -> Result<Json<api::ApiResponse<api::ThermalCurveResponse>>, ApiError> {
    debug!("Request: GET /api/v0/curves/list");

    let curves = state.config.thermal_curves().await;
    let curve_map = curves.curves.clone();

    let response = api::ThermalCurveResponse { curves: curve_map };

    info!("Listed {} thermal curves", response.curves.len());
    api_ok!(response)
}

/// Gets a single thermal curve by name.
///
/// # Endpoint
///
/// `GET /api/v0/curve/{name}/get`
pub(crate) async fn get_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<api::ApiResponse<api::SingleCurveResponse>>, ApiError> {
    debug!("Request: GET /api/v0/curve/{}/get", name);

    let curves = state.config.thermal_curves().await;

    match curves.get(&name) {
        Some(curve) => {
            let response = api::SingleCurveResponse {
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
pub(crate) async fn add_curve(
    State(state): State<AppState>,
    Json(request): Json<api::AddCurveRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
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
            Some(desc) => ThermalCurve::with_description(curve_name, request.points.clone(), desc),
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
/// `POST /api/v0/curve/{name}/update`
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
pub(crate) async fn update_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<api::UpdateCurveRequest>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
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
                let existing_desc = curves.get(&name).and_then(|c| c.description.clone());
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
/// `DELETE /api/v0/curve/{name}`
pub(crate) async fn delete_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<api::ApiResponse<()>>, ApiError> {
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
/// `GET /api/v0/curve/{name}/interpolate?temp=X`
pub(crate) async fn interpolate_curve(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<InterpolateQuery>,
) -> Result<Json<api::ApiResponse<api::InterpolateResponse>>, ApiError> {
    debug!(
        "Request: GET /api/v0/curve/{}/interpolate?temp={}",
        name, params.temp
    );

    let curves = state.config.thermal_curves().await;

    match curves.get(&name) {
        Some(curve) => {
            let pwm = curve.interpolate(params.temp);
            let response = api::InterpolateResponse {
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
        let points = vec![CurvePoint::new(80.0, 100), CurvePoint::new(30.0, 25)];
        let result = validate_points(&points);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ascending"));
    }

    #[test]
    fn test_validate_points_pwm_out_of_range() {
        let points = vec![CurvePoint::new(30.0, 25), CurvePoint::new(80.0, 150)];
        let result = validate_points(&points);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_points_temp_out_of_range() {
        let points = vec![CurvePoint::new(-100.0, 25), CurvePoint::new(80.0, 100)];
        let result = validate_points(&points);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside valid range"));
    }
}

/// Integration tests that exercise actual HTTP handlers
#[cfg(test)]
mod integration_tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use http_body_util::BodyExt;
    use openfan_core::BoardType;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::api::{create_router, AppState};
    use crate::config::RuntimeConfig;

    struct TestApp {
        router: Router,
        _config_dir: TempDir,
    }

    impl TestApp {
        async fn new() -> Self {
            let board_info = BoardType::OpenFanStandard.to_board_info();
            let config_dir = tempfile::tempdir().unwrap();

            let data_dir = config_dir.path().join("data");
            std::fs::create_dir_all(&data_dir).unwrap();

            let data_dir_str = data_dir.to_string_lossy().replace('\\', "\\\\");
            let config_content = format!(
                r#"data_dir = "{}"

[server]
hostname = "localhost"
port = 3000
communication_timeout = 1

[hardware]
hostname = "localhost"
port = 3000
communication_timeout = 1
"#,
                data_dir_str
            );

            let config_path = config_dir.path().join("config.toml");
            std::fs::write(&config_path, config_content).unwrap();

            let config = RuntimeConfig::load(&config_path).await.unwrap();
            let state = AppState::new(board_info, config, None);

            TestApp {
                router: create_router(state),
                _config_dir: config_dir,
            }
        }

        fn router(&self) -> Router {
            self.router.clone()
        }
    }

    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_list_curves() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/curves/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();
        assert!(data.get("curves").is_some());
    }

    #[tokio::test]
    async fn test_add_curve_valid() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "test-curve", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 100}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_add_curve_with_description() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "described-curve", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 100}], "description": "A test curve"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_add_curve_invalid_name() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "invalid name", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 100}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_curve_too_few_points() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "one-point", "points": [{"temp_c": 50.0, "pwm": 50}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_curve_wrong_temp_order() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "wrong-order", "points": [{"temp_c": 80.0, "pwm": 100}, {"temp_c": 30.0, "pwm": 25}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_curve_pwm_too_high() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "high-pwm", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 150}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_curve_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/curve/nonexistent/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_get_curve() {
        let app = TestApp::new().await;

        // Add curve
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "to-get", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 100}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Get curve
        let get_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/curve/to-get/get")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_curve_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/v0/curve/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_delete_curve() {
        let app = TestApp::new().await;

        // Add curve
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "to-delete", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 100}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Delete curve
        let delete_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/v0/curve/to-delete")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_interpolate_curve_not_found() {
        let app = TestApp::new().await;

        let response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/curve/nonexistent/interpolate?temp=50.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_add_then_interpolate_curve() {
        let app = TestApp::new().await;

        // Add curve
        let add_response = app
            .router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v0/curves/add")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name": "interp-test", "points": [{"temp_c": 30.0, "pwm": 25}, {"temp_c": 80.0, "pwm": 100}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);

        // Interpolate
        let interp_response = app
            .router()
            .oneshot(
                Request::builder()
                    .uri("/api/v0/curve/interp-test/interpolate?temp=55.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(interp_response.status(), StatusCode::OK);

        let body = body_string(interp_response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let data = json.get("data").unwrap();
        assert!(data.get("temperature").is_some());
        assert!(data.get("pwm").is_some());
    }
}
