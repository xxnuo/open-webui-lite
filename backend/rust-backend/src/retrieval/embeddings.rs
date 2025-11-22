use async_openai::{
    config::OpenAIConfig,
    types::{CreateEmbeddingRequest, EmbeddingInput},
    Client,
};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info};

#[cfg(feature = "embeddings")]
use std::path::PathBuf;
#[cfg(feature = "embeddings")]
use tokio::sync::RwLock;
#[cfg(feature = "embeddings")]
use tracing::warn;

#[cfg(feature = "embeddings")]
use candle_core::{Device, Tensor};
#[cfg(feature = "embeddings")]
use candle_nn::VarBuilder;
#[cfg(feature = "embeddings")]
use hf_hub::{api::sync::Api, Repo, RepoType};
#[cfg(feature = "embeddings")]
use tokenizers::Tokenizer;

/// Error types for embedding operations
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Model error: {0}")]
    ModelError(String),
}

/// Trait for embedding providers
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for a list of texts
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Generate embeddings with optional prefix
    async fn embed_with_prefix(
        &self,
        texts: Vec<String>,
        prefix: Option<&str>,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        // Default implementation: prepend prefix if provided
        if let Some(prefix_str) = prefix {
            let prefixed_texts: Vec<String> = texts
                .into_iter()
                .map(|text| format!("{}{}", prefix_str, text))
                .collect();
            self.embed(prefixed_texts).await
        } else {
            self.embed(texts).await
        }
    }

    /// Get the dimension of the embeddings
    fn dimension(&self) -> usize;

    /// Get the model name
    fn model_name(&self) -> &str;
}

/// Wrapper for embedding functions with configurable prefixes
pub struct EmbeddingFunction {
    provider: Arc<dyn EmbeddingProvider>,
    query_prefix: String,
    content_prefix: String,
}

impl EmbeddingFunction {
    /// Create a new embedding function with prefixes
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        query_prefix: String,
        content_prefix: String,
    ) -> Self {
        Self {
            provider,
            query_prefix,
            content_prefix,
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, EmbeddingError> {
        let provider = EmbeddingFactory::from_env()?;
        let query_prefix = std::env::var("RAG_EMBEDDING_QUERY_PREFIX").unwrap_or_default();
        let content_prefix = std::env::var("RAG_EMBEDDING_CONTENT_PREFIX").unwrap_or_default();

        Ok(Self::new(provider, query_prefix, content_prefix))
    }

    /// Generate embeddings for query texts
    pub async fn embed_query(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let prefix = if self.query_prefix.is_empty() {
            None
        } else {
            Some(self.query_prefix.as_str())
        };
        self.provider.embed_with_prefix(texts, prefix).await
    }

    /// Generate embeddings for content/document texts
    pub async fn embed_content(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let prefix = if self.content_prefix.is_empty() {
            None
        } else {
            Some(self.content_prefix.as_str())
        };
        self.provider.embed_with_prefix(texts, prefix).await
    }

    /// Generate embeddings without prefix
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        self.provider.embed(texts).await
    }

    /// Get the underlying provider
    pub fn provider(&self) -> &Arc<dyn EmbeddingProvider> {
        &self.provider
    }
}

/// OpenAI embedding provider
pub struct OpenAIEmbeddings {
    client: Client<OpenAIConfig>,
    model: String,
    dimension: usize,
    /// Semaphore to limit concurrent requests
    semaphore: Arc<Semaphore>,
}

impl OpenAIEmbeddings {
    /// Create a new OpenAI embedding provider
    pub fn new(api_key: Option<String>, model: Option<String>) -> Result<Self, EmbeddingError> {
        let api_key = api_key
            .or_else(|| std::env::var("RAG_OPENAI_API_KEY").ok())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| {
                EmbeddingError::ConfigError(
                    "OPENAI_API_KEY not set and no API key provided".to_string(),
                )
            })?;

        let model = model.unwrap_or_else(|| "text-embedding-3-small".to_string());

        // Determine dimension based on model
        let dimension = match model.as_str() {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => 1536, // Default
        };

        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        // Limit concurrent requests to avoid rate limits
        let max_concurrent = std::env::var("OPENAI_MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        info!(
            "Initialized OpenAI embeddings: model={}, dimension={}, max_concurrent={}",
            model, dimension, max_concurrent
        );

        Ok(Self {
            client,
            model,
            dimension,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, EmbeddingError> {
        let api_key = std::env::var("RAG_OPENAI_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .ok();
        let model = std::env::var("RAG_EMBEDDING_MODEL").ok();
        Self::new(api_key, model)
    }

    /// Embed texts in batches to avoid API limits
    async fn embed_batch(
        &self,
        texts: Vec<String>,
        batch_size: usize,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut all_embeddings = Vec::new();

        for chunk in texts.chunks(batch_size) {
            let _permit = self.semaphore.acquire().await.map_err(|e| {
                EmbeddingError::ApiError(format!("Failed to acquire semaphore: {}", e))
            })?;

            debug!("Embedding batch of {} texts", chunk.len());

            let request = CreateEmbeddingRequest {
                model: self.model.clone(),
                input: EmbeddingInput::StringArray(chunk.to_vec()),
                encoding_format: None,
                user: None,
                dimensions: None,
            };

            let response = self
                .client
                .embeddings()
                .create(request)
                .await
                .map_err(|e| EmbeddingError::ApiError(format!("OpenAI API error: {}", e)))?;

            for embedding_data in response.data {
                all_embeddings.push(embedding_data.embedding);
            }
        }

        Ok(all_embeddings)
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OpenAIEmbeddings {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        info!("Generating embeddings for {} texts", texts.len());

        // OpenAI has a limit of ~8000 tokens per request, so batch accordingly
        // Assuming average of 100 tokens per text, batch size of 50 should be safe
        let batch_size = 50;

        self.embed_batch(texts, batch_size).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Knox Chat embedding provider
pub struct KnoxChatEmbeddings {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    dimension: usize,
    /// Semaphore to limit concurrent requests
    semaphore: Arc<Semaphore>,
}

impl KnoxChatEmbeddings {
    /// Create a new Knox Chat embedding provider
    pub fn new(
        api_key: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Self, EmbeddingError> {
        let api_key = api_key
            .or_else(|| std::env::var("KNOXCHAT_API_KEY").ok())
            .ok_or_else(|| {
                EmbeddingError::ConfigError(
                    "KNOXCHAT_API_KEY not set and no API key provided".to_string(),
                )
            })?;

        let base_url = base_url
            .or_else(|| std::env::var("KNOXCHAT_BASE_URL").ok())
            .unwrap_or_else(|| "https://knox.chat".to_string());

        let model = model.unwrap_or_else(|| "voyage-3.5".to_string());

        // Determine dimension based on model
        // Common Knox Chat / Voyage models and their dimensions
        let dimension = match model.as_str() {
            "voyage-3.5" => 1024,
            "voyage-3.5-lite" => 1024,
            "voyage-code-3" => 2048,
            "voyage-finance-2" => 1024,
            "voyage-law-2" => 1024,
            "voyage-code-2" => 1024,
            "voyage-3-large" => 2048,

            _ => 1024, // Default
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| {
                EmbeddingError::ConfigError(format!("Failed to create HTTP client: {}", e))
            })?;

        // Limit concurrent requests
        let max_concurrent = std::env::var("KNOXCHAT_MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        info!(
            "Initialized Knox Chat embeddings: model={}, dimension={}, max_concurrent={}, base_url={}",
            model, dimension, max_concurrent, base_url
        );

        Ok(Self {
            client,
            api_key,
            base_url,
            model,
            dimension,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, EmbeddingError> {
        let api_key = std::env::var("KNOXCHAT_API_KEY").ok();
        let base_url = std::env::var("KNOXCHAT_BASE_URL").ok();
        let model = std::env::var("RAG_EMBEDDING_MODEL").ok();
        Self::new(api_key, base_url, model)
    }

    /// Embed texts in batches
    async fn embed_batch(
        &self,
        texts: Vec<String>,
        batch_size: usize,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut all_embeddings = Vec::new();

        for chunk in texts.chunks(batch_size) {
            let _permit = self.semaphore.acquire().await.map_err(|e| {
                EmbeddingError::ApiError(format!("Failed to acquire semaphore: {}", e))
            })?;

            debug!("Embedding batch of {} texts", chunk.len());

            let url = format!("{}/v1/embeddings", self.base_url);

            let payload = serde_json::json!({
                "input": chunk,
                "model": self.model,
            });

            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| {
                    EmbeddingError::ApiError(format!("Knox Chat API request failed: {}", e))
                })?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(EmbeddingError::ApiError(format!(
                    "Knox Chat API error ({}): {}",
                    status, error_text
                )));
            }

            let response_json: serde_json::Value = response.json().await.map_err(|e| {
                EmbeddingError::ApiError(format!("Failed to parse Knox Chat response: {}", e))
            })?;

            // Parse response format: {"data": [{"embedding": [...], "index": 0}, ...]}
            let data = response_json
                .get("data")
                .and_then(|d| d.as_array())
                .ok_or_else(|| {
                    EmbeddingError::ApiError(
                        "Invalid Knox Chat response format: missing 'data' array".to_string(),
                    )
                })?;

            for item in data {
                let embedding = item
                    .get("embedding")
                    .and_then(|e| e.as_array())
                    .ok_or_else(|| {
                        EmbeddingError::ApiError(
                            "Invalid Knox Chat response format: missing 'embedding' array"
                                .to_string(),
                        )
                    })?
                    .iter()
                    .map(|v| {
                        v.as_f64().map(|f| f as f32).ok_or_else(|| {
                            EmbeddingError::ApiError("Invalid embedding value".to_string())
                        })
                    })
                    .collect::<Result<Vec<f32>, _>>()?;

                all_embeddings.push(embedding);
            }
        }

        Ok(all_embeddings)
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for KnoxChatEmbeddings {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        info!(
            "Generating embeddings for {} texts using Knox Chat",
            texts.len()
        );

        // Knox Chat can handle reasonable batch sizes
        let batch_size = 50;

        self.embed_batch(texts, batch_size).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Sentence Transformer embedding provider (local models)
#[cfg(feature = "embeddings")]
pub struct SentenceTransformerEmbeddings {
    model_name: String,
    model_path: PathBuf,
    dimension: usize,
    normalize: bool,
    /// Cache for loaded model components
    /// Using RwLock for thread-safe lazy loading
    model_cache: Arc<RwLock<Option<ModelComponents>>>,
}

#[cfg(feature = "embeddings")]
struct ModelComponents {
    model: candle_transformers::models::bert::BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

#[cfg(feature = "embeddings")]
impl SentenceTransformerEmbeddings {
    /// Create a new SentenceTransformer embedding provider
    pub fn new(model_name: String, auto_update: bool) -> Result<Self, EmbeddingError> {
        info!("Initializing SentenceTransformer: {}", model_name);

        // Get model path (download if needed)
        let model_path = Self::get_model_path(&model_name, auto_update)?;

        // Determine dimension based on model name
        let dimension = Self::get_model_dimension(&model_name);

        // Most sentence transformers normalize embeddings by default
        let normalize = true;

        Ok(Self {
            model_name,
            model_path,
            dimension,
            normalize,
            model_cache: Arc::new(RwLock::new(None)),
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, EmbeddingError> {
        let model_name = std::env::var("RAG_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "sentence-transformers/all-MiniLM-L6-v2".to_string());
        let auto_update = false; // Don't auto-update by default
        Self::new(model_name, auto_update)
    }

    /// Get the local path to a model, downloading from HuggingFace Hub if needed
    fn get_model_path(model_name: &str, auto_update: bool) -> Result<PathBuf, EmbeddingError> {
        let cache_dir = std::env::var("SENTENCE_TRANSFORMERS_HOME")
            .ok()
            .map(PathBuf::from);

        let local_files_only = !auto_update;

        // Check if it's already a local path
        let model_path = PathBuf::from(model_name);
        if model_path.exists() {
            info!("Using local model path: {:?}", model_path);
            return Ok(model_path);
        }

        // Normalize model name for HuggingFace
        let repo_id = if model_name.contains('/') {
            model_name.to_string()
        } else {
            format!("sentence-transformers/{}", model_name)
        };

        info!("Downloading model from HuggingFace: {}", repo_id);

        // Use HuggingFace Hub API to get/download model
        // Note: hf_hub crate uses HF_HOME environment variable for cache directory
        // We'll set it if cache_dir is provided
        if let Some(cache) = &cache_dir {
            if let Some(cache_str) = cache.to_str() {
                std::env::set_var("HF_HOME", cache_str);
            }
        }

        let api = Api::new().map_err(|e| {
            EmbeddingError::ConfigError(format!("Failed to initialize HF API: {}", e))
        })?;

        let repo = api.repo(Repo::new(repo_id.clone(), RepoType::Model));

        // Try to get model info (this will download if not cached)
        let model_dir = if local_files_only {
            // Try to use cached version
            repo.get("config.json")
                .map_err(|e| {
                    EmbeddingError::ConfigError(format!(
                        "Model not found in cache and auto_update=false: {}. Error: {}",
                        repo_id, e
                    ))
                })?
                .parent()
                .ok_or_else(|| EmbeddingError::ConfigError("Invalid model path".to_string()))?
                .to_path_buf()
        } else {
            // Download if needed
            let config_path = repo.get("config.json").map_err(|e| {
                EmbeddingError::ConfigError(format!("Failed to download model config: {}", e))
            })?;
            config_path
                .parent()
                .ok_or_else(|| EmbeddingError::ConfigError("Invalid model path".to_string()))?
                .to_path_buf()
        };

        info!("Model path: {:?}", model_dir);
        Ok(model_dir)
    }

    /// Get the embedding dimension for a given model
    fn get_model_dimension(model_name: &str) -> usize {
        // Common sentence transformer models and their dimensions
        match model_name {
            s if s.contains("all-MiniLM-L6-v2") => 384,
            s if s.contains("all-MiniLM-L12-v2") => 384,
            s if s.contains("all-mpnet-base-v2") => 768,
            s if s.contains("paraphrase-MiniLM-L6-v2") => 384,
            s if s.contains("paraphrase-mpnet-base-v2") => 768,
            s if s.contains("bge-small-en-v1.5") => 384,
            s if s.contains("bge-base-en-v1.5") => 768,
            s if s.contains("bge-large-en-v1.5") => 1024,
            s if s.contains("e5-small-v2") => 384,
            s if s.contains("e5-base-v2") => 768,
            s if s.contains("e5-large-v2") => 1024,
            _ => {
                warn!(
                    "Unknown model dimension for {}, defaulting to 384",
                    model_name
                );
                384
            }
        }
    }

    /// Load model components (lazy loading)
    async fn load_model(&self) -> Result<(), EmbeddingError> {
        // Check if already loaded
        {
            let cache = self.model_cache.read().await;
            if cache.is_some() {
                return Ok(());
            }
        }

        // Load model in blocking task
        let model_path = self.model_path.clone();
        let model_name = self.model_name.clone();

        let components = tokio::task::spawn_blocking(move || {
            Self::load_model_blocking(&model_path, &model_name)
        })
        .await
        .map_err(|e| EmbeddingError::ModelError(format!("Task join error: {}", e)))??;

        // Store in cache
        let mut cache = self.model_cache.write().await;
        *cache = Some(components);

        Ok(())
    }

    /// Load model components (blocking operation)
    fn load_model_blocking(
        model_path: &PathBuf,
        model_name: &str,
    ) -> Result<ModelComponents, EmbeddingError> {
        use candle_transformers::models::bert::{BertModel, Config as BertConfig};

        info!("Loading model from: {:?}", model_path);

        // Load tokenizer
        let tokenizer_path = model_path.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to load tokenizer: {}", e)))?;

        // Load config
        let config_path = model_path.join("config.json");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to read config: {}", e)))?;
        let config: BertConfig = serde_json::from_str(&config_str)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to parse config: {}", e)))?;

        // Determine device (CPU for now, could add CUDA support later)
        let device = Device::Cpu;
        info!("Using device: {:?}", device);

        // Load model weights
        let weights_path = model_path.join("model.safetensors");
        let weights_path_alt = model_path.join("pytorch_model.bin");

        let vb = if weights_path.exists() {
            unsafe {
                VarBuilder::from_mmaped_safetensors(
                    &[weights_path],
                    candle_core::DType::F32,
                    &device,
                )
                .map_err(|e| {
                    EmbeddingError::ModelError(format!("Failed to load safetensors: {}", e))
                })?
            }
        } else if weights_path_alt.exists() {
            VarBuilder::from_pth(&weights_path_alt, candle_core::DType::F32, &device).map_err(
                |e| EmbeddingError::ModelError(format!("Failed to load pytorch weights: {}", e)),
            )?
        } else {
            return Err(EmbeddingError::ModelError(
                "No model weights found (model.safetensors or pytorch_model.bin)".to_string(),
            ));
        };

        let model = BertModel::load(vb, &config)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to load BERT model: {}", e)))?;

        info!("Model loaded successfully: {}", model_name);

        Ok(ModelComponents {
            model,
            tokenizer,
            device,
        })
    }

    /// Perform mean pooling on token embeddings
    fn mean_pooling(
        token_embeddings: &Tensor,
        attention_mask: &Tensor,
    ) -> Result<Tensor, EmbeddingError> {
        // token_embeddings shape: [batch_size, seq_len, hidden_size]
        // attention_mask shape: [batch_size, seq_len]

        // Expand attention mask to match token embeddings
        let attention_mask_expanded = attention_mask
            .unsqueeze(2)
            .map_err(|e| {
                EmbeddingError::ModelError(format!("Failed to expand attention mask: {}", e))
            })?
            .broadcast_as(token_embeddings.shape())
            .map_err(|e| {
                EmbeddingError::ModelError(format!("Failed to broadcast attention mask: {}", e))
            })?;

        // Apply attention mask
        let masked_embeddings = (token_embeddings * &attention_mask_expanded)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to apply mask: {}", e)))?;

        // Sum along sequence dimension
        let sum_embeddings = masked_embeddings
            .sum(1)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to sum embeddings: {}", e)))?;

        // Sum attention mask to get the number of tokens
        let sum_mask = attention_mask
            .sum(1)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to sum mask: {}", e)))?
            .unsqueeze(1)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to unsqueeze mask: {}", e)))?;

        // Compute mean
        let mean_pooled = sum_embeddings
            .broadcast_div(&sum_mask)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to compute mean: {}", e)))?;

        Ok(mean_pooled)
    }

    /// Normalize embeddings
    fn normalize_embeddings(embeddings: &Tensor) -> Result<Tensor, EmbeddingError> {
        // Compute L2 norm
        let norm = embeddings
            .sqr()
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to square: {}", e)))?
            .sum_keepdim(1)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to sum: {}", e)))?
            .sqrt()
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to sqrt: {}", e)))?;

        // Normalize
        let normalized = embeddings
            .broadcast_div(&norm)
            .map_err(|e| EmbeddingError::ModelError(format!("Failed to normalize: {}", e)))?;

        Ok(normalized)
    }

    /// Generate embeddings for texts with the loaded model
    async fn embed_with_model(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        // Ensure model is loaded
        self.load_model().await?;

        let model_cache = self.model_cache.clone();
        let normalize = self.normalize;

        // Run inference in blocking task
        tokio::task::spawn_blocking(move || {
            let cache = futures::executor::block_on(model_cache.read());
            let components = cache
                .as_ref()
                .ok_or_else(|| EmbeddingError::ModelError("Model not loaded".to_string()))?;

            let mut all_embeddings = Vec::new();

            for text in texts {
                // Tokenize
                let encoding = components.tokenizer.encode(text, true).map_err(|e| {
                    EmbeddingError::ModelError(format!("Tokenization failed: {}", e))
                })?;

                let token_ids: Vec<u32> = encoding.get_ids().to_vec();
                let attention_mask: Vec<u32> = encoding.get_attention_mask().to_vec();

                // Convert to tensors
                let token_ids_tensor = Tensor::new(token_ids.as_slice(), &components.device)
                    .map_err(|e| {
                        EmbeddingError::ModelError(format!("Failed to create token tensor: {}", e))
                    })?
                    .unsqueeze(0)
                    .map_err(|e| {
                        EmbeddingError::ModelError(format!("Failed to unsqueeze tokens: {}", e))
                    })?;

                let attention_mask_tensor =
                    Tensor::new(attention_mask.as_slice(), &components.device)
                        .map_err(|e| {
                            EmbeddingError::ModelError(format!(
                                "Failed to create attention mask: {}",
                                e
                            ))
                        })?
                        .unsqueeze(0)
                        .map_err(|e| {
                            EmbeddingError::ModelError(format!("Failed to unsqueeze mask: {}", e))
                        })?;

                let token_type_ids = Tensor::zeros(
                    (1, token_ids.len()),
                    candle_core::DType::U32,
                    &components.device,
                )
                .map_err(|e| {
                    EmbeddingError::ModelError(format!("Failed to create token type ids: {}", e))
                })?;

                // Run model (attention_mask is optional third parameter)
                let token_embeddings = components
                    .model
                    .forward(&token_ids_tensor, &token_type_ids, None)
                    .map_err(|e| {
                        EmbeddingError::ModelError(format!("Model forward failed: {}", e))
                    })?;

                // Mean pooling
                let pooled = Self::mean_pooling(&token_embeddings, &attention_mask_tensor)?;

                // Normalize if configured
                let final_embedding = if normalize {
                    Self::normalize_embeddings(&pooled)?
                } else {
                    pooled
                };

                // Convert to Vec<f32>
                let embedding_vec = final_embedding
                    .squeeze(0)
                    .map_err(|e| EmbeddingError::ModelError(format!("Failed to squeeze: {}", e)))?
                    .to_vec1::<f32>()
                    .map_err(|e| {
                        EmbeddingError::ModelError(format!("Failed to convert to vec: {}", e))
                    })?;

                all_embeddings.push(embedding_vec);
            }

            Ok(all_embeddings)
        })
        .await
        .map_err(|e| EmbeddingError::ModelError(format!("Task join error: {}", e)))?
    }
}

#[cfg(feature = "embeddings")]
#[async_trait::async_trait]
impl EmbeddingProvider for SentenceTransformerEmbeddings {
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        info!(
            "Generating embeddings for {} texts using {}",
            texts.len(),
            self.model_name
        );
        self.embed_with_model(texts).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

/// Factory for creating embedding providers
pub struct EmbeddingFactory;

impl EmbeddingFactory {
    /// Create an embedding provider from environment variables
    pub fn from_env() -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
        let engine = std::env::var("RAG_EMBEDDING_ENGINE")
            .unwrap_or_else(|_| "".to_string())
            .to_lowercase();

        info!(
            "Creating embedding provider: engine={}",
            if engine.is_empty() {
                "sentence-transformers (local)"
            } else {
                &engine
            }
        );

        match engine.as_str() {
            "" => {
                // Empty string means use local sentence transformers (default behavior like Python)
                #[cfg(feature = "embeddings")]
                {
                    let provider = SentenceTransformerEmbeddings::from_env()?;
                    Ok(Arc::new(provider))
                }
                #[cfg(not(feature = "embeddings"))]
                {
                    Err(EmbeddingError::ConfigError(
                        "Local embeddings support not compiled. Enable the 'embeddings' feature or set RAG_EMBEDDING_ENGINE to 'openai' or 'knoxchat'".to_string()
                    ))
                }
            }
            "openai" => {
                let provider = OpenAIEmbeddings::from_env()?;
                Ok(Arc::new(provider))
            }
            "knoxchat" | "knox" => {
                let provider = KnoxChatEmbeddings::from_env()?;
                Ok(Arc::new(provider))
            }
            "local" | "sentence-transformers" => {
                #[cfg(feature = "embeddings")]
                {
                    let provider = SentenceTransformerEmbeddings::from_env()?;
                    Ok(Arc::new(provider))
                }
                #[cfg(not(feature = "embeddings"))]
                {
                    Err(EmbeddingError::ConfigError(
                        "Local embeddings support not compiled. Enable the 'embeddings' feature".to_string()
                    ))
                }
            }
            _ => Err(EmbeddingError::ConfigError(format!(
                "Unsupported embedding engine: {}. Supported: openai, knoxchat, local, sentence-transformers, or '' (empty for local)",
                engine
            ))),
        }
    }

    /// Create an OpenAI embedding provider with custom configuration
    pub fn create_openai(
        api_key: Option<String>,
        model: Option<String>,
    ) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
        let provider = OpenAIEmbeddings::new(api_key, model)?;
        Ok(Arc::new(provider))
    }

    /// Create a Knox Chat embedding provider with custom configuration
    pub fn create_knoxchat(
        api_key: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
        let provider = KnoxChatEmbeddings::new(api_key, base_url, model)?;
        Ok(Arc::new(provider))
    }

    /// Create a sentence transformer embedding provider with custom configuration
    #[cfg(feature = "embeddings")]
    pub fn create_sentence_transformer(
        model_name: String,
        auto_update: bool,
    ) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
        let provider = SentenceTransformerEmbeddings::new(model_name, auto_update)?;
        Ok(Arc::new(provider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires API key
    async fn test_openai_embeddings() {
        let provider = OpenAIEmbeddings::from_env().unwrap();
        let texts = vec!["Hello world".to_string(), "Test embedding".to_string()];

        let embeddings = provider.embed(texts).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), provider.dimension());
    }

    #[test]
    fn test_embedding_dimension() {
        // Test without making API calls
        let provider = OpenAIEmbeddings::new(
            Some("test_key".to_string()),
            Some("text-embedding-3-small".to_string()),
        )
        .unwrap();

        assert_eq!(provider.dimension(), 1536);
        assert_eq!(provider.model_name(), "text-embedding-3-small");
    }
}
