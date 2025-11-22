use super::chroma::{ChromaClient, ChromaConfig};
use super::types::{VectorDB, VectorError};
use std::sync::Arc;
use tracing::info;

/// Supported vector database types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorDBType {
    Chroma,
    // Future: Qdrant, Milvus, etc.
}

impl VectorDBType {
    /// Parse vector database type from string
    pub fn from_str(s: &str) -> Result<Self, VectorError> {
        match s.to_lowercase().as_str() {
            "chroma" => Ok(VectorDBType::Chroma),
            _ => Err(VectorError::ConfigError(format!(
                "Unsupported VECTOR_DB type: {}. Supported types: chroma",
                s
            ))),
        }
    }
}

/// Factory for creating vector database clients
pub struct VectorDBFactory;

impl VectorDBFactory {
    /// Create a vector database client based on the specified type
    pub async fn create(db_type: VectorDBType) -> Result<Arc<dyn VectorDB>, VectorError> {
        info!("Creating vector database client: {:?}", db_type);

        match db_type {
            VectorDBType::Chroma => {
                let config = ChromaConfig::from_env()?;
                let client = ChromaClient::new(config).await?;
                Ok(Arc::new(client))
            }
        }
    }

    /// Create a vector database client from environment variables
    /// Reads VECTOR_DB environment variable (defaults to "chroma")
    pub async fn from_env() -> Result<Arc<dyn VectorDB>, VectorError> {
        let db_type_str = std::env::var("VECTOR_DB").unwrap_or_else(|_| "chroma".to_string());
        let db_type = VectorDBType::from_str(&db_type_str)?;

        info!(
            "Initializing vector database from environment: {}",
            db_type_str
        );

        Self::create(db_type).await
    }

    /// Create a Chroma client with custom configuration
    pub async fn create_chroma(config: ChromaConfig) -> Result<Arc<dyn VectorDB>, VectorError> {
        let client = ChromaClient::new(config).await?;
        Ok(Arc::new(client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_db_type_from_str() {
        assert_eq!(
            VectorDBType::from_str("chroma").unwrap(),
            VectorDBType::Chroma
        );
        assert_eq!(
            VectorDBType::from_str("CHROMA").unwrap(),
            VectorDBType::Chroma
        );
        assert!(VectorDBType::from_str("unsupported").is_err());
    }

    #[tokio::test]
    #[ignore] // Requires Chroma instance
    async fn test_factory_from_env() {
        std::env::set_var("VECTOR_DB", "chroma");
        std::env::set_var("CHROMA_HTTP_HOST", "localhost");
        std::env::set_var("CHROMA_HTTP_PORT", "8000");

        let client = VectorDBFactory::from_env().await;
        assert!(client.is_ok());
    }
}
