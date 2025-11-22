use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::tool::ToolUserResponse;
use crate::models::tool_runtime::{ExecutionContext, ToolExecutionRequest, UserContext};
use crate::services::group::GroupService;
use crate::services::tool::ToolService;
use crate::services::tool_runtime::ToolRuntimeService;
use crate::services::user::UserService;
use crate::utils::misc::{has_access, has_permission};
use crate::AppState;

/// Parse JSON tool definition and extract OpenAI-compatible function specs
fn parse_json_tool_specs(content: &str) -> AppResult<Value> {
    // Parse the JSON content
    let tool_def: Value = serde_json::from_str(content)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON format: {}", e)))?;

    // Extract tools array
    let tools = tool_def
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| {
            AppError::BadRequest("Missing 'tools' array in JSON definition".to_string())
        })?;

    // Convert each tool to OpenAI function calling spec
    let specs: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unnamed_tool");
            let description = tool
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");

            // Extract parameters
            let parameters = tool.get("parameters").cloned().unwrap_or_else(|| json!({}));

            // Convert our parameter format to OpenAI format
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();

            if let Some(params_obj) = parameters.as_object() {
                for (param_name, param_def) in params_obj {
                    if let Some(param_obj) = param_def.as_object() {
                        let mut prop = serde_json::Map::new();

                        // Type
                        if let Some(type_val) = param_obj.get("type") {
                            prop.insert("type".to_string(), type_val.clone());
                        }

                        // Description
                        if let Some(desc_val) = param_obj.get("description") {
                            prop.insert("description".to_string(), desc_val.clone());
                        }

                        // Enum values
                        if let Some(enum_val) = param_obj.get("enum") {
                            prop.insert("enum".to_string(), enum_val.clone());
                        }

                        properties.insert(param_name.clone(), Value::Object(prop));

                        // Check if required
                        if param_obj
                            .get("required")
                            .and_then(|r| r.as_bool())
                            .unwrap_or(false)
                        {
                            required.push(param_name.clone());
                        }
                    }
                }
            }

            // Build OpenAI function spec
            json!({
                "name": name,
                "description": description,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required
                }
            })
        })
        .collect();

    Ok(Value::Array(specs))
}

#[derive(Debug, Deserialize, Validate)]
struct ToolForm {
    #[validate(length(min = 1))]
    id: String,
    #[validate(length(min = 1))]
    name: String,
    content: String,
    meta: serde_json::Value,
    #[serde(default)]
    access_control: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct LoadUrlForm {
    url: String,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tools)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tools)),
    )
    .service(
        web::resource("/list")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_list)),
    )
    .service(
        web::resource("/export")
            .wrap(AuthMiddleware)
            .route(web::get().to(export_tools)),
    )
    .service(
        web::resource("/create")
            .wrap(AuthMiddleware)
            .route(web::post().to(create_new_tool)),
    )
    .service(
        web::resource("/id/{id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_by_id)),
    )
    .service(
        web::resource("/id/{id}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_tool_by_id)),
    )
    .service(
        web::resource("/id/{id}/delete")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_tool_by_id)),
    )
    .service(
        web::resource("/id/{id}/valves")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_valves)),
    )
    .service(
        web::resource("/id/{id}/valves/spec")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_valves_spec)),
    )
    .service(
        web::resource("/id/{id}/valves/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_tool_valves)),
    )
    .service(
        web::resource("/id/{id}/valves/user")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_user_valves)),
    )
    .service(
        web::resource("/id/{id}/valves/user/spec")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_user_valves_spec)),
    )
    .service(
        web::resource("/id/{id}/valves/user/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_tool_user_valves)),
    )
    .service(
        web::resource("/load/url")
            .wrap(AuthMiddleware)
            .route(web::post().to(load_tool_from_url)),
    )
    .service(
        web::resource("/id/{id}/execute")
            .wrap(AuthMiddleware)
            .route(web::post().to(execute_tool)),
    )
    .service(
        web::resource("/id/{id}/chain")
            .wrap(AuthMiddleware)
            .route(web::post().to(execute_tool_chain)),
    )
    .service(
        web::resource("/import")
            .wrap(AuthMiddleware)
            .route(web::post().to(import_tools)),
    )
    .service(
        web::resource("/id/{id}/test")
            .wrap(AuthMiddleware)
            .route(web::post().to(test_tool)),
    )
    .service(
        web::resource("/library")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_library)),
    )
    .service(
        web::resource("/library/{id}")
            .wrap(AuthMiddleware)
            .route(web::post().to(install_from_library)),
    )
    .service(
        web::resource("/schema")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_tool_schema)),
    )
    .service(
        web::resource("/builder/templates")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_builder_templates)),
    )
    .service(
        web::resource("/builder/generate")
            .wrap(AuthMiddleware)
            .route(web::post().to(generate_from_builder)),
    );
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

// POST /load/url - Load tool from URL (admin only)
async fn load_tool_from_url(
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

    let file_name = url_parts.last().unwrap_or(&"tool");
    let tool_name = if file_name.ends_with(".py")
        && !file_name.starts_with("main.py")
        && !file_name.starts_with("index.py")
        && !file_name.starts_with("__init__.py")
    {
        file_name.trim_end_matches(".py")
    } else if url_parts.len() > 1 {
        url_parts[url_parts.len() - 2]
    } else {
        "tool"
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
            "Failed to fetch tool: HTTP {}",
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
        "name": tool_name,
        "content": content,
    })))
}

// GET / - Get all tools with access filtering
async fn get_tools(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);
    let user_service = UserService::new(&state.db);

    let config = state.config.read().unwrap();
    let bypass_admin_access = config.bypass_admin_access_control.unwrap_or(false);
    drop(config);

    let tools = if auth_user.user.role == "admin" && bypass_admin_access {
        tool_service.get_all_tools().await?
    } else {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        let all_tools = tool_service.get_all_tools().await?;
        all_tools
            .into_iter()
            .filter(|t| {
                t.user_id == auth_user.user.id
                    || has_access(
                        &auth_user.user.id,
                        "read",
                        &t.get_access_control(),
                        &user_group_ids,
                    )
            })
            .collect()
    };

    // Get unique user IDs
    let user_ids: HashSet<String> = tools.iter().map(|t| t.user_id.clone()).collect();

    // Fetch users
    let mut users_map: HashMap<String, serde_json::Value> = HashMap::new();
    for user_id in user_ids {
        if let Ok(Some(user)) = user_service.get_user_by_id(&user_id).await {
            users_map.insert(
                user_id.clone(),
                json!({
                    "id": user.id,
                    "name": user.name,
                    "email": user.email,
                    "role": user.role,
                    "profile_image_url": user.profile_image_url,
                }),
            );
        }
    }

    // TODO: Implement OpenAPI Tool Servers integration
    // TODO: Implement MCP Tool Servers integration

    let response: Vec<ToolUserResponse> = tools
        .into_iter()
        .map(|t| {
            let user = users_map.get(&t.user_id).cloned();
            // TODO: Implement UserValves detection
            ToolUserResponse::from_tool_and_user(t, user, Some(false))
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

// GET /list - Get tool list (with write access)
async fn get_tool_list(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);
    let user_service = UserService::new(&state.db);

    let config = state.config.read().unwrap();
    let bypass_admin_access = config.bypass_admin_access_control.unwrap_or(false);
    drop(config);

    let tools = if auth_user.user.role == "admin" && bypass_admin_access {
        tool_service.get_all_tools().await?
    } else {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        let all_tools = tool_service.get_all_tools().await?;
        all_tools
            .into_iter()
            .filter(|t| {
                t.user_id == auth_user.user.id
                    || has_access(
                        &auth_user.user.id,
                        "write",
                        &t.get_access_control(),
                        &user_group_ids,
                    )
            })
            .collect()
    };

    // Get unique user IDs
    let user_ids: HashSet<String> = tools.iter().map(|t| t.user_id.clone()).collect();

    // Fetch users
    let mut users_map: HashMap<String, serde_json::Value> = HashMap::new();
    for user_id in user_ids {
        if let Ok(Some(user)) = user_service.get_user_by_id(&user_id).await {
            users_map.insert(
                user_id.clone(),
                json!({
                    "id": user.id,
                    "name": user.name,
                    "email": user.email,
                    "role": user.role,
                    "profile_image_url": user.profile_image_url,
                }),
            );
        }
    }

    let response: Vec<ToolUserResponse> = tools
        .into_iter()
        .map(|t| {
            let user = users_map.get(&t.user_id).cloned();
            ToolUserResponse::from_tool_and_user(t, user, Some(false))
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

// GET /export - Export all tools (admin only)
async fn export_tools(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let tool_service = ToolService::new(&state.db);
    let tools = tool_service.get_all_tools().await?;

    Ok(HttpResponse::Ok().json(tools))
}

// POST /create - Create a new tool
async fn create_new_tool(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form: web::Json<ToolForm>,
) -> AppResult<HttpResponse> {
    // Check workspace.tools permission
    if auth_user.user.role != "admin" {
        let config = state.config.read().unwrap();
        let user_permissions = config.user_permissions.clone();
        drop(config);

        if !has_permission(&auth_user.user.id, "workspace.tools", &user_permissions) {
            return Err(AppError::Unauthorized("Unauthorized".to_string()));
        }
    }

    form.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Validate tool ID (alphanumeric and underscores only)
    if !form.id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(AppError::BadRequest(
            "Only alphanumeric characters and underscores are allowed in the id".to_string(),
        ));
    }

    let tool_id = form.id.to_lowercase();
    let tool_service = ToolService::new(&state.db);

    // Check if tool already exists
    if tool_service.get_tool_by_id(&tool_id).await?.is_some() {
        return Err(AppError::BadRequest("Tool ID already exists".to_string()));
    }

    // Parse JSON tool definition and extract specs
    let specs = parse_json_tool_specs(&form.content)?;

    let tool = tool_service
        .create_tool(
            &tool_id,
            &auth_user.user.id,
            &form.name,
            &form.content,
            specs,
            form.meta.clone(),
            form.access_control.clone(),
        )
        .await?;

    Ok(HttpResponse::Ok().json(tool))
}

// GET /id/{id} - Get tool by ID
async fn get_tool_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check access
    if auth_user.user.role != "admin" && tool.user_id != auth_user.user.id {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "read",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Unauthorized("Tool not found".to_string()));
        }
    }

    Ok(HttpResponse::Ok().json(tool))
}

// POST /id/{id}/update - Update tool by ID
async fn update_tool_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<ToolForm>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check write access
    if tool.user_id != auth_user.user.id && auth_user.user.role != "admin" {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "write",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Unauthorized("Unauthorized".to_string()));
        }
    }

    // Parse JSON tool definition and extract specs
    let specs = parse_json_tool_specs(&form.content)?;

    let updated_tool = tool_service
        .update_tool(
            &id,
            Some(&form.name),
            Some(&form.content),
            Some(specs),
            Some(form.meta.clone()),
            form.access_control.clone(),
        )
        .await?;

    Ok(HttpResponse::Ok().json(updated_tool))
}

// DELETE /id/{id}/delete - Delete tool by ID
async fn delete_tool_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check write access
    if tool.user_id != auth_user.user.id && auth_user.user.role != "admin" {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "write",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Unauthorized("Unauthorized".to_string()));
        }
    }

    tool_service.delete_tool(&id).await?;

    Ok(HttpResponse::Ok().json(true))
}

// GET /id/{id}/valves - Get tool valves
async fn get_tool_valves(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    let valves = tool.valves.unwrap_or_else(|| json!({}));
    Ok(HttpResponse::Ok().json(valves))
}

// GET /id/{id}/valves/spec - Get tool valves spec
async fn get_tool_valves_spec(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    // TODO: Implement tool module loading and Valves spec extraction
    Ok(HttpResponse::Ok().json(json!(null)))
}

// POST /id/{id}/valves/update - Update tool valves
async fn update_tool_valves(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    valves: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check write access
    if tool.user_id != auth_user.user.id && auth_user.user.role != "admin" {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "write",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Forbidden("Access prohibited".to_string()));
        }
    }

    let valves_data = valves.into_inner();
    tool_service
        .update_tool_valves(&id, valves_data.clone())
        .await?;

    Ok(HttpResponse::Ok().json(valves_data))
}

// GET /id/{id}/valves/user - Get tool user valves
async fn get_tool_user_valves(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let _tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // TODO: Get user valves from user settings
    // For now, return empty object
    Ok(HttpResponse::Ok().json(json!({})))
}

// GET /id/{id}/valves/user/spec - Get tool user valves spec
async fn get_tool_user_valves_spec(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    // TODO: Implement UserValves spec extraction
    Ok(HttpResponse::Ok().json(json!(null)))
}

// POST /id/{id}/valves/user/update - Update tool user valves
async fn update_tool_user_valves(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    valves: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    let _tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // TODO: Update user valves in user settings
    let valves_data = valves.into_inner();
    Ok(HttpResponse::Ok().json(valves_data))
}

#[derive(Debug, Deserialize)]
struct ToolExecuteForm {
    tool_name: String,
    parameters: HashMap<String, Value>,
    #[serde(default)]
    environment: HashMap<String, String>,
}

// POST /id/{id}/execute - Execute a tool
async fn execute_tool(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<ToolExecuteForm>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    // Check if tool exists and user has access
    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check access
    if auth_user.user.role != "admin" && tool.user_id != auth_user.user.id {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "read",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Unauthorized("Tool not found".to_string()));
        }
    }

    // Get environment variables from config or form
    let mut environment = form.environment.clone();

    // Add system environment variables if needed
    if let Ok(val) = std::env::var("OPENWEATHER_API_KEY") {
        environment
            .entry("OPENWEATHER_API_KEY".to_string())
            .or_insert(val);
    }

    // Build execution context
    let context = ExecutionContext {
        user: Some(UserContext {
            id: auth_user.user.id.clone(),
            name: auth_user.user.name.clone(),
            email: auth_user.user.email.clone(),
            role: Some(auth_user.user.role.clone()),
        }),
        environment,
        session: HashMap::new(),
    };

    // Build execution request
    let request = ToolExecutionRequest {
        tool_id: id.to_string(),
        tool_name: form.tool_name.clone(),
        parameters: form.parameters.clone(),
        context,
    };

    // Execute tool
    let runtime_service = ToolRuntimeService::new();
    let response = runtime_service.execute_tool(&state.db, request).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct ToolChainExecuteForm {
    chain_name: String,
    initial_parameters: HashMap<String, Value>,
    #[serde(default)]
    environment: HashMap<String, String>,
}

// POST /id/{id}/chain - Execute a tool chain
async fn execute_tool_chain(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<ToolChainExecuteForm>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    // Check if tool exists and user has access
    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check access
    if auth_user.user.role != "admin" && tool.user_id != auth_user.user.id {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "read",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Unauthorized("Tool not found".to_string()));
        }
    }

    // Get environment variables
    let mut environment = form.environment.clone();
    if let Ok(val) = std::env::var("OPENWEATHER_API_KEY") {
        environment
            .entry("OPENWEATHER_API_KEY".to_string())
            .or_insert(val);
    }

    // Build execution context
    let context = ExecutionContext {
        user: Some(UserContext {
            id: auth_user.user.id.clone(),
            name: auth_user.user.name.clone(),
            email: auth_user.user.email.clone(),
            role: Some(auth_user.user.role.clone()),
        }),
        environment,
        session: HashMap::new(),
    };

    // Execute tool chain
    let runtime_service = ToolRuntimeService::new();
    let response = runtime_service
        .execute_tool_chain(
            &state.db,
            &id,
            &form.chain_name,
            form.initial_parameters.clone(),
            context,
        )
        .await?;

    Ok(HttpResponse::Ok().json(response))
}

// POST /import - Import tools from JSON/YAML file (batch import)
#[derive(Debug, Deserialize)]
struct ImportToolsForm {
    tools: Vec<ToolImportItem>,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Deserialize)]
struct ToolImportItem {
    id: String,
    name: String,
    content: String,
    #[serde(default)]
    meta: serde_json::Value,
    #[serde(default)]
    access_control: Option<serde_json::Value>,
}

async fn import_tools(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form: web::Json<ImportToolsForm>,
) -> AppResult<HttpResponse> {
    // Check admin or workspace.tools permission
    if auth_user.user.role != "admin" {
        let config = state.config.read().unwrap();
        let user_permissions = config.user_permissions.clone();
        drop(config);

        if !has_permission(&auth_user.user.id, "workspace.tools", &user_permissions) {
            return Err(AppError::Unauthorized("Unauthorized".to_string()));
        }
    }

    let tool_service = ToolService::new(&state.db);
    let mut results = Vec::new();

    for tool_item in &form.tools {
        // Validate tool ID
        if !tool_item
            .id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            results.push(json!({
                "id": tool_item.id,
                "status": "error",
                "message": "Invalid ID format (alphanumeric and underscores only)"
            }));
            continue;
        }

        let tool_id = tool_item.id.to_lowercase();

        // Check if tool exists
        let existing_tool = tool_service.get_tool_by_id(&tool_id).await?;

        if existing_tool.is_some() && !form.overwrite {
            results.push(json!({
                "id": tool_id,
                "status": "skipped",
                "message": "Tool already exists (use overwrite=true to replace)"
            }));
            continue;
        }

        // Parse JSON tool definition and extract specs
        let specs = match parse_json_tool_specs(&tool_item.content) {
            Ok(s) => s,
            Err(e) => {
                results.push(json!({
                    "id": tool_id,
                    "status": "error",
                    "message": format!("Invalid tool definition: {}", e)
                }));
                continue;
            }
        };

        // Create or update tool
        let result = if existing_tool.is_some() {
            tool_service
                .update_tool(
                    &tool_id,
                    Some(&tool_item.name),
                    Some(&tool_item.content),
                    Some(specs),
                    Some(tool_item.meta.clone()),
                    tool_item.access_control.clone(),
                )
                .await
        } else {
            tool_service
                .create_tool(
                    &tool_id,
                    &auth_user.user.id,
                    &tool_item.name,
                    &tool_item.content,
                    specs,
                    tool_item.meta.clone(),
                    tool_item.access_control.clone(),
                )
                .await
        };

        match result {
            Ok(_) => {
                results.push(json!({
                    "id": tool_id,
                    "status": if existing_tool.is_some() { "updated" } else { "created" },
                    "message": "Success"
                }));
            }
            Err(e) => {
                results.push(json!({
                    "id": tool_id,
                    "status": "error",
                    "message": format!("Database error: {}", e)
                }));
            }
        }
    }

    Ok(HttpResponse::Ok().json(json!({
        "imported": results.iter().filter(|r| r["status"] == "created" || r["status"] == "updated").count(),
        "skipped": results.iter().filter(|r| r["status"] == "skipped").count(),
        "errors": results.iter().filter(|r| r["status"] == "error").count(),
        "results": results
    })))
}

// POST /id/{id}/test - Test tool execution with sample parameters
#[derive(Debug, Deserialize)]
struct TestToolForm {
    tool_name: String,
    parameters: HashMap<String, Value>,
    #[serde(default)]
    environment: HashMap<String, String>,
}

async fn test_tool(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<TestToolForm>,
) -> AppResult<HttpResponse> {
    let tool_service = ToolService::new(&state.db);

    // Check if tool exists and user has access
    let tool = tool_service
        .get_tool_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))?;

    // Check access
    if auth_user.user.role != "admin" && tool.user_id != auth_user.user.id {
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "read",
            &tool.get_access_control(),
            &user_group_ids,
        ) {
            return Err(AppError::Unauthorized("Tool not found".to_string()));
        }
    }

    // Build execution context
    let mut environment = form.environment.clone();

    // Add system environment variables
    for key in &["OPENWEATHER_API_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY"] {
        if let Ok(val) = std::env::var(key) {
            environment.entry(key.to_string()).or_insert(val);
        }
    }

    let context = ExecutionContext {
        user: Some(UserContext {
            id: auth_user.user.id.clone(),
            name: auth_user.user.name.clone(),
            email: auth_user.user.email.clone(),
            role: Some(auth_user.user.role.clone()),
        }),
        environment,
        session: HashMap::new(),
    };

    // Build execution request
    let request = ToolExecutionRequest {
        tool_id: id.to_string(),
        tool_name: form.tool_name.clone(),
        parameters: form.parameters.clone(),
        context,
    };

    // Execute tool
    let start_time = std::time::Instant::now();
    let runtime_service = ToolRuntimeService::new();
    let response = runtime_service.execute_tool(&state.db, request).await?;
    let execution_time = start_time.elapsed();

    Ok(HttpResponse::Ok().json(json!({
        "success": response.success,
        "result": response.result,
        "error": response.error,
        "metadata": response.metadata,
        "test_execution_time_ms": execution_time.as_millis()
    })))
}

// GET /library - Get available tools from built-in library
async fn get_tool_library(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Built-in tool library with common useful tools
    let library = vec![
        json!({
            "id": "weather_tools",
            "name": "Weather Tools",
            "description": "Get weather information using wttr.in API",
            "version": "1.0.0",
            "category": "Utilities",
            "author": "Open WebUI",
            "tags": ["weather", "api", "utilities"],
            "preview": {
                "tools_count": 3,
                "tool_names": ["get_weather", "get_user_info", "get_current_time"]
            }
        }),
        json!({
            "id": "advanced_features",
            "name": "Advanced Tool Features Demo",
            "description": "Demonstrates expression evaluation, tool chaining, conditional execution, error handling, rate limiting, and caching",
            "version": "2.0.0",
            "category": "Examples",
            "author": "Open WebUI",
            "tags": ["advanced", "demo", "chaining", "expressions"],
            "preview": {
                "tools_count": 7,
                "tool_names": ["calculator", "get_weather", "backup_weather_service", "reliable_weather", "analyze_temperature", "format_weather_report", "get_current_time"]
            }
        }),
        json!({
            "id": "http_api_template",
            "name": "HTTP API Tool Template",
            "description": "Template for creating custom HTTP API integrations",
            "version": "1.0.0",
            "category": "Templates",
            "author": "Open WebUI",
            "tags": ["template", "http", "api"],
            "preview": {
                "tools_count": 1,
                "tool_names": ["custom_api"]
            }
        }),
        json!({
            "id": "calculator",
            "name": "Calculator & Expressions",
            "description": "Safe mathematical and logical expression evaluator",
            "version": "1.0.0",
            "category": "Utilities",
            "author": "Open WebUI",
            "tags": ["calculator", "math", "expressions"],
            "preview": {
                "tools_count": 2,
                "tool_names": ["calculator", "advanced_calculator"]
            }
        }),
        json!({
            "id": "context_tools",
            "name": "Context & User Info Tools",
            "description": "Access user context, session data, and system information",
            "version": "1.0.0",
            "category": "System",
            "author": "Open WebUI",
            "tags": ["context", "user", "system"],
            "preview": {
                "tools_count": 3,
                "tool_names": ["get_user_info", "get_session_data", "get_system_time"]
            }
        }),
    ];

    Ok(HttpResponse::Ok().json(json!({
        "library": library,
        "total": library.len()
    })))
}

// POST /library/{id} - Install a tool from the library
async fn install_from_library(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    library_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    // Check admin or workspace.tools permission
    if auth_user.user.role != "admin" {
        let config = state.config.read().unwrap();
        let user_permissions = config.user_permissions.clone();
        drop(config);

        if !has_permission(&auth_user.user.id, "workspace.tools", &user_permissions) {
            return Err(AppError::Unauthorized("Unauthorized".to_string()));
        }
    }

    // Map library IDs to actual JSON files
    let library_content = match library_id.as_str() {
        "weather_tools" => include_str!("../../examples/weather_tool.json"),
        "advanced_features" => include_str!("../../examples/advanced_tool_features.json"),
        "http_api_template" => {
            r#"{
  "name": "HTTP API Tool Template",
  "description": "Template for creating custom HTTP API integrations",
  "version": "1.0.0",
  "tools": [
    {
      "name": "custom_api",
      "description": "Make a custom HTTP API request",
      "type": "http_api",
      "parameters": {
        "endpoint": {
          "type": "string",
          "description": "API endpoint path",
          "required": true
        },
        "query": {
          "type": "string",
          "description": "Query parameter",
          "required": false
        }
      },
      "handler": {
        "type": "http",
        "method": "GET",
        "url": "https://api.example.com/{{endpoint}}",
        "params": {
          "q": "{{query}}"
        },
        "headers": {
          "Authorization": "Bearer {{env.API_KEY}}"
        },
        "response": {
          "transform": "Result: {{body.data}}"
        }
      }
    }
  ],
  "environment": {
    "required": ["API_KEY"],
    "optional": []
  }
}"#
        }
        "calculator" => {
            r#"{
  "name": "Calculator & Expressions",
  "description": "Safe mathematical and logical expression evaluator",
  "version": "1.0.0",
  "tools": [
    {
      "name": "calculator",
      "description": "Evaluate mathematical expressions",
      "type": "expression",
      "parameters": {
        "expression": {
          "type": "string",
          "description": "Mathematical expression (e.g., '2 + 2 * 3')",
          "required": true
        }
      },
      "handler": {
        "type": "expression",
        "engine": "evalexpr",
        "expression": "{{expression}}"
      }
    },
    {
      "name": "advanced_calculator",
      "description": "Calculate with variables",
      "type": "expression",
      "parameters": {
        "x": {
          "type": "number",
          "description": "First number",
          "required": true
        },
        "y": {
          "type": "number",
          "description": "Second number",
          "required": true
        },
        "operation": {
          "type": "string",
          "description": "Operation: add, subtract, multiply, divide",
          "required": true
        }
      },
      "handler": {
        "type": "expression",
        "engine": "evalexpr",
        "expression": "if(operation == \"add\", x + y, if(operation == \"subtract\", x - y, if(operation == \"multiply\", x * y, if(operation == \"divide\", x / y, 0))))"
      }
    }
  ],
  "environment": {
    "required": [],
    "optional": []
  }
}"#
        }
        "context_tools" => {
            r#"{
  "name": "Context & User Info Tools",
  "description": "Access user context, session data, and system information",
  "version": "1.0.0",
  "tools": [
    {
      "name": "get_user_info",
      "description": "Get current user information",
      "type": "context",
      "parameters": {},
      "handler": {
        "type": "context",
        "template": "User: {{user.name}} ({{user.email}}) - Role: {{user.role}} - ID: {{user.id}}"
      }
    },
    {
      "name": "get_session_data",
      "description": "Get session information",
      "type": "context",
      "parameters": {},
      "handler": {
        "type": "context",
        "template": "Session for user {{user.name}} - Email: {{user.email}}"
      }
    },
    {
      "name": "get_system_time",
      "description": "Get current system time",
      "type": "function",
      "parameters": {},
      "handler": {
        "type": "built_in",
        "function": "datetime.now"
      }
    }
  ],
  "environment": {
    "required": [],
    "optional": []
  }
}"#
        }
        _ => {
            return Err(AppError::NotFound(format!(
                "Library tool '{}' not found",
                library_id
            )));
        }
    };

    // Parse the library content
    let tool_def: Value = serde_json::from_str(library_content)
        .map_err(|e| AppError::BadRequest(format!("Invalid library content: {}", e)))?;

    let name = tool_def
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Library Tool");
    let description = tool_def
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");

    // Generate a unique ID from library_id
    let tool_id = format!("library_{}", library_id.as_str());
    let tool_service = ToolService::new(&state.db);

    // Check if already installed
    if let Some(_existing) = tool_service.get_tool_by_id(&tool_id).await? {
        return Err(AppError::BadRequest(format!(
            "Tool '{}' is already installed",
            name
        )));
    }

    // Parse specs
    let specs = parse_json_tool_specs(library_content)?;

    // Create tool
    let tool = tool_service
        .create_tool(
            &tool_id,
            &auth_user.user.id,
            name,
            library_content,
            specs,
            json!({
                "description": description,
                "library_source": library_id.as_str()
            }),
            None,
        )
        .await?;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "tool": tool,
        "message": format!("Successfully installed '{}' from library", name)
    })))
}

// GET /schema - Get JSON Schema for tool definitions
async fn get_tool_schema(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Tool Definition",
        "description": "JSON-based declarative tool system for secure tool execution",
        "type": "object",
        "required": ["name", "tools"],
        "properties": {
            "name": {
                "type": "string",
                "description": "Name of the tool collection",
                "minLength": 1
            },
            "description": {
                "type": "string",
                "description": "Description of the tool collection"
            },
            "version": {
                "type": "string",
                "description": "Version of the tool collection",
                "pattern": "^\\d+\\.\\d+\\.\\d+$"
            },
            "tools": {
                "type": "array",
                "description": "Array of tool specifications",
                "minItems": 1,
                "items": {
                    "$ref": "#/definitions/tool"
                }
            },
            "mcp_servers": {
                "type": "object",
                "description": "MCP server configurations",
                "additionalProperties": {
                    "$ref": "#/definitions/mcp_server"
                }
            },
            "environment": {
                "$ref": "#/definitions/environment"
            },
            "rate_limits": {
                "type": "object",
                "description": "Rate limit configurations per tool",
                "additionalProperties": {
                    "$ref": "#/definitions/rate_limit"
                }
            },
            "cache_config": {
                "$ref": "#/definitions/cache_config"
            },
            "tool_chains": {
                "type": "array",
                "description": "Tool chain configurations for sequential execution",
                "items": {
                    "$ref": "#/definitions/tool_chain"
                }
            }
        },
        "definitions": {
            "tool": {
                "type": "object",
                "required": ["name", "description", "type", "parameters", "handler"],
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Tool name (alphanumeric and underscores)",
                        "pattern": "^[a-zA-Z0-9_]+$"
                    },
                    "description": {
                        "type": "string",
                        "description": "Tool description for LLM understanding"
                    },
                    "type": {
                        "type": "string",
                        "enum": ["http_api", "expression", "context", "mcp", "function"],
                        "description": "Type of tool handler"
                    },
                    "parameters": {
                        "type": "object",
                        "description": "Tool parameters specification",
                        "additionalProperties": {
                            "$ref": "#/definitions/parameter"
                        }
                    },
                    "handler": {
                        "description": "Tool handler configuration",
                        "oneOf": [
                            {"$ref": "#/definitions/http_handler"},
                            {"$ref": "#/definitions/expression_handler"},
                            {"$ref": "#/definitions/context_handler"},
                            {"$ref": "#/definitions/mcp_handler"},
                            {"$ref": "#/definitions/builtin_handler"}
                        ]
                    },
                    "error_handling": {
                        "$ref": "#/definitions/error_handling"
                    },
                    "cache_enabled": {
                        "type": "boolean",
                        "default": false
                    }
                }
            },
            "parameter": {
                "type": "object",
                "required": ["type"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["string", "number", "integer", "boolean", "array", "object"]
                    },
                    "description": {
                        "type": "string"
                    },
                    "required": {
                        "type": "boolean",
                        "default": false
                    },
                    "enum": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "default": {
                        "description": "Default value for optional parameters"
                    }
                }
            },
            "http_handler": {
                "type": "object",
                "required": ["type", "method", "url"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["http"]
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"]
                    },
                    "url": {
                        "type": "string",
                        "format": "uri",
                        "description": "API endpoint URL (supports {{variable}} templates)"
                    },
                    "params": {
                        "type": "object",
                        "description": "Query parameters",
                        "additionalProperties": {"type": "string"}
                    },
                    "headers": {
                        "type": "object",
                        "description": "HTTP headers",
                        "additionalProperties": {"type": "string"}
                    },
                    "body": {
                        "description": "Request body (for POST/PUT/PATCH)"
                    },
                    "response": {
                        "$ref": "#/definitions/response_transform"
                    }
                }
            },
            "expression_handler": {
                "type": "object",
                "required": ["type", "engine", "expression"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["expression"]
                    },
                    "engine": {
                        "type": "string",
                        "enum": ["evalexpr"],
                        "default": "evalexpr"
                    },
                    "expression": {
                        "type": "string",
                        "description": "Mathematical or logical expression"
                    }
                }
            },
            "context_handler": {
                "type": "object",
                "required": ["type", "template"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["context"]
                    },
                    "template": {
                        "type": "string",
                        "description": "Template string with {{variable}} placeholders"
                    }
                }
            },
            "mcp_handler": {
                "type": "object",
                "required": ["type", "server", "tool"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["mcp"]
                    },
                    "server": {
                        "type": "string",
                        "description": "MCP server name (references mcp_servers)"
                    },
                    "tool": {
                        "type": "string",
                        "description": "Tool name on the MCP server"
                    }
                }
            },
            "builtin_handler": {
                "type": "object",
                "required": ["type", "function"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["built_in"]
                    },
                    "function": {
                        "type": "string",
                        "enum": ["datetime.now", "datetime.timestamp"]
                    }
                }
            },
            "response_transform": {
                "type": "object",
                "properties": {
                    "transform": {
                        "type": "string",
                        "description": "Template for transforming response (supports {{body.field}} and {{headers.name}})"
                    },
                    "extract": {
                        "type": "string",
                        "description": "JSON path to extract from response (e.g., 'data.result')"
                    }
                }
            },
            "error_handling": {
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type", "max_attempts", "initial_delay_ms", "max_delay_ms"],
                        "properties": {
                            "type": {"enum": ["retry"]},
                            "max_attempts": {"type": "integer", "minimum": 1},
                            "initial_delay_ms": {"type": "integer", "minimum": 0},
                            "max_delay_ms": {"type": "integer", "minimum": 0}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "fallback_tool"],
                        "properties": {
                            "type": {"enum": ["fallback"]},
                            "fallback_tool": {"type": "string"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "value"],
                        "properties": {
                            "type": {"enum": ["default"]},
                            "value": {"description": "Default value to return on error"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": {"enum": ["fail"]}
                        }
                    }
                ]
            },
            "mcp_server": {
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {
                        "type": "string",
                        "format": "uri"
                    },
                    "auth_type": {
                        "type": "string",
                        "enum": ["none", "bearer", "api_key"]
                    },
                    "auth_token": {
                        "type": "string",
                        "description": "Authentication token (supports {{env.VAR}} templates)"
                    }
                }
            },
            "environment": {
                "type": "object",
                "properties": {
                    "required": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Required environment variables"
                    },
                    "optional": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional environment variables"
                    }
                }
            },
            "rate_limit": {
                "type": "object",
                "required": ["requests", "window_seconds"],
                "properties": {
                    "requests": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Number of requests allowed"
                    },
                    "window_seconds": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Time window in seconds"
                    }
                }
            },
            "cache_config": {
                "type": "object",
                "required": ["ttl_seconds"],
                "properties": {
                    "ttl_seconds": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Cache TTL in seconds"
                    },
                    "max_size_mb": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Maximum cache size in MB"
                    }
                }
            },
            "tool_chain": {
                "type": "object",
                "required": ["name", "description", "steps"],
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Chain name"
                    },
                    "description": {
                        "type": "string",
                        "description": "Chain description"
                    },
                    "steps": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "$ref": "#/definitions/tool_chain_step"
                        }
                    }
                }
            },
            "tool_chain_step": {
                "type": "object",
                "required": ["tool_name"],
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Name of tool to execute"
                    },
                    "parameter_mapping": {
                        "type": "object",
                        "description": "Map parameters from previous step output",
                        "additionalProperties": {"type": "string"}
                    },
                    "condition": {
                        "type": "string",
                        "description": "Conditional expression (executes step only if true)"
                    },
                    "error_handling": {
                        "$ref": "#/definitions/error_handling"
                    }
                }
            }
        },
        "examples": [
            {
                "name": "Simple Weather Tool",
                "tools": [
                    {
                        "name": "get_weather",
                        "description": "Get current weather for a city",
                        "type": "http_api",
                        "parameters": {
                            "city": {
                                "type": "string",
                                "description": "City name",
                                "required": true
                            }
                        },
                        "handler": {
                            "type": "http",
                            "method": "GET",
                            "url": "https://wttr.in/{{city}}",
                            "params": {"format": "j1"},
                            "response": {
                                "transform": "Weather in {{params.city}}: {{body.current_condition[0].temp_C}}°C"
                            }
                        }
                    }
                ]
            }
        ]
    });

    Ok(HttpResponse::Ok().json(schema))
}

// GET /builder/templates - Get visual builder templates
async fn get_builder_templates(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let templates = vec![
        json!({
            "id": "http_api",
            "name": "HTTP API Tool",
            "description": "Create a tool that calls an external HTTP API",
            "category": "API Integration",
            "icon": "🌐",
            "fields": [
                {
                    "name": "tool_name",
                    "label": "Tool Name",
                    "type": "text",
                    "required": true,
                    "placeholder": "my_api_tool",
                    "validation": "^[a-zA-Z0-9_]+$"
                },
                {
                    "name": "description",
                    "label": "Description",
                    "type": "textarea",
                    "required": true,
                    "placeholder": "What does this tool do?"
                },
                {
                    "name": "http_method",
                    "label": "HTTP Method",
                    "type": "select",
                    "required": true,
                    "options": ["GET", "POST", "PUT", "PATCH", "DELETE"],
                    "default": "GET"
                },
                {
                    "name": "url",
                    "label": "API URL",
                    "type": "url",
                    "required": true,
                    "placeholder": "https://api.example.com/endpoint"
                },
                {
                    "name": "parameters",
                    "label": "Parameters",
                    "type": "parameter_list",
                    "description": "Define tool parameters (available as {{param_name}} in URL, headers, body)"
                },
                {
                    "name": "headers",
                    "label": "HTTP Headers",
                    "type": "key_value_list",
                    "placeholder_key": "Header-Name",
                    "placeholder_value": "Header-Value"
                },
                {
                    "name": "response_transform",
                    "label": "Response Transform (Optional)",
                    "type": "textarea",
                    "placeholder": "{{body.data.result}}",
                    "description": "Template to transform the API response"
                }
            ]
        }),
        json!({
            "id": "calculator",
            "name": "Calculator Tool",
            "description": "Create a mathematical expression evaluator",
            "category": "Utilities",
            "icon": "🔢",
            "fields": [
                {
                    "name": "tool_name",
                    "label": "Tool Name",
                    "type": "text",
                    "required": true,
                    "placeholder": "my_calculator"
                },
                {
                    "name": "description",
                    "label": "Description",
                    "type": "textarea",
                    "required": true
                },
                {
                    "name": "expression",
                    "label": "Expression",
                    "type": "textarea",
                    "required": true,
                    "placeholder": "x + y * 2",
                    "description": "Mathematical expression using parameter variables"
                },
                {
                    "name": "parameters",
                    "label": "Parameters",
                    "type": "parameter_list",
                    "description": "Define numeric parameters for the expression"
                }
            ]
        }),
        json!({
            "id": "context",
            "name": "Context Tool",
            "description": "Create a tool that accesses user/session context",
            "category": "System",
            "icon": "👤",
            "fields": [
                {
                    "name": "tool_name",
                    "label": "Tool Name",
                    "type": "text",
                    "required": true
                },
                {
                    "name": "description",
                    "label": "Description",
                    "type": "textarea",
                    "required": true
                },
                {
                    "name": "template",
                    "label": "Output Template",
                    "type": "textarea",
                    "required": true,
                    "placeholder": "User: {{user.name}} ({{user.email}})",
                    "description": "Available: {{user.name}}, {{user.email}}, {{user.id}}, {{user.role}}"
                }
            ]
        }),
        json!({
            "id": "tool_chain",
            "name": "Tool Chain",
            "description": "Create a sequential workflow of multiple tools",
            "category": "Advanced",
            "icon": "🔗",
            "fields": [
                {
                    "name": "chain_name",
                    "label": "Chain Name",
                    "type": "text",
                    "required": true
                },
                {
                    "name": "description",
                    "label": "Description",
                    "type": "textarea",
                    "required": true
                },
                {
                    "name": "steps",
                    "label": "Chain Steps",
                    "type": "chain_step_list",
                    "description": "Define the sequence of tool executions"
                }
            ]
        }),
    ];

    Ok(HttpResponse::Ok().json(json!({
        "templates": templates,
        "total": templates.len()
    })))
}

// POST /builder/generate - Generate tool definition from visual builder
#[derive(Debug, Deserialize)]
struct BuilderGenerateForm {
    template_id: String,
    fields: HashMap<String, Value>,
}

async fn generate_from_builder(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    form: web::Json<BuilderGenerateForm>,
) -> AppResult<HttpResponse> {
    let tool_content = match form.template_id.as_str() {
        "http_api" => {
            let tool_name = form
                .fields
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing tool_name".to_string()))?;
            let description = form
                .fields
                .get("description")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing description".to_string()))?;
            let method = form
                .fields
                .get("http_method")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing http_method".to_string()))?;
            let url = form
                .fields
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing url".to_string()))?;

            let parameters = form
                .fields
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let headers = form
                .fields
                .get("headers")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let response_transform = form
                .fields
                .get("response_transform")
                .and_then(|v| v.as_str());

            let mut response_obj = serde_json::Map::new();
            if let Some(transform) = response_transform {
                if !transform.is_empty() {
                    response_obj.insert("transform".to_string(), json!(transform));
                }
            }

            json!({
                "name": format!("{} Tool", tool_name),
                "description": description,
                "version": "1.0.0",
                "tools": [{
                    "name": tool_name,
                    "description": description,
                    "type": "http_api",
                    "parameters": parameters,
                    "handler": {
                        "type": "http",
                        "method": method,
                        "url": url,
                        "headers": headers,
                        "response": response_obj
                    }
                }],
                "environment": {
                    "required": [],
                    "optional": []
                }
            })
        }
        "calculator" => {
            let tool_name = form
                .fields
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing tool_name".to_string()))?;
            let description = form
                .fields
                .get("description")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing description".to_string()))?;
            let expression = form
                .fields
                .get("expression")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing expression".to_string()))?;
            let parameters = form
                .fields
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}));

            json!({
                "name": format!("{} Calculator", tool_name),
                "description": description,
                "version": "1.0.0",
                "tools": [{
                    "name": tool_name,
                    "description": description,
                    "type": "expression",
                    "parameters": parameters,
                    "handler": {
                        "type": "expression",
                        "engine": "evalexpr",
                        "expression": expression
                    }
                }],
                "environment": {
                    "required": [],
                    "optional": []
                }
            })
        }
        "context" => {
            let tool_name = form
                .fields
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing tool_name".to_string()))?;
            let description = form
                .fields
                .get("description")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing description".to_string()))?;
            let template = form
                .fields
                .get("template")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("Missing template".to_string()))?;

            json!({
                "name": format!("{} Context Tool", tool_name),
                "description": description,
                "version": "1.0.0",
                "tools": [{
                    "name": tool_name,
                    "description": description,
                    "type": "context",
                    "parameters": {},
                    "handler": {
                        "type": "context",
                        "template": template
                    }
                }],
                "environment": {
                    "required": [],
                    "optional": []
                }
            })
        }
        _ => {
            return Err(AppError::BadRequest(format!(
                "Unknown template: {}",
                form.template_id
            )));
        }
    };

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "content": serde_json::to_string_pretty(&tool_content).unwrap(),
        "parsed": tool_content
    })))
}
