use actix_web::{web, HttpRequest, HttpResponse};

use crate::error::{AppError, AppResult};
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::folder::{
    FolderForm, FolderIsExpandedForm, FolderModel, FolderNameIdResponse, FolderParentIdForm,
    FolderUpdateForm,
};
use crate::services::chat::ChatService;
use crate::services::folder::FolderService;
use crate::utils::misc::has_permission;
use crate::AppState;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_folders))
            .route(web::post().to(create_folder)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_folders))
            .route(web::post().to(create_folder)),
    )
    .service(
        web::resource("/{id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_folder_by_id))
            .route(web::delete().to(delete_folder_by_id)),
    )
    .service(
        web::resource("/{id}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_folder_by_id)),
    )
    .service(
        web::resource("/{id}/update/parent")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_folder_parent_by_id)),
    )
    .service(
        web::resource("/{id}/update/expanded")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_folder_expanded_by_id)),
    );
}

async fn get_folders(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);
    let mut folders = folder_service.get_folders_by_user_id(&auth_user.id).await?;

    // Verify folder data integrity - check if files and collections referenced in folder.data still exist
    // Note: Full access control checking requires group resolution which is expensive.
    // For now, we do basic existence checking. Files and knowledge bases have their own
    // access controls that are enforced when accessed.
    use crate::services::file::FileService;
    use crate::services::group::GroupService;
    use crate::services::knowledge::KnowledgeService;

    let file_service = FileService::new(&state.db);
    let knowledge_service = KnowledgeService::new(&state.db);
    let group_service = GroupService::new(&state.db);

    // Get user groups once for all checks
    let user_groups = group_service
        .get_groups_by_member_id(&auth_user.id)
        .await
        .unwrap_or_default();
    let user_group_ids: std::collections::HashSet<String> =
        user_groups.iter().map(|g| g.id.clone()).collect();

    for folder in folders.iter_mut() {
        folder.parse_json_fields();

        if let Some(ref mut data) = folder.data {
            if let Some(files_array) = data.get("files").and_then(|f| f.as_array()).cloned() {
                let original_count = files_array.len();
                let mut valid_files = Vec::new();

                for file_obj in &files_array {
                    let file_type = file_obj.get("type").and_then(|t| t.as_str());
                    let file_id = file_obj.get("id").and_then(|id| id.as_str());

                    let mut is_valid = false;

                    match (file_type, file_id) {
                        (Some("file"), Some(id)) => {
                            // Check if file exists (basic check)
                            is_valid = file_service
                                .get_file_by_id(id)
                                .await
                                .ok()
                                .flatten()
                                .is_some();
                        }
                        (Some("collection"), Some(id)) => {
                            // Check if user has read access to the knowledge base
                            is_valid = knowledge_service
                                .check_access_by_user_id(id, &auth_user.id, "read", &user_group_ids)
                                .await
                                .unwrap_or(false);
                        }
                        _ => {
                            // Other types are always valid
                            is_valid = true;
                        }
                    }

                    if is_valid {
                        valid_files.push(file_obj.clone());
                    }
                }

                // Update the data with valid files only if something changed
                let new_count = valid_files.len();

                if new_count < original_count {
                    if let Some(data_obj) = data.as_object_mut() {
                        data_obj.insert("files".to_string(), serde_json::Value::Array(valid_files));
                    }

                    // Update the folder in the database with cleaned data
                    use crate::models::folder::FolderUpdateForm;
                    let _ = folder_service
                        .update_folder_by_id_and_user_id(
                            &folder.id,
                            &auth_user.id,
                            &FolderUpdateForm {
                                name: None,
                                data: Some(data.clone()),
                                meta: None,
                            },
                        )
                        .await;
                }
            }
        }
    }

    let response: Vec<FolderNameIdResponse> = folders
        .into_iter()
        .map(|f| FolderNameIdResponse::from(f))
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn create_folder(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<FolderForm>,
) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);

    // Check if folder with same name already exists
    let existing = folder_service
        .get_folder_by_parent_id_and_user_id_and_name(None, &auth_user.id, &payload.name)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Folder already exists".to_string()));
    }

    let folder = folder_service
        .insert_new_folder(&auth_user.id, &payload)
        .await?;

    Ok(HttpResponse::Ok().json(FolderModel::from(folder)))
}

async fn get_folder_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);

    let folder = folder_service
        .get_folder_by_id_and_user_id(&id, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

    Ok(HttpResponse::Ok().json(FolderModel::from(folder)))
}

async fn update_folder_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<FolderUpdateForm>,
) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);

    let folder = folder_service
        .get_folder_by_id_and_user_id(&id, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

    // If renaming, check for name conflicts
    if let Some(new_name) = &payload.name {
        let existing = folder_service
            .get_folder_by_parent_id_and_user_id_and_name(
                folder.parent_id.as_deref(),
                &auth_user.id,
                new_name,
            )
            .await?;

        if let Some(existing) = existing {
            if existing.id != *id {
                return Err(AppError::BadRequest("Folder already exists".to_string()));
            }
        }
    }

    let updated_folder = folder_service
        .update_folder_by_id_and_user_id(&id, &auth_user.id, &payload)
        .await?;

    Ok(HttpResponse::Ok().json(FolderModel::from(updated_folder)))
}

async fn update_folder_parent_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<FolderParentIdForm>,
) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);

    let folder = folder_service
        .get_folder_by_id_and_user_id(&id, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

    // Check if a folder with the same name already exists at the new parent location
    let existing = folder_service
        .get_folder_by_parent_id_and_user_id_and_name(
            payload.parent_id.as_deref(),
            &auth_user.id,
            &folder.name,
        )
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Folder already exists".to_string()));
    }

    let updated_folder = folder_service
        .update_folder_parent_id_by_id_and_user_id(&id, &auth_user.id, payload.parent_id.as_deref())
        .await?;

    Ok(HttpResponse::Ok().json(FolderModel::from(updated_folder)))
}

async fn update_folder_expanded_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<FolderIsExpandedForm>,
) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);

    // Verify folder exists and belongs to user
    let _ = folder_service
        .get_folder_by_id_and_user_id(&id, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

    let updated_folder = folder_service
        .update_folder_is_expanded_by_id_and_user_id(&id, &auth_user.id, payload.is_expanded)
        .await?;

    Ok(HttpResponse::Ok().json(FolderModel::from(updated_folder)))
}

async fn delete_folder_by_id(
    _req: HttpRequest,
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let folder_service = FolderService::new(&state.db);
    let chat_service = ChatService::new(&state.db);

    // Check if folder contains chats and user has permission to delete them
    let chat_count = folder_service
        .count_chats_by_folder_id_and_user_id(&id, &auth_user.id)
        .await?;

    if chat_count > 0 {
        let config = state.config.read().unwrap();
        let has_delete_permission = auth_user.role == "admin"
            || has_permission(&auth_user.id, "chat.delete", &config.user_permissions);

        if !has_delete_permission {
            return Err(AppError::Forbidden("Access prohibited".to_string()));
        }
    }

    // Verify folder exists and belongs to user
    let _ = folder_service
        .get_folder_by_id_and_user_id(&id, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

    // Delete folder and all its children
    let folder_ids = folder_service
        .delete_folder_by_id_and_user_id(&id, &auth_user.id)
        .await?;

    // Delete all chats in deleted folders
    for folder_id in &folder_ids {
        chat_service
            .delete_chats_by_folder_id(folder_id, &auth_user.id)
            .await?;
    }

    Ok(HttpResponse::Ok().json(true))
}
