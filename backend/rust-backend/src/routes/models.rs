use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::error::{AppError, AppResult};
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::model::{Model, ModelForm, ModelResponse, ModelUserResponse};
use crate::services::group::GroupService;
use crate::services::model::ModelService;
use crate::services::user::UserService;
use crate::utils::misc::{has_access, has_permission};
use crate::AppState;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("", web::get().to(get_models))
            .route("/", web::get().to(get_models))
            .route("/base", web::get().to(get_base_models))
            .route("/create", web::post().to(create_model))
            .route("/export", web::get().to(export_models))
            .route("/import", web::post().to(import_models))
            .route("/sync", web::post().to(sync_models))
            .route("/model", web::get().to(get_model_by_id))
            .route(
                "/model/profile/image",
                web::get().to(get_model_profile_image),
            )
            .route("/model/toggle", web::post().to(toggle_model_by_id))
            .route("/model/update", web::post().to(update_model_by_id))
            .route("/model/delete", web::delete().to(delete_model_by_id))
            .route("/delete/all", web::delete().to(delete_all_models)),
    );
}

// GET / - Get models for current user
async fn get_models(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let model_service = ModelService::new(&state.db);
    let user_service = UserService::new(&state.db);

    // Admins with bypass can see all models
    let config = state.config.read().unwrap();
    let bypass_admin_access_control = config.bypass_admin_access_control.unwrap_or(false);
    drop(config);

    let models = if auth_user.user.role == "admin" && bypass_admin_access_control {
        model_service.get_models().await?
    } else {
        // Get user's groups for access control
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        // Filter models by user ownership or access control
        let all_models = model_service.get_models().await?;
        all_models
            .into_iter()
            .filter(|model| {
                model.user_id == auth_user.user.id
                    || has_access(
                        &auth_user.user.id,
                        "read",
                        &model.access_control,
                        &user_group_ids,
                    )
            })
            .collect()
    };

    // Get unique user IDs
    let user_ids: HashSet<String> = models.iter().map(|m| m.user_id.clone()).collect();

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

    let response: Vec<ModelUserResponse> = models
        .into_iter()
        .map(|m| {
            let user = users_map.get(&m.user_id).cloned();
            ModelUserResponse::from_model_and_user(m, user)
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

// GET /base - Get base models (admin only)
async fn get_base_models(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let model_service = ModelService::new(&state.db);
    let models = model_service.get_base_models().await?;

    let response: Vec<ModelResponse> = models.into_iter().map(ModelResponse::from).collect();

    Ok(HttpResponse::Ok().json(response))
}

// POST /create - Create a new model
async fn create_model(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<ModelForm>,
) -> AppResult<HttpResponse> {
    // Check user permissions
    if auth_user.user.role != "admin" {
        let config = state.config.read().unwrap();
        let user_permissions = config.user_permissions.clone();
        drop(config);

        // Check if user has workspace.models permission
        if !has_permission(&auth_user.user.id, "workspace.models", &user_permissions) {
            return Err(AppError::Forbidden("Permission denied".to_string()));
        }
    }

    let model_service = ModelService::new(&state.db);

    // Check if model ID already exists
    if model_service
        .get_model_by_id(&form_data.id)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest("Model ID already taken".to_string()));
    }

    let model = model_service
        .insert_new_model(form_data.into_inner(), &auth_user.user.id)
        .await?;

    Ok(HttpResponse::Ok().json(ModelResponse::from(model)))
}

// GET /export - Export all models (admin only)
async fn export_models(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let model_service = ModelService::new(&state.db);
    let models = model_service.get_models().await?;

    let response: Vec<ModelResponse> = models.into_iter().map(ModelResponse::from).collect();

    Ok(HttpResponse::Ok().json(response))
}

// POST /import - Import models (admin only)
#[derive(Debug, Deserialize)]
struct ImportModelsForm {
    models: Vec<serde_json::Value>,
}

async fn import_models(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<ImportModelsForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let model_service = ModelService::new(&state.db);

    for model_data in &form_data.models {
        if let Some(model_id) = model_data["id"].as_str() {
            // Check if model exists
            if let Some(existing) = model_service.get_model_by_id(model_id).await? {
                // Update existing model
                let form = ModelForm {
                    id: existing.id.clone(),
                    base_model_id: model_data
                        .get("base_model_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or(existing.base_model_id),
                    name: model_data
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or(existing.name),
                    meta: model_data
                        .get("meta")
                        .cloned()
                        .or(existing.meta)
                        .unwrap_or_else(|| json!({})),
                    params: model_data.get("params").cloned().unwrap_or(existing.params),
                    access_control: model_data
                        .get("access_control")
                        .cloned()
                        .or(existing.access_control),
                };

                model_service.update_model_by_id(model_id, form).await?;
            } else {
                // Insert new model
                let form: ModelForm = serde_json::from_value(model_data.clone())
                    .map_err(|e| AppError::BadRequest(format!("Invalid model data: {}", e)))?;

                model_service
                    .insert_new_model(form, &auth_user.user.id)
                    .await?;
            }
        }
    }

    Ok(HttpResponse::Ok().json(true))
}

// POST /sync - Sync models (admin only)
#[derive(Debug, Deserialize)]
struct SyncModelsForm {
    models: Vec<Model>,
}

async fn sync_models(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<SyncModelsForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let model_service = ModelService::new(&state.db);
    let synced = model_service
        .sync_models(&auth_user.user.id, form_data.models.clone())
        .await?;

    Ok(HttpResponse::Ok().json(synced))
}

// GET /model?id= - Get model by ID
#[derive(Debug, Deserialize)]
struct ModelQuery {
    id: String,
}

async fn get_model_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ModelQuery>,
) -> AppResult<HttpResponse> {
    let model_service = ModelService::new(&state.db);

    let model = model_service
        .get_model_by_id(&query.id)
        .await?
        .ok_or(AppError::NotFound("Model not found".to_string()))?;

    // Check access
    let config = state.config.read().unwrap();
    let bypass_admin_access_control = config.bypass_admin_access_control.unwrap_or(false);
    drop(config);

    if auth_user.user.role == "admin" && bypass_admin_access_control {
        return Ok(HttpResponse::Ok().json(ModelResponse::from(model)));
    }

    if model.user_id == auth_user.user.id {
        return Ok(HttpResponse::Ok().json(ModelResponse::from(model)));
    }

    // Check access control
    let group_service = GroupService::new(&state.db);
    let groups = group_service
        .get_groups_by_member_id(&auth_user.user.id)
        .await?;
    let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

    if has_access(
        &auth_user.user.id,
        "read",
        &model.access_control,
        &user_group_ids,
    ) {
        return Ok(HttpResponse::Ok().json(ModelResponse::from(model)));
    }

    Err(AppError::Forbidden("Access denied".to_string()))
}

// GET /model/profile/image?id= - Get model profile image
async fn get_model_profile_image(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    query: web::Query<ModelQuery>,
) -> AppResult<HttpResponse> {
    let model_service = ModelService::new(&state.db);

    let model = model_service
        .get_model_by_id(&query.id)
        .await?
        .ok_or(AppError::NotFound("Model not found".to_string()))?;

    // Check if model has profile image URL
    if let Some(meta) = &model.meta {
        if let Some(profile_image_url) = meta.get("profile_image_url").and_then(|v| v.as_str()) {
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
    }

    // Return default favicon
    let config = state.config.read().unwrap();
    let static_dir = &config.static_dir;
    let favicon_path = std::path::Path::new(static_dir).join("favicon.png");
    
    // Try external file first
    if let Ok(image_data) = std::fs::read(&favicon_path) {
        return Ok(HttpResponse::Ok()
            .content_type("image/png")
            .body(image_data));
    }
    
    // Fall back to embedded file
    use crate::static_files::FrontendAssets;
    if let Some(content) = FrontendAssets::get("static/favicon.png") {
        return Ok(HttpResponse::Ok()
            .content_type("image/png")
            .body(content.data.into_owned()));
    }
    
    Err(AppError::NotFound("Default image not found".to_string()))
}

// POST /model/toggle?id= - Toggle model visibility
async fn toggle_model_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ModelQuery>,
) -> AppResult<HttpResponse> {
    let model_service = ModelService::new(&state.db);

    let model = model_service
        .get_model_by_id(&query.id)
        .await?
        .ok_or(AppError::NotFound("Model not found".to_string()))?;

    // Check permissions
    if auth_user.user.role != "admin" && model.user_id != auth_user.user.id {
        // Check write access
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "write",
            &model.access_control,
            &user_group_ids,
        ) {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let toggled = model_service.toggle_model_by_id(&query.id).await?;

    Ok(HttpResponse::Ok().json(ModelResponse::from(toggled)))
}

// POST /model/update?id= - Update model
async fn update_model_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ModelQuery>,
    form_data: web::Json<ModelForm>,
) -> AppResult<HttpResponse> {
    let model_service = ModelService::new(&state.db);

    let model = model_service
        .get_model_by_id(&query.id)
        .await?
        .ok_or(AppError::NotFound("Model not found".to_string()))?;

    // Check permissions
    if model.user_id != auth_user.user.id && auth_user.user.role != "admin" {
        // Check write access
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "write",
            &model.access_control,
            &user_group_ids,
        ) {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let updated = model_service
        .update_model_by_id(&query.id, form_data.into_inner())
        .await?;

    Ok(HttpResponse::Ok().json(ModelResponse::from(updated)))
}

// DELETE /model/delete?id= - Delete model by ID
async fn delete_model_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ModelQuery>,
) -> AppResult<HttpResponse> {
    let model_service = ModelService::new(&state.db);

    let model = model_service
        .get_model_by_id(&query.id)
        .await?
        .ok_or(AppError::NotFound("Model not found".to_string()))?;

    // Check permissions
    if auth_user.user.role != "admin" && model.user_id != auth_user.user.id {
        // Check write access
        let group_service = GroupService::new(&state.db);
        let groups = group_service
            .get_groups_by_member_id(&auth_user.user.id)
            .await?;
        let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

        if !has_access(
            &auth_user.user.id,
            "write",
            &model.access_control,
            &user_group_ids,
        ) {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = model_service.delete_model_by_id(&query.id).await?;

    Ok(HttpResponse::Ok().json(result))
}

// DELETE /delete/all - Delete all models (admin only)
async fn delete_all_models(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let model_service = ModelService::new(&state.db);
    let result = model_service.delete_all_models().await?;

    Ok(HttpResponse::Ok().json(result))
}
