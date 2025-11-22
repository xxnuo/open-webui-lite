use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use crate::error::{AppError, AppResult};
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::message::{MessageForm, MessageResponse};
use crate::models::user::User;
use crate::services::channel::ChannelService;
use crate::services::message::MessageService;
use crate::services::user::UserService;
use crate::AppState;

#[derive(Debug, Serialize)]
struct ChannelResponse {
    id: String,
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    channel_type: Option<String>,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_control: Option<serde_json::Value>,
    created_at: i64,
    updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    write_access: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
struct ChannelForm {
    #[validate(length(min = 1))]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
    #[serde(default)]
    access_control: Option<serde_json::Value>,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_channels)),
    )
    .service(
        web::resource("/")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_channels)),
    )
    .service(
        web::resource("/list")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_all_channels)),
    )
    .service(
        web::resource("/create")
            .wrap(AuthMiddleware)
            .route(web::post().to(create_new_channel)),
    )
    .service(
        web::resource("/{id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_channel_by_id)),
    )
    .service(
        web::resource("/{id}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_channel_by_id)),
    )
    .service(
        web::resource("/{id}/delete")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_channel_by_id)),
    )
    // Message routes
    .service(
        web::resource("/{id}/messages")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_channel_messages)),
    )
    .service(
        web::resource("/{id}/messages/post")
            .wrap(AuthMiddleware)
            .route(web::post().to(post_new_message)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_channel_message)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}/thread")
            .wrap(AuthMiddleware)
            .route(web::get().to(get_channel_thread_messages)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}/update")
            .wrap(AuthMiddleware)
            .route(web::post().to(update_message_by_id)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}/delete")
            .wrap(AuthMiddleware)
            .route(web::delete().to(delete_message_by_id)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}/reactions/add")
            .wrap(AuthMiddleware)
            .route(web::post().to(add_reaction_to_message)),
    )
    .service(
        web::resource("/{id}/messages/{message_id}/reactions/remove")
            .wrap(AuthMiddleware)
            .route(web::post().to(remove_reaction_from_message)),
    );
}

async fn get_channels(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let channel_service = ChannelService::new(&state.db);
    let channels = channel_service
        .get_channels_by_user_id(&auth_user.user.id)
        .await?;

    let response: Vec<ChannelResponse> = channels
        .iter()
        .map(|channel| ChannelResponse {
            id: channel.id.clone(),
            user_id: channel.user_id.clone(),
            channel_type: channel.channel_type.clone(),
            name: channel.name.clone(),
            description: channel.description.clone(),
            data: channel.data.clone(),
            meta: channel.meta.clone(),
            access_control: channel.access_control.clone(),
            created_at: channel.created_at,
            updated_at: channel.updated_at,
            write_access: None,
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn get_all_channels(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let channel_service = ChannelService::new(&state.db);
    let channels = if auth_user.user.role == "admin" {
        channel_service.get_all_channels().await?
    } else {
        channel_service
            .get_channels_by_user_id(&auth_user.user.id)
            .await?
    };

    let response: Vec<ChannelResponse> = channels
        .iter()
        .map(|channel| ChannelResponse {
            id: channel.id.clone(),
            user_id: channel.user_id.clone(),
            channel_type: channel.channel_type.clone(),
            name: channel.name.clone(),
            description: channel.description.clone(),
            data: channel.data.clone(),
            meta: channel.meta.clone(),
            access_control: channel.access_control.clone(),
            created_at: channel.created_at,
            updated_at: channel.updated_at,
            write_access: None,
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

async fn create_new_channel(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form: web::Json<ChannelForm>,
) -> AppResult<HttpResponse> {
    // Only admin can create channels
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    form.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let channel_service = ChannelService::new(&state.db);

    // Generate ID
    let id = uuid::Uuid::new_v4().to_string();

    let channel = channel_service
        .create_channel(
            &id,
            &auth_user.user.id,
            &form.name.to_lowercase(),
            form.description.as_deref(),
            None, // type
            form.data.clone(),
            form.meta.clone(),
            form.access_control.clone(),
        )
        .await?;

    let response = ChannelResponse {
        id: channel.id.clone(),
        user_id: channel.user_id.clone(),
        channel_type: channel.channel_type.clone(),
        name: channel.name.clone(),
        description: channel.description.clone(),
        data: channel.data.clone(),
        meta: channel.meta.clone(),
        access_control: channel.access_control.clone(),
        created_at: channel.created_at,
        updated_at: channel.updated_at,
        write_access: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

async fn get_channel_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Check access control
    let write_access = if auth_user.user.role == "admin" || channel.user_id == auth_user.user.id {
        true
    } else {
        crate::utils::access_control::has_access(
            &state.db,
            &auth_user.user.id,
            "write",
            channel.access_control.as_ref(),
            false,
        )
        .await
        .unwrap_or(false)
    };

    let response = ChannelResponse {
        id: channel.id.clone(),
        user_id: channel.user_id.clone(),
        channel_type: channel.channel_type.clone(),
        name: channel.name.clone(),
        description: channel.description.clone(),
        data: channel.data.clone(),
        meta: channel.meta.clone(),
        access_control: channel.access_control.clone(),
        created_at: channel.created_at,
        updated_at: channel.updated_at,
        write_access: Some(write_access),
    };

    Ok(HttpResponse::Ok().json(response))
}

async fn update_channel_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<ChannelForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let channel_service = ChannelService::new(&state.db);
    let _channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    form.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let updated_channel = channel_service
        .update_channel(
            &id,
            Some(&form.name.to_lowercase()),
            form.description.as_deref(),
            None, // type
            form.data.clone(),
            form.meta.clone(),
            form.access_control.clone(),
        )
        .await?;

    let response = ChannelResponse {
        id: updated_channel.id.clone(),
        user_id: updated_channel.user_id.clone(),
        channel_type: updated_channel.channel_type.clone(),
        name: updated_channel.name.clone(),
        description: updated_channel.description.clone(),
        data: updated_channel.data.clone(),
        meta: updated_channel.meta.clone(),
        access_control: updated_channel.access_control.clone(),
        created_at: updated_channel.created_at,
        updated_at: updated_channel.updated_at,
        write_access: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

async fn delete_channel_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let channel_service = ChannelService::new(&state.db);
    let _channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    channel_service.delete_channel(&id).await?;

    Ok(HttpResponse::Ok().json(true))
}

// Message response with user information
#[derive(Debug, Serialize)]
struct MessageUserResponse {
    #[serde(flatten)]
    message: MessageResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<UserNameResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to_message: Option<Box<MessageUserResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_reply_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reactions: Option<Vec<crate::models::message::Reaction>>,
}

#[derive(Debug, Serialize)]
struct UserNameResponse {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_image_url: Option<String>,
}

impl From<User> for UserNameResponse {
    fn from(user: User) -> Self {
        UserNameResponse {
            id: user.id,
            name: user.name,
            email: Some(user.email),
            profile_image_url: Some(user.profile_image_url),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PaginationQuery {
    #[serde(default)]
    skip: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn get_channel_messages(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    query: web::Query<PaginationQuery>,
) -> AppResult<HttpResponse> {
    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Check read access
    if auth_user.user.role != "admin" && channel.user_id != auth_user.user.id {
        let has_read_access = crate::utils::access_control::has_access(
            &state.db,
            &auth_user.user.id,
            "read",
            channel.access_control.as_ref(),
            false,
        )
        .await?;

        if !has_read_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let message_service = MessageService::new(&state.db);
    let messages = message_service
        .get_messages_by_channel_id(&id, query.skip, query.limit)
        .await?;

    let mut response = Vec::new();
    for message in messages {
        let message_response = message_service.to_message_response(message.clone()).await?;
        let reply_count = message_service
            .get_thread_replies_count(&message.id)
            .await
            .ok();
        let latest_reply_at = message_service
            .get_latest_thread_reply_at(&message.id)
            .await
            .ok()
            .flatten();
        let reactions = message_service.get_reactions(&message.id).await.ok();

        response.push(MessageUserResponse {
            message: message_response,
            user: None, // User is already in message_response
            reply_to_message: None,
            reply_count,
            latest_reply_at,
            reactions,
        });
    }

    Ok(HttpResponse::Ok().json(response))
}

async fn post_new_message(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    id: web::Path<String>,
    form: web::Json<MessageForm>,
) -> AppResult<HttpResponse> {
    let channel_id = id.into_inner();
    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&channel_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Check write access
    if auth_user.user.role != "admin" && channel.user_id != auth_user.user.id {
        let has_write_access = crate::utils::access_control::has_access(
            &state.db,
            &auth_user.user.id,
            "write",
            channel.access_control.as_ref(),
            false,
        )
        .await?;

        if !has_write_access {
            return Err(AppError::Forbidden("Write access denied".to_string()));
        }
    }

    let message_service = MessageService::new(&state.db);
    let message = message_service
        .create_message(&channel_id, &auth_user.user.id, &form)
        .await?;

    // Convert to MessageResponse with user information
    let message_response = message_service.to_message_response(message.clone()).await?;

    // Emit Socket.IO event for real-time updates
    if let Some(ref socketio_handler) = state.socketio_handler {
        let user_service = UserService::new(&state.db);
        if let Some(user) = user_service
            .get_user_by_id(&auth_user.user.id)
            .await
            .ok()
            .flatten()
        {
            let event_data = json!({
                "channel_id": &channel_id,
                "message_id": &message.id,
                "data": {
                    "type": "message",
                    "data": &message_response,
                },
                "user": UserNameResponse::from(user.clone()),
                "channel": {
                    "id": channel.id,
                    "name": channel.name,
                }
            });

            // Broadcast to all users in the channel room
            let room = format!("channel:{}", channel_id);
            let _ = socketio_handler
                .broadcast_to_room(&room, "channel-events", event_data, None)
                .await;

            // If this is a reply to a parent message, emit a separate event for the parent
            if let Some(ref parent_id) = message.parent_id {
                if let Some(parent_message) = message_service
                    .get_message_by_id(parent_id)
                    .await
                    .ok()
                    .flatten()
                {
                    let parent_message_response =
                        message_service.to_message_response(parent_message).await?;
                    let parent_event_data = json!({
                        "channel_id": &channel_id,
                        "message_id": parent_id,
                        "data": {
                            "type": "message:reply",
                            "data": parent_message_response,
                        },
                        "user": UserNameResponse::from(user),
                        "channel": {
                            "id": channel.id,
                            "name": channel.name,
                        }
                    });
                    let _ = socketio_handler
                        .broadcast_to_room(&room, "channel-events", parent_event_data, None)
                        .await;
                }
            }
        }
    }

    Ok(HttpResponse::Ok().json(message_response))
}

async fn get_channel_message(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();

    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Check read access
    if auth_user.user.role != "admin" && channel.user_id != auth_user.user.id {
        let has_read_access = crate::utils::access_control::has_access(
            &state.db,
            &auth_user.user.id,
            "read",
            channel.access_control.as_ref(),
            false,
        )
        .await?;

        if !has_read_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let message_service = MessageService::new(&state.db);
    let message = message_service
        .get_message_by_id(&message_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    if message.channel_id.as_ref() != Some(&id) {
        return Err(AppError::BadRequest(
            "Message does not belong to this channel".to_string(),
        ));
    }

    let message_response = message_service.to_message_response(message).await?;

    Ok(HttpResponse::Ok().json(MessageUserResponse {
        message: message_response,
        user: None, // User is already in message_response
        reply_to_message: None,
        reply_count: None,
        latest_reply_at: None,
        reactions: None,
    }))
}

async fn get_channel_thread_messages(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
    query: web::Query<PaginationQuery>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();

    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Check read access
    if auth_user.user.role != "admin" && channel.user_id != auth_user.user.id {
        let has_read_access = crate::utils::access_control::has_access(
            &state.db,
            &auth_user.user.id,
            "read",
            channel.access_control.as_ref(),
            false,
        )
        .await?;

        if !has_read_access {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let message_service = MessageService::new(&state.db);
    let messages = message_service
        .get_thread_messages(&id, &message_id, query.skip, query.limit)
        .await?;

    let mut response = Vec::new();
    for message in messages {
        let message_response = message_service.to_message_response(message.clone()).await?;
        let reactions = message_service.get_reactions(&message.id).await.ok();

        response.push(MessageUserResponse {
            message: message_response,
            user: None, // User is already in message_response
            reply_to_message: None,
            reply_count: Some(0),
            latest_reply_at: None,
            reactions,
        });
    }

    Ok(HttpResponse::Ok().json(response))
}

async fn update_message_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
    form: web::Json<MessageForm>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();

    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let message_service = MessageService::new(&state.db);
    let message = message_service
        .get_message_by_id(&message_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    if message.channel_id.as_ref() != Some(&id) {
        return Err(AppError::BadRequest(
            "Message does not belong to this channel".to_string(),
        ));
    }

    // Check permission: admin, message owner, or channel owner
    if auth_user.user.role != "admin"
        && message.user_id != auth_user.user.id
        && channel.user_id != auth_user.user.id
    {
        return Err(AppError::Forbidden(
            "You don't have permission to update this message".to_string(),
        ));
    }

    let updated_message = message_service.update_message(&message_id, &form).await?;
    let message_response = message_service
        .to_message_response(updated_message.clone())
        .await?;

    // Emit Socket.IO event for real-time updates
    if let Some(ref socketio_handler) = state.socketio_handler {
        let user_service = UserService::new(&state.db);
        if let Some(user) = user_service
            .get_user_by_id(&auth_user.user.id)
            .await
            .ok()
            .flatten()
        {
            let event_data = json!({
                "channel_id": &id,
                "message_id": &message_id,
                "data": {
                    "type": "message:update",
                    "data": &message_response,
                },
                "user": UserNameResponse::from(user),
                "channel": {
                    "id": channel.id,
                    "name": channel.name,
                }
            });

            // Broadcast to all users in the channel room
            let room = format!("channel:{}", id);
            let _ = socketio_handler
                .broadcast_to_room(&room, "channel-events", event_data, None)
                .await;
        }
    }

    Ok(HttpResponse::Ok().json(message_response))
}

async fn delete_message_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();

    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let message_service = MessageService::new(&state.db);
    let message = message_service
        .get_message_by_id(&message_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    if message.channel_id.as_ref() != Some(&id) {
        return Err(AppError::BadRequest(
            "Message does not belong to this channel".to_string(),
        ));
    }

    // Check permission
    if auth_user.user.role != "admin"
        && message.user_id != auth_user.user.id
        && channel.user_id != auth_user.user.id
    {
        return Err(AppError::Forbidden(
            "You don't have permission to delete this message".to_string(),
        ));
    }

    message_service.delete_message(&message_id).await?;

    // Emit Socket.IO event for real-time updates
    if let Some(ref socketio_handler) = state.socketio_handler {
        let user_service = UserService::new(&state.db);
        if let Some(user) = user_service
            .get_user_by_id(&auth_user.user.id)
            .await
            .ok()
            .flatten()
        {
            let event_data = json!({
                "channel_id": &id,
                "message_id": &message_id,
                "data": {
                    "type": "message:delete",
                    "data": {
                        "id": &message_id,
                        "user": UserNameResponse::from(user.clone()),
                    },
                },
                "user": UserNameResponse::from(user),
                "channel": {
                    "id": channel.id,
                    "name": channel.name,
                }
            });

            // Broadcast to all users in the channel room
            let room = format!("channel:{}", id);
            let _ = socketio_handler
                .broadcast_to_room(&room, "channel-events", event_data, None)
                .await;
        }
    }

    Ok(HttpResponse::Ok().json(true))
}

#[derive(Debug, Deserialize)]
struct ReactionForm {
    name: String,
}

async fn add_reaction_to_message(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
    form: web::Json<ReactionForm>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();

    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let message_service = MessageService::new(&state.db);
    let message = message_service
        .get_message_by_id(&message_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    if message.channel_id.as_ref() != Some(&id) {
        return Err(AppError::BadRequest(
            "Message does not belong to this channel".to_string(),
        ));
    }

    message_service
        .add_reaction(&message_id, &auth_user.user.id, &form.name)
        .await?;

    // Emit Socket.IO event for real-time updates
    if let Some(ref socketio_handler) = state.socketio_handler {
        let user_service = UserService::new(&state.db);
        if let Some(user) = user_service
            .get_user_by_id(&auth_user.user.id)
            .await
            .ok()
            .flatten()
        {
            let reactions = message_service.get_reactions(&message_id).await.ok();

            let event_data = json!({
                "channel_id": &id,
                "message_id": &message_id,
                "data": {
                    "type": "message:reaction:add",
                    "data": {
                        "id": &message_id,
                        "name": &form.name,
                        "reactions": reactions,
                    },
                },
                "user": UserNameResponse::from(user),
                "channel": {
                    "id": channel.id,
                    "name": channel.name,
                }
            });

            // Broadcast to all users in the channel room
            let room = format!("channel:{}", id);
            let _ = socketio_handler
                .broadcast_to_room(&room, "channel-events", event_data, None)
                .await;
        }
    }

    Ok(HttpResponse::Ok().json(true))
}

async fn remove_reaction_from_message(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<(String, String)>,
    form: web::Json<ReactionForm>,
) -> AppResult<HttpResponse> {
    let (id, message_id) = path.into_inner();

    let channel_service = ChannelService::new(&state.db);
    let channel = channel_service
        .get_channel_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let message_service = MessageService::new(&state.db);
    let message = message_service
        .get_message_by_id(&message_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    if message.channel_id.as_ref() != Some(&id) {
        return Err(AppError::BadRequest(
            "Message does not belong to this channel".to_string(),
        ));
    }

    message_service
        .remove_reaction(&message_id, &auth_user.user.id, &form.name)
        .await?;

    // Emit Socket.IO event for real-time updates
    if let Some(ref socketio_handler) = state.socketio_handler {
        let user_service = UserService::new(&state.db);
        if let Some(user) = user_service
            .get_user_by_id(&auth_user.user.id)
            .await
            .ok()
            .flatten()
        {
            let reactions = message_service.get_reactions(&message_id).await.ok();

            let event_data = json!({
                "channel_id": &id,
                "message_id": &message_id,
                "data": {
                    "type": "message:reaction:remove",
                    "data": {
                        "id": &message_id,
                        "name": &form.name,
                        "reactions": reactions,
                    },
                },
                "user": UserNameResponse::from(user),
                "channel": {
                    "id": channel.id,
                    "name": channel.name,
                }
            });

            // Broadcast to all users in the channel room
            let room = format!("channel:{}", id);
            let _ = socketio_handler
                .broadcast_to_room(&room, "channel-events", event_data, None)
                .await;
        }
    }

    Ok(HttpResponse::Ok().json(true))
}
