use actix_web::{web, HttpResponse};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    middleware::{AuthMiddleware, AuthUser},
    utils::chat_completion::{self, StreamingContext},
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIConfigResponse {
    #[serde(rename = "ENABLE_OPENAI_API")]
    enable_openai_api: bool,
    #[serde(rename = "OPENAI_API_BASE_URLS")]
    openai_api_base_urls: Vec<String>,
    #[serde(rename = "OPENAI_API_KEYS")]
    openai_api_keys: Vec<String>,
    #[serde(rename = "OPENAI_API_CONFIGS")]
    openai_api_configs: serde_json::Value,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("/config", web::get().to(get_config))
            .route("/config/update", web::post().to(update_config))
            .route("/models", web::get().to(get_models))
            .route("/models/{url_idx}", web::get().to(get_models_by_idx))
            .route("/verify", web::post().to(verify_connection))
            .route("/audio/speech", web::post().to(audio_speech))
            .route("/embeddings", web::post().to(embeddings_endpoint))
            .route("/chat/completions", web::post().to(chat_completions))
            .route("/{path:.*}", web::to(proxy_request)),
    );
}

async fn get_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(OpenAIConfigResponse {
        enable_openai_api: config.enable_openai_api,
        openai_api_base_urls: config.openai_api_base_urls.clone(),
        openai_api_keys: config.openai_api_keys.clone(),
        openai_api_configs: config.openai_api_configs.clone(),
    }))
}

async fn update_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<OpenAIConfigResponse>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();

        config.enable_openai_api = form_data.enable_openai_api;
        config.openai_api_base_urls = form_data.openai_api_base_urls.clone();
        config.openai_api_keys = form_data.openai_api_keys.clone();

        // Ensure keys and URLs have the same length
        let urls_len = config.openai_api_base_urls.len();
        let keys_len = config.openai_api_keys.len();

        if keys_len != urls_len {
            if keys_len > urls_len {
                config.openai_api_keys.truncate(urls_len);
            } else {
                // Pad with empty strings
                let missing = urls_len - keys_len;
                config.openai_api_keys.extend(vec!["".to_string(); missing]);
            }
        }

        config.openai_api_configs = form_data.openai_api_configs.clone();
    }

    // Persist to database (best-effort, like Python)
    let config = state.config.read().unwrap();
    let openai_json = serde_json::json!({
        "enable": config.enable_openai_api,
        "api_base_urls": config.openai_api_base_urls,
        "api_keys": config.openai_api_keys,
        "api_configs": config.openai_api_configs
    });

    let _ = crate::services::ConfigService::update_section(&state.db, "openai", openai_json).await;

    Ok(HttpResponse::Ok().json(OpenAIConfigResponse {
        enable_openai_api: config.enable_openai_api,
        openai_api_base_urls: config.openai_api_base_urls.clone(),
        openai_api_keys: config.openai_api_keys.clone(),
        openai_api_configs: config.openai_api_configs.clone(),
    }))
}

// Get all OpenAI models from all configured endpoints
async fn get_models(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_openai_api {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "data": []
        })));
    }

    let mut all_models = Vec::new();
    let client = reqwest::Client::new();

    // Fetch models from each configured OpenAI endpoint
    for (idx, url) in config.openai_api_base_urls.iter().enumerate() {
        if let Some(key) = config.openai_api_keys.get(idx) {
            // Get API config for this endpoint
            let api_config = config
                .openai_api_configs
                .get(idx.to_string())
                .or_else(|| config.openai_api_configs.get(url))
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();

            // Check if this endpoint is enabled
            let enabled = api_config
                .get("enable")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if !enabled {
                continue;
            }

            // Check if it's Azure
            let is_azure = api_config
                .get("azure")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if is_azure {
                // For Azure, use model_ids from config
                if let Some(model_ids) = api_config.get("model_ids").and_then(|v| v.as_array()) {
                    for model_id in model_ids {
                        if let Some(id_str) = model_id.as_str() {
                            all_models.push(serde_json::json!({
                                "id": id_str,
                                "name": id_str,
                                "object": "model",
                                "owned_by": "azure",
                                "urlIdx": idx,
                                "connection_type": "external"
                            }));
                        }
                    }
                }
            } else {
                // Fetch models from the endpoint
                match client
                    .get(format!("{}/models", url))
                    .header("Authorization", format!("Bearer {}", key))
                    .header("Content-Type", "application/json")
                    .send()
                    .await
                {
                    Ok(response) if response.status().is_success() => {
                        if let Ok(models_response) = response.json::<serde_json::Value>().await {
                            if let Some(data) =
                                models_response.get("data").and_then(|v| v.as_array())
                            {
                                for model in data {
                                    if let Some(model_id) = model.get("id").and_then(|v| v.as_str())
                                    {
                                        // Filter out certain OpenAI models
                                        if url.contains("api.openai.com") {
                                            let should_skip = [
                                                "babbage",
                                                "dall-e",
                                                "davinci",
                                                "embedding",
                                                "tts",
                                                "whisper",
                                            ]
                                            .iter()
                                            .any(|name| model_id.contains(name));

                                            if should_skip {
                                                continue;
                                            }
                                        }

                                        all_models.push(serde_json::json!({
                                            "id": model_id,
                                            "name": model.get("name").and_then(|v| v.as_str()).unwrap_or(model_id),
                                            "object": "model",
                                            "owned_by": model.get("owned_by").and_then(|v| v.as_str()).unwrap_or("openai"),
                                            "openai": model,
                                            "connection_type": "external",
                                            "urlIdx": idx
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    Ok(response) => {
                        eprintln!(
                            "Failed to fetch models from {}: status {}",
                            url,
                            response.status()
                        );
                    }
                    Err(e) => {
                        eprintln!("Error fetching models from {}: {}", url, e);
                    }
                }
            }
        }
    }

    // Cache the models in app state (like Python's OPENAI_MODELS)
    {
        let mut cache = state.models_cache.write().unwrap();
        cache.clear();
        for model in &all_models {
            if let Some(model_id) = model.get("id").and_then(|v| v.as_str()) {
                cache.insert(model_id.to_string(), model.clone());
            }
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": all_models
    })))
}

// Get models from a specific OpenAI endpoint by index
async fn get_models_by_idx(
    state: web::Data<AppState>,
    url_idx: web::Path<usize>,
    _auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let idx = url_idx.into_inner();
    let config = state.config.read().unwrap();

    if idx >= config.openai_api_base_urls.len() {
        return Err(AppError::NotFound("OpenAI endpoint not found".to_string()));
    }

    let url = &config.openai_api_base_urls[idx];
    let key = config
        .openai_api_keys
        .get(idx)
        .map(|s| s.as_str())
        .unwrap_or("");

    // Get API config for this endpoint
    let api_config = config
        .openai_api_configs
        .get(idx.to_string())
        .or_else(|| config.openai_api_configs.get(url))
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // Check if it's Azure
    let is_azure = api_config
        .get("azure")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_azure {
        // For Azure, return model_ids from config
        let model_ids = api_config
            .get("model_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "data": model_ids,
            "object": "list"
        })));
    }

    // Fetch models from the endpoint
    let client = reqwest::Client::new();

    match client
        .get(format!("{}/models", url))
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            if let Ok(mut models_response) = response.json::<serde_json::Value>().await {
                // Filter OpenAI API models
                if url.contains("api.openai.com") {
                    if let Some(data) = models_response
                        .get_mut("data")
                        .and_then(|v| v.as_array_mut())
                    {
                        data.retain(|model| {
                            if let Some(model_id) = model.get("id").and_then(|v| v.as_str()) {
                                ![
                                    "babbage",
                                    "dall-e",
                                    "davinci",
                                    "embedding",
                                    "tts",
                                    "whisper",
                                ]
                                .iter()
                                .any(|name| model_id.contains(name))
                            } else {
                                false
                            }
                        });
                    }
                }

                // Check if it's a pipeline
                if models_response.get("pipelines").is_some() {
                    models_response["pipelines"] = serde_json::json!(true);
                }

                Ok(HttpResponse::Ok().json(models_response))
            } else {
                Err(AppError::InternalServerError(
                    "Failed to parse models response".to_string(),
                ))
            }
        }
        Ok(response) => {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            eprintln!(
                "Failed to fetch models from {}: {} - {}",
                url, status, error_text
            );

            // Return empty list instead of error
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "data": [],
                "object": "list",
                "error": format!("Failed to fetch models: {}", status)
            })))
        }
        Err(e) => {
            eprintln!("Error fetching models from {}: {}", url, e);

            // Return empty list instead of error
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "data": [],
                "object": "list",
                "error": format!("Error: {}", e)
            })))
        }
    }
}

/// Public helper to get first enabled OpenAI endpoint for tasks like title generation
pub fn get_openai_endpoint(
    config: &crate::config::Config,
    _model_id: &str,
) -> Result<(String, String, serde_json::Value), AppError> {
    // Find first enabled endpoint
    for (idx, endpoint_url) in config.openai_api_base_urls.iter().enumerate() {
        let api_cfg = config
            .openai_api_configs
            .get(idx.to_string())
            .or_else(|| config.openai_api_configs.get(endpoint_url))
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let enabled = api_cfg
            .get("enable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if enabled {
            let endpoint_key = config
                .openai_api_keys
                .get(idx)
                .map(|s| s.to_string())
                .unwrap_or_default();
            return Ok((
                endpoint_url.clone(),
                endpoint_key,
                serde_json::json!(api_cfg),
            ));
        }
    }

    Err(AppError::NotFound(
        "No enabled OpenAI endpoint configured".to_string(),
    ))
}

/// Helper function to get endpoint from cache or config (non-direct routing)
fn get_endpoint_from_cache_or_config(
    state: &web::Data<AppState>,
    config: &crate::config::Config,
    model_id: &str,
    model_item: &serde_json::Value,
    payload_obj: &serde_json::Value,
) -> Result<(String, String, serde_json::Value), AppError> {
    // Try to get urlIdx from model_item, payload, or cache
    let url_idx = model_item
        .get("urlIdx")
        .and_then(|v| v.as_u64())
        .or_else(|| payload_obj.get("urlIdx").and_then(|v| v.as_u64()))
        .or_else(|| {
            // Try to find model in cache and get its urlIdx
            let cache = state.models_cache.read().unwrap();
            cache
                .get(model_id)
                .and_then(|model| model.get("urlIdx"))
                .and_then(|v| v.as_u64())
        })
        .map(|i| i as usize);

    let selected_idx = if let Some(idx) = url_idx {
        // Use the provided urlIdx
        if idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(format!(
                "OpenAI endpoint index {} not found",
                idx
            )));
        }
        idx
    } else {
        // No urlIdx found, try to find the first enabled endpoint
        let mut found_idx = None;
        for (idx, endpoint_url) in config.openai_api_base_urls.iter().enumerate() {
            let api_cfg = config
                .openai_api_configs
                .get(idx.to_string())
                .or_else(|| config.openai_api_configs.get(endpoint_url))
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();

            let enabled = api_cfg
                .get("enable")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if enabled {
                found_idx = Some(idx);
                break;
            }
        }

        found_idx.ok_or_else(|| {
            AppError::NotFound("No enabled OpenAI endpoint configured".to_string())
        })?
    };

    let endpoint_url = config.openai_api_base_urls[selected_idx].clone();
    let endpoint_key = config
        .openai_api_keys
        .get(selected_idx)
        .map(|s| s.to_string())
        .unwrap_or_default();

    let api_cfg = config
        .openai_api_configs
        .get(selected_idx.to_string())
        .or_else(|| config.openai_api_configs.get(&endpoint_url))
        .cloned()
        .unwrap_or(serde_json::json!({}));

    tracing::info!(
        "Using endpoint {} (idx: {}) for model {}",
        endpoint_url,
        selected_idx,
        model_id
    );
    Ok((endpoint_url, endpoint_key, api_cfg))
}

// Verify connection endpoint - test OpenAI API connection
#[derive(Debug, Deserialize)]
struct VerifyConnectionRequest {
    url: String,
    key: String,
    #[serde(default)]
    config: serde_json::Value,
}

async fn verify_connection(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    payload: web::Json<VerifyConnectionRequest>,
) -> Result<HttpResponse, AppError> {
    let url = &payload.url;
    let key = &payload.key;
    let api_config = if payload.config.is_null() {
        serde_json::json!({})
    } else {
        payload.config.clone()
    };

    let client = reqwest::Client::new();

    // Check if it's Azure
    let is_azure = api_config
        .get("azure")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_azure {
        let api_version = api_config
            .get("api_version")
            .and_then(|v| v.as_str())
            .unwrap_or("2023-03-15-preview");

        let auth_type = api_config
            .get("auth_type")
            .and_then(|v| v.as_str())
            .unwrap_or("bearer");

        let mut request_builder =
            client.get(format!("{}/openai/models?api-version={}", url, api_version));

        // Only set api-key header if not using Azure Entra ID
        if auth_type != "azure_ad" && auth_type != "microsoft_entra_id" {
            request_builder = request_builder.header("api-key", key);
        }

        match request_builder.send().await {
            Ok(response) => {
                let status_code = response.status().as_u16();
                if let Ok(json_body) = response.json::<serde_json::Value>().await {
                    Ok(HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(status_code)
                            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
                    )
                    .json(json_body))
                } else {
                    Ok(HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(status_code)
                            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
                    )
                    .finish())
                }
            }
            Err(e) => Err(AppError::InternalServerError(format!(
                "Connection error: {}",
                e
            ))),
        }
    } else {
        // Regular OpenAI endpoint
        match client
            .get(format!("{}/models", url))
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await
        {
            Ok(response) => {
                let status_code = response.status().as_u16();
                if let Ok(json_body) = response.json::<serde_json::Value>().await {
                    Ok(HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(status_code)
                            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
                    )
                    .json(json_body))
                } else {
                    Ok(HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(status_code)
                            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
                    )
                    .finish())
                }
            }
            Err(e) => Err(AppError::InternalServerError(format!(
                "Connection error: {}",
                e
            ))),
        }
    }
}

// Audio speech endpoint - TTS via OpenAI
async fn audio_speech(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    req: actix_web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    // Try to find OpenAI endpoint
    let config = state.config.read().unwrap();

    let openai_idx = config
        .openai_api_base_urls
        .iter()
        .position(|url| url.contains("api.openai.com"));

    let idx = openai_idx.unwrap_or(0);

    if idx >= config.openai_api_base_urls.len() {
        return Err(AppError::NotFound(
            "OpenAI endpoint not configured".to_string(),
        ));
    }

    let url = config.openai_api_base_urls[idx].clone();
    let key = config
        .openai_api_keys
        .get(idx)
        .map(|s| s.to_string())
        .unwrap_or_default();

    let api_config = config
        .openai_api_configs
        .get(idx.to_string())
        .or_else(|| config.openai_api_configs.get(&url))
        .cloned()
        .unwrap_or(serde_json::json!({}));

    // Get cache directory from config
    let cache_base_dir = config.cache_dir.clone();
    drop(config); // Release lock

    // Calculate hash for caching
    let hash = format!("{:x}", md5::compute(&body));

    // Check cache directory (use config cache_dir + audio/speech subdirectory)
    let cache_dir = std::path::PathBuf::from(&cache_base_dir)
        .join("audio")
        .join("speech");
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        tracing::warn!("Failed to create cache directory: {}", e);
    }

    let file_path = cache_dir.join(format!("{}.mp3", hash));

    // Check if cached
    if file_path.exists() {
        return Ok(HttpResponse::Ok().content_type("audio/mpeg").body(
            std::fs::read(&file_path).map_err(|e| {
                AppError::InternalServerError(format!("Failed to read cached file: {}", e))
            })?,
        ));
    }

    // Make request to OpenAI
    let client = reqwest::Client::new();
    let mut request_builder = client
        .post(format!("{}/audio/speech", url))
        .header("Content-Type", "application/json")
        .body(body.to_vec());

    let auth_type = api_config
        .get("auth_type")
        .and_then(|v| v.as_str())
        .unwrap_or("bearer");

    if auth_type != "none" && !key.is_empty() {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", key));
    }

    match request_builder.send().await {
        Ok(response) if response.status().is_success() => {
            let audio_bytes = response.bytes().await.map_err(|e| {
                AppError::InternalServerError(format!("Failed to read audio response: {}", e))
            })?;

            // Cache the result
            if let Err(e) = std::fs::write(&file_path, &audio_bytes) {
                tracing::warn!("Failed to cache audio file: {}", e);
            }

            Ok(HttpResponse::Ok()
                .content_type("audio/mpeg")
                .body(audio_bytes.to_vec()))
        }
        Ok(response) => {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            Err(AppError::InternalServerError(format!(
                "OpenAI API error: {} - {}",
                status, error_text
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Request error: {}",
            e
        ))),
    }
}

// Embeddings endpoint
async fn embeddings_endpoint(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_openai_api {
        return Err(AppError::NotImplemented(
            "OpenAI API is not enabled".to_string(),
        ));
    }

    let model_id = payload.get("model").and_then(|v| v.as_str()).unwrap_or("");

    // Find the endpoint for this model
    let mut idx = 0;

    // Try to find in cache first
    {
        let cache = state.models_cache.read().unwrap();
        if let Some(model) = cache.get(model_id) {
            if let Some(url_idx) = model.get("urlIdx").and_then(|v| v.as_u64()) {
                idx = url_idx as usize;
            }
        }
    }

    if idx >= config.openai_api_base_urls.len() {
        idx = 0; // Fallback to first endpoint
    }

    let url = config.openai_api_base_urls[idx].clone();
    let key = config
        .openai_api_keys
        .get(idx)
        .map(|s| s.to_string())
        .unwrap_or_default();

    let api_config = config
        .openai_api_configs
        .get(idx.to_string())
        .or_else(|| config.openai_api_configs.get(&url))
        .cloned()
        .unwrap_or(serde_json::json!({}));

    drop(config);

    // Make request
    let client = reqwest::Client::new();
    let mut request_builder = client
        .post(format!("{}/embeddings", url))
        .header("Content-Type", "application/json")
        .json(&payload.into_inner());

    let auth_type = api_config
        .get("auth_type")
        .and_then(|v| v.as_str())
        .unwrap_or("bearer");

    if auth_type != "none" && !key.is_empty() {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", key));
    }

    match request_builder.send().await {
        Ok(response) if response.status().is_success() => {
            let json_response = response.json::<serde_json::Value>().await.map_err(|e| {
                AppError::InternalServerError(format!("Failed to parse response: {}", e))
            })?;
            Ok(HttpResponse::Ok().json(json_response))
        }
        Ok(response) => {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            Err(AppError::InternalServerError(format!(
                "OpenAI API error: {} - {}",
                status, error_text
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Request error: {}",
            e
        ))),
    }
}

// Proxy endpoint - catch-all for other OpenAI API paths
async fn proxy_request(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    req: actix_web::HttpRequest,
    path: web::Path<String>,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    let idx = 0; // Default to first endpoint

    if idx >= config.openai_api_base_urls.len() {
        return Err(AppError::NotFound(
            "OpenAI endpoint not configured".to_string(),
        ));
    }

    let url = config.openai_api_base_urls[idx].clone();
    let key = config
        .openai_api_keys
        .get(idx)
        .map(|s| s.to_string())
        .unwrap_or_default();

    let api_config = config
        .openai_api_configs
        .get(idx.to_string())
        .or_else(|| config.openai_api_configs.get(&url))
        .cloned()
        .unwrap_or(serde_json::json!({}));

    drop(config);

    let request_path = path.into_inner();
    let method = req.method().clone();

    // Check if Azure
    let is_azure = api_config
        .get("azure")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let client = reqwest::Client::new();
    let request_url = if is_azure {
        let api_version = api_config
            .get("api_version")
            .and_then(|v| v.as_str())
            .unwrap_or("2023-03-15-preview");

        // Parse JSON body to get model for Azure
        let mut body_json = serde_json::from_slice::<serde_json::Value>(&body).ok();
        let mut modified_url = url.clone();
        let mut modified_body = body.to_vec();

        if let Some(ref mut payload) = body_json {
            if let Some(model) = payload.get("model").and_then(|m| m.as_str()) {
                modified_url = format!("{}/openai/deployments/{}", url, model);

                // Filter Azure-allowed parameters
                // This is simplified; Python has more complex filtering
                modified_body = serde_json::to_vec(&payload).unwrap_or(body.to_vec());
            }
        }

        format!(
            "{}/{}?api-version={}",
            modified_url, request_path, api_version
        )
    } else {
        format!("{}/{}", url, request_path)
    };

    let mut request_builder = match method.as_str() {
        "GET" => client.get(&request_url),
        "POST" => client.post(&request_url),
        "PUT" => client.put(&request_url),
        "DELETE" => client.delete(&request_url),
        "PATCH" => client.patch(&request_url),
        _ => client.get(&request_url),
    };

    request_builder = request_builder
        .header("Content-Type", "application/json")
        .body(body.to_vec());

    let auth_type = api_config
        .get("auth_type")
        .and_then(|v| v.as_str())
        .unwrap_or("bearer");

    if is_azure {
        if auth_type != "azure_ad" && auth_type != "microsoft_entra_id" {
            request_builder = request_builder.header("api-key", key);
        }
    } else if auth_type != "none" && !key.is_empty() {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", key));
    }

    match request_builder.send().await {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let actix_status = actix_web::http::StatusCode::from_u16(status_code)
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/json")
                .to_string();

            // Check if streaming
            if content_type.contains("text/event-stream") {
                use bytes::Bytes;
                let stream = response.bytes_stream().map(move |result| match result {
                    Ok(bytes) => Ok::<Bytes, actix_web::Error>(bytes),
                    Err(e) => Err(actix_web::error::ErrorInternalServerError(e)),
                });

                Ok(HttpResponse::build(actix_status)
                    .content_type(content_type)
                    .streaming(stream))
            } else {
                // Regular response
                if let Ok(json_body) = response.json::<serde_json::Value>().await {
                    Ok(HttpResponse::build(actix_status).json(json_body))
                } else {
                    Ok(HttpResponse::build(actix_status).finish())
                }
            }
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Proxy request error: {}",
            e
        ))),
    }
}

// Chat completions endpoint - proxy to the appropriate OpenAI endpoint
async fn chat_completions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    handle_chat_completions(state, auth_user, payload).await
}

/// Process streaming response and emit events via Socket.IO (wrapper function)
/// Delegates to chat_completion module for actual implementation
async fn process_streaming_via_socketio(
    response: reqwest::Response,
    state: &web::Data<AppState>,
    user_id: &str,
    model_id: String,
    messages: Vec<serde_json::Value>,
    chat_id: Option<String>,
    message_id: Option<String>,
    session_id: Option<String>,
    should_generate_title: bool,
    model_item: serde_json::Value,
    endpoint_url: String,
    endpoint_key: String,
    tool_ids: Vec<String>,
    tool_specs: Vec<serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create streaming context
    let context = StreamingContext {
        state: state.clone(),
        user_id: user_id.to_string(),
        model_id,
        messages,
        chat_id,
        message_id,
        session_id,
        should_generate_title,
        model_item,
        endpoint_url,
        endpoint_key,
        tool_ids,
        tool_specs,
        delta_chunk_size: None, // TODO: Extract from request params when frontend supports it
    };

    // Delegate to chat_completion module
    chat_completion::process_streaming_via_socketio(response, context).await
}

// ============================================================================
// The following functions have been moved to utils/chat_completion.rs:
// - Socket.IO streaming logic (process_streaming_via_socketio implementation)
// - upsert_chat_message
// - generate_and_update_title
// - DEFAULT_TITLE_GENERATION_PROMPT_TEMPLATE
// - Tool execution and multi-turn conversation logic
// ============================================================================

// Public handler for chat completions that can be called from main.rs
pub async fn handle_chat_completions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    // Check if OpenAI API is enabled
    let enable_openai_api = {
        let config = state.config.read().unwrap();
        config.enable_openai_api
    };

    if !enable_openai_api {
        return Err(AppError::NotImplemented(
            "OpenAI API is not enabled".to_string(),
        ));
    }

    // Get model ID from payload
    let model_id = payload
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Model ID is required".to_string()))?
        .to_string();

    // Extract model_item from payload (matching Python's behavior exactly)
    let mut payload_obj = payload.into_inner();
    let model_item = payload_obj
        .as_object_mut()
        .and_then(|obj| obj.remove("model_item"))
        .unwrap_or(serde_json::json!({}));

    // Extract Socket.IO streaming metadata from top-level (frontend sends these at root level)
    let session_id = payload_obj
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let chat_id = payload_obj
        .get("chat_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let message_id = payload_obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract background_tasks for title generation, etc.
    let background_tasks = payload_obj
        .as_object_mut()
        .and_then(|obj| obj.remove("background_tasks"))
        .unwrap_or(serde_json::json!({}));

    let should_generate_title = background_tasks
        .get("title_generation")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Extract messages for title generation before removing from payload
    let messages = payload_obj
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Extract tool_ids BEFORE removing from payload
    let tool_ids = payload_obj
        .get("tool_ids")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    tracing::info!(
        "📋 Chat completion request received - tool_ids: {:?}",
        tool_ids
    );
    if !tool_ids.is_empty() {
        tracing::info!("🔧 Tools requested in chat: {}", tool_ids.join(", "));
    }

    // Remove these from payload before forwarding to LLM API
    if let Some(obj) = payload_obj.as_object_mut() {
        obj.remove("session_id");
        obj.remove("chat_id");
        obj.remove("id");
        // Remove other frontend-only fields
        obj.remove("filter_ids");
        obj.remove("tool_ids");
        obj.remove("tool_servers");
        obj.remove("features");
        obj.remove("variables");
        obj.remove("model_item");
    }

    // Prepare tool specs storage (moved outside if block for later use)
    let mut all_tool_specs = Vec::new();

    // Load and inject tools if tool_ids are provided
    if !tool_ids.is_empty() {
        use crate::models::tool_runtime::ToolDefinition;
        use crate::services::tool::ToolService;

        tracing::info!(
            "🔄 Loading {} tool(s) for chat completion: {:?}",
            tool_ids.len(),
            tool_ids
        );

        let tool_service = ToolService::new(&state.db);

        for tool_id in &tool_ids {
            match tool_service.get_tool_by_id(tool_id).await {
                Ok(Some(tool)) => {
                    // Check access permissions
                    if tool.user_id != auth_user.user.id && auth_user.user.role != "admin" {
                        use crate::services::group::GroupService;
                        use crate::utils::misc::has_access;
                        use std::collections::HashSet;

                        let group_service = GroupService::new(&state.db);
                        let groups = group_service
                            .get_groups_by_member_id(&auth_user.user.id)
                            .await
                            .unwrap_or_default();
                        let user_group_ids: HashSet<String> =
                            groups.into_iter().map(|g| g.id).collect();

                        if !has_access(
                            &auth_user.user.id,
                            "read",
                            &tool.get_access_control(),
                            &user_group_ids,
                        ) {
                            tracing::warn!(
                                "User {} does not have access to tool {}",
                                auth_user.user.id,
                                tool_id
                            );
                            continue;
                        }
                    }

                    // Parse tool definition and extract OpenAI specs
                    match ToolDefinition::from_json(&tool.content) {
                        Ok(tool_def) => {
                            let specs = tool_def.to_openai_specs();
                            tracing::info!(
                                "Loaded {} tool spec(s) from tool {}",
                                specs.len(),
                                tool_id
                            );
                            all_tool_specs.extend(specs);
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to parse tool definition for {}: {}",
                                tool_id,
                                e
                            );
                        }
                    }
                }
                Ok(None) => {
                    tracing::warn!("Tool {} not found", tool_id);
                }
                Err(e) => {
                    tracing::error!("Failed to load tool {}: {}", tool_id, e);
                }
            }
        }

        // Inject tools into payload in OpenAI format
        if !all_tool_specs.is_empty() {
            tracing::info!(
                "✅ Injecting {} tool spec(s) into chat completion request",
                all_tool_specs.len()
            );

            let tools_array: Vec<serde_json::Value> = all_tool_specs
                .iter()
                .map(|spec| {
                    tracing::debug!(
                        "Tool spec: {}",
                        serde_json::to_string_pretty(spec).unwrap_or_default()
                    );
                    serde_json::json!({
                        "type": "function",
                        "function": spec
                    })
                })
                .collect();

            if let Some(obj) = payload_obj.as_object_mut() {
                obj.insert("tools".to_string(), serde_json::json!(tools_array));
                // Set tool_choice to "auto" to let the LLM decide when to use tools
                if !obj.contains_key("tool_choice") {
                    obj.insert("tool_choice".to_string(), serde_json::json!("auto"));
                }
                tracing::info!("🎯 Tool choice set to: auto");
            }
        } else {
            tracing::warn!(
                "⚠️  No tool specs were loaded even though tool_ids were provided: {:?}",
                tool_ids
            );
        }
    } else {
        tracing::debug!("ℹ️  No tools requested for this chat completion");
    }

    // ============================================================================
    // PROCESS FILES/NOTES AS CONTEXT (RAG)
    // ============================================================================
    // Extract files from root level (frontend sends it there, not in metadata)
    // Then extract/create metadata object
    let files_from_root = payload_obj
        .as_object_mut()
        .and_then(|obj| obj.remove("files"));

    let mut metadata = payload_obj
        .as_object_mut()
        .and_then(|obj| obj.remove("metadata"))
        .unwrap_or(serde_json::json!({}));

    // Add files to metadata if they exist (matching Python backend behavior)
    if let Some(files) = files_from_root {
        if let Some(metadata_obj) = metadata.as_object_mut() {
            metadata_obj.insert("files".to_string(), files);
        }
    }

    // Process files/notes/chats as RAG context if present
    let mut sources = Vec::new();
    if let Some(files_array) = metadata.get("files").and_then(|f| f.as_array()) {
        if !files_array.is_empty() {
            tracing::info!(
                "📎 Processing {} file attachment(s) for RAG context",
                files_array.len()
            );

            // Parse file items
            let mut file_items: Vec<crate::utils::retrieval::FileItem> = files_array
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect();

            // Deduplicate files (same as Python backend)
            // Use a HashSet to track seen items based on JSON representation
            let mut seen = std::collections::HashSet::new();
            file_items.retain(|item| {
                let key = serde_json::to_string(item).unwrap_or_default();
                seen.insert(key)
            });

            if file_items.len() < files_array.len() {
                tracing::debug!(
                    "📋 Removed {} duplicate file(s), processing {} unique file(s)",
                    files_array.len() - file_items.len(),
                    file_items.len()
                );
            }

            if !file_items.is_empty() {
                // Get user groups for access control
                use crate::services::group::GroupService;
                use std::collections::HashSet;

                let group_service = GroupService::new(&state.db);
                let groups = group_service
                    .get_groups_by_member_id(&auth_user.user.id)
                    .await
                    .unwrap_or_default();
                let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

                // Extract sources from file items (notes, files, chats, etc.)
                match crate::utils::retrieval::get_sources_from_items(
                    &state,
                    file_items.clone(),
                    &auth_user.user,
                    &user_group_ids,
                )
                .await
                {
                    Ok(extracted_sources) => {
                        // Count unique source IDs (matching Python's sources_count logic)
                        let unique_ids: std::collections::HashSet<String> = extracted_sources
                            .iter()
                            .filter_map(|s| {
                                s.source
                                    .get("id")
                                    .and_then(|id| id.as_str())
                                    .map(String::from)
                            })
                            .collect();

                        sources = extracted_sources;
                        tracing::info!(
                            "✅ Successfully extracted {} source(s) from {} unique document(s)",
                            sources.len(),
                            unique_ids.len()
                        );

                        // Inject sources into messages if we have any
                        if !sources.is_empty() {
                            // Get RAG template from config
                            let rag_template = {
                                let config = state.config.read().unwrap();
                                config.rag_template.clone()
                            };

                            // Get mutable reference to messages array
                            if let Some(messages_value) = payload_obj.get_mut("messages") {
                                if let Some(messages_array) = messages_value.as_array_mut() {
                                    match crate::utils::retrieval::inject_sources_into_messages(
                                        sources.clone(),
                                        messages_array,
                                        &rag_template,
                                    ) {
                                        Ok(_) => {
                                            tracing::info!(
                                                "✅ Successfully injected RAG context into user message"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "❌ Failed to inject RAG context: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to extract sources from file items: {}", e);
                    }
                }
            }
        }
    } else {
        tracing::debug!("ℹ️  No file attachments in this chat completion request");
    }

    tracing::debug!(
        "Chat completion request - model_id: {}, model_item: {}",
        model_id,
        serde_json::to_string(&model_item).unwrap_or_default()
    );

    // Check if this is a direct connection request (Python: if not model_item.get("direct", False))
    let is_direct = model_item
        .get("direct")
        .and_then(|d| d.as_bool())
        .unwrap_or(false);

    tracing::debug!(
        "is_direct: {}, session_id: {:?}, model_id: {}",
        is_direct,
        session_id,
        model_id
    );

    // If direct connections not enabled, just use regular OpenAI routing
    let config = state.config.read().unwrap();
    let (url, key, api_config) = if is_direct && config.enable_direct_connections {
        // Direct connection - look up URL and key from user settings using urlIdx
        // The frontend sends urlIdx which points to the user's directConnections settings

        // First, try explicit url/key in model_item (rare case)
        let direct_url = model_item
            .get("url")
            .and_then(|u| u.as_str())
            .filter(|s| !s.is_empty());

        let direct_key = model_item.get("key").and_then(|k| k.as_str()).unwrap_or("");

        if let Some(url_str) = direct_url {
            tracing::info!(
                "Using direct connection with explicit URL: {} for model {} by user {}",
                url_str,
                model_id,
                auth_user.user.email
            );

            let item_config = model_item
                .get("config")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            (url_str.to_string(), direct_key.to_string(), item_config)
        } else {
            // No explicit URL - look up from user settings using urlIdx
            let url_idx = if let Some(idx_str) = model_item.get("urlIdx").and_then(|v| v.as_str()) {
                idx_str.to_string()
            } else if let Some(idx_num) = model_item.get("urlIdx").and_then(|v| v.as_u64()) {
                idx_num.to_string()
            } else {
                return Err(AppError::BadRequest(
                    "Direct connection requires urlIdx in model_item to look up user connection settings".to_string()
                ));
            };

            tracing::debug!(
                "Looking up direct connection from user settings with urlIdx: {}",
                url_idx
            );

            // Get user settings to find their direct connections
            tracing::debug!("User settings: {:?}", auth_user.user.settings);
            let user_settings = auth_user.user.settings.as_ref()
                .ok_or_else(|| AppError::BadRequest(
                    "User settings not found. Please configure your direct connections in Settings > Connections.".to_string()
                ))?;

            // Direct connections can be at settings.directConnections OR settings.ui.directConnections
            let direct_connections = user_settings.get("directConnections").or_else(|| {
                user_settings
                    .get("ui")
                    .and_then(|ui| ui.get("directConnections"))
            });

            if direct_connections.is_none() {
                tracing::warn!(
                    "Direct connections not configured for user {}. User settings structure: {:?}",
                    auth_user.user.email,
                    user_settings
                );
                return Err(AppError::BadRequest(
                    "Direct connections not configured. Please add your OpenAI API connections in Settings > Connections.".to_string()
                ));
            }

            let direct_connections = direct_connections.unwrap();

            // Extract URL and key arrays from settings
            tracing::debug!("Direct connections object: {:?}", direct_connections);

            let urls = direct_connections.get("OPENAI_API_BASE_URLS")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    tracing::error!("OPENAI_API_BASE_URLS missing or not an array in direct connections");
                    AppError::BadRequest("Direct connections configuration is invalid. Please check your Settings > Connections.".to_string())
                })?;

            let keys = direct_connections.get("OPENAI_API_KEYS")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    tracing::error!("OPENAI_API_KEYS missing or not an array in direct connections");
                    AppError::BadRequest("Direct connections configuration is invalid. Please check your Settings > Connections.".to_string())
                })?;

            let configs = direct_connections.get("OPENAI_API_CONFIGS")
                .and_then(|v| v.as_object())
                .ok_or_else(|| {
                    tracing::error!("OPENAI_API_CONFIGS missing or not an object in direct connections");
                    AppError::BadRequest("Direct connections configuration is invalid. Please check your Settings > Connections.".to_string())
                })?;

            // Parse urlIdx as number
            let idx: usize = url_idx
                .parse()
                .map_err(|_| AppError::BadRequest(format!("Invalid urlIdx: {}", url_idx)))?;

            tracing::debug!(
                "Looking up connection at index {} from {} total URLs",
                idx,
                urls.len()
            );

            // Get URL and key at that index
            let connection_url = urls.get(idx)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    tracing::error!("No URL found at index {}. Available URLs: {} total", idx, urls.len());
                    AppError::BadRequest(format!(
                        "Connection not found at index {}. You have {} connection(s) configured. Please check your Settings > Connections.", 
                        idx, urls.len()
                    ))
                })?;

            let connection_key = keys.get(idx).and_then(|v| v.as_str()).unwrap_or("");

            // Get config for this connection (might be at string index)
            let connection_config = configs
                .get(&url_idx)
                .or_else(|| configs.get(connection_url))
                .cloned()
                .unwrap_or(serde_json::json!({}));

            tracing::info!(
                "Using direct connection from user settings: {} (idx: {}) for model {} by user {}",
                connection_url,
                idx,
                model_id,
                auth_user.user.email
            );

            (
                connection_url.to_string(),
                connection_key.to_string(),
                connection_config,
            )
        }
    } else if is_direct && !config.enable_direct_connections {
        // Direct requested but not enabled - return error message
        return Err(AppError::BadRequest(
            "Direct connections are not enabled. Please enable them in Admin Settings > Connections".to_string()
        ));
    } else {
        // Regular (non-direct) routing
        // SMART FALLBACK: If user has direct connections configured, use their first one
        // This allows chat/notes to work even when frontend doesn't pass model_item
        if config.enable_direct_connections {
            if let Some(user_settings) = auth_user.user.settings.as_ref() {
                let direct_connections = user_settings.get("directConnections").or_else(|| {
                    user_settings
                        .get("ui")
                        .and_then(|ui| ui.get("directConnections"))
                });

                if let Some(dc) = direct_connections {
                    if let (Some(urls), Some(keys)) = (
                        dc.get("OPENAI_API_BASE_URLS").and_then(|v| v.as_array()),
                        dc.get("OPENAI_API_KEYS").and_then(|v| v.as_array()),
                    ) {
                        if !urls.is_empty() {
                            let first_url = urls[0].as_str().unwrap_or("");
                            let first_key = keys.get(0).and_then(|k| k.as_str()).unwrap_or("");

                            if !first_url.is_empty() {
                                let configs =
                                    dc.get("OPENAI_API_CONFIGS").and_then(|v| v.as_object());

                                let first_config = configs
                                    .and_then(|c| c.get("0").or_else(|| c.get(first_url)))
                                    .cloned()
                                    .unwrap_or(serde_json::json!({}));

                                tracing::info!(
                                    "Chat using user's first direct connection (smart fallback): {} for model {} by user {}",
                                    first_url,
                                    model_id,
                                    auth_user.user.email
                                );

                                (first_url.to_string(), first_key.to_string(), first_config)
                            } else {
                                // Empty URL, fall back to global config
                                get_endpoint_from_cache_or_config(
                                    &state,
                                    &config,
                                    &model_id,
                                    &model_item,
                                    &payload_obj,
                                )?
                            }
                        } else {
                            // No URLs configured, fall back to global config
                            get_endpoint_from_cache_or_config(
                                &state,
                                &config,
                                &model_id,
                                &model_item,
                                &payload_obj,
                            )?
                        }
                    } else {
                        // Invalid structure, fall back to global config
                        get_endpoint_from_cache_or_config(
                            &state,
                            &config,
                            &model_id,
                            &model_item,
                            &payload_obj,
                        )?
                    }
                } else {
                    // No direct connections in settings, fall back to global config
                    get_endpoint_from_cache_or_config(
                        &state,
                        &config,
                        &model_id,
                        &model_item,
                        &payload_obj,
                    )?
                }
            } else {
                // No user settings, fall back to global config
                get_endpoint_from_cache_or_config(
                    &state,
                    &config,
                    &model_id,
                    &model_item,
                    &payload_obj,
                )?
            }
        } else {
            // Direct connections not enabled, use global config
            get_endpoint_from_cache_or_config(
                &state,
                &config,
                &model_id,
                &model_item,
                &payload_obj,
            )?
        }
    };

    // Prepare the request to the OpenAI-compatible endpoint
    let client = reqwest::Client::new();
    let mut request_builder = client
        .post(format!("{}/chat/completions", url))
        .header("Content-Type", "application/json");

    // Add authorization header based on auth_type
    let auth_type = api_config
        .get("auth_type")
        .and_then(|v| v.as_str())
        .unwrap_or("bearer");

    match auth_type {
        "none" => {
            // No authentication
        }
        _ => {
            // Default to bearer token for all other cases
            if !key.is_empty() {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", key));
            }
        } // TODO: Add support for other auth types like "session", "system_oauth", "azure_ad"
    }

    // Forward the modified payload (already extracted earlier)

    match request_builder.json(&payload_obj).send().await {
        Ok(response) if response.status().is_success() => {
            // Check if it's a streaming response
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            let is_stream = payload_obj
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            tracing::debug!(
                "Response content-type: {}, stream param: {}",
                content_type,
                is_stream
            );

            if is_stream && content_type.contains("text/event-stream") {
                tracing::debug!("🔴 STREAMING: Real-time SSE response");

                // Check if we should use Socket.IO for streaming (all three required: session_id, chat_id, message_id)
                let use_socketio = session_id.is_some()
                    && chat_id.is_some()
                    && message_id.is_some()
                    && state.socket_state.is_some();

                tracing::info!("Socket.IO check - session_id: {:?}, chat_id: {:?}, message_id: {:?}, use_socketio: {}", 
                    session_id, chat_id, message_id, use_socketio);

                if use_socketio {
                    // Use Socket.IO streaming - process the stream and emit events
                    tracing::info!(
                        "🔵 Using Socket.IO for real-time streaming to session: {:?}",
                        session_id
                    );

                    // Spawn a task to process the stream via Socket.IO
                    let state_clone = state.clone();
                    let user_id = auth_user.user.id.clone();
                    let session_id_owned = session_id.clone();
                    let model_id_owned = model_id.clone();
                    let messages_owned = messages.clone();
                    let should_generate_title_owned = should_generate_title;
                    let model_item_owned = model_item.clone();
                    let url_owned = url.clone();
                    let key_owned = key.clone();
                    let tool_ids_owned = tool_ids.clone();
                    let all_tool_specs_owned = all_tool_specs.clone();

                    tokio::spawn(async move {
                        if let Err(e) = process_streaming_via_socketio(
                            response,
                            &state_clone,
                            &user_id,
                            model_id_owned,
                            messages_owned,
                            chat_id,
                            message_id,
                            session_id_owned,
                            should_generate_title_owned,
                            model_item_owned,
                            url_owned,
                            key_owned,
                            tool_ids_owned,
                            all_tool_specs_owned,
                        )
                        .await
                        {
                            tracing::error!("Error processing Socket.IO stream: {}", e);
                        }
                    });

                    // Return an immediate success response
                    // The actual streaming happens via Socket.IO
                    Ok(HttpResponse::Ok().json(serde_json::json!({
                        "status": "streaming",
                        "message": "Streaming via Socket.IO"
                    })))
                } else {
                    // Use traditional HTTP SSE streaming (no Socket.IO)
                    tracing::debug!("Using HTTP SSE streaming (no Socket.IO metadata)");
                    chat_completion::create_sse_stream(response)
                }
            } else {
                // Return JSON response
                tracing::debug!("Returning JSON response");
                if let Ok(json_response) = response.json::<serde_json::Value>().await {
                    Ok(HttpResponse::Ok().json(json_response))
                } else {
                    Err(AppError::InternalServerError(
                        "Failed to parse response".to_string(),
                    ))
                }
            }
        }
        Ok(response) => {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("OpenAI API error: {} - {}", status, error_text);
            Err(AppError::InternalServerError(format!(
                "OpenAI API error: {} - {}",
                status, error_text
            )))
        }
        Err(e) => {
            tracing::error!("Error calling OpenAI API: {}", e);
            Err(AppError::InternalServerError(format!(
                "Error calling OpenAI API: {}",
                e
            )))
        }
    }
}
