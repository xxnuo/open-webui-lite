/// Helper functions for vector database operations in knowledge routes
use crate::error::{AppError, AppResult};
use crate::retrieval::{chunk_text, EmbeddingProvider, VectorDB, VectorError};
use crate::services::file::FileService;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// VectorItem structure for vector database operations
#[derive(Debug, Clone)]
pub struct VectorItem {
    pub id: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub metadata: serde_json::Value,
}

/// Process a file and add its embeddings to the vector database
pub async fn process_and_index_file(
    vector_db: &Arc<dyn VectorDB>,
    embedding_provider: &Arc<dyn EmbeddingProvider>,
    file_service: &FileService<'_>,
    file_id: &str,
    knowledge_id: &str,
) -> AppResult<usize> {
    info!(
        "Processing file {} for knowledge base {}",
        file_id, knowledge_id
    );

    // Get file
    let file = file_service
        .get_file_by_id(file_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("File {} not found", file_id)))?;

    // Check if file has data
    let file_data = file
        .data
        .ok_or_else(|| AppError::BadRequest("File has no processed data".to_string()))?;

    // Extract content from file data
    // The content could be in different fields depending on file type
    let content = extract_content_from_file_data(&file_data)?;

    if content.trim().is_empty() {
        warn!("File {} has no extractable content", file_id);
        return Ok(0);
    }

    // Chunk the content
    let chunk_size = std::env::var("CHUNK_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    let chunk_overlap = std::env::var("CHUNK_OVERLAP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    debug!(
        "Chunking content with size={}, overlap={}",
        chunk_size, chunk_overlap
    );
    let chunks = chunk_text(&content, chunk_size, chunk_overlap);

    if chunks.is_empty() {
        warn!("No chunks generated for file {}", file_id);
        return Ok(0);
    }

    info!("Generated {} chunks for file {}", chunks.len(), file_id);

    // Generate embeddings
    let texts: Vec<String> = chunks.iter().map(|s| s.to_string()).collect();
    let embeddings = embedding_provider
        .embed(texts)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to generate embeddings: {}", e)))?;

    debug!("Generated {} embeddings", embeddings.len());

    // Create vector items
    let items: Vec<crate::retrieval::vector::types::VectorItem> = chunks
        .into_iter()
        .zip(embeddings)
        .enumerate()
        .map(
            |(idx, (chunk, embedding))| crate::retrieval::vector::types::VectorItem {
                id: format!("{}-chunk-{}", file_id, idx),
                text: chunk,
                vector: embedding,
                metadata: json!({
                    "file_id": file_id,
                    "knowledge_id": knowledge_id,
                    "chunk_index": idx,
                    "filename": file.filename,
                }),
            },
        )
        .collect();

    let item_count = items.len();

    // Insert into vector database (upsert to handle updates)
    vector_db
        .upsert(knowledge_id, items)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to index file: {}", e)))?;

    info!(
        "Successfully indexed {} chunks from file {} to knowledge base {}",
        item_count, file_id, knowledge_id
    );

    Ok(item_count)
}

/// Delete a file's vectors from the knowledge base
pub async fn delete_file_vectors(
    vector_db: &Arc<dyn VectorDB>,
    knowledge_id: &str,
    file_id: &str,
) -> AppResult<()> {
    info!(
        "Deleting vectors for file {} from knowledge base {}",
        file_id, knowledge_id
    );

    // Check if collection exists
    let has_collection = vector_db
        .has_collection(knowledge_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check collection: {}", e)))?;

    if !has_collection {
        debug!(
            "Collection {} does not exist, nothing to delete",
            knowledge_id
        );
        return Ok(());
    }

    // Delete by metadata filter
    let filter = json!({"file_id": file_id});

    vector_db
        .delete(knowledge_id, None, Some(filter))
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete file vectors: {}", e)))?;

    info!(
        "Successfully deleted vectors for file {} from knowledge base {}",
        file_id, knowledge_id
    );

    Ok(())
}

/// Delete an entire knowledge base collection
pub async fn delete_knowledge_collection(
    vector_db: &Arc<dyn VectorDB>,
    knowledge_id: &str,
) -> AppResult<()> {
    info!("Deleting collection for knowledge base {}", knowledge_id);

    // Check if collection exists
    let has_collection = vector_db
        .has_collection(knowledge_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check collection: {}", e)))?;

    if !has_collection {
        debug!(
            "Collection {} does not exist, nothing to delete",
            knowledge_id
        );
        return Ok(());
    }

    vector_db
        .delete_collection(knowledge_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete collection: {}", e)))?;

    info!(
        "Successfully deleted collection for knowledge base {}",
        knowledge_id
    );

    Ok(())
}

/// Reset a knowledge base (delete and recreate collection)
pub async fn reset_knowledge_vectors(
    vector_db: &Arc<dyn VectorDB>,
    knowledge_id: &str,
) -> AppResult<()> {
    info!("Resetting vectors for knowledge base {}", knowledge_id);

    delete_knowledge_collection(vector_db, knowledge_id).await?;

    info!(
        "Successfully reset vectors for knowledge base {}",
        knowledge_id
    );

    Ok(())
}

/// Extract text content from file data JSON
fn extract_content_from_file_data(file_data: &serde_json::Value) -> AppResult<String> {
    // Try different possible content fields
    if let Some(content) = file_data.get("content").and_then(|v| v.as_str()) {
        return Ok(content.to_string());
    }

    if let Some(text) = file_data.get("text").and_then(|v| v.as_str()) {
        return Ok(text.to_string());
    }

    if let Some(body) = file_data.get("body").and_then(|v| v.as_str()) {
        return Ok(body.to_string());
    }

    // If it's a structured document, try to extract all text fields
    if let Some(obj) = file_data.as_object() {
        let mut all_text = String::new();
        for (key, value) in obj {
            if let Some(text) = value.as_str() {
                if !text.trim().is_empty() && !key.starts_with("_") && key != "id" && key != "type"
                {
                    if !all_text.is_empty() {
                        all_text.push_str("\n\n");
                    }
                    all_text.push_str(text);
                }
            }
        }
        if !all_text.is_empty() {
            return Ok(all_text);
        }
    }

    Err(AppError::BadRequest(
        "No extractable text content found in file data".to_string(),
    ))
}

/// Check if RAG is enabled and return vector DB and embedding provider
pub fn get_rag_components(
    vector_db: &Option<Arc<dyn VectorDB>>,
    embedding_provider: &Option<Arc<dyn EmbeddingProvider>>,
) -> Option<(Arc<dyn VectorDB>, Arc<dyn EmbeddingProvider>)> {
    match (vector_db, embedding_provider) {
        (Some(vdb), Some(ep)) => Some((vdb.clone(), ep.clone())),
        _ => None,
    }
}

/// Log RAG disabled warning
pub fn log_rag_disabled(operation: &str) {
    debug!(
        "RAG is disabled, skipping vector operation: {}. Set ENABLE_RAG=true and configure vector database to enable.",
        operation
    );
}
