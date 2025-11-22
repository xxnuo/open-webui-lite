use super::types::{GetResult, SearchResult, VectorDB, VectorError, VectorItem};
use async_trait::async_trait;
use chromadb::client::{ChromaAuthMethod, ChromaClient as ChromaDbClient, ChromaClientOptions};
use chromadb::collection::{ChromaCollection, CollectionEntries, GetOptions, QueryOptions};
use serde_json::{Map, Value};
use tracing::{debug, info, warn};

/// ChromaDB client implementation
pub struct ChromaClient {
    client: ChromaDbClient,
    config: ChromaConfig,
}

/// Configuration for ChromaDB
#[derive(Debug, Clone)]
pub struct ChromaConfig {
    pub url: Option<String>,
    pub database: String,
    pub auth: ChromaAuthMethod,
}

impl Default for ChromaConfig {
    fn default() -> Self {
        Self {
            url: None,
            database: "default_database".to_string(),
            auth: ChromaAuthMethod::None,
        }
    }
}

impl ChromaConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Result<Self, VectorError> {
        // First check if CHROMA_URL is provided (full URL)
        let url = if let Ok(chroma_url) = std::env::var("CHROMA_URL") {
            Some(chroma_url)
        } else if let Ok(host) = std::env::var("CHROMA_HTTP_HOST") {
            // Construct URL from CHROMA_HTTP_HOST and CHROMA_HTTP_PORT
            let port = std::env::var("CHROMA_HTTP_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(8000);

            // Check if SSL is enabled
            let ssl = std::env::var("CHROMA_HTTP_SSL")
                .unwrap_or_else(|_| "false".to_string())
                .to_lowercase()
                == "true";

            let protocol = if ssl { "https" } else { "http" };

            Some(format!("{}://{}:{}", protocol, host, port))
        } else {
            None
        };

        let database =
            std::env::var("CHROMA_DATABASE").unwrap_or_else(|_| "default_database".to_string());

        // Check for auth token
        let auth = if let Ok(token) = std::env::var("CHROMA_AUTH_TOKEN") {
            ChromaAuthMethod::TokenAuth {
                token,
                header: chromadb::client::ChromaTokenHeader::Authorization,
            }
        } else {
            ChromaAuthMethod::None
        };

        Ok(Self {
            url,
            database,
            auth,
        })
    }
}

impl ChromaClient {
    /// Create a new ChromaClient with the given configuration
    pub async fn new(config: ChromaConfig) -> Result<Self, VectorError> {
        info!(
            "Initializing ChromaDB client: {:?} (database: {})",
            config.url, config.database
        );

        let options = ChromaClientOptions {
            url: config.url.clone(),
            database: config.database.clone(),
            auth: config.auth.clone(),
        };

        let client = ChromaDbClient::new(options).await.map_err(|e| {
            VectorError::ConnectionError(format!("Failed to connect to ChromaDB: {}", e))
        })?;

        info!("Successfully connected to ChromaDB");

        Ok(Self { client, config })
    }

    /// Get or create a collection
    async fn get_or_create_collection(
        &self,
        collection_name: &str,
    ) -> Result<ChromaCollection, VectorError> {
        debug!("Getting or creating collection: {}", collection_name);

        self.client
            .get_or_create_collection(collection_name, None)
            .await
            .map_err(|e| {
                VectorError::DatabaseError(format!(
                    "Failed to get or create collection '{}': {}",
                    collection_name, e
                ))
            })
    }

    /// Get an existing collection
    async fn get_collection(&self, collection_name: &str) -> Result<ChromaCollection, VectorError> {
        debug!("Getting collection: {}", collection_name);

        self.client
            .get_collection(collection_name)
            .await
            .map_err(|e| {
                VectorError::CollectionNotFound(format!(
                    "Collection '{}' not found: {}",
                    collection_name, e
                ))
            })
    }

    /// Convert our VectorItem to ChromaDB CollectionEntries
    fn items_to_entries<'a>(items: &'a [VectorItem]) -> CollectionEntries<'a> {
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        let embeddings: Vec<Vec<f32>> = items.iter().map(|item| item.vector.clone()).collect();
        let documents: Vec<&str> = items.iter().map(|item| item.text.as_str()).collect();
        let metadatas: Vec<Map<String, Value>> = items
            .iter()
            .map(|item| {
                item.metadata
                    .as_object()
                    .cloned()
                    .unwrap_or_else(|| Map::new())
            })
            .collect();

        CollectionEntries {
            ids,
            embeddings: Some(embeddings),
            metadatas: Some(metadatas),
            documents: Some(documents),
        }
    }
}

#[async_trait]
impl VectorDB for ChromaClient {
    async fn has_collection(&self, collection_name: &str) -> Result<bool, VectorError> {
        debug!("Checking if collection exists: {}", collection_name);

        match self.client.get_collection(collection_name).await {
            Ok(_) => {
                debug!("Collection '{}' exists", collection_name);
                Ok(true)
            }
            Err(_) => {
                debug!("Collection '{}' does not exist", collection_name);
                Ok(false)
            }
        }
    }

    async fn delete_collection(&self, collection_name: &str) -> Result<(), VectorError> {
        info!("Deleting collection: {}", collection_name);

        self.client
            .delete_collection(collection_name)
            .await
            .map_err(|e| {
                VectorError::DatabaseError(format!(
                    "Failed to delete collection '{}': {}",
                    collection_name, e
                ))
            })?;

        info!("Successfully deleted collection: {}", collection_name);
        Ok(())
    }

    async fn insert(
        &self,
        collection_name: &str,
        items: Vec<VectorItem>,
    ) -> Result<(), VectorError> {
        if items.is_empty() {
            debug!("No items to insert into collection: {}", collection_name);
            return Ok(());
        }

        info!(
            "Inserting {} items into collection: {}",
            items.len(),
            collection_name
        );

        let collection = self.get_or_create_collection(collection_name).await?;
        let entries = Self::items_to_entries(&items);

        collection.add(entries, None).await.map_err(|e| {
            VectorError::OperationError(format!(
                "Failed to insert items into collection '{}': {}",
                collection_name, e
            ))
        })?;

        info!(
            "Successfully inserted {} items into collection: {}",
            items.len(),
            collection_name
        );
        Ok(())
    }

    async fn upsert(
        &self,
        collection_name: &str,
        items: Vec<VectorItem>,
    ) -> Result<(), VectorError> {
        if items.is_empty() {
            debug!("No items to upsert into collection: {}", collection_name);
            return Ok(());
        }

        info!(
            "Upserting {} items into collection: {}",
            items.len(),
            collection_name
        );

        let collection = self.get_or_create_collection(collection_name).await?;
        let entries = Self::items_to_entries(&items);

        collection.upsert(entries, None).await.map_err(|e| {
            VectorError::OperationError(format!(
                "Failed to upsert items into collection '{}': {}",
                collection_name, e
            ))
        })?;

        info!(
            "Successfully upserted {} items into collection: {}",
            items.len(),
            collection_name
        );
        Ok(())
    }

    async fn search(
        &self,
        collection_name: &str,
        vectors: Vec<Vec<f32>>,
        limit: usize,
    ) -> Result<SearchResult, VectorError> {
        debug!(
            "Searching collection '{}' with {} query vectors, limit: {}",
            collection_name,
            vectors.len(),
            limit
        );

        let collection = self.get_collection(collection_name).await?;

        let query_options = QueryOptions {
            query_embeddings: Some(vectors),
            query_texts: None,
            n_results: Some(limit),
            where_metadata: None,
            where_document: None,
            include: Some(vec!["metadatas", "documents", "distances"]),
        };

        let result = collection.query(query_options, None).await.map_err(|e| {
            VectorError::OperationError(format!(
                "Failed to search collection '{}': {}",
                collection_name, e
            ))
        })?;

        // Convert ChromaDB QueryResult to our SearchResult
        let search_result = SearchResult {
            ids: Some(result.ids),
            documents: result.documents.map(|docs| {
                docs.into_iter()
                    .map(|doc_vec| doc_vec.into_iter().collect())
                    .collect()
            }),
            metadatas: result.metadatas.map(|metas| {
                metas
                    .into_iter()
                    .map(|meta_vec| {
                        meta_vec
                            .into_iter()
                            .map(|m| Value::Object(m.unwrap_or_default()))
                            .collect()
                    })
                    .collect()
            }),
            distances: result.distances,
        };

        debug!(
            "Search returned {} results from collection: {}",
            search_result
                .ids
                .as_ref()
                .map(|ids| ids.iter().map(|v| v.len()).sum::<usize>())
                .unwrap_or(0),
            collection_name
        );

        Ok(search_result)
    }

    async fn query(
        &self,
        collection_name: &str,
        filter: Value,
        limit: Option<usize>,
    ) -> Result<GetResult, VectorError> {
        debug!(
            "Querying collection '{}' with filter: {:?}, limit: {:?}",
            collection_name, filter, limit
        );

        let collection = self.get_collection(collection_name).await?;

        let get_options = GetOptions {
            ids: vec![],
            where_metadata: Some(filter),
            limit,
            offset: None,
            where_document: None,
            include: Some(vec!["metadatas".to_string(), "documents".to_string()]),
        };

        let result = collection.get(get_options).await.map_err(|e| {
            VectorError::OperationError(format!(
                "Failed to query collection '{}': {}",
                collection_name, e
            ))
        })?;

        // Convert ChromaDB GetResult to our GetResult
        let get_result = GetResult {
            ids: Some(vec![result.ids]),
            documents: result
                .documents
                .map(|docs| vec![docs.into_iter().map(|d| d.unwrap_or_default()).collect()]),
            metadatas: result.metadatas.map(|metas| {
                vec![metas
                    .into_iter()
                    .map(|m| Value::Object(m.unwrap_or_default()))
                    .collect()]
            }),
        };

        debug!(
            "Query returned {} results from collection: {}",
            get_result
                .ids
                .as_ref()
                .and_then(|ids| ids.first())
                .map(|ids| ids.len())
                .unwrap_or(0),
            collection_name
        );

        Ok(get_result)
    }

    async fn get(&self, collection_name: &str) -> Result<GetResult, VectorError> {
        debug!("Getting all items from collection: {}", collection_name);

        let collection = self.get_collection(collection_name).await?;

        let get_options = GetOptions {
            ids: vec![],
            where_metadata: None,
            limit: None,
            offset: None,
            where_document: None,
            include: Some(vec!["metadatas".to_string(), "documents".to_string()]),
        };

        let result = collection.get(get_options).await.map_err(|e| {
            VectorError::OperationError(format!(
                "Failed to get items from collection '{}': {}",
                collection_name, e
            ))
        })?;

        // Convert ChromaDB GetResult to our GetResult
        let get_result = GetResult {
            ids: Some(vec![result.ids]),
            documents: result
                .documents
                .map(|docs| vec![docs.into_iter().map(|d| d.unwrap_or_default()).collect()]),
            metadatas: result.metadatas.map(|metas| {
                vec![metas
                    .into_iter()
                    .map(|m| Value::Object(m.unwrap_or_default()))
                    .collect()]
            }),
        };

        debug!(
            "Retrieved {} items from collection: {}",
            get_result
                .ids
                .as_ref()
                .and_then(|ids| ids.first())
                .map(|ids| ids.len())
                .unwrap_or(0),
            collection_name
        );

        Ok(get_result)
    }

    async fn delete(
        &self,
        collection_name: &str,
        ids: Option<Vec<String>>,
        filter: Option<Value>,
    ) -> Result<(), VectorError> {
        debug!(
            "Deleting items from collection '{}' with ids: {:?}, filter: {:?}",
            collection_name, ids, filter
        );

        let collection = self.get_collection(collection_name).await?;

        // Convert ids to &str if provided
        let ids_refs: Option<Vec<&str>> = ids
            .as_ref()
            .map(|id_vec| id_vec.iter().map(|s| s.as_str()).collect());

        collection
            .delete(ids_refs, filter, None)
            .await
            .map_err(|e| {
                VectorError::OperationError(format!(
                    "Failed to delete items from collection '{}': {}",
                    collection_name, e
                ))
            })?;

        info!(
            "Successfully deleted items from collection: {}",
            collection_name
        );
        Ok(())
    }

    async fn reset(&self) -> Result<(), VectorError> {
        warn!("Resetting vector database (deleting all collections)");

        // ChromaDB client doesn't expose a list_collections method in the public API
        // For safety, we'll just log a warning
        warn!("Reset operation not fully implemented - ChromaDB client API limitation");
        warn!("Please manually delete collections or use ChromaDB admin tools");

        Ok(())
    }

    async fn get_collection_metadata(
        &self,
        collection_name: &str,
    ) -> Result<std::collections::HashMap<String, Value>, VectorError> {
        debug!("Getting metadata for collection: {}", collection_name);

        let collection = self.get_collection(collection_name).await?;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "name".to_string(),
            Value::String(collection.name().to_string()),
        );
        metadata.insert("id".to_string(), Value::String(collection.id().to_string()));

        if let Some(meta) = collection.metadata() {
            metadata.insert("metadata".to_string(), Value::Object(meta.clone()));
        }

        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Chroma instance
    // Run with: docker run -p 8000:8000 chromadb/chroma

    #[tokio::test]
    #[ignore] // Ignore by default since it requires external service
    async fn test_chroma_connection() {
        let config = ChromaConfig::default();
        let client = ChromaClient::new(config).await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn test_collection_operations() {
        let config = ChromaConfig::default();
        let client = ChromaClient::new(config).await.unwrap();

        let collection_name = "test_collection";

        // Create and check collection
        let item = VectorItem {
            id: "test1".to_string(),
            text: "test document".to_string(),
            vector: vec![0.1, 0.2, 0.3],
            metadata: serde_json::json!({"key": "value"}),
        };

        client.insert(collection_name, vec![item]).await.unwrap();

        assert!(client.has_collection(collection_name).await.unwrap());

        // Cleanup
        client.delete_collection(collection_name).await.unwrap();
        assert!(!client.has_collection(collection_name).await.unwrap());
    }
}
