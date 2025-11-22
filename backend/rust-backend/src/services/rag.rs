use crate::config::Config;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub top_k: usize,
    pub filters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk: DocumentChunk,
    pub score: f32,
}

#[allow(dead_code)]
pub struct RAGService {
    client: Client,
    config: Config,
}

#[allow(dead_code)]
impl RAGService {
    pub async fn new(config: Config) -> AppResult<Self> {
        Ok(RAGService {
            client: Client::new(),
            config,
        })
    }

    /// Generate embeddings for text using the configured embedding engine
    pub async fn generate_embedding(&mut self, text: &str) -> AppResult<Vec<f32>> {
        // Use the centralized embedding generation from utils
        let embeddings = crate::utils::embeddings::generate_embeddings(
            &self.config.rag_embedding_engine,
            &self.config.rag_embedding_model,
            None, // base_url - would need to be added to config if using API
            None, // api_key - would need to be added to config if using API
            vec![text.to_string()],
            None, // dimension
        )
        .await?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| AppError::InternalServerError("No embedding generated".to_string()))
    }

    /// Chunk document into smaller pieces
    pub fn chunk_document(
        &self,
        text: &str,
        chunk_size: usize,
        chunk_overlap: usize,
    ) -> Vec<String> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();

        let mut start = 0;
        while start < len {
            let end = (start + chunk_size).min(len);
            let chunk: String = chars[start..end].iter().collect();
            chunks.push(chunk);

            if end >= len {
                break;
            }
            start = end - chunk_overlap;
        }

        chunks
    }

    /// Index document chunks with embeddings
    pub async fn index_document(
        &mut self,
        doc_id: &str,
        text: &str,
        metadata: serde_json::Value,
    ) -> AppResult<Vec<DocumentChunk>> {
        let chunk_size = self.config.chunk_size;
        let chunk_overlap = self.config.chunk_overlap;

        let text_chunks = self.chunk_document(text, chunk_size, chunk_overlap);

        let mut chunks = Vec::new();
        for (idx, content) in text_chunks.into_iter().enumerate() {
            let embedding = self.generate_embedding(&content).await?;
            chunks.push(DocumentChunk {
                id: format!("{}-{}", doc_id, idx),
                content,
                metadata: metadata.clone(),
                embedding: Some(embedding),
            });
        }

        Ok(chunks)
    }

    /// Calculate cosine similarity between two vectors
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return 0.0;
        }

        dot_product / (magnitude_a * magnitude_b)
    }

    /// Search for relevant chunks
    pub async fn search(
        &mut self,
        request: SearchRequest,
        chunks: &[DocumentChunk],
    ) -> AppResult<Vec<SearchResult>> {
        let query_embedding = self.generate_embedding(&request.query).await?;

        let mut results: Vec<SearchResult> = chunks
            .iter()
            .filter_map(|chunk| {
                if let Some(embedding) = &chunk.embedding {
                    let score = self.cosine_similarity(&query_embedding, embedding);
                    Some(SearchResult {
                        chunk: chunk.clone(),
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Return top K results
        Ok(results.into_iter().take(request.top_k).collect())
    }

    /// Rerank results using cross-encoder
    pub async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
    ) -> AppResult<Vec<SearchResult>> {
        // Simple reranking based on exact match and keyword overlap
        // In production, use a proper cross-encoder model
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut reranked: Vec<SearchResult> = results
            .into_iter()
            .map(|mut result| {
                let content_lower = result.chunk.content.to_lowercase();

                // Boost for exact phrase match
                if content_lower.contains(&query_lower) {
                    result.score += 0.3;
                }

                // Boost for keyword matches
                let matches = query_words
                    .iter()
                    .filter(|word| content_lower.contains(*word))
                    .count();
                result.score += (matches as f32 / query_words.len() as f32) * 0.2;

                result
            })
            .collect();

        reranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(reranked)
    }
}
