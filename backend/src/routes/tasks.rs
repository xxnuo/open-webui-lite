use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::AppError,
    middleware::{AuthMiddleware, AuthUser},
    AppState,
};

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/config")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_task_config)),
    )
    .service(
        web::resource("/config/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_task_config)),
    )
    .service(
        web::resource("/title/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_title)),
    )
    .service(
        web::resource("/follow_up/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_follow_up)),
    )
    .service(
        web::resource("/tags/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_tags)),
    )
    .service(
        web::resource("/image_prompt/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_image_prompt)),
    )
    .service(
        web::resource("/queries/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_queries)),
    )
    .service(
        web::resource("/auto/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_autocomplete)),
    )
    .service(
        web::resource("/emoji/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_emoji)),
    )
    .service(
        web::resource("/moa/completions")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_moa)),
    );
}

#[derive(Debug, Serialize)]
struct TaskConfig {
    #[serde(rename = "TASK_MODEL")]
    task_model: Option<String>,
    #[serde(rename = "TASK_MODEL_EXTERNAL")]
    task_model_external: Option<String>,
    #[serde(rename = "ENABLE_TITLE_GENERATION")]
    enable_title_generation: bool,
    #[serde(rename = "TITLE_GENERATION_PROMPT_TEMPLATE")]
    title_generation_prompt_template: String,
    #[serde(rename = "IMAGE_PROMPT_GENERATION_PROMPT_TEMPLATE")]
    image_prompt_generation_prompt_template: String,
    #[serde(rename = "ENABLE_AUTOCOMPLETE_GENERATION")]
    enable_autocomplete_generation: bool,
    #[serde(rename = "AUTOCOMPLETE_GENERATION_INPUT_MAX_LENGTH")]
    autocomplete_generation_input_max_length: i32,
    #[serde(rename = "TAGS_GENERATION_PROMPT_TEMPLATE")]
    tags_generation_prompt_template: String,
    #[serde(rename = "FOLLOW_UP_GENERATION_PROMPT_TEMPLATE")]
    follow_up_generation_prompt_template: String,
    #[serde(rename = "ENABLE_FOLLOW_UP_GENERATION")]
    enable_follow_up_generation: bool,
    #[serde(rename = "ENABLE_TAGS_GENERATION")]
    enable_tags_generation: bool,
    #[serde(rename = "ENABLE_SEARCH_QUERY_GENERATION")]
    enable_search_query_generation: bool,
    #[serde(rename = "ENABLE_RETRIEVAL_QUERY_GENERATION")]
    enable_retrieval_query_generation: bool,
    #[serde(rename = "QUERY_GENERATION_PROMPT_TEMPLATE")]
    query_generation_prompt_template: String,
    #[serde(rename = "TOOLS_FUNCTION_CALLING_PROMPT_TEMPLATE")]
    tools_function_calling_prompt_template: String,
}

#[derive(Debug, Deserialize)]
struct TaskConfigUpdate {
    #[serde(rename = "TASK_MODEL")]
    task_model: Option<String>,
    #[serde(rename = "TASK_MODEL_EXTERNAL")]
    task_model_external: Option<String>,
    #[serde(rename = "ENABLE_TITLE_GENERATION")]
    enable_title_generation: bool,
    #[serde(rename = "TITLE_GENERATION_PROMPT_TEMPLATE")]
    title_generation_prompt_template: String,
    #[serde(rename = "IMAGE_PROMPT_GENERATION_PROMPT_TEMPLATE")]
    image_prompt_generation_prompt_template: String,
    #[serde(rename = "ENABLE_AUTOCOMPLETE_GENERATION")]
    enable_autocomplete_generation: bool,
    #[serde(rename = "AUTOCOMPLETE_GENERATION_INPUT_MAX_LENGTH")]
    autocomplete_generation_input_max_length: i32,
    #[serde(rename = "TAGS_GENERATION_PROMPT_TEMPLATE")]
    tags_generation_prompt_template: String,
    #[serde(rename = "FOLLOW_UP_GENERATION_PROMPT_TEMPLATE")]
    follow_up_generation_prompt_template: String,
    #[serde(rename = "ENABLE_FOLLOW_UP_GENERATION")]
    enable_follow_up_generation: bool,
    #[serde(rename = "ENABLE_TAGS_GENERATION")]
    enable_tags_generation: bool,
    #[serde(rename = "ENABLE_SEARCH_QUERY_GENERATION")]
    enable_search_query_generation: bool,
    #[serde(rename = "ENABLE_RETRIEVAL_QUERY_GENERATION")]
    enable_retrieval_query_generation: bool,
    #[serde(rename = "QUERY_GENERATION_PROMPT_TEMPLATE")]
    query_generation_prompt_template: String,
    #[serde(rename = "TOOLS_FUNCTION_CALLING_PROMPT_TEMPLATE")]
    tools_function_calling_prompt_template: String,
}

async fn get_task_config(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    let response = TaskConfig {
        task_model: config.task_model.clone(),
        task_model_external: config.task_model_external.clone(),
        enable_title_generation: config.enable_title_generation,
        title_generation_prompt_template: config.title_generation_prompt_template.clone(),
        image_prompt_generation_prompt_template: config
            .image_prompt_generation_prompt_template
            .clone(),
        enable_autocomplete_generation: config.enable_autocomplete_generation,
        autocomplete_generation_input_max_length: config.autocomplete_generation_input_max_length,
        tags_generation_prompt_template: config.tags_generation_prompt_template.clone(),
        follow_up_generation_prompt_template: config.follow_up_generation_prompt_template.clone(),
        enable_follow_up_generation: config.enable_follow_up_generation,
        enable_tags_generation: config.enable_tags_generation,
        enable_search_query_generation: config.enable_search_query_generation,
        enable_retrieval_query_generation: config.enable_retrieval_query_generation,
        query_generation_prompt_template: config.query_generation_prompt_template.clone(),
        tools_function_calling_prompt_template: config
            .tools_function_calling_prompt_template
            .clone(),
    };

    Ok(HttpResponse::Ok().json(response))
}

async fn update_task_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<TaskConfigUpdate>,
) -> Result<HttpResponse, AppError> {
    // Check if user is admin
    if auth_user.role != "admin" {
        return Err(AppError::Unauthorized("Admin access required".to_string()));
    }

    let mut config = state.config.write().unwrap();

    config.task_model = payload.task_model.clone();
    config.task_model_external = payload.task_model_external.clone();
    config.enable_title_generation = payload.enable_title_generation;
    config.title_generation_prompt_template = payload.title_generation_prompt_template.clone();
    config.image_prompt_generation_prompt_template =
        payload.image_prompt_generation_prompt_template.clone();
    config.enable_autocomplete_generation = payload.enable_autocomplete_generation;
    config.autocomplete_generation_input_max_length =
        payload.autocomplete_generation_input_max_length;
    config.tags_generation_prompt_template = payload.tags_generation_prompt_template.clone();
    config.follow_up_generation_prompt_template =
        payload.follow_up_generation_prompt_template.clone();
    config.enable_follow_up_generation = payload.enable_follow_up_generation;
    config.enable_tags_generation = payload.enable_tags_generation;
    config.enable_search_query_generation = payload.enable_search_query_generation;
    config.enable_retrieval_query_generation = payload.enable_retrieval_query_generation;
    config.query_generation_prompt_template = payload.query_generation_prompt_template.clone();
    config.tools_function_calling_prompt_template =
        payload.tools_function_calling_prompt_template.clone();

    let response = TaskConfig {
        task_model: config.task_model.clone(),
        task_model_external: config.task_model_external.clone(),
        enable_title_generation: config.enable_title_generation,
        title_generation_prompt_template: config.title_generation_prompt_template.clone(),
        image_prompt_generation_prompt_template: config
            .image_prompt_generation_prompt_template
            .clone(),
        enable_autocomplete_generation: config.enable_autocomplete_generation,
        autocomplete_generation_input_max_length: config.autocomplete_generation_input_max_length,
        tags_generation_prompt_template: config.tags_generation_prompt_template.clone(),
        follow_up_generation_prompt_template: config.follow_up_generation_prompt_template.clone(),
        enable_follow_up_generation: config.enable_follow_up_generation,
        enable_tags_generation: config.enable_tags_generation,
        enable_search_query_generation: config.enable_search_query_generation,
        enable_retrieval_query_generation: config.enable_retrieval_query_generation,
        query_generation_prompt_template: config.query_generation_prompt_template.clone(),
        tools_function_calling_prompt_template: config
            .tools_function_calling_prompt_template
            .clone(),
    };

    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct CompletionRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    model_item: Option<serde_json::Value>,
}

/// Generate a title for a chat based on its messages
async fn generate_title(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    // Check if title generation is enabled
    if !config.enable_title_generation {
        return Ok(HttpResponse::Ok().json(json!({
            "detail": "Title generation is disabled"
        })));
    }

    // Get the last 2 messages for title generation
    let messages_for_title: Vec<_> = payload
        .messages
        .iter()
        .rev()
        .take(2)
        .rev()
        .cloned()
        .collect();

    // Build the title generation prompt
    let template = if config.title_generation_prompt_template.is_empty() {
        DEFAULT_TITLE_GENERATION_PROMPT_TEMPLATE.to_string()
    } else {
        config.title_generation_prompt_template.clone()
    };

    // Replace {{MESSAGES:END:2}} with the actual messages
    let messages_text = messages_for_title
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = template.replace("{{MESSAGES:END:2}}", &messages_text);

    drop(config); // Release lock before calling completion

    // Call OpenAI API for completion with direct connection support
    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        50,
        0.1,
    )
    .await
}

async fn generate_follow_up(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_follow_up_generation {
        return Ok(HttpResponse::Ok().json(json!({
            "detail": "Follow-up generation is disabled"
        })));
    }

    let template = if config.follow_up_generation_prompt_template.is_empty() {
        DEFAULT_FOLLOW_UP_GENERATION_PROMPT_TEMPLATE.to_string()
    } else {
        config.follow_up_generation_prompt_template.clone()
    };

    let messages_text = format_messages(&payload.messages);
    let prompt = template.replace("{{MESSAGES}}", &messages_text);

    drop(config);

    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        200,
        0.7,
    )
    .await
}

async fn generate_tags(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_tags_generation {
        return Ok(HttpResponse::Ok().json(json!({
            "detail": "Tags generation is disabled"
        })));
    }

    let template = if config.tags_generation_prompt_template.is_empty() {
        DEFAULT_TAGS_GENERATION_PROMPT_TEMPLATE.to_string()
    } else {
        config.tags_generation_prompt_template.clone()
    };

    let messages_text = format_messages(&payload.messages);
    let prompt = template.replace("{{MESSAGES}}", &messages_text);

    drop(config);

    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        50,
        0.5,
    )
    .await
}

async fn generate_image_prompt(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    let template = if config.image_prompt_generation_prompt_template.is_empty() {
        DEFAULT_IMAGE_PROMPT_GENERATION_PROMPT_TEMPLATE.to_string()
    } else {
        config.image_prompt_generation_prompt_template.clone()
    };

    let user_prompt = payload.prompt.as_deref().unwrap_or("");
    let prompt = template.replace("{{PROMPT}}", user_prompt);

    drop(config);

    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        200,
        0.7,
    )
    .await
}

async fn generate_queries(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_search_query_generation {
        return Ok(HttpResponse::Ok().json(json!({
            "detail": "Query generation is disabled"
        })));
    }

    let template = if config.query_generation_prompt_template.is_empty() {
        DEFAULT_QUERY_GENERATION_PROMPT_TEMPLATE.to_string()
    } else {
        config.query_generation_prompt_template.clone()
    };

    let messages_text = format_messages(&payload.messages);
    let prompt = template.replace("{{MESSAGES}}", &messages_text);

    drop(config);

    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        100,
        0.3,
    )
    .await
}

async fn generate_autocomplete(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_autocomplete_generation {
        return Ok(HttpResponse::Ok().json(json!({
            "detail": "Autocomplete generation is disabled"
        })));
    }

    let user_prompt = payload.prompt.as_deref().unwrap_or("");

    // Check max length
    if user_prompt.len() > config.autocomplete_generation_input_max_length as usize {
        return Ok(HttpResponse::Ok().json(json!({
            "detail": "Input too long for autocomplete"
        })));
    }

    let template = DEFAULT_AUTOCOMPLETE_GENERATION_PROMPT_TEMPLATE;
    let prompt = template.replace("{{PROMPT}}", user_prompt);

    drop(config);

    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        100,
        0.7,
    )
    .await
}

async fn generate_emoji(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<CompletionRequest>,
) -> Result<HttpResponse, AppError> {
    let messages_text = format_messages(&payload.messages);
    let prompt = DEFAULT_EMOJI_GENERATION_PROMPT_TEMPLATE.replace("{{MESSAGES}}", &messages_text);

    call_openai_completion(
        &state,
        &auth_user,
        &payload.model,
        payload.model_item.as_ref(),
        &prompt,
        10,
        0.5,
    )
    .await
}

async fn generate_moa(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    // MOA (Mixture of Agents) response aggregation
    let empty_vec = vec![];
    let responses = payload
        .get("responses")
        .and_then(|r| r.as_array())
        .unwrap_or(&empty_vec);
    let query = payload.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let model = payload
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("gpt-3.5-turbo");

    let model_item = payload.get("model_item").cloned();

    let responses_text = responses
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "Response {}:\n{}",
                i + 1,
                r.get("content").and_then(|c| c.as_str()).unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = DEFAULT_MOA_GENERATION_PROMPT_TEMPLATE
        .replace("{{QUERY}}", query)
        .replace("{{RESPONSES}}", &responses_text);

    call_openai_completion(
        &state,
        &auth_user,
        model,
        model_item.as_ref(),
        &prompt,
        500,
        0.7,
    )
    .await
}

// Helper function to call OpenAI completion API with proper direct connection support
async fn call_openai_completion(
    state: &web::Data<AppState>,
    auth_user: &AuthUser,
    model: &str,
    model_item: Option<&serde_json::Value>,
    prompt: &str,
    max_tokens: i32,
    temperature: f32,
) -> Result<HttpResponse, AppError> {
    // Build the chat completion request payload
    let mut completion_payload = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ],
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": false
    });

    // If model_item is provided, add it to the payload
    // This enables direct connection routing
    if let Some(item) = model_item {
        if let Some(obj) = completion_payload.as_object_mut() {
            obj.insert("model_item".to_string(), item.clone());
        }
    }

    // Use the existing get_endpoint_and_route_request helper to properly handle direct connections
    let (url, key, api_config) = get_endpoint_and_route_request(
        state,
        auth_user,
        model,
        model_item.cloned().unwrap_or(serde_json::json!({})),
        &completion_payload,
    )?;

    tracing::info!(
        "Task completion - using endpoint: {} for model: {} (user: {})",
        url,
        model,
        auth_user.user.email
    );

    // Make the API request
    let client = reqwest::Client::new();
    let mut request_builder = client
        .post(format!("{}/chat/completions", url.trim_end_matches('/')))
        .header("Content-Type", "application/json");

    // Add authorization header based on auth_type
    let auth_type = api_config
        .get("auth_type")
        .and_then(|v| v.as_str())
        .unwrap_or("bearer");

    match auth_type {
        "none" => {
            // No authentication
            tracing::debug!("Task completion - no authentication required");
        }
        _ => {
            // Default to bearer token for all other cases
            if !key.is_empty() {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", key));
                tracing::debug!("Task completion - using bearer token authentication");
            } else {
                tracing::warn!("Task completion - no API key available for model {}", model);
            }
        }
    }

    match request_builder.json(&completion_payload).send().await {
        Ok(response) if response.status().is_success() => {
            let json_response = response.json::<serde_json::Value>().await.map_err(|e| {
                AppError::InternalServerError(format!("Failed to parse response: {}", e))
            })?;

            Ok(HttpResponse::Ok().json(json_response))
        }
        Ok(response) => {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("Task completion API error: {} - {}", status, error_text);
            Err(AppError::InternalServerError(format!(
                "API call failed with status {}: {}",
                status, error_text
            )))
        }
        Err(e) => {
            tracing::error!("Task completion API request error: {}", e);
            Err(AppError::InternalServerError(format!(
                "API request failed: {}",
                e
            )))
        }
    }
}

// Helper to get endpoint and route request (extracted from chat_completions logic)
fn get_endpoint_and_route_request(
    state: &web::Data<AppState>,
    auth_user: &AuthUser,
    model_id: &str,
    model_item: serde_json::Value,
    _payload_obj: &serde_json::Value,
) -> Result<(String, String, serde_json::Value), AppError> {
    // Check if this is a direct connection request
    let is_direct = model_item
        .get("direct")
        .and_then(|d| d.as_bool())
        .unwrap_or(false);

    let config = state.config.read().unwrap();

    if is_direct && config.enable_direct_connections {
        // Direct connection - look up URL and key from user settings using urlIdx
        // First, try explicit url/key in model_item (rare case)
        let direct_url = model_item
            .get("url")
            .and_then(|u| u.as_str())
            .filter(|s| !s.is_empty());

        let direct_key = model_item.get("key").and_then(|k| k.as_str()).unwrap_or("");

        if let Some(url_str) = direct_url {
            tracing::info!(
                "Task using direct connection with explicit URL: {} for model {} by user {}",
                url_str,
                model_id,
                auth_user.user.email
            );

            let item_config = model_item
                .get("config")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            return Ok((url_str.to_string(), direct_key.to_string(), item_config));
        }

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
            "Task using direct connection from user settings: {} (idx: {}) for model {} by user {}",
            connection_url,
            idx,
            model_id,
            auth_user.user.email
        );

        Ok((
            connection_url.to_string(),
            connection_key.to_string(),
            connection_config,
        ))
    } else if is_direct && !config.enable_direct_connections {
        // Direct requested but not enabled - return error message
        Err(AppError::BadRequest(
            "Direct connections are not enabled. Please enable them in Admin Settings > Connections".to_string()
        ))
    } else {
        // Regular (non-direct) routing
        // SMART FALLBACK: If user has direct connections configured, use their first one
        // This allows tasks to work even when frontend doesn't pass model_item
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
                                    "Task using user's first direct connection (smart fallback): {} for model {} by user {}",
                                    first_url,
                                    model_id,
                                    auth_user.user.email
                                );

                                return Ok((
                                    first_url.to_string(),
                                    first_key.to_string(),
                                    first_config,
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Final fallback: use global config endpoints
        if config.openai_api_base_urls.is_empty() {
            return Err(AppError::InternalServerError(
                "No OpenAI endpoint configured. Please configure direct connections in Settings > Connections or configure global OpenAI endpoints in Admin Settings.".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[0].clone();
        let endpoint_key = config.openai_api_keys.get(0).cloned().unwrap_or_default();
        let api_config = config
            .openai_api_configs
            .get("0")
            .or_else(|| config.openai_api_configs.get(&endpoint_url))
            .cloned()
            .unwrap_or(serde_json::json!({}));

        tracing::info!(
            "Task using global endpoint: {} for model {} by user {}",
            endpoint_url,
            model_id,
            auth_user.user.email
        );

        Ok((endpoint_url, endpoint_key, api_config))
    }
}

// Helper function to format messages
fn format_messages(messages: &[serde_json::Value]) -> String {
    messages
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// Default prompt templates
const DEFAULT_TITLE_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Generate a concise, 3-5 word title with an emoji summarizing the chat history.
### Guidelines:
- The title should clearly represent the main theme or subject of the conversation.
- Use emojis that enhance understanding of the topic, but avoid quotation marks or special formatting.
- Write the title in the chat's primary language; default to English if multilingual.
- Prioritize accuracy over excessive creativity; keep it clear and simple.
- Your entire response must consist solely of the JSON object, without any introductory or concluding text.
- The output must be a single, raw JSON object, without any markdown code fences or other encapsulating text.
- Ensure no conversational text, affirmations, or explanations precede or follow the raw JSON output, as this will cause direct parsing failure.
### Output:
JSON format: { "title": "your concise title here" }
### Examples:
- { "title": "📉 Stock Market Trends" },
- { "title": "🍪 Perfect Chocolate Chip Recipe" },
- { "title": "Evolution of Music Streaming" },
- { "title": "Remote Work Productivity Tips" },
- { "title": "Artificial Intelligence in Healthcare" },
- { "title": "🎮 Video Game Development Insights" }
### Chat History:
<chat_history>
{{MESSAGES:END:2}}
</chat_history>"#;

const DEFAULT_FOLLOW_UP_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Generate 3-5 relevant follow-up questions based on the conversation.
### Output:
JSON format: { "questions": ["question 1", "question 2", "question 3"] }
### Chat History:
{{MESSAGES}}"#;

const DEFAULT_TAGS_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Generate 3-5 relevant tags for categorizing this conversation.
### Output:
JSON format: { "tags": ["tag1", "tag2", "tag3"] }
### Chat History:
{{MESSAGES}}"#;

const DEFAULT_IMAGE_PROMPT_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Enhance the following prompt for image generation, making it more detailed and descriptive.
### Original Prompt:
{{PROMPT}}
### Output:
JSON format: { "prompt": "enhanced prompt here" }"#;

const DEFAULT_QUERY_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Generate 2-3 search queries to find relevant information for this conversation.
### Output:
JSON format: { "queries": ["query 1", "query 2"] }
### Chat History:
{{MESSAGES}}"#;

const DEFAULT_AUTOCOMPLETE_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Complete the following text naturally and concisely.
### Input:
{{PROMPT}}
### Output:
JSON format: { "completion": "completed text here" }"#;

const DEFAULT_EMOJI_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Suggest a single emoji that best represents the mood or topic of this conversation.
### Output:
JSON format: { "emoji": "😊" }
### Chat History:
{{MESSAGES}}"#;

const DEFAULT_MOA_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Synthesize the following responses into a single, coherent answer to the query.
### Query:
{{QUERY}}
### Responses:
{{RESPONSES}}
### Output:
Provide a synthesized response that combines the best aspects of all responses."#;
