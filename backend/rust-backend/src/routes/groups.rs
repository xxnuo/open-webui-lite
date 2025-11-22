use actix_web::{web, HttpResponse};

use crate::error::AppResult;
use crate::middleware::{AdminMiddleware, AuthMiddleware, AuthUser};
use crate::models::group::{GroupForm, GroupResponse, GroupUpdateForm, UserIdsForm};
use crate::services::group::GroupService;
use crate::services::user::UserService;
use crate::AppState;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_groups)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_groups)),
    )
    .service(
        web::resource("/create")
            .wrap(AdminMiddleware)
            .route(web::post().to(create_new_group)),
    )
    .service(
        web::resource("/id/{id}")
            .wrap(AdminMiddleware)
            .route(web::get().to(get_group_by_id)),
    )
    .service(
        web::resource("/id/{id}/update")
            .wrap(AdminMiddleware)
            .route(web::post().to(update_group_by_id)),
    )
    .service(
        web::resource("/id/{id}/users/add")
            .wrap(AdminMiddleware)
            .route(web::post().to(add_users_to_group)),
    )
    .service(
        web::resource("/id/{id}/users/remove")
            .wrap(AdminMiddleware)
            .route(web::post().to(remove_users_from_group)),
    )
    .service(
        web::resource("/id/{id}/delete")
            .wrap(AdminMiddleware)
            .route(web::delete().to(delete_group_by_id)),
    );
}

async fn get_groups(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);

    let groups = if auth_user.role == "admin" {
        group_service.get_all_groups().await?
    } else {
        group_service.get_groups_by_member_id(&auth_user.id).await?
    };

    let response: Vec<GroupResponse> = groups.into_iter().map(GroupResponse::from).collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn create_new_group(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<GroupForm>,
) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);

    let group = group_service
        .insert_new_group(&auth_user.id, &payload)
        .await?;

    Ok(HttpResponse::Ok().json(GroupResponse::from(group)))
}

async fn get_group_by_id(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);

    let group = group_service
        .get_group_by_id(&id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound("Group not found".to_string()))?;

    Ok(HttpResponse::Ok().json(GroupResponse::from(group)))
}

async fn update_group_by_id(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<GroupUpdateForm>,
) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);
    let user_service = UserService::new(&state.db);

    // Validate user IDs if provided
    let mut form_data = payload.into_inner();
    if let Some(ref user_ids) = form_data.user_ids {
        let valid_ids = user_service.get_valid_user_ids(user_ids).await?;
        form_data.user_ids = Some(valid_ids);
    }

    let group = group_service.update_group_by_id(&id, &form_data).await?;

    Ok(HttpResponse::Ok().json(GroupResponse::from(group)))
}

async fn add_users_to_group(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<UserIdsForm>,
) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);
    let user_service = UserService::new(&state.db);

    // Validate user IDs
    let empty_vec = vec![];
    let user_ids = payload.user_ids.as_ref().unwrap_or(&empty_vec);
    let valid_ids = user_service.get_valid_user_ids(user_ids).await?;

    let group = group_service.add_users_to_group(&id, &valid_ids).await?;

    Ok(HttpResponse::Ok().json(GroupResponse::from(group)))
}

async fn remove_users_from_group(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<UserIdsForm>,
) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);

    let empty_vec = vec![];
    let user_ids = payload.user_ids.as_ref().unwrap_or(&empty_vec);

    let group = group_service.remove_users_from_group(&id, user_ids).await?;

    Ok(HttpResponse::Ok().json(GroupResponse::from(group)))
}

async fn delete_group_by_id(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let group_service = GroupService::new(&state.db);

    let result = group_service.delete_group_by_id(&id).await?;

    Ok(HttpResponse::Ok().json(result))
}
