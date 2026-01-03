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
    /// Create a builder for constructing a controller entry
    ///
    /// # Example
    ///
    /// ```ignore
    /// let entry = ControllerEntry::builder("main", board_info)
    ///     .maybe_description(Some("Main chassis fans".to_string()))
    ///     .maybe_connection_manager(Some(cm))
    ///     .build();
    /// ```
    pub fn builder(id: impl Into<String>, board_info: BoardInfo) -> ControllerEntryBuilder {
        ControllerEntryBuilder::new(id, board_info)
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

/// Builder for constructing [`ControllerEntry`] instances
///
/// Provides a fluent API for creating controller entries with optional fields.
///
/// # Example
///
/// ```ignore
/// use openfan_core::BoardType;
///
/// let board_info = BoardType::OpenFanStandard.to_board_info();
///
/// // Minimal entry (mock mode, no description)
/// let entry = ControllerEntry::builder("default", board_info.clone())
///     .build();
///
/// // Full entry with all options
/// let entry = ControllerEntry::builder("main", board_info)
///     .maybe_description(Some("Main chassis controller".to_string()))
///     .maybe_connection_manager(Some(connection_manager))
///     .build();
/// ```
pub struct ControllerEntryBuilder {
    id: String,
    board_info: BoardInfo,
    connection_manager: Option<Arc<ConnectionManager>>,
    description: Option<String>,
}

impl ControllerEntryBuilder {
    /// Create a new builder with required fields
    pub fn new(id: impl Into<String>, board_info: BoardInfo) -> Self {
        Self {
            id: id.into(),
            board_info,
            connection_manager: None,
            description: None,
        }
    }

    /// Set an optional connection manager
    ///
    /// If `None`, the controller operates in mock mode.
    pub fn maybe_connection_manager(mut self, cm: Option<Arc<ConnectionManager>>) -> Self {
        self.connection_manager = cm;
        self
    }

    /// Set an optional description for this controller
    pub fn maybe_description(mut self, desc: Option<String>) -> Self {
        self.description = desc;
        self
    }

    /// Build the controller entry
    pub fn build(self) -> ControllerEntry {
        ControllerEntry {
            id: self.id,
            board_info: self.board_info,
            connection_manager: self.connection_manager,
            description: self.description,
        }
    }
}

/// Registry managing multiple fan controllers
///
/// Thread-safe container for all controller entries with lookup by ID.
///
/// # Example
///
/// ```ignore
/// use openfan_core::BoardType;
///
/// let registry = ControllerRegistry::new();
///
/// // Register controllers
/// let board = BoardType::OpenFanStandard.to_board_info();
/// let entry = ControllerEntry::builder("main", board).build();
/// registry.register(entry).await?;
///
/// // Look up and list controllers
/// let ctrl = registry.get_or_err("main").await?;
/// println!("Controller {} has {} fans", ctrl.id(), ctrl.board_info().fan_count);
/// ```
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

    /// List all controller entries
    pub async fn list(&self) -> Vec<Arc<ControllerEntry>> {
        let controllers = self.controllers.read().await;
        controllers.values().cloned().collect()
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
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_register_controller() {
        let registry = ControllerRegistry::new();
        let entry = ControllerEntry::builder("main", mock_board_info()).build();

        registry.register(entry).await.unwrap();

        assert_eq!(registry.list().await.len(), 1);
    }

    #[tokio::test]
    async fn test_register_duplicate_fails() {
        let registry = ControllerRegistry::new();
        let entry1 = ControllerEntry::builder("main", mock_board_info()).build();
        let entry2 = ControllerEntry::builder("main", mock_board_info()).build();

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
        let entry = ControllerEntry::builder("main", mock_board_info())
            .maybe_description(Some("Main chassis".to_string()))
            .build();

        registry.register(entry).await.unwrap();

        let retrieved = registry.get("main").await.unwrap();
        assert_eq!(retrieved.id(), "main");
        assert_eq!(retrieved.description(), Some("Main chassis"));
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
            .register(ControllerEntry::builder("main", mock_board_info()).build())
            .await
            .unwrap();
        registry
            .register(ControllerEntry::builder("gpu", mock_board_info()).build())
            .await
            .unwrap();

        let list = registry.list().await;
        assert_eq!(list.len(), 2);

        let ids: Vec<_> = list.iter().map(|e| e.id()).collect();
        assert!(ids.contains(&"main"));
        assert!(ids.contains(&"gpu"));
    }

    #[tokio::test]
    async fn test_controller_entry_is_mock() {
        let mock_entry = ControllerEntry::builder("mock", mock_board_info()).build();
        assert!(mock_entry.is_mock());
    }

    // Builder pattern tests

    #[test]
    fn test_builder_minimal() {
        let entry = ControllerEntry::builder("default", mock_board_info()).build();

        assert_eq!(entry.id(), "default");
        assert!(entry.is_mock());
        assert!(entry.description().is_none());
    }

    #[test]
    fn test_builder_with_description() {
        let entry = ControllerEntry::builder("main", mock_board_info())
            .maybe_description(Some("Main chassis controller".to_string()))
            .build();

        assert_eq!(entry.id(), "main");
        assert_eq!(entry.description(), Some("Main chassis controller"));
        assert!(entry.is_mock());
    }

    #[test]
    fn test_builder_maybe_description_none() {
        let entry = ControllerEntry::builder("test", mock_board_info())
            .maybe_description(None)
            .build();

        assert!(entry.description().is_none());
    }

    #[test]
    fn test_builder_maybe_connection_manager_none() {
        let entry = ControllerEntry::builder("test", mock_board_info())
            .maybe_connection_manager(None)
            .build();

        assert!(entry.is_mock());
        assert!(entry.connection_manager().is_none());
    }

    #[test]
    fn test_builder_chaining() {
        // Test that all builder methods can be chained
        let board = BoardType::Custom { fan_count: 4 }.to_board_info();
        let entry = ControllerEntry::builder("gpu", board)
            .maybe_description(Some("GPU cooling fans".to_string()))
            .maybe_connection_manager(None)
            .build();

        assert_eq!(entry.id(), "gpu");
        assert_eq!(entry.description(), Some("GPU cooling fans"));
        assert_eq!(entry.board_info().fan_count, 4);
        assert!(entry.is_mock());
    }

    #[tokio::test]
    async fn test_builder_with_registry() {
        let registry = ControllerRegistry::new();

        // Use builder to create entry and register it
        let entry = ControllerEntry::builder("main", mock_board_info())
            .maybe_description(Some("Main controller".to_string()))
            .build();

        registry.register(entry).await.unwrap();

        let retrieved = registry.get("main").await.unwrap();
        assert_eq!(retrieved.description(), Some("Main controller"));
    }
}
