use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::AppError,
    middleware::{AuthMiddleware, AuthUser},
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigUpdate {
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionsConfigResponse {
    #[serde(rename = "ENABLE_DIRECT_CONNECTIONS")]
    enable_direct_connections: bool,
    #[serde(rename = "ENABLE_BASE_MODELS_CACHE")]
    enable_base_models_cache: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelsConfigForm {
    #[serde(rename = "DEFAULT_MODELS")]
    default_models: Option<String>,
    #[serde(rename = "MODEL_ORDER_LIST")]
    model_order_list: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CodeExecutionConfigForm {
    #[serde(rename = "ENABLE_CODE_EXECUTION")]
    enable_code_execution: bool,
    #[serde(rename = "CODE_EXECUTION_ENGINE")]
    code_execution_engine: String,
    #[serde(rename = "CODE_EXECUTION_JUPYTER_URL")]
    code_execution_jupyter_url: Option<String>,
    #[serde(rename = "CODE_EXECUTION_JUPYTER_AUTH")]
    code_execution_jupyter_auth: Option<String>,
    #[serde(rename = "CODE_EXECUTION_JUPYTER_AUTH_TOKEN")]
    code_execution_jupyter_auth_token: Option<String>,
    #[serde(rename = "CODE_EXECUTION_JUPYTER_AUTH_PASSWORD")]
    code_execution_jupyter_auth_password: Option<String>,
    #[serde(rename = "CODE_EXECUTION_JUPYTER_TIMEOUT")]
    code_execution_jupyter_timeout: Option<i32>,
    #[serde(rename = "CODE_EXECUTION_SANDBOX_URL")]
    code_execution_sandbox_url: Option<String>,
    #[serde(rename = "CODE_EXECUTION_SANDBOX_TIMEOUT")]
    code_execution_sandbox_timeout: Option<i32>,
    #[serde(rename = "CODE_EXECUTION_SANDBOX_ENABLE_POOL")]
    code_execution_sandbox_enable_pool: Option<bool>,
    #[serde(rename = "CODE_EXECUTION_SANDBOX_POOL_SIZE")]
    code_execution_sandbox_pool_size: Option<i32>,
    #[serde(rename = "CODE_EXECUTION_SANDBOX_POOL_MAX_REUSE")]
    code_execution_sandbox_pool_max_reuse: Option<i32>,
    #[serde(rename = "CODE_EXECUTION_SANDBOX_POOL_MAX_AGE")]
    code_execution_sandbox_pool_max_age: Option<i32>,
    #[serde(rename = "ENABLE_CODE_INTERPRETER")]
    enable_code_interpreter: bool,
    #[serde(rename = "CODE_INTERPRETER_ENGINE")]
    code_interpreter_engine: String,
    #[serde(rename = "CODE_INTERPRETER_PROMPT_TEMPLATE")]
    code_interpreter_prompt_template: Option<String>,
    #[serde(rename = "CODE_INTERPRETER_JUPYTER_URL")]
    code_interpreter_jupyter_url: Option<String>,
    #[serde(rename = "CODE_INTERPRETER_JUPYTER_AUTH")]
    code_interpreter_jupyter_auth: Option<String>,
    #[serde(rename = "CODE_INTERPRETER_JUPYTER_AUTH_TOKEN")]
    code_interpreter_jupyter_auth_token: Option<String>,
    #[serde(rename = "CODE_INTERPRETER_JUPYTER_AUTH_PASSWORD")]
    code_interpreter_jupyter_auth_password: Option<String>,
    #[serde(rename = "CODE_INTERPRETER_JUPYTER_TIMEOUT")]
    code_interpreter_jupyter_timeout: Option<i32>,
    #[serde(rename = "CODE_INTERPRETER_SANDBOX_URL")]
    code_interpreter_sandbox_url: Option<String>,
    #[serde(rename = "CODE_INTERPRETER_SANDBOX_TIMEOUT")]
    code_interpreter_sandbox_timeout: Option<i32>,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("/", web::get().to(get_configs))
            .route("/", web::post().to(update_configs))
            .route("/export", web::get().to(export_config))
            .route("/import", web::post().to(import_config))
            .route("/features", web::get().to(get_features))
            .route("/banners", web::get().to(get_banners))
            .route("/banners", web::post().to(set_banners))
            .route("/connections", web::get().to(get_connections_config))
            .route("/connections", web::post().to(set_connections_config))
            .route("/code_execution", web::get().to(get_code_execution_config))
            .route("/code_execution", web::post().to(set_code_execution_config))
            .route("/models", web::get().to(get_models_config))
            .route("/models", web::post().to(set_models_config))
            .route("/suggestions", web::post().to(set_default_suggestions))
            .route("/tool_servers", web::get().to(get_tool_servers_config))
            .route("/tool_servers", web::post().to(set_tool_servers_config)),
    );
}

async fn get_configs(
    state: web::Data<AppState>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(json!({
        "status": true,
        "enable_signup": config.enable_signup,
        "enable_login_form": config.enable_login_form,
        "enable_api_key": config.enable_api_key,
        "enable_openai_api": config.enable_openai_api,
        "enable_channels": config.enable_channels,
        "enable_notes": config.enable_notes,
        "enable_community_sharing": config.enable_community_sharing,
        "enable_message_rating": config.enable_message_rating,
        "enable_image_generation": config.enable_image_generation,
        "enable_code_execution": config.enable_code_execution,
        "enable_web_search": config.enable_web_search,
    })))
}

async fn update_configs(
    _state: web::Data<AppState>,
    _user: AuthUser,
    _payload: web::Json<ConfigUpdate>,
) -> Result<HttpResponse, AppError> {
    // TODO: Implement config updates
    Err(AppError::NotImplemented(
        "Config update not implemented yet".to_string(),
    ))
}

async fn export_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can export config
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    // Export all config as JSON
    Ok(HttpResponse::Ok().json(serde_json::to_value(&*config).unwrap()))
}

#[derive(Debug, Deserialize)]
struct ImportConfigForm {
    config: serde_json::Value,
}

async fn import_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    _form_data: web::Json<ImportConfigForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can import config
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // TODO: Implement config import from JSON
    // For now, just return the current config
    let config = state.config.read().unwrap();
    Ok(HttpResponse::Ok().json(serde_json::to_value(&*config).unwrap()))
}

async fn get_features(
    state: web::Data<AppState>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(json!({
        "enable_signup": config.enable_signup,
        "enable_login_form": config.enable_login_form,
        "enable_api_key": config.enable_api_key,
        "enable_channels": config.enable_channels,
        "enable_notes": config.enable_notes,
        "enable_image_generation": config.enable_image_generation,
        "enable_code_execution": config.enable_code_execution,
        "enable_web_search": config.enable_web_search,
        "enable_admin_chat_access": config.enable_admin_chat_access,
        "enable_admin_export": config.enable_admin_export,
        "enable_community_sharing": config.enable_community_sharing,
        "enable_message_rating": config.enable_message_rating,
    })))
}

async fn get_banners(
    state: web::Data<AppState>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();
    Ok(HttpResponse::Ok().json(&config.banners))
}

#[derive(Debug, Deserialize)]
struct SetBannersForm {
    banners: serde_json::Value,
}

async fn set_banners(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<SetBannersForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can set banners
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();
        config.banners = form_data.banners.clone();
    }

    // Persist to database (best-effort)
    let config = state.config.read().unwrap();
    let ui_json = serde_json::json!({
        "banners": config.banners
    });
    let _ = crate::services::ConfigService::update_section(&state.db, "ui", ui_json).await;

    Ok(HttpResponse::Ok().json(&config.banners))
}

async fn get_connections_config(
    state: web::Data<AppState>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(ConnectionsConfigResponse {
        enable_direct_connections: config.enable_direct_connections,
        enable_base_models_cache: config.enable_base_models_cache,
    }))
}

async fn set_connections_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<ConnectionsConfigResponse>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update connections config
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config (matching Python's behavior exactly)
    {
        let mut config = state.config.write().unwrap();
        config.enable_direct_connections = payload.enable_direct_connections;
        config.enable_base_models_cache = payload.enable_base_models_cache;
    }

    // Try to persist to database, but don't fail if it doesn't work
    // (Python's PersistentConfig.save() is also best-effort)
    let config_json = serde_json::json!({
        "enable_direct_connections": payload.enable_direct_connections,
        "enable_base_models_cache": payload.enable_base_models_cache
    });

    if let Err(e) =
        crate::services::ConfigService::update_section(&state.db, "connections", config_json).await
    {
        tracing::warn!("Failed to persist connections config to database: {}", e);
    }

    // Also update the "direct" section for backward compatibility
    let direct_json = serde_json::json!({
        "enable": payload.enable_direct_connections
    });
    if let Err(e) =
        crate::services::ConfigService::update_section(&state.db, "direct", direct_json).await
    {
        tracing::warn!("Failed to persist direct config to database: {}", e);
    }

    // Return success even if DB save failed (config is updated in memory)
    let config = state.config.read().unwrap();
    Ok(HttpResponse::Ok().json(ConnectionsConfigResponse {
        enable_direct_connections: config.enable_direct_connections,
        enable_base_models_cache: config.enable_base_models_cache,
    }))
}

async fn get_code_execution_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(CodeExecutionConfigForm {
        enable_code_execution: config.enable_code_execution,
        code_execution_engine: config.code_execution_engine.clone(),
        code_execution_jupyter_url: config.code_execution_jupyter_url.clone(),
        code_execution_jupyter_auth: config.code_execution_jupyter_auth.clone(),
        code_execution_jupyter_auth_token: config.code_execution_jupyter_auth_token.clone(),
        code_execution_jupyter_auth_password: config.code_execution_jupyter_auth_password.clone(),
        code_execution_jupyter_timeout: config.code_execution_jupyter_timeout,
        enable_code_interpreter: config.enable_code_interpreter,
        code_interpreter_engine: config.code_interpreter_engine.clone(),
        code_interpreter_prompt_template: config.code_interpreter_prompt_template.clone(),
        code_interpreter_jupyter_url: config.code_interpreter_jupyter_url.clone(),
        code_interpreter_jupyter_auth: config.code_interpreter_jupyter_auth.clone(),
        code_interpreter_jupyter_auth_token: config.code_interpreter_jupyter_auth_token.clone(),
        code_interpreter_jupyter_auth_password: config
            .code_interpreter_jupyter_auth_password
            .clone(),
        code_interpreter_jupyter_timeout: config.code_interpreter_jupyter_timeout,
        code_execution_sandbox_url: config.code_execution_sandbox_url.clone(),
        code_execution_sandbox_timeout: config.code_execution_sandbox_timeout,
        code_execution_sandbox_enable_pool: config.code_execution_sandbox_enable_pool,
        code_execution_sandbox_pool_size: config.code_execution_sandbox_pool_size,
        code_execution_sandbox_pool_max_reuse: config.code_execution_sandbox_pool_max_reuse,
        code_execution_sandbox_pool_max_age: config.code_execution_sandbox_pool_max_age,
        code_interpreter_sandbox_url: config.code_interpreter_sandbox_url.clone(),
        code_interpreter_sandbox_timeout: config.code_interpreter_sandbox_timeout,
    }))
}

async fn set_code_execution_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<CodeExecutionConfigForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();

        config.enable_code_execution = form_data.enable_code_execution;
        config.code_execution_engine = form_data.code_execution_engine.clone();
        config.code_execution_jupyter_url = form_data.code_execution_jupyter_url.clone();
        config.code_execution_jupyter_auth = form_data.code_execution_jupyter_auth.clone();
        config.code_execution_jupyter_auth_token =
            form_data.code_execution_jupyter_auth_token.clone();
        config.code_execution_jupyter_auth_password =
            form_data.code_execution_jupyter_auth_password.clone();
        config.code_execution_jupyter_timeout = form_data.code_execution_jupyter_timeout;
        config.code_execution_sandbox_url = form_data.code_execution_sandbox_url.clone();
        config.code_execution_sandbox_timeout = form_data.code_execution_sandbox_timeout;
        config.code_execution_sandbox_enable_pool = form_data.code_execution_sandbox_enable_pool;
        config.code_execution_sandbox_pool_size = form_data.code_execution_sandbox_pool_size;
        config.code_execution_sandbox_pool_max_reuse =
            form_data.code_execution_sandbox_pool_max_reuse;
        config.code_execution_sandbox_pool_max_age = form_data.code_execution_sandbox_pool_max_age;

        config.enable_code_interpreter = form_data.enable_code_interpreter;
        config.code_interpreter_engine = form_data.code_interpreter_engine.clone();
        config.code_interpreter_prompt_template =
            form_data.code_interpreter_prompt_template.clone();
        config.code_interpreter_jupyter_url = form_data.code_interpreter_jupyter_url.clone();
        config.code_interpreter_jupyter_auth = form_data.code_interpreter_jupyter_auth.clone();
        config.code_interpreter_jupyter_auth_token =
            form_data.code_interpreter_jupyter_auth_token.clone();
        config.code_interpreter_jupyter_auth_password =
            form_data.code_interpreter_jupyter_auth_password.clone();
        config.code_interpreter_jupyter_timeout = form_data.code_interpreter_jupyter_timeout;
        config.code_interpreter_sandbox_url = form_data.code_interpreter_sandbox_url.clone();
        config.code_interpreter_sandbox_timeout = form_data.code_interpreter_sandbox_timeout;
    }

    // Persist to database (best-effort, like Python)
    let config = state.config.read().unwrap();
    let code_execution_json = serde_json::json!({
        "engine": config.code_execution_engine,
        "jupyter_url": config.code_execution_jupyter_url,
        "jupyter_auth": config.code_execution_jupyter_auth,
        "jupyter_auth_token": config.code_execution_jupyter_auth_token,
        "jupyter_auth_password": config.code_execution_jupyter_auth_password,
        "jupyter_timeout": config.code_execution_jupyter_timeout,
        "sandbox_url": config.code_execution_sandbox_url,
        "sandbox_timeout": config.code_execution_sandbox_timeout,
        "sandbox_enable_pool": config.code_execution_sandbox_enable_pool,
        "sandbox_pool_size": config.code_execution_sandbox_pool_size,
        "sandbox_pool_max_reuse": config.code_execution_sandbox_pool_max_reuse,
        "sandbox_pool_max_age": config.code_execution_sandbox_pool_max_age
    });
    let _ = crate::services::ConfigService::update_section(
        &state.db,
        "code_execution",
        code_execution_json,
    )
    .await;

    let code_interpreter_json = serde_json::json!({
        "engine": config.code_interpreter_engine,
        "prompt_template": config.code_interpreter_prompt_template,
        "jupyter_url": config.code_interpreter_jupyter_url,
        "jupyter_auth": config.code_interpreter_jupyter_auth,
        "jupyter_auth_token": config.code_interpreter_jupyter_auth_token,
        "jupyter_auth_password": config.code_interpreter_jupyter_auth_password,
        "jupyter_timeout": config.code_interpreter_jupyter_timeout,
        "sandbox_url": config.code_interpreter_sandbox_url,
        "sandbox_timeout": config.code_interpreter_sandbox_timeout
    });
    let _ = crate::services::ConfigService::update_section(
        &state.db,
        "code_interpreter",
        code_interpreter_json,
    )
    .await;

    let features_json = serde_json::json!({
        "enable_code_execution": config.enable_code_execution,
        "enable_code_interpreter": config.enable_code_interpreter
    });
    let _ =
        crate::services::ConfigService::update_section(&state.db, "features", features_json).await;

    Ok(HttpResponse::Ok().json(CodeExecutionConfigForm {
        enable_code_execution: config.enable_code_execution,
        code_execution_engine: config.code_execution_engine.clone(),
        code_execution_jupyter_url: config.code_execution_jupyter_url.clone(),
        code_execution_jupyter_auth: config.code_execution_jupyter_auth.clone(),
        code_execution_jupyter_auth_token: config.code_execution_jupyter_auth_token.clone(),
        code_execution_jupyter_auth_password: config.code_execution_jupyter_auth_password.clone(),
        code_execution_jupyter_timeout: config.code_execution_jupyter_timeout,
        code_execution_sandbox_url: config.code_execution_sandbox_url.clone(),
        code_execution_sandbox_timeout: config.code_execution_sandbox_timeout,
        code_execution_sandbox_enable_pool: config.code_execution_sandbox_enable_pool,
        code_execution_sandbox_pool_size: config.code_execution_sandbox_pool_size,
        code_execution_sandbox_pool_max_reuse: config.code_execution_sandbox_pool_max_reuse,
        code_execution_sandbox_pool_max_age: config.code_execution_sandbox_pool_max_age,
        enable_code_interpreter: config.enable_code_interpreter,
        code_interpreter_engine: config.code_interpreter_engine.clone(),
        code_interpreter_prompt_template: config.code_interpreter_prompt_template.clone(),
        code_interpreter_jupyter_url: config.code_interpreter_jupyter_url.clone(),
        code_interpreter_jupyter_auth: config.code_interpreter_jupyter_auth.clone(),
        code_interpreter_jupyter_auth_token: config.code_interpreter_jupyter_auth_token.clone(),
        code_interpreter_jupyter_auth_password: config
            .code_interpreter_jupyter_auth_password
            .clone(),
        code_interpreter_jupyter_timeout: config.code_interpreter_jupyter_timeout,
        code_interpreter_sandbox_url: config.code_interpreter_sandbox_url.clone(),
        code_interpreter_sandbox_timeout: config.code_interpreter_sandbox_timeout,
    }))
}

async fn get_models_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(ModelsConfigForm {
        default_models: Some(config.default_models.clone()),
        model_order_list: Some(config.model_order_list.clone()),
    }))
}

async fn set_models_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<ModelsConfigForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();

        if let Some(default_models) = &form_data.default_models {
            config.default_models = default_models.clone();
        }
        if let Some(model_order_list) = &form_data.model_order_list {
            config.model_order_list = model_order_list.clone();
        }
    }

    // Persist to database (best-effort)
    let config = state.config.read().unwrap();
    let models_json = serde_json::json!({
        "default_models": config.default_models,
        "model_order_list": config.model_order_list
    });
    let _ = crate::services::ConfigService::update_section(&state.db, "models", models_json).await;

    Ok(HttpResponse::Ok().json(ModelsConfigForm {
        default_models: Some(config.default_models.clone()),
        model_order_list: Some(config.model_order_list.clone()),
    }))
}

#[derive(Debug, Deserialize)]
struct SetDefaultSuggestionsForm {
    suggestions: serde_json::Value,
}

async fn set_default_suggestions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<SetDefaultSuggestionsForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can set suggestions
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();
        config.default_prompt_suggestions = form_data.suggestions.clone();
    }

    // Persist to database (best-effort)
    let config = state.config.read().unwrap();
    let ui_json = serde_json::json!({
        "default_prompt_suggestions": config.default_prompt_suggestions
    });
    let _ = crate::services::ConfigService::update_section(&state.db, "ui", ui_json).await;

    Ok(HttpResponse::Ok().json(&config.default_prompt_suggestions))
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolServersConfigForm {
    #[serde(rename = "TOOL_SERVER_CONNECTIONS")]
    tool_server_connections: serde_json::Value,
}

async fn get_tool_servers_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(ToolServersConfigForm {
        tool_server_connections: config.tool_server_connections.clone(),
    }))
}

async fn set_tool_servers_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<ToolServersConfigForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();
        config.tool_server_connections = form_data.tool_server_connections.clone();
    }

    // Persist to database (best-effort)
    let config = state.config.read().unwrap();
    let tool_servers_json = serde_json::json!({
        "connections": config.tool_server_connections
    });
    let _ = crate::services::ConfigService::update_section(
        &state.db,
        "tool_servers",
        tool_servers_json,
    )
    .await;

    Ok(HttpResponse::Ok().json(ToolServersConfigForm {
        tool_server_connections: config.tool_server_connections.clone(),
    }))
}
