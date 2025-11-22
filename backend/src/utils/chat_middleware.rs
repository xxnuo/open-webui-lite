use crate::{
    db::Database,
    error::AppResult,
    models::{chat::Chat, folder::Folder, function::Function},
};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Metadata for chat completion requests
#[derive(Debug, Clone)]
pub struct ChatMetadata {
    pub user_id: String,
    pub chat_id: Option<String>,
    pub message_id: Option<String>,
    pub session_id: Option<String>,
    pub filter_ids: Vec<String>,
    pub tool_ids: Option<Vec<String>>,
    pub files: Option<Vec<Value>>,
    pub features: Option<Value>,
    pub variables: Option<Value>,
}

/// Process chat payload through the complete middleware chain
///
/// Order of operations:
/// 1. Apply model params to form data
/// 2. Apply system prompt
/// 3. Process folder (project) system prompt and files
/// 4. Process model knowledge
/// 5. Pipeline inlet filters
/// 6. Function inlet filters
/// 7. Memory handler (if enabled)
/// 8. Web search handler (if enabled)
/// 9. Image generation handler (if enabled)
/// 10. Code interpreter prompt (if enabled)
/// 11. Tool/function calling setup
/// 12. File processing
pub async fn process_chat_payload(
    db: &Database,
    http_client: &reqwest::Client,
    mut form_data: Value,
    user: &crate::models::user::User,
    metadata: &ChatMetadata,
    model: &crate::services::models::Model,
    config: &crate::config::Config,
) -> AppResult<(Value, Vec<Value>)> {
    use tracing::{debug, warn};

    debug!("Processing chat payload through middleware chain");

    // Extract and store features for later processing
    let features = form_data
        .get("features")
        .cloned()
        .or_else(|| metadata.features.clone());

    // 1. Apply model params to form data
    form_data = apply_model_params_to_form_data(form_data, model)?;

    // 2. Apply system prompt if present
    let system_prompt_content = form_data
        .get("messages")
        .and_then(|m| m.as_array())
        .and_then(|messages| get_system_message(messages))
        .and_then(|msg| msg.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    if let Some(content) = system_prompt_content {
        form_data = apply_system_prompt_to_body(&content, form_data, metadata, user)?;
    }

    // 3. Process folder (project) system prompt and files
    if let Some(chat_id) = &metadata.chat_id {
        form_data = process_folder_context(db, form_data, chat_id, &user.id, metadata).await?;
    }

    // 4. Process model knowledge (RAG)
    if let Some(knowledge) = model
        .info
        .as_ref()
        .and_then(|i| i.meta.as_ref())
        .and_then(|m| m.knowledge.as_ref())
    {
        form_data = process_model_knowledge(form_data, knowledge)?;
    }

    // 5. Pipeline inlet filters
    if config.enable_pipeline_filters {
        match process_pipeline_inlet_filters(http_client, form_data.clone(), user, model, config)
            .await
        {
            Ok(filtered) => form_data = filtered,
            Err(e) => {
                warn!("Pipeline inlet filter error: {}", e);
                // Continue without pipeline filtering
            }
        }
    }

    // 6. Function inlet filters
    match process_function_inlet_filters(db, form_data.clone(), user, model, metadata).await {
        Ok(filtered) => form_data = filtered,
        Err(e) => {
            warn!("Function inlet filter error: {}", e);
            // Continue without function filtering
        }
    }

    // 7-10. Process features if enabled
    if let Some(features_obj) = features {
        form_data = process_features(form_data, &features_obj, user, config).await?;
    }

    // 11. Tool/function calling setup
    form_data = setup_tools(form_data, metadata, model)?;

    // 12. File processing
    form_data = process_files(form_data, metadata)?;

    // Return processed form data and any sources/events
    let sources = Vec::new(); // TODO: Collect sources from various handlers
    Ok((form_data, sources))
}

/// Apply model parameters to form data
fn apply_model_params_to_form_data(
    mut form_data: Value,
    model: &crate::services::models::Model,
) -> AppResult<Value> {
    if let Some(info) = &model.info {
        if let Some(params) = &info.params {
            // Apply model parameters (temperature, top_p, etc.)
            if let Some(obj) = form_data.as_object_mut() {
                for (key, value) in params.as_object().unwrap_or(&serde_json::Map::new()) {
                    // Skip internal keys
                    if key.starts_with("_") || key == "urlIdx" || key == "user_id" {
                        continue;
                    }

                    // Apply parameter if not already set in request
                    if !obj.contains_key(key) {
                        obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }

    Ok(form_data)
}

/// Get system message from messages array
fn get_system_message(messages: &[Value]) -> Option<&Value> {
    messages.iter().find(|m| {
        m.get("role")
            .and_then(|r| r.as_str())
            .map(|r| r == "system")
            .unwrap_or(false)
    })
}

/// Apply system prompt to message body
fn apply_system_prompt_to_body(
    system_prompt: &str,
    mut form_data: Value,
    _metadata: &ChatMetadata,
    _user: &crate::models::user::User,
) -> AppResult<Value> {
    // TODO: Template variable replacement ({{USER_NAME}}, etc.)

    if let Some(messages) = form_data.get_mut("messages").and_then(|m| m.as_array_mut()) {
        // Check if system message already exists
        let has_system = messages.iter().any(|m| {
            m.get("role")
                .and_then(|r| r.as_str())
                .map(|r| r == "system")
                .unwrap_or(false)
        });

        if !has_system {
            // Insert system message at the beginning
            messages.insert(
                0,
                json!({
                    "role": "system",
                    "content": system_prompt
                }),
            );
        }
    }

    Ok(form_data)
}

/// Process folder context (project system prompts and files)
async fn process_folder_context(
    db: &Database,
    mut form_data: Value,
    chat_id: &str,
    user_id: &str,
    metadata: &ChatMetadata,
) -> AppResult<Value> {
    // Query chat to get folder_id
    let chat: Option<Chat> =
        sqlx::query_as::<_, Chat>(r#"SELECT * FROM "chat" WHERE id = $1 AND user_id = $2"#)
            .bind(chat_id)
            .bind(user_id)
            .fetch_optional(db.pool())
            .await?;

    if let Some(chat) = chat {
        if let Some(folder_id) = chat.folder_id {
            // Query folder
            let folder: Option<Folder> = sqlx::query_as::<_, Folder>(
                r#"SELECT * FROM "folder" WHERE id = $1 AND user_id = $2"#,
            )
            .bind(&folder_id)
            .bind(user_id)
            .fetch_optional(db.pool())
            .await?;

            if let Some(folder) = folder {
                if let Some(data) = folder.data {
                    // Apply system prompt from folder
                    if let Some(system_prompt) = data.get("system_prompt").and_then(|s| s.as_str())
                    {
                        form_data = apply_system_prompt_to_body(
                            system_prompt,
                            form_data,
                            metadata,
                            &crate::models::user::User {
                                id: user_id.to_string(),
                                email: String::new(),
                                name: String::new(),
                                username: None,
                                role: String::new(),
                                profile_image_url: String::new(),
                                bio: None,
                                gender: None,
                                date_of_birth: None,
                                created_at: 0,
                                updated_at: 0,
                                last_active_at: 0,
                                api_key: None,
                                settings: None,
                                info: None,
                                oauth_sub: None,
                            },
                        )?;
                    }

                    // Add files from folder
                    if let Some(folder_files) = data.get("files").and_then(|f| f.as_array()) {
                        let existing_files = form_data
                            .get("files")
                            .and_then(|f| f.as_array())
                            .cloned()
                            .unwrap_or_default();

                        let mut all_files = folder_files.clone();
                        all_files.extend(existing_files);

                        if let Some(obj) = form_data.as_object_mut() {
                            obj.insert("files".to_string(), Value::Array(all_files));
                        }
                    }
                }
            }
        }
    }

    Ok(form_data)
}

/// Process model knowledge (add knowledge files to request)
fn process_model_knowledge(
    mut form_data: Value,
    knowledge: &[crate::services::models::KnowledgeItem],
) -> AppResult<Value> {
    let mut knowledge_files = Vec::new();

    for item in knowledge {
        if let Some(collection_name) = &item.collection_name {
            knowledge_files.push(json!({
                "id": collection_name,
                "name": item.name.as_deref().unwrap_or(collection_name),
                "legacy": true
            }));
        } else if let Some(collection_names) = &item.collection_names {
            knowledge_files.push(json!({
                "name": item.name.as_deref().unwrap_or("Knowledge"),
                "type": "collection",
                "collection_names": collection_names,
                "legacy": true
            }));
        }
    }

    if !knowledge_files.is_empty() {
        let existing_files = form_data
            .get("files")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();

        let mut all_files = existing_files;
        all_files.extend(knowledge_files);

        if let Some(obj) = form_data.as_object_mut() {
            obj.insert("files".to_string(), Value::Array(all_files));
        }
    }

    Ok(form_data)
}

/// Process pipeline inlet filters
async fn process_pipeline_inlet_filters(
    http_client: &reqwest::Client,
    mut form_data: Value,
    user: &crate::models::user::User,
    model: &crate::services::models::Model,
    config: &crate::config::Config,
) -> AppResult<Value> {
    // Get sorted pipeline filters for this model
    let filters = get_sorted_pipeline_filters(model, config)?;

    for filter in filters {
        // Get URL index for this filter
        let url_idx = filter.get("urlIdx").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        if url_idx >= config.openai_api_base_urls.len() {
            continue;
        }

        let base_url = &config.openai_api_base_urls[url_idx];
        let api_key = config
            .openai_api_keys
            .get(url_idx)
            .cloned()
            .unwrap_or_default();

        if api_key.is_empty() {
            continue;
        }

        let filter_id = filter.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let url = format!("{}/{}/filter/inlet", base_url, filter_id);

        let request_data = json!({
            "user": {
                "id": user.id,
                "email": user.email,
                "name": user.name,
                "role": user.role
            },
            "body": form_data
        });

        // Call pipeline filter
        let response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request_data)
            .send()
            .await?;

        if response.status().is_success() {
            form_data = response.json().await?;
        } else {
            let error_text = response.text().await.unwrap_or_default();
            tracing::warn!("Pipeline filter {} failed: {}", filter_id, error_text);
        }
    }

    Ok(form_data)
}

/// Get sorted pipeline filters for a model
fn get_sorted_pipeline_filters(
    model: &crate::services::models::Model,
    _config: &crate::config::Config,
) -> AppResult<Vec<Value>> {
    let mut filters = Vec::new();

    // TODO: Query pipeline models from database or cache
    // For now, check if this model itself is a pipeline
    if let Some(pipeline) = &model.pipeline {
        if pipeline.pipeline_type.as_deref() == Some("filter") {
            // This model is a filter pipeline
            if let Some(info) = &model.info {
                if let Some(params) = &info.params {
                    filters.push(json!({
                        "id": model.id,
                        "urlIdx": params.get("urlIdx").cloned().unwrap_or(json!(0)),
                        "pipeline": pipeline
                    }));
                }
            }
        }
    }

    // Sort by priority
    filters.sort_by(|a, b| {
        let a_priority = a
            .get("pipeline")
            .and_then(|p| p.get("priority"))
            .and_then(|p| p.as_i64())
            .unwrap_or(0);
        let b_priority = b
            .get("pipeline")
            .and_then(|p| p.get("priority"))
            .and_then(|p| p.as_i64())
            .unwrap_or(0);
        a_priority.cmp(&b_priority)
    });

    Ok(filters)
}

/// Process function inlet filters
async fn process_function_inlet_filters(
    db: &Database,
    form_data: Value,
    _user: &crate::models::user::User,
    model: &crate::services::models::Model,
    metadata: &ChatMetadata,
) -> AppResult<Value> {
    // Get filter IDs from model and metadata
    let mut filter_ids = metadata.filter_ids.clone();

    // Add global and model-specific filters
    if let Some(info) = &model.info {
        if let Some(params) = &info.params {
            if let Some(model_filters) = params.get("filter_ids").and_then(|f| f.as_array()) {
                for filter_id in model_filters {
                    if let Some(id) = filter_id.as_str() {
                        filter_ids.push(id.to_string());
                    }
                }
            }
        }
    }

    // Deduplicate
    filter_ids.sort();
    filter_ids.dedup();

    // Query filter functions from database
    for filter_id in filter_ids {
        let filter: Option<Function> = sqlx::query_as::<_, Function>(
            r#"SELECT * FROM "function" WHERE id = $1 AND type = 'filter' AND is_active = true"#,
        )
        .bind(&filter_id)
        .fetch_optional(db.pool())
        .await?;

        if let Some(_filter) = filter {
            // TODO: Load and execute Python filter function
            // This requires Python runtime integration (PyO3)
            // For now, skip function execution
            tracing::debug!(
                "Function filter {} found but execution not yet implemented",
                filter_id
            );
        }
    }

    Ok(form_data)
}

/// Process features (memory, web_search, image_generation, code_interpreter)
async fn process_features(
    mut form_data: Value,
    features: &Value,
    _user: &crate::models::user::User,
    config: &crate::config::Config,
) -> AppResult<Value> {
    // Memory
    if features
        .get("memory")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        // TODO: Implement memory handler
        tracing::debug!("Memory feature enabled but not yet implemented");
    }

    // Web search
    if features
        .get("web_search")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        // TODO: Implement web search handler
        tracing::debug!("Web search feature enabled but not yet implemented");
    }

    // Image generation
    if features
        .get("image_generation")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        // TODO: Implement image generation handler
        tracing::debug!("Image generation feature enabled but not yet implemented");
    }

    // Code interpreter
    if features
        .get("code_interpreter")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && config.enable_code_interpreter
    {
        // Add code interpreter prompt to messages
        let prompt = if let Some(template) = &config.code_interpreter_prompt_template {
            if !template.is_empty() {
                template.as_str()
            } else {
                "You are a helpful assistant with access to code execution capabilities."
            }
        } else {
            "You are a helpful assistant with access to code execution capabilities."
        };

        if let Some(messages) = form_data.get_mut("messages").and_then(|m| m.as_array_mut()) {
            // Add to first user message or create new one
            if let Some(first_user_msg) = messages.iter_mut().find(|m| {
                m.get("role")
                    .and_then(|r| r.as_str())
                    .map(|r| r == "user")
                    .unwrap_or(false)
            }) {
                if let Some(content) = first_user_msg.get_mut("content") {
                    if let Some(content_str) = content.as_str() {
                        *content = json!(format!("{}\n\n{}", prompt, content_str));
                    }
                }
            }
        }
    }

    Ok(form_data)
}

/// Setup tools for function calling
fn setup_tools(
    form_data: Value,
    metadata: &ChatMetadata,
    _model: &crate::services::models::Model,
) -> AppResult<Value> {
    // TODO: Load tool specifications from database
    // TODO: Setup MCP clients if needed
    // TODO: Add tools to form_data based on function_calling mode

    if let Some(tool_ids) = &metadata.tool_ids {
        tracing::debug!("Tools requested: {:?}", tool_ids);
        // TODO: Implement tool loading and setup
    }

    Ok(form_data)
}

/// Process files in the request
fn process_files(mut form_data: Value, metadata: &ChatMetadata) -> AppResult<Value> {
    if let Some(files) = &metadata.files {
        // Deduplicate files
        let mut unique_files: HashMap<String, Value> = HashMap::new();
        for file in files {
            let file_str = serde_json::to_string(file).unwrap_or_default();
            unique_files.insert(file_str.clone(), file.clone());
        }

        let files_array: Vec<Value> = unique_files.values().cloned().collect();

        if let Some(obj) = form_data.as_object_mut() {
            obj.insert("files".to_string(), Value::Array(files_array));
        }
    }

    Ok(form_data)
}

/// Process chat response through outlet filters
pub async fn process_chat_response(
    http_client: &reqwest::Client,
    response: Value,
    user: &crate::models::user::User,
    model: &crate::services::models::Model,
    config: &crate::config::Config,
) -> AppResult<Value> {
    if !config.enable_pipeline_filters {
        return Ok(response);
    }

    // Process through pipeline outlet filters (in reverse order)
    process_pipeline_outlet_filters(http_client, response, user, model, config).await
}

/// Process pipeline outlet filters
async fn process_pipeline_outlet_filters(
    http_client: &reqwest::Client,
    mut response: Value,
    user: &crate::models::user::User,
    model: &crate::services::models::Model,
    config: &crate::config::Config,
) -> AppResult<Value> {
    let mut filters = get_sorted_pipeline_filters(model, config)?;
    filters.reverse(); // Outlet filters run in reverse order

    for filter in filters {
        let url_idx = filter.get("urlIdx").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        if url_idx >= config.openai_api_base_urls.len() {
            continue;
        }

        let base_url = &config.openai_api_base_urls[url_idx];
        let api_key = config
            .openai_api_keys
            .get(url_idx)
            .cloned()
            .unwrap_or_default();

        if api_key.is_empty() {
            continue;
        }

        let filter_id = filter.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let url = format!("{}/{}/filter/outlet", base_url, filter_id);

        let request_data = json!({
            "user": {
                "id": user.id,
                "email": user.email,
                "name": user.name,
                "role": user.role
            },
            "body": response
        });

        let http_response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request_data)
            .send()
            .await?;

        if http_response.status().is_success() {
            response = http_response.json().await?;
        } else {
            tracing::warn!("Pipeline outlet filter {} failed", filter_id);
        }
    }

    Ok(response)
}
