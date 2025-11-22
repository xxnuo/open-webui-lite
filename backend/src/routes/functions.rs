use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::services::function::FunctionService;
use crate::AppState;

#[derive(Debug, Serialize)]
struct FunctionResponse {
    id: String,
    user_id: String,
    name: String,
    #[serde(rename = "type")]
    function_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    meta: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_global: Option<bool>,
    updated_at: i64,
    created_at: i64,
}

#[derive(Debug, Deserialize, Validate)]
struct FunctionForm {
    #[validate(length(min = 1))]
    id: String,
    #[validate(length(min = 1))]
    name: String,
    content: String,
    meta: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct SyncFunctionsForm {
    functions: Vec<FunctionWithValves>,
}

#[derive(Debug, Deserialize)]
struct FunctionWithValves {
    id: String,
    user_id: String,
    name: String,
    #[serde(rename = "type")]
    function_type: String,
    content: String,
    meta: serde_json::Value,
    #[serde(default)]
    valves: Option<serde_json::Value>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    is_global: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LoadUrlForm {
    url: String,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_functions)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_functions)),
    )
    .service(
        web::resource("/export")
            .wrap(AuthMiddleware)
            .route(web::get().to(export_functions)),
    )
    .service(
        web::resource("/load/url")
            .wrap(AuthMiddleware)
            .route(web::post().to(load_function_from_url)),
    )
    .service(
        web::resource("/sync")
            .wrap(AuthMiddleware)
            .route(web::post().to(sync_functions)),
    )
    .service(
        web::resource("/create")
            .wrap(AuthMiddleware)
            .route(web::post().to(create_new_function)),
    )
    .service(
        web::resource("/id/{id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_function_by_id)),
    )
    .service(
        web::resource("/id/{id}/toggle")
            .wrap(AuthMiddleware)
            .route(web::post().to(toggle_function_by_id)),
    )
    .service(
        web::resource("/id/{id}/toggle/global")
            .wrap(AuthMiddleware)
            .route(web::post().to(toggle_global_by_id)),
    )
    .service(
        web::resource("/id/{id}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_function_by_id)),
    )
    .service(
        web::resource("/id/{id}/delete")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_function_by_id)),
    )
    .service(
        web::resource("/id/{id}/valves")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_function_valves)),
    )
    .service(
        web::resource("/id/{id}/valves/spec")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_function_valves_spec)),
    )
    .service(
        web::resource("/id/{id}/valves/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_function_valves)),
    )
    .service(
        web::resource("/id/{id}/valves/user")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_function_user_valves)),
    )
    .service(
        web::resource("/id/{id}/valves/user/spec")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_function_user_valves_spec)),
    )
    .service(
        web::resource("/id/{id}/valves/user/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_function_user_valves)),
    );
}

async fn get_functions(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let function_service = FunctionService::new(&state.db);
    let functions = function_service.get_all_functions().await?;

    let response: Vec<FunctionResponse> = functions
        .iter()
        .map(|func| FunctionResponse {
            id: func.id.clone(),
            user_id: func.user_id.clone(),
            name: func.name.clone(),
            function_type: func.type_.clone(),
            content: None, // Don't return content in list view
            meta: func.meta.clone().unwrap_or(json!({"description": ""})),
            is_active: Some(func.is_active),
            is_global: Some(func.is_global),
            updated_at: func.updated_at,
            created_at: func.created_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn export_functions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let functions = function_service.get_all_functions().await?;

    Ok(HttpResponse::Ok().json(functions))
}

async fn load_function_from_url(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
    form: web::Json<LoadUrlForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let url = form.url.clone();
    if url.is_empty() {
        return Err(AppError::BadRequest("Please enter a valid URL".to_string()));
    }

    // Transform GitHub URLs to raw content URLs
    let url = github_url_to_raw_url(&url);
    let url_parts: Vec<&str> = url.trim_end_matches('/').split('/').collect();

    let file_name = url_parts.last().unwrap_or(&"function");
    let function_name = if file_name.ends_with(".py")
        && !file_name.starts_with("main.py")
        && !file_name.starts_with("index.py")
        && !file_name.starts_with("__init__.py")
    {
        file_name.trim_end_matches(".py")
    } else if url_parts.len() > 1 {
        url_parts[url_parts.len() - 2]
    } else {
        "function"
    };

    // Fetch content from URL
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::BadRequest(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to fetch URL: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "Failed to fetch function: HTTP {}",
            response.status()
        )));
    }

    let content = response
        .text()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read response: {}", e)))?;

    if content.is_empty() {
        return Err(AppError::BadRequest(
            "No data received from the URL".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(json!({
        "name": function_name,
        "content": content,
    })))
}

fn github_url_to_raw_url(url: &str) -> String {
    // Handle 'tree' (folder) URLs
    if let Some(caps) = regex::Regex::new(r"https://github\.com/([^/]+)/([^/]+)/tree/([^/]+)/(.*)")
        .unwrap()
        .captures(url)
    {
        let org = &caps[1];
        let repo = &caps[2];
        let branch = &caps[3];
        let path = caps[4].trim_end_matches('/');
        return format!(
            "https://raw.githubusercontent.com/{}/{}/refs/heads/{}/{}/main.py",
            org, repo, branch, path
        );
    }

    // Handle 'blob' (file) URLs
    if let Some(caps) = regex::Regex::new(r"https://github\.com/([^/]+)/([^/]+)/blob/([^/]+)/(.*)")
        .unwrap()
        .captures(url)
    {
        let org = &caps[1];
        let repo = &caps[2];
        let branch = &caps[3];
        let path = &caps[4];
        return format!(
            "https://raw.githubusercontent.com/{}/{}/refs/heads/{}/{}",
            org, repo, branch, path
        );
    }

    url.to_string()
}

async fn sync_functions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form: web::Json<SyncFunctionsForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);

    // Sync each function
    for func in &form.functions {
        // Check if function exists
        if let Some(_existing) = function_service.get_function_by_id(&func.id).await? {
            // Update existing function
            function_service
                .update_function(
                    &func.id,
                    Some(&func.name),
                    Some(&func.function_type),
                    Some(&func.content),
                    Some(func.meta.clone()),
                    func.is_active.unwrap_or(true),
                    func.is_global.unwrap_or(false),
                )
                .await?;

            // Update valves separately if provided
            if let Some(ref valves) = func.valves {
                function_service
                    .update_function_valves(&func.id, valves.clone())
                    .await?;
            }
        } else {
            // Create new function
            function_service
                .create_function(
                    &func.id,
                    &auth_user.user.id,
                    &func.name,
                    &func.function_type,
                    &func.content,
                    func.meta.clone(),
                    func.is_active.unwrap_or(true),
                    func.is_global.unwrap_or(false),
                )
                .await?;

            // Update valves separately if provided
            if let Some(ref valves) = func.valves {
                function_service
                    .update_function_valves(&func.id, valves.clone())
                    .await?;
            }
        }
    }

    // Return all functions with valves
    let functions = function_service.get_all_functions().await?;
    Ok(HttpResponse::Ok().json(functions))
}

async fn create_new_function(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form: web::Json<FunctionForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    form.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Validate ID contains only alphanumeric and underscores
    if !form.id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(AppError::BadRequest(
            "Only alphanumeric characters and underscores are allowed in the id".to_string(),
        ));
    }

    let function_service = FunctionService::new(&state.db);

    // Check if function already exists
    if function_service
        .get_function_by_id(&form.id.to_lowercase())
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("Function ID already exists".to_string()));
    }

    // Create function with default type "pipe"
    let function = function_service
        .create_function(
            &form.id.to_lowercase(),
            &auth_user.user.id,
            &form.name,
            "pipe", // Default type
            &form.content,
            form.meta.clone(),
            true,  // is_active
            false, // is_global
        )
        .await?;

    let response = FunctionResponse {
        id: function.id.clone(),
        user_id: function.user_id.clone(),
        name: function.name.clone(),
        function_type: function.type_.clone(),
        content: Some(function.content.clone()),
        meta: function.meta.clone().unwrap_or(json!({})),
        is_active: Some(function.is_active),
        is_global: Some(function.is_global),
        updated_at: function.updated_at,
        created_at: function.created_at,
    };

    Ok(HttpResponse::Ok().json(response))
}

async fn get_function_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    Ok(HttpResponse::Ok().json(function))
}

async fn toggle_function_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    let new_is_active = !function.is_active;
    let updated_function = function_service
        .update_function(
            &id,
            None,
            None,
            None,
            None,
            new_is_active,
            function.is_global,
        )
        .await?;

    Ok(HttpResponse::Ok().json(updated_function))
}

async fn toggle_global_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    let new_is_global = !function.is_global;
    let updated_function = function_service
        .update_function(
            &id,
            None,
            None,
            None,
            None,
            function.is_active,
            new_is_global,
        )
        .await?;

    Ok(HttpResponse::Ok().json(updated_function))
}

async fn update_function_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<FunctionForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let _function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    let updated_function = function_service
        .update_function(
            &id,
            Some(&form.name),
            Some("pipe"), // Default type
            Some(&form.content),
            Some(form.meta.clone()),
            true,  // Keep active
            false, // Keep not global
        )
        .await?;

    Ok(HttpResponse::Ok().json(updated_function))
}

async fn delete_function_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let _function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    function_service.delete_function(&id).await?;

    Ok(HttpResponse::Ok().json(true))
}

async fn get_function_valves(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    let valves = function.valves.unwrap_or(json!({}));
    Ok(HttpResponse::Ok().json(valves))
}

async fn get_function_valves_spec(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // TODO: Implement function module loading and Valves spec extraction
    Ok(HttpResponse::Ok().json(json!(null)))
}

async fn update_function_valves(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    valves: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let function_service = FunctionService::new(&state.db);
    let _function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    let valves_data = valves.into_inner();
    function_service
        .update_function_valves(&id, valves_data.clone())
        .await?;
    Ok(HttpResponse::Ok().json(valves_data))
}

async fn get_function_user_valves(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let function_service = FunctionService::new(&state.db);
    let _function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    // TODO: Get user valves from user settings
    Ok(HttpResponse::Ok().json(json!({})))
}

async fn get_function_user_valves_spec(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    // TODO: Implement UserValves spec extraction
    Ok(HttpResponse::Ok().json(json!(null)))
}

async fn update_function_user_valves(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
    valves: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let function_service = FunctionService::new(&state.db);
    let _function = function_service
        .get_function_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Function not found".to_string()))?;

    // TODO: Update user valves in user settings
    let valves_data = valves.into_inner();
    Ok(HttpResponse::Ok().json(valves_data))
}
