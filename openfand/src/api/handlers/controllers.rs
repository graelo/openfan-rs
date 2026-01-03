//! Controller management API handlers
//!
//! Endpoints for listing and managing multiple fan controllers.

use axum::{
    extract::{Path, State},
    Json,
};
use openfan_core::api::{ApiResponse, ControllerInfo, ControllersListResponse};
use tracing::{info, warn};

use crate::api::{error::ApiError, AppState};

/// GET /api/v0/controllers
///
/// List all registered controllers with their status.
pub async fn list_controllers(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ControllersListResponse>>, ApiError> {
    let controllers = state.registry.list().await;

    let controller_list: Vec<ControllerInfo> = controllers
        .iter()
        .map(|entry| ControllerInfo {
            id: entry.id().to_string(),
            board_name: entry.board_info().name.clone(),
            fan_count: entry.board_info().fan_count,
            description: entry.description().map(String::from),
            mock_mode: entry.is_mock(),
            connected: entry.is_connected(),
        })
        .collect();

    let response = ControllersListResponse {
        count: controller_list.len(),
        controllers: controller_list,
    };

    Ok(Json(ApiResponse::success(response)))
}

/// GET /api/v0/controller/{id}/info
///
/// Get detailed info about a specific controller.
pub async fn get_controller_info(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
) -> Result<Json<ApiResponse<ControllerInfo>>, ApiError> {
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    let info = ControllerInfo {
        id: entry.id().to_string(),
        board_name: entry.board_info().name.clone(),
        fan_count: entry.board_info().fan_count,
        description: entry.description().map(String::from),
        mock_mode: entry.is_mock(),
        connected: entry.is_connected(),
    };

    Ok(Json(ApiResponse::success(info)))
}

/// POST /api/v0/controller/{id}/reconnect
///
/// Force a reconnection attempt for a specific controller.
pub async fn reconnect_controller(
    State(state): State<AppState>,
    Path(controller_id): Path<String>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    let entry = state
        .registry
        .get_or_err(&controller_id)
        .await
        .map_err(ApiError::from)?;

    if let Some(cm) = entry.connection_manager() {
        info!("Forcing reconnection for controller '{}'", controller_id);
        // Trigger reconnect asynchronously - success/failure will be reflected in future status checks
        let _ = cm.force_reconnect().await;
        Ok(Json(ApiResponse::success(format!(
            "Reconnection triggered for controller '{}'",
            controller_id
        ))))
    } else {
        warn!(
            "Controller '{}' is in mock mode, cannot reconnect",
            controller_id
        );
        Err(ApiError::bad_request(format!(
            "Controller '{}' is in mock mode",
            controller_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::create_router;
    use crate::config::RuntimeConfig;
    use crate::controllers::{ControllerEntry, ControllerRegistry};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use openfan_core::{board::BoardType, config::StaticConfig};
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn create_test_app() -> axum::Router {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let static_config = StaticConfig::with_data_dir(temp_dir.path().join("data"));
        tokio::fs::write(&config_path, static_config.to_toml().unwrap())
            .await
            .unwrap();
        let config = RuntimeConfig::load(&config_path).await.unwrap();

        // Create registry with two mock controllers
        let registry = ControllerRegistry::new();
        let main_board = BoardType::OpenFanStandard.to_board_info();
        let gpu_board = BoardType::Custom { fan_count: 4 }.to_board_info();

        registry
            .register(
                ControllerEntry::builder("main", main_board.clone())
                    .maybe_description(Some("Main chassis controller".to_string()))
                    .build(),
            )
            .await
            .unwrap();
        registry
            .register(ControllerEntry::builder("gpu", gpu_board).build())
            .await
            .unwrap();

        let state = AppState::new(Arc::new(registry), Arc::new(config), main_board, None);
        create_router(state)
    }

    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_list_controllers() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controllers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["status"], "success");
        assert_eq!(json["data"]["count"], 2);
        assert_eq!(json["data"]["controllers"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_get_controller_info_main() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/main/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["status"], "success");
        assert_eq!(json["data"]["id"], "main");
        assert_eq!(json["data"]["board_name"], "OpenFAN Standard");
        assert_eq!(json["data"]["fan_count"], 10);
        assert_eq!(json["data"]["description"], "Main chassis controller");
        assert_eq!(json["data"]["mock_mode"], true);
    }

    #[tokio::test]
    async fn test_get_controller_info_gpu() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/gpu/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["status"], "success");
        assert_eq!(json["data"]["id"], "gpu");
        assert_eq!(json["data"]["board_name"], "Custom Board (4 fans)");
        assert_eq!(json["data"]["fan_count"], 4);
    }

    #[tokio::test]
    async fn test_get_controller_info_not_found() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v0/controller/nonexistent/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_reconnect_mock_controller_fails() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v0/controller/main/reconnect")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should fail because controller is in mock mode
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        assert!(body.contains("mock mode"));
    }
}
