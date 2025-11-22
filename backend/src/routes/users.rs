use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppResult;
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::{UpdateUserRoleRequest, UserResponse};
use crate::services::UserService;
use crate::AppState;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .service(web::resource("").route(web::get().to(list_users)))
            .service(web::resource("/").route(web::get().to(list_users)))
            .route("/all", web::get().to(get_all_users))
            .route("/active", web::get().to(get_active_users))
            .route("/search", web::get().to(search_users))
            .route("/groups", web::get().to(get_user_groups))
            .route("/permissions", web::get().to(get_user_permissions))
            .service(
                web::resource("/{id}")
                    .route(web::get().to(get_user_by_id))
                    .route(web::delete().to(delete_user)),
            )
            .route("/{id}/role", web::post().to(update_user_role))
            .route("/{id}/update", web::post().to(update_user_by_id))
            .route("/{id}/profile/image", web::get().to(get_user_profile_image))
            .route("/{id}/active", web::get().to(get_user_active_status))
            .route("/{id}/groups", web::get().to(get_user_groups_by_id))
            .route(
                "/{id}/oauth/sessions",
                web::get().to(get_user_oauth_sessions),
            )
            .route("/user/settings", web::get().to(get_user_settings))
            .route(
                "/user/settings/update",
                web::post().to(update_user_settings),
            )
            .route("/user/info", web::get().to(get_user_info))
            .route("/user/info/update", web::post().to(update_user_info))
            .route(
                "/default/permissions",
                web::get().to(get_default_user_permissions),
            )
            .route(
                "/default/permissions",
                web::post().to(update_default_user_permissions),
            ),
    );
}

#[derive(Deserialize)]
struct ListUsersQuery {
    skip: Option<i64>,
    limit: Option<i64>,
    query: Option<String>,
    order_by: Option<String>,
    direction: Option<String>,
    page: Option<i64>,
}

async fn list_users(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ListUsersQuery>,
) -> AppResult<HttpResponse> {
    // Only admins can list users
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let user_service = UserService::new(&state.db);
    let page = query.page.unwrap_or(1).max(1);
    let limit = 30; // PAGE_ITEM_COUNT
    let skip = (page - 1) * limit;

    let users = user_service.list_users(skip, limit).await?;
    let total = user_service.count_users().await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "users": users.into_iter().map(UserResponse::from).collect::<Vec<_>>(),
        "total": total
    })))
}

async fn get_all_users(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    // Only admins can get all users
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let user_service = UserService::new(&state.db);
    let users = user_service.list_users(0, 10000).await?; // Get all users (up to reasonable limit)
    let total = user_service.count_users().await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "users": users.into_iter().map(UserResponse::from).collect::<Vec<_>>(),
        "total": total
    })))
}

// Get active users (returns list of active user IDs)
async fn get_active_users(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // TODO: Implement active users tracking via WebSocket/Socket.IO state
    // For now, return empty list
    Ok(HttpResponse::Ok().json(json!({ "user_ids": [] })))
}

// Search users by query
async fn search_users(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ListUsersQuery>,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);
    let page = 1; // Always first page for search
    let limit = 30;
    let skip = 0;

    let users = user_service.list_users(skip, limit).await?;
    let total = user_service.count_users().await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "users": users.into_iter().map(UserResponse::from).collect::<Vec<_>>(),
        "total": total
    })))
}

// Get current user's groups
async fn get_user_groups(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // TODO: Implement groups retrieval
    Ok(HttpResponse::Ok().json(Vec::<serde_json::Value>::new()))
}

// Get current user's permissions
async fn get_user_permissions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let config = state.config.read().unwrap();

    // TODO: Implement actual permission checking based on user groups and user-specific permissions
    // For now, return default permissions
    let permissions = config.user_permissions.clone();

    Ok(HttpResponse::Ok().json(permissions))
}

async fn get_user_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);

    // Handle shared chat user IDs
    let actual_user_id = if id.starts_with("shared-") {
        let chat_id = id.strip_prefix("shared-").unwrap();
        // TODO: Get user_id from chat
        // For now, just return error
        return Err(crate::error::AppError::NotFound(
            "Chat not found".to_string(),
        ));
    } else {
        id.to_string()
    };

    let user = user_service.get_user_by_id(&actual_user_id).await?.ok_or(
        crate::error::AppError::NotFound("User not found".to_string()),
    )?;

    // Return limited info (name, profile image, active status)
    Ok(HttpResponse::Ok().json(json!({
        "name": user.name,
        "profile_image_url": user.profile_image_url,
        "active": false, // TODO: Get from Socket.IO state
    })))
}

// Get user profile image
async fn get_user_profile_image(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);
    let user = user_service
        .get_user_by_id(&id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    let profile_image_url = user.profile_image_url;
    if !profile_image_url.is_empty() {
        if profile_image_url.starts_with("http") {
            // Redirect to external URL
            return Ok(HttpResponse::Found()
                .append_header(("Location", profile_image_url))
                .finish());
        } else if profile_image_url.starts_with("data:image") {
            // Return base64 encoded image
            if let Some(comma_pos) = profile_image_url.find(',') {
                let base64_data = &profile_image_url[comma_pos + 1..];
                use base64::{engine::general_purpose, Engine};
                if let Ok(image_data) = general_purpose::STANDARD.decode(base64_data) {
                    return Ok(HttpResponse::Ok()
                        .content_type("image/png")
                        .body(image_data));
                }
            }
        }
    }

    // Return default user avatar
    let config = state.config.read().unwrap();
    let static_dir = &config.static_dir;
    let user_avatar_path = std::path::Path::new(static_dir).join("user.png");
    
    // Try external file first
    if let Ok(image_data) = std::fs::read(&user_avatar_path) {
        return Ok(HttpResponse::Ok()
            .content_type("image/png")
            .body(image_data));
    }
    
    // Fall back to embedded file
    use crate::static_files::FrontendAssets;
    if let Some(content) = FrontendAssets::get("static/user.png") {
        return Ok(HttpResponse::Ok()
            .content_type("image/png")
            .body(content.data.into_owned()));
    }
    
    Err(crate::error::AppError::NotFound(
        "Default avatar not found".to_string(),
    ))
}

// Get user active status
async fn get_user_active_status(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    // TODO: Get from Socket.IO state
    Ok(HttpResponse::Ok().json(json!({ "active": false })))
}

// Get user groups by ID (admin only)
async fn get_user_groups_by_id(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    // TODO: Implement groups retrieval
    Ok(HttpResponse::Ok().json(Vec::<serde_json::Value>::new()))
}

// Get user OAuth sessions (admin only)
async fn get_user_oauth_sessions(
    _state: web::Data<AppState>,
    auth_user: AuthUser,
    _id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    // TODO: Implement OAuth sessions retrieval
    Ok(HttpResponse::Ok().json(json!({})))
}

async fn update_user_role(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    req: web::Json<UpdateUserRoleRequest>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let user_service = UserService::new(&state.db);
    user_service.update_user_role(&id, &req.role).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"success": true})))
}

async fn delete_user(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let user_service = UserService::new(&state.db);
    user_service.delete_user(&id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"success": true})))
}

async fn get_user_settings(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);
    let user = user_service
        .get_user_by_id(&auth_user.user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    // Return settings from user model or default structure
    let mut settings = user.settings.unwrap_or_else(|| serde_json::json!({}));

    // Ensure ui field exists
    if settings.is_object() && !settings.as_object().unwrap().contains_key("ui") {
        settings["ui"] = serde_json::json!({});
    }

    Ok(HttpResponse::Ok().json(settings))
}

async fn update_user_settings(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    settings: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);

    // Update user settings in database
    user_service
        .update_user_settings(&auth_user.user.id, &settings.into_inner())
        .await?;

    // Retrieve and return updated settings
    let user = user_service
        .get_user_by_id(&auth_user.user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    let mut settings = user.settings.unwrap_or_else(|| serde_json::json!({}));

    // Ensure ui field exists
    if settings.is_object() && !settings.as_object().unwrap().contains_key("ui") {
        settings["ui"] = serde_json::json!({});
    }

    Ok(HttpResponse::Ok().json(settings))
}

async fn get_user_info(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);
    let user = user_service
        .get_user_by_id(&auth_user.user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    Ok(HttpResponse::Ok().json(user.info.unwrap_or_else(|| json!({}))))
}

async fn update_user_info(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    info: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);

    let user = user_service
        .get_user_by_id(&auth_user.user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    let mut current_info = user.info.unwrap_or_else(|| json!({}));

    // Merge new info with existing info
    if let (Some(current_obj), Some(new_obj)) = (current_info.as_object_mut(), info.as_object()) {
        for (key, value) in new_obj {
            current_obj.insert(key.clone(), value.clone());
        }
    }

    // TODO: Update user info in database
    // user_service.update_user_info(&auth_user.user.id, &current_info).await?;

    Ok(HttpResponse::Ok().json(current_info))
}

#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    name: String,
    email: String,
    role: String,
    profile_image_url: String,
    password: Option<String>,
}

async fn update_user_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form_data: web::Json<UpdateUserRequest>,
) -> AppResult<HttpResponse> {
    // Only admins can update other users
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let user_service = UserService::new(&state.db);

    // Prevent modification of the primary admin user by other admins
    // TODO: Check if this is the first user and enforce restrictions

    let user = user_service
        .get_user_by_id(&id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    // Check if email is being changed and if it's already taken
    if form_data.email.to_lowercase() != user.email {
        if user_service
            .get_user_by_email(&form_data.email.to_lowercase())
            .await?
            .is_some()
        {
            return Err(crate::error::AppError::BadRequest(
                "Email already taken".to_string(),
            ));
        }
    }

    // TODO: Update password if provided
    // TODO: Update email in auth table
    // TODO: Update user in database

    Ok(HttpResponse::Ok().json(UserResponse::from(user)))
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspacePermissions {
    models: bool,
    knowledge: bool,
    prompts: bool,
    tools: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SharingPermissions {
    public_models: bool,
    public_knowledge: bool,
    public_prompts: bool,
    public_tools: bool,
    public_notes: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatPermissions {
    controls: bool,
    valves: bool,
    system_prompt: bool,
    params: bool,
    file_upload: bool,
    delete: bool,
    delete_message: bool,
    continue_response: bool,
    regenerate_response: bool,
    rate_response: bool,
    edit: bool,
    share: bool,
    export: bool,
    stt: bool,
    tts: bool,
    call: bool,
    multiple_models: bool,
    temporary: bool,
    temporary_enforced: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct FeaturesPermissions {
    direct_tool_servers: bool,
    web_search: bool,
    image_generation: bool,
    code_interpreter: bool,
    notes: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserPermissions {
    workspace: WorkspacePermissions,
    sharing: SharingPermissions,
    chat: ChatPermissions,
    features: FeaturesPermissions,
}

async fn get_default_user_permissions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let config = state.config.read().unwrap();

    // Parse user_permissions from config or return defaults
    let permissions = if let Some(workspace) = config.user_permissions.get("workspace") {
        json!({
            "workspace": workspace,
            "sharing": config.user_permissions.get("sharing").unwrap_or(&json!({})),
            "chat": config.user_permissions.get("chat").unwrap_or(&json!({})),
            "features": config.user_permissions.get("features").unwrap_or(&json!({})),
        })
    } else {
        json!({
            "workspace": {
                "models": false,
                "knowledge": false,
                "prompts": false,
                "tools": false
            },
            "sharing": {
                "public_models": true,
                "public_knowledge": true,
                "public_prompts": true,
                "public_tools": true,
                "public_notes": true
            },
            "chat": {
                "controls": true,
                "valves": true,
                "system_prompt": true,
                "params": true,
                "file_upload": true,
                "delete": true,
                "delete_message": true,
                "continue_response": true,
                "regenerate_response": true,
                "rate_response": true,
                "edit": true,
                "share": true,
                "export": true,
                "stt": true,
                "tts": true,
                "call": true,
                "multiple_models": true,
                "temporary": true,
                "temporary_enforced": false
            },
            "features": {
                "direct_tool_servers": false,
                "web_search": true,
                "image_generation": true,
                "code_interpreter": true,
                "notes": true
            }
        })
    };

    Ok(HttpResponse::Ok().json(permissions))
}

async fn update_default_user_permissions(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let mut config = state.config.write().unwrap();
    config.user_permissions = form_data.into_inner();

    // TODO: Persist to database

    Ok(HttpResponse::Ok().json(&config.user_permissions))
}
