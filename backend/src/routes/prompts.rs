use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::error::{AppError, AppResult};
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::prompt::{PromptForm, PromptModel, PromptUserResponse};
use crate::services::group::GroupService;
use crate::services::prompt::PromptService;
use crate::services::user::UserService;
use crate::utils::misc::{has_access, has_permission};
use crate::AppState;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_prompts)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_prompts)),
    )
    .service(
        web::resource("/list")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_prompt_list)),
    )
    .service(
        web::resource("/create")
            .wrap(AuthMiddleware)
            .route(web::post().to(create_new_prompt)),
    )
    .service(
        web::resource("/command/{command}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_prompt_by_command)),
    )
    .service(
        web::resource("/command/{command}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_prompt_by_command)),
    )
    .service(
        web::resource("/command/{command}/delete")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_prompt_by_command)),
    );
}

async fn get_prompts(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let prompt_service = PromptService::new(&state.db);
    let config = state.config.read().unwrap();

    let all_prompts =
        if auth_user.role == "admin" && config.bypass_admin_access_control.unwrap_or(false) {
            prompt_service.get_all_prompts().await?
        } else {
            // Get user's groups
            let group_service = GroupService::new(&state.db);
            let groups = group_service.get_groups_by_member_id(&auth_user.id).await?;
            let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

            // Filter prompts by access control
            let all = prompt_service.get_all_prompts().await?;
            all.into_iter()
                .filter(|p| {
                    p.user_id == auth_user.id
                        || has_access(&auth_user.id, "read", &p.access_control, &user_group_ids)
                })
                .collect()
        };

    let response: Vec<PromptModel> = all_prompts.into_iter().map(PromptModel::from).collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn get_prompt_list(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let prompt_service = PromptService::new(&state.db);
    let user_service = UserService::new(&state.db);
    let config = state.config.read().unwrap();

    let all_prompts =
        if auth_user.role == "admin" && config.bypass_admin_access_control.unwrap_or(false) {
            prompt_service.get_all_prompts().await?
        } else {
            // Get user's groups
            let group_service = GroupService::new(&state.db);
            let groups = group_service.get_groups_by_member_id(&auth_user.id).await?;
            let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

            // Filter prompts by write access
            let all = prompt_service.get_all_prompts().await?;
            all.into_iter()
                .filter(|p| {
                    p.user_id == auth_user.id
                        || has_access(&auth_user.id, "write", &p.access_control, &user_group_ids)
                })
                .collect()
        };

    // Get unique user IDs
    let user_ids: HashSet<String> = all_prompts.iter().map(|p| p.user_id.clone()).collect();

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

    let response: Vec<PromptUserResponse> = all_prompts
        .into_iter()
        .map(|p| {
            let user = users_map.get(&p.user_id).cloned();
            PromptUserResponse::from_prompt_and_user(p, user)
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn create_new_prompt(
    _req: HttpRequest,
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<PromptForm>,
) -> AppResult<HttpResponse> {
    let config = state.config.read().unwrap();

    // Check workspace permissions
    if auth_user.role != "admin"
        && !has_permission(&auth_user.id, "workspace.prompts", &config.user_permissions)
    {
        return Err(AppError::Unauthorized("Unauthorized".to_string()));
    }

    let prompt_service = PromptService::new(&state.db);

    // Check if prompt with same command already exists
    let existing = prompt_service
        .get_prompt_by_command(&payload.command)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Command already taken".to_string()));
    }

    let prompt = prompt_service
        .insert_new_prompt(&auth_user.id, &payload)
        .await?;

    Ok(HttpResponse::Ok().json(PromptModel::from(prompt)))
}

async fn get_prompt_by_command(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    command: web::Path<String>,
) -> AppResult<HttpResponse> {
    let prompt_service = PromptService::new(&state.db);

    let prompt = prompt_service
        .get_prompt_by_command(&format!("/{}", command))
        .await?
        .ok_or_else(|| AppError::NotFound("Prompt not found".to_string()))?;

    // Check access
    let group_service = GroupService::new(&state.db);
    let groups = group_service.get_groups_by_member_id(&auth_user.id).await?;
    let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

    if auth_user.role == "admin"
        || prompt.user_id == auth_user.id
        || has_access(
            &auth_user.id,
            "read",
            &prompt.access_control,
            &user_group_ids,
        )
    {
        Ok(HttpResponse::Ok().json(PromptModel::from(prompt)))
    } else {
        Err(AppError::Unauthorized("Not found".to_string()))
    }
}

async fn update_prompt_by_command(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    command: web::Path<String>,
    payload: web::Json<PromptForm>,
) -> AppResult<HttpResponse> {
    let prompt_service = PromptService::new(&state.db);

    let prompt = prompt_service
        .get_prompt_by_command(&format!("/{}", command))
        .await?
        .ok_or_else(|| AppError::NotFound("Prompt not found".to_string()))?;

    // Check write access
    let group_service = GroupService::new(&state.db);
    let groups = group_service.get_groups_by_member_id(&auth_user.id).await?;
    let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

    if prompt.user_id != auth_user.id
        && !has_access(
            &auth_user.id,
            "write",
            &prompt.access_control,
            &user_group_ids,
        )
        && auth_user.role != "admin"
    {
        return Err(AppError::Forbidden("Access prohibited".to_string()));
    }

    let updated_prompt = prompt_service
        .update_prompt_by_command(&format!("/{}", command), &payload)
        .await?;

    Ok(HttpResponse::Ok().json(PromptModel::from(updated_prompt)))
}

async fn delete_prompt_by_command(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    command: web::Path<String>,
) -> AppResult<HttpResponse> {
    let prompt_service = PromptService::new(&state.db);

    let prompt = prompt_service
        .get_prompt_by_command(&format!("/{}", command))
        .await?
        .ok_or_else(|| AppError::NotFound("Prompt not found".to_string()))?;

    // Check write access
    let group_service = GroupService::new(&state.db);
    let groups = group_service.get_groups_by_member_id(&auth_user.id).await?;
    let user_group_ids: HashSet<String> = groups.into_iter().map(|g| g.id).collect();

    if prompt.user_id != auth_user.id
        && !has_access(
            &auth_user.id,
            "write",
            &prompt.access_control,
            &user_group_ids,
        )
        && auth_user.role != "admin"
    {
        return Err(AppError::Forbidden("Access prohibited".to_string()));
    }

    let result = prompt_service
        .delete_prompt_by_command(&format!("/{}", command))
        .await?;

    Ok(HttpResponse::Ok().json(result))
}
