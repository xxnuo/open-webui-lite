use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    middleware::{AuthMiddleware, AuthUser},
    models::note::{NoteForm, NoteModel, NoteTitleIdResponse, NoteUpdateForm, NoteUserResponse},
    services::{group::GroupService, note::NoteService, user::UserService},
    utils::misc::{has_access, has_permission},
    AppState,
};

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_notes)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_notes)),
    )
    .service(
        web::resource("/list")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_note_list)),
    )
    .service(
        web::resource("/create")
            .wrap(AuthMiddleware)
            .route(web::post().to(create_new_note)),
    )
    .service(
        web::resource("/{id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_note_by_id)),
    )
    .service(
        web::resource("/{id}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_note_by_id)),
    )
    .service(
        web::resource("/{id}/delete")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_note_by_id)),
    );
}

/// GET / - Get notes with permission filtering
async fn get_notes(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    // Check if user has notes feature permission
    let config = state.config.read().unwrap();
    if auth_user.user.role != "admin"
        && !has_permission(
            &auth_user.user.id,
            "features.notes",
            &config.user_permissions,
        )
    {
        return Err(AppError::Unauthorized(
            "User does not have permission for notes".to_string(),
        ));
    }
    drop(config);

    let note_service = NoteService::new(&state.db);
    let group_service = GroupService::new(&state.db);
    let user_service = UserService::new(&state.db);

    // Get user's groups
    let user_groups = group_service
        .get_groups_by_member_id(&auth_user.user.id)
        .await?;
    let user_group_ids: std::collections::HashSet<String> =
        user_groups.into_iter().map(|g| g.id).collect();

    // Get notes with write permission
    let notes = note_service
        .get_notes_by_permission(&auth_user.user.id, &user_group_ids, "write", None, None)
        .await?;

    let mut note_responses = Vec::new();
    for note in notes {
        let user = user_service.get_user_by_id(&note.user_id).await?;
        let user_json = user.map(|u| {
            serde_json::json!({
                "id": u.id,
                "name": u.name,
                "email": u.email,
                "role": u.role,
                "profile_image_url": u.profile_image_url
            })
        });

        note_responses.push(NoteUserResponse::from_note_and_user(note, user_json));
    }

    Ok(HttpResponse::Ok().json(note_responses))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    page: Option<i64>,
}

/// GET /list - Get note list (paginated)
async fn get_note_list(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ListQuery>,
) -> AppResult<HttpResponse> {
    // Check if user has notes feature permission
    let config = state.config.read().unwrap();
    if auth_user.user.role != "admin"
        && !has_permission(
            &auth_user.user.id,
            "features.notes",
            &config.user_permissions,
        )
    {
        return Err(AppError::Unauthorized(
            "User does not have permission for notes".to_string(),
        ));
    }
    drop(config);

    let note_service = NoteService::new(&state.db);
    let group_service = GroupService::new(&state.db);

    // Get user's groups
    let user_groups = group_service
        .get_groups_by_member_id(&auth_user.user.id)
        .await?;
    let user_group_ids: std::collections::HashSet<String> =
        user_groups.into_iter().map(|g| g.id).collect();

    // Calculate pagination
    let (skip, limit) = if let Some(page) = query.page {
        let limit = 60;
        let skip = (page - 1) * limit;
        (Some(skip), Some(limit))
    } else {
        (None, None)
    };

    // Get notes with write permission
    let notes = note_service
        .get_notes_by_permission(&auth_user.user.id, &user_group_ids, "write", skip, limit)
        .await?;

    let note_list: Vec<NoteTitleIdResponse> = notes
        .into_iter()
        .map(|note| NoteTitleIdResponse {
            id: note.id,
            title: note.title,
            updated_at: note.updated_at,
            created_at: note.created_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(note_list))
}

/// POST /create - Create new note
async fn create_new_note(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<NoteForm>,
) -> AppResult<HttpResponse> {
    // Check if user has notes feature permission
    let config = state.config.read().unwrap();
    if auth_user.user.role != "admin"
        && !has_permission(
            &auth_user.user.id,
            "features.notes",
            &config.user_permissions,
        )
    {
        return Err(AppError::Unauthorized(
            "User does not have permission for notes".to_string(),
        ));
    }
    drop(config);

    let note_service = NoteService::new(&state.db);
    let note = note_service
        .insert_new_note(&auth_user.user.id, &form_data)
        .await?;

    Ok(HttpResponse::Ok().json(NoteModel::from(note)))
}

/// GET /{id} - Get note by ID
async fn get_note_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let note_id = path.into_inner();

    // Check if user has notes feature permission
    let config = state.config.read().unwrap();
    if auth_user.user.role != "admin"
        && !has_permission(
            &auth_user.user.id,
            "features.notes",
            &config.user_permissions,
        )
    {
        return Err(AppError::Unauthorized(
            "User does not have permission for notes".to_string(),
        ));
    }
    drop(config);

    let note_service = NoteService::new(&state.db);
    let mut note = note_service
        .get_note_by_id(&note_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Note not found".to_string()))?;

    note.parse_json_fields();

    // Check access
    if auth_user.user.role != "admin" {
        if auth_user.user.id != note.user_id {
            let group_service = GroupService::new(&state.db);
            let user_groups = group_service
                .get_groups_by_member_id(&auth_user.user.id)
                .await?;
            let user_group_ids: std::collections::HashSet<String> =
                user_groups.into_iter().map(|g| g.id).collect();

            if !has_access(
                &auth_user.user.id,
                "read",
                &note.access_control,
                &user_group_ids,
            ) {
                return Err(AppError::Forbidden("Access denied".to_string()));
            }
        }
    }

    Ok(HttpResponse::Ok().json(NoteModel::from(note)))
}

/// POST /{id}/update - Update note by ID
async fn update_note_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<String>,
    mut form_data: web::Json<NoteUpdateForm>,
) -> AppResult<HttpResponse> {
    let note_id = path.into_inner();

    // Check if user has notes feature permission
    let config = state.config.read().unwrap();
    if auth_user.user.role != "admin"
        && !has_permission(
            &auth_user.user.id,
            "features.notes",
            &config.user_permissions,
        )
    {
        return Err(AppError::Unauthorized(
            "User does not have permission for notes".to_string(),
        ));
    }
    let can_share_public = has_permission(
        &auth_user.user.id,
        "sharing.public_notes",
        &config.user_permissions,
    );
    drop(config);

    let note_service = NoteService::new(&state.db);
    let mut note = note_service
        .get_note_by_id(&note_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Note not found".to_string()))?;

    note.parse_json_fields();

    // Check access
    if auth_user.user.role != "admin" {
        if auth_user.user.id != note.user_id {
            let group_service = GroupService::new(&state.db);
            let user_groups = group_service
                .get_groups_by_member_id(&auth_user.user.id)
                .await?;
            let user_group_ids: std::collections::HashSet<String> =
                user_groups.into_iter().map(|g| g.id).collect();

            if !has_access(
                &auth_user.user.id,
                "write",
                &note.access_control,
                &user_group_ids,
            ) {
                return Err(AppError::Forbidden("Access denied".to_string()));
            }
        }
    }

    // Check if user can share publicly
    if auth_user.user.role != "admin" && form_data.access_control.is_none() && !can_share_public {
        form_data.access_control = Some(serde_json::json!({}));
    }

    let updated_note = note_service.update_note_by_id(&note_id, &form_data).await?;

    // Emit Socket.IO event to notify all connected clients
    if let Some(event_handler) = &state.socketio_handler {
        let note_model = NoteModel::from(updated_note.clone());
        let note_json = serde_json::to_value(&note_model).unwrap_or(serde_json::json!({}));
        let room = format!("note:{}", note_id);

        if let Err(e) = event_handler
            .broadcast_to_room(&room, "note-events", note_json, None)
            .await
        {
            tracing::warn!("Failed to emit note-events: {}", e);
        }
    }

    Ok(HttpResponse::Ok().json(NoteModel::from(updated_note)))
}

/// DELETE /{id}/delete - Delete note by ID
async fn delete_note_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let note_id = path.into_inner();

    // Check if user has notes feature permission
    let config = state.config.read().unwrap();
    if auth_user.user.role != "admin"
        && !has_permission(
            &auth_user.user.id,
            "features.notes",
            &config.user_permissions,
        )
    {
        return Err(AppError::Unauthorized(
            "User does not have permission for notes".to_string(),
        ));
    }
    drop(config);

    let note_service = NoteService::new(&state.db);
    let mut note = note_service
        .get_note_by_id(&note_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Note not found".to_string()))?;

    note.parse_json_fields();

    // Check access
    if auth_user.user.role != "admin" {
        if auth_user.user.id != note.user_id {
            let group_service = GroupService::new(&state.db);
            let user_groups = group_service
                .get_groups_by_member_id(&auth_user.user.id)
                .await?;
            let user_group_ids: std::collections::HashSet<String> =
                user_groups.into_iter().map(|g| g.id).collect();

            if !has_access(
                &auth_user.user.id,
                "write",
                &note.access_control,
                &user_group_ids,
            ) {
                return Err(AppError::Forbidden("Access denied".to_string()));
            }
        }
    }

    note_service.delete_note_by_id(&note_id).await?;

    Ok(HttpResponse::Ok().json(true))
}
