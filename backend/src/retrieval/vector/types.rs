use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a single vector item to be stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorItem {
    pub id: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub metadata: serde_json::Value,
}

/// Result from get/query operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResult {
    pub ids: Option<Vec<Vec<String>>>,
    pub documents: Option<Vec<Vec<String>>>,
    pub metadatas: Option<Vec<Vec<serde_json::Value>>>,
}

/// Result from search operations (includes distances)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub ids: Option<Vec<Vec<String>>>,
    pub documents: Option<Vec<Vec<String>>>,
    pub metadatas: Option<Vec<Vec<serde_json::Value>>>,
    pub distances: Option<Vec<Vec<f32>>>,
}

impl From<SearchResult> for GetResult {
    fn from(search: SearchResult) -> Self {
        GetResult {
            ids: search.ids,
            documents: search.documents,
            metadatas: search.metadatas,
        }
    }
}

/// Error types for vector database operations
#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("Collection not found: {0}")]
    CollectionNotFound(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Operation failed: {0}")]
    OperationError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Abstract trait for vector database operations
/// Matches Python's VectorDBBase interface
#[async_trait]
pub trait VectorDB: Send + Sync {
    /// Check if a collection exists in the vector database
    async fn has_collection(&self, collection_name: &str) -> Result<bool, VectorError>;

    /// Delete a collection from the vector database
    async fn delete_collection(&self, collection_name: &str) -> Result<(), VectorError>;

    /// Insert vectors into a collection
    async fn insert(
        &self,
        collection_name: &str,
        items: Vec<VectorItem>,
    ) -> Result<(), VectorError>;

    /// Upsert (insert or update) vectors in a collection
    async fn upsert(
        &self,
        collection_name: &str,
        items: Vec<VectorItem>,
    ) -> Result<(), VectorError>;

    /// Search for similar vectors in a collection
    async fn search(
        &self,
        collection_name: &str,
        vectors: Vec<Vec<f32>>,
        limit: usize,
    ) -> Result<SearchResult, VectorError>;

    /// Query vectors from a collection using metadata filter
    async fn query(
        &self,
        collection_name: &str,
        filter: serde_json::Value,
        limit: Option<usize>,
    ) -> Result<GetResult, VectorError>;

    /// Retrieve all vectors from a collection
    async fn get(&self, collection_name: &str) -> Result<GetResult, VectorError>;

    /// Delete vectors by ID or filter from a collection
    async fn delete(
        &self,
        collection_name: &str,
        ids: Option<Vec<String>>,
        filter: Option<serde_json::Value>,
    ) -> Result<(), VectorError>;

    /// Reset the vector database (delete all collections)
    async fn reset(&self) -> Result<(), VectorError>;

    /// Get collection metadata
    async fn get_collection_metadata(
        &self,
        _collection_name: &str,
    ) -> Result<HashMap<String, serde_json::Value>, VectorError> {
        // Default implementation - can be overridden
        Ok(HashMap::new())
    }
}
