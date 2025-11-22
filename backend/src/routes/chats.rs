use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::error::AppResult;
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::chat::{ChatResponse, CreateChatRequest, UpdateChatRequest};
use crate::services::chat::ChatService;
use crate::AppState;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chats))
            .route(web::delete().to(delete_all_user_chats)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chats))
            .route(web::delete().to(delete_all_user_chats)),
    )
    .service(
        web::resource("/list")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chat_list)),
    )
    .service(
        web::resource("/list/user/{user_id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_user_chat_list_by_user_id)),
    )
    .service(
        web::resource("/search")
            .wrap(AuthMiddleware)
            .route(web::get().to(search_user_chats)),
    )
    .service(
        web::resource("/all")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_all_chats)),
    )
    .service(
        web::resource("/all/archived")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_user_archived_chats)),
    )
    .service(
        web::resource("/all/db")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_all_user_chats)),
    )
    .service(
        web::resource("/all/tags")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_all_tags)),
    )
    .service(
        web::resource("/archived")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_archived_session_user_chat_list)),
    )
    .service(
        web::resource("/pinned")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_pinned_chats)),
    )
    .service(
        web::resource("/folder/{folder_id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chats_by_folder_id)),
    )
    .service(
        web::resource("/folder/{folder_id}/list")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chat_list_by_folder_id)),
    )
    .service(
        web::resource("/new")
            .wrap(AuthMiddleware)
            .route(web::post().to(create_new_chat)),
    )
    .service(
        web::resource("/import")
            .wrap(AuthMiddleware)
            .route(web::post().to(import_chat)),
    )
    .service(
        web::resource("/tags")
            .wrap(AuthMiddleware)
            .route(web::post().to(get_user_chat_list_by_tag_name)),
    )
    .service(
        web::resource("/share/{share_id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_shared_chat_by_id)),
    )
    .service(
        web::resource("/archive/all")
            .wrap(AuthMiddleware)
            .route(web::post().to(archive_all_chats)),
    )
    .service(
        web::resource("/unarchive/all")
            .wrap(AuthMiddleware)
            .route(web::post().to(unarchive_all_chats)),
    )
    .service(
        web::resource("/{id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chat))
            .route(web::post().to(update_chat))
            .route(web::delete().to(delete_chat)),
    )
    .service(
        web::resource("/{id}/pinned")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chat_pinned_status)),
    )
    .service(
        web::resource("/{id}/pin")
            .wrap(AuthMiddleware)
            .route(web::post().to(toggle_chat_pinned)),
    )
    .service(
        web::resource("/{id}/archive")
            .wrap(AuthMiddleware)
            .route(web::post().to(toggle_chat_archived)),
    )
    .service(
        web::resource("/{id}/clone")
            .wrap(AuthMiddleware)
            .route(web::post().to(clone_chat_by_id)),
    )
    .service(
        web::resource("/{id}/clone/shared")
            .wrap(AuthMiddleware)
            .route(web::post().to(clone_shared_chat_by_id)),
    )
    .service(
        web::resource("/{id}/share")
            .wrap(AuthMiddleware)
            .route(web::post().to(share_chat))
            .route(web::delete().to(delete_share_chat)),
    )
    .service(
        web::resource("/{id}/folder")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_chat_folder_id_by_id)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_chat_message_by_id)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}/event")
            .wrap(AuthMiddleware)
            .route(web::post().to(send_chat_message_event_by_id)),
    )
    .service(
        web::resource("/{id}/tags")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_chat_tags))
            .route(web::post().to(add_tag_by_id_and_tag_name))
            .route(web::delete().to(delete_tag_by_id_and_tag_name)),
    )
    .service(
        web::resource("/{id}/tags/all")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_all_tags_by_id)),
    );
}

#[derive(Debug, Deserialize)]
pub struct ChatFormData {
    pub chat: serde_json::Value,
    pub folder_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportChatRequest {
    pub chat: serde_json::Value,
    pub folder_id: Option<String>,
    pub pinned: Option<bool>,
    pub meta: Option<serde_json::Value>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ChatListQuery {
    pub page: Option<i64>,
}

async fn get_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ChatListQuery>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let page = query.page.unwrap_or(1);
    let limit = 50;
    let skip = (page - 1) * limit;

    let chats = service
        .get_chats_by_user_id(&auth_user.id, false, skip, limit)
        .await?;

    let responses: Vec<ChatResponse> = chats.into_iter().map(|c| c.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

async fn get_all_chats(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chats = service
        .get_chats_by_user_id(&auth_user.id, true, 0, 100)
        .await?;

    let responses: Vec<ChatResponse> = chats.into_iter().map(|c| c.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

async fn get_all_user_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chats = service
        .get_chats_by_user_id(&auth_user.id, false, 0, 1000)
        .await?;

    let responses: Vec<ChatResponse> = chats.into_iter().map(|c| c.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

async fn get_all_tags(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // TODO: Implement actual tags fetching from database
    // For now, return empty array to prevent frontend errors
    Ok(HttpResponse::Ok().json(json!([])))
}

async fn get_pinned_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chats = service.get_pinned_chats_by_user_id(&auth_user.id).await?;

    let responses: Vec<ChatResponse> = chats.into_iter().map(|c| c.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

async fn create_new_chat(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<ChatFormData>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);

    let id = uuid::Uuid::new_v4().to_string();
    let title = payload
        .chat
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let req = CreateChatRequest {
        id: id.clone(),
        title,
        chat: payload.chat.clone(),
        folder_id: payload.folder_id.clone(),
        archived: Some(false),
        pinned: Some(false),
        share_id: None,
        meta: None,
    };

    let chat = service.create_chat(&auth_user.id, req).await?;
    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

async fn import_chat(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    payload: web::Json<ImportChatRequest>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);

    let id = uuid::Uuid::new_v4().to_string();
    let title = payload
        .chat
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let req = CreateChatRequest {
        id: id.clone(),
        title,
        chat: payload.chat.clone(),
        folder_id: payload.folder_id.clone(),
        archived: Some(false),
        pinned: payload.pinned,
        share_id: None,
        meta: payload.meta.clone(),
    };

    let chat = service.create_chat(&auth_user.id, req).await?;
    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

async fn get_chat(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .get_chat_by_id_and_user_id(&id, &auth_user.id)
        .await?;

    match chat {
        Some(c) => {
            let response: ChatResponse = c.into();
            Ok(HttpResponse::Ok().json(response))
        }
        None => Ok(HttpResponse::NotFound().json(json!({"error": "Chat not found"}))),
    }
}

async fn get_chat_pinned_status(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .get_chat_by_id_and_user_id(&id, &auth_user.id)
        .await?;

    match chat {
        Some(c) => Ok(HttpResponse::Ok().json(c.pinned)),
        None => Ok(HttpResponse::NotFound().json(json!(null))),
    }
}

async fn update_chat(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    payload: web::Json<ChatFormData>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);

    let title = payload
        .chat
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let req = UpdateChatRequest {
        title,
        chat: Some(payload.chat.clone()),
        folder_id: payload.folder_id.clone(),
        archived: None,
        pinned: None,
    };

    let chat = service.update_chat(&id, &auth_user.id, req).await?;
    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_chat(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    service.delete_chat(&id, &auth_user.id).await?;
    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

async fn toggle_chat_pinned(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service.toggle_chat_pinned(&id, &auth_user.id).await?;
    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

async fn toggle_chat_archived(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service.toggle_chat_archived(&id, &auth_user.id).await?;
    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

async fn share_chat(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let share_id = service.create_shared_chat(&id).await?;
    Ok(HttpResponse::Ok().json(json!({"share_id": share_id})))
}

async fn delete_share_chat(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    service.delete_shared_chat(&id).await?;
    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

async fn archive_all_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    service.archive_all_chats(&auth_user.id).await?;
    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

async fn unarchive_all_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    service.unarchive_all_chats(&auth_user.id).await?;
    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

async fn delete_all_user_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    service.delete_all_chats_by_user_id(&auth_user.id).await?;
    Ok(HttpResponse::Ok().json(true))
}

#[derive(Debug, Deserialize)]
pub struct ChatListQueryParams {
    pub page: Option<i64>,
    pub include_folders: Option<bool>,
}

async fn get_chat_list(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ChatListQueryParams>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let page = query.page.unwrap_or(1);
    let limit = 60;
    let skip = (page - 1) * limit;

    let chats = service
        .get_chat_title_id_list_by_user_id(&auth_user.id, skip, limit)
        .await?;
    Ok(HttpResponse::Ok().json(chats))
}

#[derive(Debug, Deserialize)]
pub struct UserChatListQuery {
    pub page: Option<i64>,
    pub query: Option<String>,
    pub order_by: Option<String>,
    pub direction: Option<String>,
}

async fn get_user_chat_list_by_user_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    user_id: web::Path<String>,
    query: web::Query<UserChatListQuery>,
) -> AppResult<HttpResponse> {
    // Check if user is admin
    if auth_user.role != "admin" {
        return Ok(HttpResponse::Unauthorized().json(json!({"error": "Access prohibited"})));
    }

    let service = ChatService::new(&state.db);
    let page = query.page.unwrap_or(1);
    let limit = 60;
    let skip = (page - 1) * limit;

    let chats = service
        .get_chat_list_by_user_id(&user_id, skip, limit)
        .await?;
    Ok(HttpResponse::Ok().json(chats))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub page: Option<i64>,
}

async fn search_user_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<SearchQuery>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let page = query.page.unwrap_or(1);
    let limit = 60;
    let skip = (page - 1) * limit;

    let chats = service
        .search_chats_by_user_id(&auth_user.id, &query.text, skip, limit)
        .await?;
    Ok(HttpResponse::Ok().json(chats))
}

async fn get_user_archived_chats(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chats = service.get_archived_chats_by_user_id(&auth_user.id).await?;

    let responses: Vec<ChatResponse> = chats.into_iter().map(|c| c.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

#[derive(Debug, Deserialize)]
pub struct ArchivedChatListQuery {
    pub page: Option<i64>,
    pub query: Option<String>,
    pub order_by: Option<String>,
    pub direction: Option<String>,
}

async fn get_archived_session_user_chat_list(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    query: web::Query<ArchivedChatListQuery>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let page = query.page.unwrap_or(1);
    let limit = 60;
    let skip = (page - 1) * limit;

    let chats = service
        .get_archived_chat_list_by_user_id(&auth_user.id, skip, limit)
        .await?;
    Ok(HttpResponse::Ok().json(chats))
}

async fn get_chats_by_folder_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    folder_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chats = service
        .get_chats_by_folder_id_full(&folder_id, &auth_user.id)
        .await?;

    let responses: Vec<ChatResponse> = chats.into_iter().map(|c| c.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

#[derive(Debug, Deserialize)]
pub struct FolderChatListQuery {
    pub page: Option<i64>,
}

async fn get_chat_list_by_folder_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    folder_id: web::Path<String>,
    query: web::Query<FolderChatListQuery>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let page = query.page.unwrap_or(1);
    let limit = 60;
    let skip = (page - 1) * limit;

    let chats = service
        .get_chat_list_by_folder_id(&folder_id, &auth_user.id, skip, limit)
        .await?;
    Ok(HttpResponse::Ok().json(chats))
}

#[derive(Debug, Deserialize)]
pub struct TagForm {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct TagFilterForm {
    pub name: String,
    pub skip: Option<i64>,
    pub limit: Option<i64>,
}

async fn get_user_chat_list_by_tag_name(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<TagFilterForm>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let skip = form_data.skip.unwrap_or(0);
    let limit = form_data.limit.unwrap_or(50);

    let chats = service
        .get_chat_list_by_user_id_and_tag_name(&auth_user.id, &form_data.name, skip, limit)
        .await?;
    Ok(HttpResponse::Ok().json(chats))
}

async fn get_shared_chat_by_id(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    share_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service.get_chat_by_share_id(&share_id).await?;

    match chat {
        Some(c) => {
            let response: ChatResponse = c.into();
            Ok(HttpResponse::Ok().json(response))
        }
        None => Ok(HttpResponse::NotFound().json(json!({"error": "Chat not found"}))),
    }
}

#[derive(Debug, Deserialize)]
pub struct CloneForm {
    pub title: Option<String>,
}

async fn clone_chat_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form_data: web::Json<CloneForm>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .get_chat_by_id_and_user_id(&id, &auth_user.id)
        .await?;

    match chat {
        Some(chat) => {
            let cloned_chat = service
                .clone_chat(&auth_user.id, chat, form_data.title.clone())
                .await?;
            let response: ChatResponse = cloned_chat.into();
            Ok(HttpResponse::Ok().json(response))
        }
        None => Ok(HttpResponse::Unauthorized().json(json!({"error": "Chat not found"}))),
    }
}

async fn clone_shared_chat_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);

    // Get chat by ID if admin, otherwise by share_id
    let chat = if auth_user.role == "admin" {
        service.get_chat_by_id(&id).await?
    } else {
        service.get_chat_by_share_id(&id).await?
    };

    match chat {
        Some(chat) => {
            let title = format!("Clone of {}", &chat.title);
            let cloned_chat = service.clone_chat(&auth_user.id, chat, Some(title)).await?;
            let response: ChatResponse = cloned_chat.into();
            Ok(HttpResponse::Ok().json(response))
        }
        None => Ok(HttpResponse::Unauthorized().json(json!({"error": "Chat not found"}))),
    }
}

#[derive(Debug, Deserialize)]
pub struct ChatFolderIdForm {
    pub folder_id: Option<String>,
}

async fn update_chat_folder_id_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form_data: web::Json<ChatFolderIdForm>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .update_chat_folder(&id, &auth_user.id, form_data.folder_id.clone())
        .await?;
    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
pub struct MessageForm {
    pub content: String,
}

async fn update_chat_message_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
    form_data: web::Json<MessageForm>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();
    let service = ChatService::new(&state.db);

    let chat = service
        .update_chat_message(&id, &message_id, &auth_user.id, &form_data.content)
        .await?;

    // TODO: Emit socket event for message update

    let response: ChatResponse = chat.into();
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
pub struct EventForm {
    pub r#type: String,
    pub data: serde_json::Value,
}

async fn send_chat_message_event_by_id(
    _state: web::Data<AppState>,
    _auth_user: AuthUser,
    _path: web::Path<(String, String)>,
    _form_data: web::Json<EventForm>,
) -> AppResult<HttpResponse> {
    // TODO: Implement socket event emission
    Ok(HttpResponse::Ok().json(true))
}

async fn get_chat_tags(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .get_chat_by_id_and_user_id(&id, &auth_user.id)
        .await?;

    match chat {
        Some(chat) => {
            // Extract tags from chat meta
            let tags = if let Some(meta) = chat.meta {
                if let Some(tags_array) = meta.get("tags").and_then(|v| v.as_array()) {
                    tags_array
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            // For now, return empty array of tag objects
            // In the future, this should fetch actual tag objects from a tags table
            let tag_objects: Vec<serde_json::Value> = tags
                .iter()
                .map(|name| {
                    json!({
                        "name": name,
                    })
                })
                .collect();

            Ok(HttpResponse::Ok().json(tag_objects))
        }
        None => Ok(HttpResponse::NotFound().json(json!({"error": "Chat not found"}))),
    }
}

async fn add_tag_by_id_and_tag_name(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form_data: web::Json<TagForm>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .add_chat_tag(&id, &auth_user.id, &form_data.name)
        .await?;

    // Return updated tags
    let tags = if let Some(meta) = chat.meta {
        if let Some(tags_array) = meta.get("tags").and_then(|v| v.as_array()) {
            tags_array
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let tag_objects: Vec<serde_json::Value> = tags
        .iter()
        .map(|name| {
            json!({
                "name": name,
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(tag_objects))
}

async fn delete_tag_by_id_and_tag_name(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form_data: web::Json<TagForm>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    let chat = service
        .delete_chat_tag(&id, &auth_user.id, &form_data.name)
        .await?;

    // Return updated tags
    let tags = if let Some(meta) = chat.meta {
        if let Some(tags_array) = meta.get("tags").and_then(|v| v.as_array()) {
            tags_array
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let tag_objects: Vec<serde_json::Value> = tags
        .iter()
        .map(|name| {
            json!({
                "name": name,
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(tag_objects))
}

async fn delete_all_tags_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = ChatService::new(&state.db);
    service.delete_all_chat_tags(&id, &auth_user.id).await?;
    Ok(HttpResponse::Ok().json(true))
}
