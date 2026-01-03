//! Controller registry for multi-controller management
//!
//! Manages multiple fan controllers, each with its own connection and board info.

use std::collections::HashMap;
use std::sync::Arc;

use openfan_core::{BoardInfo, OpenFanError, Result};
use tokio::sync::RwLock;

use super::ConnectionManager;

/// An individual controller entry in the registry
pub struct ControllerEntry {
    id: String,
    board_info: BoardInfo,
    connection_manager: Option<Arc<ConnectionManager>>,
    description: Option<String>,
}

impl ControllerEntry {
    /// Create a new controller entry
    pub fn new(
        id: impl Into<String>,
        board_info: BoardInfo,
        connection_manager: Option<Arc<ConnectionManager>>,
    ) -> Self {
        Self {
            id: id.into(),
            board_info,
            connection_manager,
            description: None,
        }
    }

    /// Create a new controller entry with a description
    pub fn with_description(
        id: impl Into<String>,
        board_info: BoardInfo,
        connection_manager: Option<Arc<ConnectionManager>>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            board_info,
            connection_manager,
            description: Some(description.into()),
        }
    }

    /// Get the controller ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the board info
    pub fn board_info(&self) -> &BoardInfo {
        &self.board_info
    }

    /// Get the description
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Get the connection manager
    pub fn connection_manager(&self) -> Option<&Arc<ConnectionManager>> {
        self.connection_manager.as_ref()
    }

    /// Check if this controller is in mock mode
    pub fn is_mock(&self) -> bool {
        self.connection_manager.is_none()
    }

    /// Check if this controller is connected
    pub fn is_connected(&self) -> bool {
        self.connection_manager.is_some()
    }
}

/// Registry managing multiple fan controllers
///
/// Thread-safe container for all controller entries with lookup by ID.
pub struct ControllerRegistry {
    /// Map of controller ID to entry
    controllers: RwLock<HashMap<String, Arc<ControllerEntry>>>,
}

impl ControllerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            controllers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new controller
    ///
    /// Returns error if a controller with the same ID already exists.
    pub async fn register(&self, entry: ControllerEntry) -> Result<()> {
        let mut controllers = self.controllers.write().await;

        let id = entry.id.clone();
        if controllers.contains_key(&id) {
            return Err(OpenFanError::DuplicateControllerId(id));
        }

        controllers.insert(id, Arc::new(entry));
        Ok(())
    }

    /// Get a controller by ID
    pub async fn get(&self, id: &str) -> Option<Arc<ControllerEntry>> {
        let controllers = self.controllers.read().await;
        controllers.get(id).cloned()
    }

    /// Get a controller by ID, returning an error if not found
    pub async fn get_or_err(&self, id: &str) -> Result<Arc<ControllerEntry>> {
        self.get(id)
            .await
            .ok_or_else(|| OpenFanError::ControllerNotFound(id.to_string()))
    }

    /// List all controller IDs
    pub async fn ids(&self) -> Vec<String> {
        let controllers = self.controllers.read().await;
        controllers.keys().cloned().collect()
    }

    /// List all controller entries
    pub async fn list(&self) -> Vec<Arc<ControllerEntry>> {
        let controllers = self.controllers.read().await;
        controllers.values().cloned().collect()
    }

    /// Get the number of registered controllers
    pub async fn len(&self) -> usize {
        let controllers = self.controllers.read().await;
        controllers.len()
    }

    /// Check if the registry is empty
    pub async fn is_empty(&self) -> bool {
        let controllers = self.controllers.read().await;
        controllers.is_empty()
    }

    /// Check if any controller is in mock mode
    pub async fn has_mock_controllers(&self) -> bool {
        let controllers = self.controllers.read().await;
        controllers.values().any(|e| e.is_mock())
    }

    /// Check if all controllers are in mock mode
    pub async fn all_mock(&self) -> bool {
        let controllers = self.controllers.read().await;
        controllers.values().all(|e| e.is_mock())
    }
}

impl Default for ControllerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfan_core::board::BoardType;

    fn mock_board_info() -> BoardInfo {
        BoardType::OpenFanStandard.to_board_info()
    }

    #[tokio::test]
    async fn test_registry_new_is_empty() {
        let registry = ControllerRegistry::new();
        assert!(registry.is_empty().await);
        assert_eq!(registry.len().await, 0);
    }

    #[tokio::test]
    async fn test_register_controller() {
        let registry = ControllerRegistry::new();
        let entry = ControllerEntry::new("main", mock_board_info(), None);

        registry.register(entry).await.unwrap();

        assert!(!registry.is_empty().await);
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_register_duplicate_fails() {
        let registry = ControllerRegistry::new();
        let entry1 = ControllerEntry::new("main", mock_board_info(), None);
        let entry2 = ControllerEntry::new("main", mock_board_info(), None);

        registry.register(entry1).await.unwrap();
        let result = registry.register(entry2).await;

        assert!(matches!(
            result,
            Err(OpenFanError::DuplicateControllerId(id)) if id == "main"
        ));
    }

    #[tokio::test]
    async fn test_get_controller() {
        let registry = ControllerRegistry::new();
        let entry =
            ControllerEntry::with_description("main", mock_board_info(), None, "Main chassis");

        registry.register(entry).await.unwrap();

        let retrieved = registry.get("main").await.unwrap();
        assert_eq!(retrieved.id, "main");
        assert_eq!(retrieved.description, Some("Main chassis".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let registry = ControllerRegistry::new();
        assert!(registry.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_get_or_err_returns_error() {
        let registry = ControllerRegistry::new();
        let result = registry.get_or_err("nonexistent").await;

        assert!(matches!(
            result,
            Err(OpenFanError::ControllerNotFound(id)) if id == "nonexistent"
        ));
    }

    #[tokio::test]
    async fn test_list_controllers() {
        let registry = ControllerRegistry::new();
        registry
            .register(ControllerEntry::new("main", mock_board_info(), None))
            .await
            .unwrap();
        registry
            .register(ControllerEntry::new("gpu", mock_board_info(), None))
            .await
            .unwrap();

        let list = registry.list().await;
        assert_eq!(list.len(), 2);

        let ids: Vec<_> = list.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"main"));
        assert!(ids.contains(&"gpu"));
    }

    #[tokio::test]
    async fn test_ids() {
        let registry = ControllerRegistry::new();
        registry
            .register(ControllerEntry::new("main", mock_board_info(), None))
            .await
            .unwrap();
        registry
            .register(ControllerEntry::new("gpu", mock_board_info(), None))
            .await
            .unwrap();

        let ids = registry.ids().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"main".to_string()));
        assert!(ids.contains(&"gpu".to_string()));
    }

    #[tokio::test]
    async fn test_mock_mode_detection() {
        let registry = ControllerRegistry::new();

        // All mock controllers
        registry
            .register(ControllerEntry::new("main", mock_board_info(), None))
            .await
            .unwrap();

        assert!(registry.has_mock_controllers().await);
        assert!(registry.all_mock().await);
    }

    #[tokio::test]
    async fn test_controller_entry_is_mock() {
        let mock_entry = ControllerEntry::new("mock", mock_board_info(), None);
        assert!(mock_entry.is_mock());
    }
}
