use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    String(String),
    Array(Vec<String>),
    TokenArray(Vec<i32>),
    TokenArrayArray(Vec<Vec<i32>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: i32,
    pub total_tokens: i32,
}

#[allow(dead_code)]
impl EmbeddingRequest {
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<(), AppError> {
        // Validate model
        if self.model.is_empty() {
            return Err(AppError::BadRequest("Model cannot be empty".to_string()));
        }

        // Validate input
        match &self.input {
            EmbeddingInput::String(s) if s.is_empty() => {
                return Err(AppError::BadRequest("Input cannot be empty".to_string()));
            }
            EmbeddingInput::Array(arr) if arr.is_empty() => {
                return Err(AppError::BadRequest(
                    "Input array cannot be empty".to_string(),
                ));
            }
            EmbeddingInput::TokenArray(arr) if arr.is_empty() => {
                return Err(AppError::BadRequest(
                    "Token array cannot be empty".to_string(),
                ));
            }
            EmbeddingInput::TokenArrayArray(arr) if arr.is_empty() => {
                return Err(AppError::BadRequest(
                    "Token array array cannot be empty".to_string(),
                ));
            }
            _ => {}
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_input_strings(&self) -> Vec<String> {
        match &self.input {
            EmbeddingInput::String(s) => vec![s.clone()],
            EmbeddingInput::Array(arr) => arr.clone(),
            EmbeddingInput::TokenArray(_) | EmbeddingInput::TokenArrayArray(_) => {
                // For token arrays, we'll need to decode them
                // For now, return empty vec as placeholder
                vec![]
            }
        }
    }
}

#[allow(dead_code)]
impl EmbeddingResponse {
    #[allow(dead_code)]
    pub fn new(model: String, embeddings: Vec<Vec<f32>>) -> Self {
        let data = embeddings
            .into_iter()
            .enumerate()
            .map(|(i, embedding)| EmbeddingData {
                object: "embedding".to_string(),
                embedding,
                index: i as i32,
            })
            .collect::<Vec<_>>();

        let total_tokens: i32 = data.iter().map(|d| d.embedding.len() as i32).sum();

        Self {
            object: "list".to_string(),
            data,
            model,
            usage: EmbeddingUsage {
                prompt_tokens: total_tokens,
                total_tokens,
            },
        }
    }
}

/// Create a mock embedding for testing purposes
#[allow(dead_code)]
pub fn create_mock_embedding(dimension: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..dimension).map(|_| rng.random::<f32>()).collect()
}

/// Generate embeddings using OpenAI API
#[allow(dead_code)]
pub async fn generate_openai_embeddings(
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: Vec<String>,
    dimension: Option<i32>,
) -> Result<Vec<Vec<f32>>, AppError> {
    let client = reqwest::Client::new();

    let mut payload = json!({
        "model": model,
        "input": texts,
    });

    if let Some(dim) = dimension {
        payload["dimensions"] = json!(dim);
    }

    let response = client
        .post(format!("{}/embeddings", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::ExternalServiceError(format!(
            "Failed to generate embeddings: {} - {}",
            status, error_text
        )));
    }

    let response_data: EmbeddingResponse = response.json().await?;

    Ok(response_data
        .data
        .into_iter()
        .map(|d| d.embedding)
        .collect())
}

/// Generate embeddings using Azure OpenAI API
#[allow(dead_code)]
pub async fn generate_azure_openai_embeddings(
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: Vec<String>,
    api_version: &str,
    dimension: Option<i32>,
) -> Result<Vec<Vec<f32>>, AppError> {
    let client = reqwest::Client::new();

    let url = format!(
        "{}/openai/deployments/{}/embeddings?api-version={}",
        base_url, model, api_version
    );

    let mut payload = json!({
        "input": texts,
    });

    if let Some(dim) = dimension {
        payload["dimensions"] = json!(dim);
    }

    let response = client
        .post(&url)
        .header("api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::ExternalServiceError(format!(
            "Failed to generate Azure embeddings: {} - {}",
            status, error_text
        )));
    }

    let response_data: EmbeddingResponse = response.json().await?;

    Ok(response_data
        .data
        .into_iter()
        .map(|d| d.embedding)
        .collect())
}

/// Generate embeddings using local candle library (pure Rust)
#[cfg(feature = "embeddings")]
#[allow(dead_code)]
pub async fn generate_local_embeddings(
    model_name: String,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, AppError> {
    // Use candle for local embedding generation
    // This is CPU-bound so we run it in a blocking task
    tokio::task::spawn_blocking(move || {
        use candle_core::{Device, Tensor};
        use candle_nn::VarBuilder;
        use candle_transformers::models::bert::{BertModel, Config as BertConfig};
        use hf_hub::{api::sync::Api, Repo, RepoType};
        use tokenizers::Tokenizer;

        // Map model name to HuggingFace model ID
        let (repo_id, revision) = match model_name.as_str() {
            "all-minilm-l6-v2" | "all-MiniLM-L6-v2" => {
                ("sentence-transformers/all-MiniLM-L6-v2", "main")
            }
            "bge-base-en-v1.5" => ("BAAI/bge-base-en-v1.5", "main"),
            "bge-small-en-v1.5" => ("BAAI/bge-small-en-v1.5", "main"),
            _ => ("sentence-transformers/all-MiniLM-L6-v2", "main"), // Default
        };

        // Download model files from HuggingFace
        let api = Api::new().map_err(|e| {
            AppError::InternalServerError(format!("Failed to initialize HF API: {}", e))
        })?;
        let repo = api.repo(Repo::with_revision(
            repo_id.to_string(),
            RepoType::Model,
            revision.to_string(),
        ));

        let tokenizer_path = repo.get("tokenizer.json").map_err(|e| {
            AppError::InternalServerError(format!("Failed to download tokenizer: {}", e))
        })?;
        let config_path = repo.get("config.json").map_err(|e| {
            AppError::InternalServerError(format!("Failed to download config: {}", e))
        })?;
        let weights_path = repo
            .get("model.safetensors")
            .or_else(|_| repo.get("pytorch_model.bin"))
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to download model weights: {}", e))
            })?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| {
            AppError::InternalServerError(format!("Failed to load tokenizer: {}", e))
        })?;

        // Load config
        let config_str = std::fs::read_to_string(config_path)
            .map_err(|e| AppError::InternalServerError(format!("Failed to read config: {}", e)))?;
        let config: BertConfig = serde_json::from_str(&config_str)
            .map_err(|e| AppError::InternalServerError(format!("Failed to parse config: {}", e)))?;

        // Load model
        let device = Device::Cpu;
        let vb = VarBuilder::from_pth(&weights_path, candle_core::DType::F32, &device)
            .or_else(|_| unsafe {
                VarBuilder::from_mmaped_safetensors(
                    &[weights_path],
                    candle_core::DType::F32,
                    &device,
                )
            })
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to load model weights: {}", e))
            })?;

        let model = BertModel::load(vb, &config).map_err(|e| {
            AppError::InternalServerError(format!("Failed to load BERT model: {}", e))
        })?;

        // Process each text
        let mut all_embeddings = Vec::new();
        for text in texts {
            // Tokenize
            let encoding = tokenizer.encode(text, true).map_err(|e| {
                AppError::InternalServerError(format!("Tokenization failed: {}", e))
            })?;
            let token_ids = encoding.get_ids().to_vec();
            let token_type_ids = encoding.get_type_ids().to_vec();

            // Convert to tensors
            let token_ids = Tensor::new(token_ids.as_slice(), &device)
                .map_err(|e| {
                    AppError::InternalServerError(format!("Failed to create token tensor: {}", e))
                })?
                .unsqueeze(0)
                .map_err(|e| {
                    AppError::InternalServerError(format!("Failed to unsqueeze: {}", e))
                })?;
            let token_type_ids = Tensor::new(token_type_ids.as_slice(), &device)
                .map_err(|e| {
                    AppError::InternalServerError(format!("Failed to create type tensor: {}", e))
                })?
                .unsqueeze(0)
                .map_err(|e| {
                    AppError::InternalServerError(format!("Failed to unsqueeze: {}", e))
                })?;

            // Run model (attention_mask is optional third parameter)
            let embeddings = model
                .forward(&token_ids, &token_type_ids, None)
                .map_err(|e| {
                    AppError::InternalServerError(format!("Model forward failed: {}", e))
                })?;

            // Mean pooling
            let embedding = embeddings
                .mean(1)
                .map_err(|e| AppError::InternalServerError(format!("Mean pooling failed: {}", e)))?
                .squeeze(0)
                .map_err(|e| AppError::InternalServerError(format!("Squeeze failed: {}", e)))?;

            // Convert to Vec<f32>
            let embedding_vec = embedding.to_vec1::<f32>().map_err(|e| {
                AppError::InternalServerError(format!("Failed to convert to vec: {}", e))
            })?;

            all_embeddings.push(embedding_vec);
        }

        Ok(all_embeddings)
    })
    .await
    .map_err(|e| AppError::InternalServerError(format!("Task join error: {}", e)))?
}

#[cfg(not(feature = "embeddings"))]
#[allow(dead_code)]
pub async fn generate_local_embeddings(
    _model_name: String,
    _texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, AppError> {
    Err(AppError::InternalServerError(
        "Local embeddings support not compiled. Enable the 'embeddings' feature or use openai/azure engine.".to_string()
    ))
}

/// Generate embeddings using the configured embedding model
/// This is the main entry point that dispatches to the appropriate backend
#[allow(dead_code)]
pub async fn generate_embeddings(
    engine: &str,
    model: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
    texts: Vec<String>,
    dimension: Option<i32>,
) -> Result<Vec<Vec<f32>>, AppError> {
    match engine {
        "openai" => {
            let url = base_url.ok_or_else(|| {
                AppError::BadRequest("Base URL required for OpenAI embeddings".to_string())
            })?;
            let key = api_key.ok_or_else(|| {
                AppError::BadRequest("API key required for OpenAI embeddings".to_string())
            })?;
            generate_openai_embeddings(url, key, model, texts, dimension).await
        }
        "azure_openai" => {
            let url = base_url.ok_or_else(|| {
                AppError::BadRequest("Base URL required for Azure OpenAI embeddings".to_string())
            })?;
            let key = api_key.ok_or_else(|| {
                AppError::BadRequest("API key required for Azure OpenAI embeddings".to_string())
            })?;
            let api_version = "2023-05-15"; // Default version
            generate_azure_openai_embeddings(url, key, model, texts, api_version, dimension).await
        }
        "local" | "" => {
            // Use local fastembed
            generate_local_embeddings(model.to_string(), texts).await
        }
        _ => Err(AppError::BadRequest(format!(
            "Unknown embedding engine: {}",
            engine
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_request_validation() {
        let valid_request = EmbeddingRequest {
            model: "text-embedding-ada-002".to_string(),
            input: EmbeddingInput::String("Hello, world!".to_string()),
            encoding_format: None,
            dimensions: None,
            user: None,
        };
        assert!(valid_request.validate().is_ok());

        let invalid_request = EmbeddingRequest {
            model: "".to_string(),
            input: EmbeddingInput::String("Hello".to_string()),
            encoding_format: None,
            dimensions: None,
            user: None,
        };
        assert!(invalid_request.validate().is_err());
    }

    #[test]
    fn test_get_input_strings() {
        let request = EmbeddingRequest {
            model: "test".to_string(),
            input: EmbeddingInput::Array(vec!["text1".to_string(), "text2".to_string()]),
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let strings = request.get_input_strings();
        assert_eq!(strings.len(), 2);
        assert_eq!(strings[0], "text1");
        assert_eq!(strings[1], "text2");
    }

    #[test]
    fn test_embedding_response_creation() {
        let embeddings = vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]];

        let response = EmbeddingResponse::new("test-model".to_string(), embeddings);

        assert_eq!(response.data.len(), 2);
        assert_eq!(response.model, "test-model");
        assert_eq!(response.usage.total_tokens, 6);
    }
}
