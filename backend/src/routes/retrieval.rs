use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    middleware::{AuthMiddleware, AuthUser},
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
struct RetrievalConfigResponse {
    #[serde(rename = "RAG_TEMPLATE")]
    rag_template: String,
    #[serde(rename = "TOP_K")]
    top_k: usize,
    #[serde(rename = "BYPASS_EMBEDDING_AND_RETRIEVAL")]
    bypass_embedding_and_retrieval: bool,
    #[serde(rename = "RAG_FULL_CONTEXT")]
    rag_full_context: bool,
    #[serde(rename = "ENABLE_RAG_HYBRID_SEARCH")]
    enable_rag_hybrid_search: bool,
    #[serde(rename = "TOP_K_RERANKER")]
    top_k_reranker: i32,
    #[serde(rename = "RELEVANCE_THRESHOLD")]
    relevance_threshold: f64,
    #[serde(rename = "HYBRID_BM25_WEIGHT")]
    hybrid_bm25_weight: f64,
    #[serde(rename = "CONTENT_EXTRACTION_ENGINE")]
    content_extraction_engine: String,
    #[serde(rename = "PDF_EXTRACT_IMAGES")]
    pdf_extract_images: bool,
    #[serde(rename = "CHUNK_SIZE")]
    chunk_size: usize,
    #[serde(rename = "CHUNK_OVERLAP")]
    chunk_overlap: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIConfig {
    url: String,
    key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AzureOpenAIConfig {
    url: String,
    key: String,
    version: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct EmbeddingConfigResponse {
    status: bool,
    embedding_engine: String,
    embedding_model: String,
    embedding_batch_size: i32,
    openai_config: OpenAIConfig,
    azure_openai_config: AzureOpenAIConfig,
}

#[derive(Debug, Deserialize)]
struct ProcessFileForm {
    file_id: String,
}

#[derive(Debug, Deserialize)]
struct ProcessTextForm {
    name: String,
    content: String,
    collection_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProcessYoutubeForm {
    url: String,
    collection_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProcessWebForm {
    url: String,
}

#[derive(Debug, Deserialize)]
struct WebSearchForm {
    query: String,
}

#[derive(Debug, Deserialize)]
struct QueryDocForm {
    collection_name: String,
    query: String,
    k: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct QueryCollectionForm {
    collection_names: Vec<String>,
    query: String,
    k: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DeleteForm {
    collection_name: String,
}

#[derive(Debug, Deserialize)]
struct ProcessFilesBatchForm {
    file_ids: Vec<String>,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("", web::get().to(get_status))
            .route("/", web::get().to(get_status))
            .route("/config", web::get().to(get_rag_config))
            .route("/config/update", web::post().to(update_rag_config))
            .route("/embedding", web::get().to(get_embedding_config))
            .route("/embedding/update", web::post().to(update_embedding_config))
            .route("/process/file", web::post().to(process_file))
            .route("/process/text", web::post().to(process_text))
            .route("/process/youtube", web::post().to(process_youtube))
            .route("/process/web", web::post().to(process_web))
            .route("/process/web/search", web::post().to(process_web_search))
            .route("/process/files/batch", web::post().to(process_files_batch))
            .route("/query/doc", web::post().to(query_doc_handler))
            .route(
                "/query/collection",
                web::post().to(query_collection_handler),
            )
            .route("/delete", web::post().to(delete_entries))
            .route("/reset/db", web::post().to(reset_vector_db))
            .route("/reset/uploads", web::post().to(reset_uploads))
            .route("/ef/{text}", web::get().to(get_embeddings_test)),
    );
}

async fn get_status(_state: web::Data<AppState>, _auth_user: AuthUser) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(json!({
        "status": true
    })))
}

async fn get_rag_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(json!({
        "status": true,
        // RAG settings
        "RAG_TEMPLATE": config.rag_template,
        "TOP_K": config.rag_top_k,
        "BYPASS_EMBEDDING_AND_RETRIEVAL": config.bypass_embedding_and_retrieval,
        "RAG_FULL_CONTEXT": config.rag_full_context,
        // Hybrid search settings
        "ENABLE_RAG_HYBRID_SEARCH": config.enable_rag_hybrid_search,
        "TOP_K_RERANKER": config.top_k_reranker,
        "RELEVANCE_THRESHOLD": config.relevance_threshold,
        "HYBRID_BM25_WEIGHT": config.hybrid_bm25_weight,
        // Content extraction settings
        "CONTENT_EXTRACTION_ENGINE": config.content_extraction_engine,
        "PDF_EXTRACT_IMAGES": config.pdf_extract_images,
        // Chunking settings
        "TEXT_SPLITTER": "RecursiveCharacterTextSplitter",
        "CHUNK_SIZE": config.chunk_size,
        "CHUNK_OVERLAP": config.chunk_overlap,
        // File upload settings
        "FILE_MAX_SIZE": 25,
        "FILE_MAX_COUNT": 10,
        // Reranking settings
        "RAG_RERANKING_MODEL": "",
        "RAG_RERANKING_ENGINE": "",
        // Web search settings - nested object
        "web": {
            "ENABLE_WEB_SEARCH": config.enable_web_search,
            "WEB_SEARCH_ENGINE": "",
            "WEB_SEARCH_RESULT_COUNT": 3,
            "YOUTUBE_LOADER_LANGUAGE": vec!["en"],
            "YOUTUBE_LOADER_PROXY_URL": "",
            "YOUTUBE_LOADER_TRANSLATION": "",
        },
    })))
}

async fn update_rag_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<RetrievalConfigResponse>,
) -> AppResult<HttpResponse> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let mut config = state.config.write().unwrap();

    config.rag_template = form_data.rag_template.clone();
    config.rag_top_k = form_data.top_k;
    config.bypass_embedding_and_retrieval = form_data.bypass_embedding_and_retrieval;
    config.rag_full_context = form_data.rag_full_context;
    config.enable_rag_hybrid_search = form_data.enable_rag_hybrid_search;
    config.top_k_reranker = form_data.top_k_reranker;
    config.relevance_threshold = form_data.relevance_threshold;
    config.hybrid_bm25_weight = form_data.hybrid_bm25_weight;
    config.content_extraction_engine = form_data.content_extraction_engine.clone();
    config.pdf_extract_images = form_data.pdf_extract_images;
    config.chunk_size = form_data.chunk_size;
    config.chunk_overlap = form_data.chunk_overlap;

    // TODO: Persist to database

    Ok(HttpResponse::Ok().json(json!({
        "status": true,
        "RAG_TEMPLATE": config.rag_template,
        "TOP_K": config.rag_top_k,
        "BYPASS_EMBEDDING_AND_RETRIEVAL": config.bypass_embedding_and_retrieval,
        "RAG_FULL_CONTEXT": config.rag_full_context,
        "ENABLE_RAG_HYBRID_SEARCH": config.enable_rag_hybrid_search,
        "TOP_K_RERANKER": config.top_k_reranker,
        "RELEVANCE_THRESHOLD": config.relevance_threshold,
        "HYBRID_BM25_WEIGHT": config.hybrid_bm25_weight,
        "CONTENT_EXTRACTION_ENGINE": config.content_extraction_engine,
        "PDF_EXTRACT_IMAGES": config.pdf_extract_images,
        "CHUNK_SIZE": config.chunk_size,
        "CHUNK_OVERLAP": config.chunk_overlap,
    })))
}

async fn get_embedding_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(EmbeddingConfigResponse {
        status: true,
        embedding_engine: config.rag_embedding_engine.clone(),
        embedding_model: config.rag_embedding_model.clone(),
        embedding_batch_size: 1,
        openai_config: OpenAIConfig {
            url: config.rag_openai_api_base_url.clone(),
            key: config.rag_openai_api_key.clone(),
        },
        azure_openai_config: AzureOpenAIConfig {
            url: String::new(),
            key: String::new(),
            version: String::new(),
        },
    }))
}

async fn update_embedding_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<EmbeddingConfigResponse>,
) -> AppResult<HttpResponse> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let mut config = state.config.write().unwrap();

    config.rag_embedding_engine = form_data.embedding_engine.clone();
    config.rag_embedding_model = form_data.embedding_model.clone();
    config.rag_openai_api_base_url = form_data.openai_config.url.clone();
    config.rag_openai_api_key = form_data.openai_config.key.clone();

    // Persist to database
    let embedding_config_json = serde_json::json!({
        "engine": config.rag_embedding_engine,
        "model": config.rag_embedding_model,
        "batch_size": form_data.embedding_batch_size,
        "openai_url": config.rag_openai_api_base_url,
        "openai_key": config.rag_openai_api_key,
        "azure_url": form_data.azure_openai_config.url,
        "azure_key": form_data.azure_openai_config.key,
        "azure_version": form_data.azure_openai_config.version,
    });

    drop(config);

    let _ = crate::services::ConfigService::update_section(
        &state.db,
        "rag_embedding",
        embedding_config_json,
    )
    .await;

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(EmbeddingConfigResponse {
        status: true,
        embedding_engine: config.rag_embedding_engine.clone(),
        embedding_model: config.rag_embedding_model.clone(),
        embedding_batch_size: form_data.embedding_batch_size,
        openai_config: OpenAIConfig {
            url: config.rag_openai_api_base_url.clone(),
            key: config.rag_openai_api_key.clone(),
        },
        azure_openai_config: AzureOpenAIConfig {
            url: form_data.azure_openai_config.url.clone(),
            key: form_data.azure_openai_config.key.clone(),
            version: form_data.azure_openai_config.version.clone(),
        },
    }))
}

async fn process_file(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<ProcessFileForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement file processing
    // 1. Get file from database
    // 2. Load file content
    // 3. Split into chunks
    // 4. Generate embeddings
    // 5. Store in vector database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "File processing not yet implemented"
    })))
}

async fn process_text(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<ProcessTextForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement text processing
    // 1. Split text into chunks
    // 2. Generate embeddings
    // 3. Store in vector database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "Text processing not yet implemented"
    })))
}

async fn process_youtube(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<ProcessYoutubeForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement YouTube video processing
    // 1. Extract video transcript
    // 2. Split into chunks
    // 3. Generate embeddings
    // 4. Store in vector database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "YouTube processing not yet implemented"
    })))
}

async fn process_web(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<ProcessWebForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement web page processing
    // 1. Fetch web page content
    // 2. Extract text
    // 3. Split into chunks
    // 4. Generate embeddings
    // 5. Store in vector database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "Web processing not yet implemented"
    })))
}

async fn process_web_search(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<WebSearchForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement web search
    // 1. Use configured search engine (Brave, DuckDuckGo, etc.)
    // 2. Return search results
    Ok(HttpResponse::Ok().json(json!({
        "results": [],
        "error": "Web search not yet implemented"
    })))
}

async fn process_files_batch(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<ProcessFilesBatchForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement batch file processing
    // 1. Process multiple files in parallel
    // 2. Generate embeddings for all
    // 3. Store in vector database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "Batch file processing not yet implemented"
    })))
}

async fn query_doc_handler(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<QueryDocForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement document query
    // 1. Generate query embedding
    // 2. Search vector database
    // 3. Return relevant chunks
    Ok(HttpResponse::Ok().json(json!({
        "results": [],
        "error": "Document query not yet implemented"
    })))
}

async fn query_collection_handler(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<QueryCollectionForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement collection query
    // 1. Generate query embedding
    // 2. Search across multiple collections
    // 3. Apply reranking if configured
    // 4. Return relevant chunks
    Ok(HttpResponse::Ok().json(json!({
        "results": [],
        "error": "Collection query not yet implemented"
    })))
}

async fn delete_entries(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
    _form_data: web::Json<DeleteForm>,
) -> AppResult<HttpResponse> {
    // Only admins can delete entries
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // TODO: Implement delete entries from collection
    // 1. Delete from vector database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "Delete entries not yet implemented"
    })))
}

async fn reset_vector_db(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can reset vector database
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // TODO: Implement vector database reset
    // 1. Clear all collections
    // 2. Reinitialize database
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "Vector DB reset not yet implemented"
    })))
}

async fn reset_uploads(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can reset uploads
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // TODO: Implement uploads directory reset
    // 1. Clear uploads directory
    Ok(HttpResponse::Ok().json(json!({
        "status": false,
        "error": "Reset uploads not yet implemented"
    })))
}

async fn get_embeddings_test(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    text: web::Path<String>,
) -> AppResult<HttpResponse> {
    // TODO: Implement embedding generation test
    // 1. Generate embedding for test text
    // 2. Return embedding vector
    Ok(HttpResponse::Ok().json(json!({
        "text": text.into_inner(),
        "embedding": [],
        "error": "Embedding generation not yet implemented"
    })))
}
