/// Socket.IO Event Handlers
///
/// Handles all Socket.IO events including:
/// - Authentication (user-join)
/// - Chat events (chat-events)
/// - Channel events (channel-events)
/// - Yjs collaborative editing (ydoc:*)
/// - Usage tracking
use crate::db::Database;
use crate::socketio::manager::SocketIOManager;
use crate::socketio::protocol::{EnginePacket, SocketPacket};
use crate::socketio::redis_adapter::RedisAdapter;
use crate::socketio::ydoc::YDocManager;
use actix_web::web;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Connection registry - maps session IDs to their websocket senders
/// This allows us to send messages to specific sessions
type ConnectionRegistry = Arc<RwLock<HashMap<String, tokio::sync::mpsc::UnboundedSender<String>>>>;

use crate::socketio::metrics::SocketIOMetrics;
use crate::socketio::presence::PresenceManager;
use crate::socketio::rate_limit::RateLimiter;
use crate::socketio::recovery::RecoveryManager;

/// Event handler for Socket.IO events
#[derive(Clone)]
pub struct EventHandler {
    manager: SocketIOManager,
    connections: ConnectionRegistry,
    auth_endpoint: String,
    ydoc_manager: YDocManager,
    redis_adapter: Option<Arc<RedisAdapter>>,
    metrics: SocketIOMetrics,
    rate_limiter: Arc<RateLimiter>,
    presence_manager: Arc<PresenceManager>,
    recovery_manager: Arc<RecoveryManager>,
    db: Database,
}

impl EventHandler {
    pub fn new(
        manager: SocketIOManager,
        auth_endpoint: String,
        ydoc_manager: YDocManager,
        redis_adapter: Option<Arc<RedisAdapter>>,
        metrics: SocketIOMetrics,
        rate_limiter: Arc<RateLimiter>,
        presence_manager: Arc<PresenceManager>,
        recovery_manager: Arc<RecoveryManager>,
        db: Database,
    ) -> Self {
        Self {
            manager,
            connections: Arc::new(RwLock::new(HashMap::new())),
            auth_endpoint,
            ydoc_manager,
            redis_adapter,
            metrics,
            rate_limiter,
            presence_manager,
            recovery_manager,
            db,
        }
    }

    /// Get metrics reference
    pub fn metrics(&self) -> &SocketIOMetrics {
        &self.metrics
    }

    /// Get rate limiter reference
    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }

    /// Get presence manager reference
    pub fn presence_manager(&self) -> &PresenceManager {
        &self.presence_manager
    }

    /// Get recovery manager reference
    pub fn recovery_manager(&self) -> &RecoveryManager {
        &self.recovery_manager
    }

    /// Get auth endpoint
    pub fn auth_endpoint(&self) -> &str {
        &self.auth_endpoint
    }

    /// Register a connection
    pub async fn register_connection(
        &self,
        sid: &str,
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    ) {
        let mut connections = self.connections.write().await;
        connections.insert(sid.to_string(), sender);
        drop(connections);

        // Record metrics
        self.metrics.record_connection().await;

        tracing::info!("Registered connection: {}", sid);
    }

    /// Unregister a connection
    pub async fn unregister_connection(&self, sid: &str) {
        // Get user ID before removing from connections
        let user_id = self
            .manager
            .get_session(sid)
            .await
            .and_then(|s| s.user_id());

        let mut connections = self.connections.write().await;
        connections.remove(sid);
        drop(connections); // Release lock before async operations

        // Record metrics
        self.metrics.record_disconnection().await;

        // Update presence if user was authenticated
        if let Some(uid) = &user_id {
            self.presence_manager.user_offline(uid).await;
        }

        // Clean up rate limiter
        self.rate_limiter.remove_session(sid).await;

        // Clean up user from all Yjs documents
        if let Err(e) = self.ydoc_manager.remove_user_from_all(sid).await {
            tracing::error!("Failed to remove user {} from ydoc documents: {}", sid, e);
        }

        tracing::info!("Unregistered connection: {}", sid);
    }

    /// Emit event to a specific session
    pub async fn emit_to_session(
        &self,
        sid: &str,
        event: &str,
        data: JsonValue,
    ) -> Result<(), String> {
        let connections = self.connections.read().await;
        if let Some(sender) = connections.get(sid) {
            let socket_packet = SocketPacket::event("/", event, data);
            let engine_packet = EnginePacket::message(socket_packet.encode().into_bytes());

            sender
                .send(engine_packet.encode())
                .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err(format!("Session not found: {}", sid))
        }
    }

    /// Emit event to all sessions of a user
    pub async fn emit_to_user(
        &self,
        user_id: &str,
        event: &str,
        data: JsonValue,
    ) -> Result<usize, String> {
        let sids = self.manager.get_user_sessions(user_id).await;
        let mut sent = 0;

        for sid in sids {
            if self
                .emit_to_session(&sid, event, data.clone())
                .await
                .is_ok()
            {
                sent += 1;
            }
        }

        Ok(sent)
    }

    /// Broadcast event to all sessions in a room
    pub async fn broadcast_to_room(
        &self,
        room: &str,
        event: &str,
        data: JsonValue,
        exclude_sid: Option<&str>,
    ) -> Result<usize, String> {
        // Broadcast to local sessions
        let sids = self.manager.get_room_sessions(room).await;
        let mut sent = 0;

        for sid in sids {
            if Some(sid.as_str()) == exclude_sid {
                continue;
            }

            if self
                .emit_to_session(&sid, event, data.clone())
                .await
                .is_ok()
            {
                sent += 1;
            }
        }

        // Publish to Redis for cross-server broadcasting
        if let Some(redis) = &self.redis_adapter {
            if let Err(e) = redis
                .publish_broadcast(
                    room.to_string(),
                    event.to_string(),
                    data.clone(),
                    exclude_sid.map(|s| s.to_string()),
                )
                .await
            {
                tracing::warn!("Failed to publish to Redis: {}", e);
            } else {
                tracing::debug!("Published broadcast to Redis for room: {}", room);
            }
        }

        Ok(sent)
    }

    /// Handle authentication (user-join event)
    pub async fn handle_user_join(
        &self,
        sid: &str,
        data: JsonValue,
        http_client: &reqwest::Client,
    ) -> Result<JsonValue, String> {
        let auth = data.get("auth").ok_or("Missing auth data")?;

        let token = auth
            .get("token")
            .and_then(|t| t.as_str())
            .ok_or("Missing token")?;

        // Authenticate with backend
        let auth_url = format!("{}/api/socketio/auth", self.auth_endpoint);

        let response = http_client
            .post(&auth_url)
            .json(&serde_json::json!({"token": token}))
            .send()
            .await
            .map_err(|e| format!("Auth request failed: {}", e))?;

        if response.status().is_success() {
            let user: JsonValue = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse user data: {}", e))?;

            // Set session user
            self.manager.set_session_user(sid, user.clone()).await?;

            let user_id = user
                .get("id")
                .and_then(|id| id.as_str())
                .ok_or("Missing user ID")?;

            // Update presence
            self.presence_manager.user_online(user_id).await;

            // Record metric
            self.metrics.record_event_received("user-join").await;

            tracing::info!(
                "User {} authenticated on session {}",
                user.get("email")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown"),
                sid
            );

            // Auto-join user to their channels (like Python backend)
            if let Err(e) = self.auto_join_user_channels(sid, user_id).await {
                tracing::warn!("Failed to auto-join user {} to channels: {}", user_id, e);
            }

            Ok(user)
        } else {
            Err(format!("Authentication failed: {}", response.status()))
        }
    }

    /// Auto-join user to all their accessible channels
    pub async fn auto_join_user_channels(&self, sid: &str, user_id: &str) -> Result<(), String> {
        use crate::services::channel::ChannelService;

        let channel_service = ChannelService::new(&self.db);

        match channel_service.get_channels_by_user_id(user_id).await {
            Ok(channels) => {
                tracing::info!(
                    "Auto-joining user {} to {} channels",
                    user_id,
                    channels.len()
                );

                for channel in channels {
                    let room = format!("channel:{}", channel.id);
                    if let Err(e) = self.manager.join_room(sid, &room).await {
                        tracing::warn!(
                            "Failed to join user {} to channel {}: {}",
                            user_id,
                            channel.id,
                            e
                        );
                    } else {
                        tracing::debug!("User {} joined channel room: {}", user_id, channel.id);
                    }
                }

                Ok(())
            }
            Err(e) => Err(format!("Failed to fetch user channels: {}", e)),
        }
    }

    /// Handle join-channels event - manually join user to their channels
    pub async fn handle_join_channels(&self, sid: &str, _data: JsonValue) -> Result<(), String> {
        // Get the session user
        let session = self
            .manager
            .get_session(sid)
            .await
            .ok_or("Session not found")?;

        let user = session.user.clone().ok_or("User not authenticated")?;

        let user_id = user
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or("Missing user ID")?;

        // Join all user's channels
        self.auto_join_user_channels(sid, user_id).await?;

        tracing::info!("User {} manually joined their channels", user_id);
        Ok(())
    }

    /// Handle usage tracking
    pub async fn handle_usage(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        if let Some(model_id) = data.get("model").and_then(|m| m.as_str()) {
            self.manager.track_usage(sid, model_id).await;
            Ok(())
        } else {
            Err("Missing model ID".to_string())
        }
    }

    /// Handle chat events
    pub async fn handle_chat_event(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        tracing::debug!("Chat event from {}: {:?}", sid, data);
        // Chat events are typically received from frontend and don't need special handling
        // The actual chat processing is done via HTTP API
        Ok(())
    }

    /// Handle channel events (broadcast to channel room)
    pub async fn handle_channel_event(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let channel_id = data
            .get("channel_id")
            .and_then(|c| c.as_str())
            .ok_or("Missing channel_id")?;

        let room = format!("channel:{}", channel_id);

        // Get session user
        let session = self
            .manager
            .get_session(sid)
            .await
            .ok_or("Session not found")?;

        let user = session.user.clone().unwrap_or(serde_json::json!({}));

        // Broadcast to room (excluding sender)
        let broadcast_data = serde_json::json!({
            "channel_id": channel_id,
            "message_id": data.get("message_id"),
            "data": data.get("data").unwrap_or(&serde_json::json!({})),
            "user": user,
        });

        self.broadcast_to_room(&room, "channel-events", broadcast_data, Some(sid))
            .await?;
        tracing::debug!("Broadcasted channel event to room: {}", room);

        Ok(())
    }

    /// Handle channel join (user joins a channel room)
    pub async fn handle_channel_join(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let channel_id = data
            .get("channel_id")
            .and_then(|c| c.as_str())
            .ok_or("Missing channel_id")?;

        let room = format!("channel:{}", channel_id);
        self.manager.join_room(sid, &room).await?;

        tracing::info!("Session {} joined channel room: {}", sid, channel_id);
        Ok(())
    }

    /// Handle channel leave (user leaves a channel room)
    pub async fn handle_channel_leave(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let channel_id = data
            .get("channel_id")
            .and_then(|c| c.as_str())
            .ok_or("Missing channel_id")?;

        let room = format!("channel:{}", channel_id);
        self.manager.leave_room(sid, &room).await?;

        tracing::info!("Session {} left channel room: {}", sid, channel_id);
        Ok(())
    }

    /// Handle Yjs document join
    pub async fn handle_ydoc_join(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let doc_id = data
            .get("document_id")
            .and_then(|d| d.as_str())
            .ok_or("Missing document_id")?;

        tracing::info!("Session {} joining Yjs document: {}", sid, doc_id);

        // Get the session user
        let session = self
            .manager
            .get_session(sid)
            .await
            .ok_or("Session not found")?;

        let user = session.user.clone().ok_or("User not authenticated")?;

        let user_id = user
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or("User ID not found")?;

        let user_role = user.get("role").and_then(|r| r.as_str()).unwrap_or("user");

        // Access control check for notes (note:xxx format)
        if doc_id.starts_with("note:") {
            let note_id = doc_id
                .strip_prefix("note:")
                .ok_or("Invalid note document ID")?;

            tracing::debug!("Checking access for user {} to note {}", user_id, note_id);

            // Check note access in database
            use crate::services::group::GroupService;
            use crate::services::note::NoteService;

            let note_service = NoteService::new(&self.db);
            let mut note = note_service
                .get_note_by_id(note_id)
                .await
                .map_err(|e| format!("Database error: {}", e))?
                .ok_or_else(|| format!("Note {} not found", note_id))?;

            note.parse_json_fields();

            // Admin has full access
            if user_role != "admin" {
                // Owner has full access
                if user_id != note.user_id {
                    // Check access control for other users
                    let group_service = GroupService::new(&self.db);
                    let user_groups = group_service
                        .get_groups_by_member_id(user_id)
                        .await
                        .map_err(|e| format!("Failed to get user groups: {}", e))?;
                    let user_group_ids: std::collections::HashSet<String> =
                        user_groups.into_iter().map(|g| g.id).collect();

                    use crate::utils::misc::has_access;
                    if !has_access(user_id, "read", &note.access_control, &user_group_ids) {
                        return Err(format!(
                            "User {} does not have access to note {}",
                            user_id, note_id
                        ));
                    }
                }
            }

            tracing::info!(
                "User {} (role: {}) granted access to note document {}",
                user_id,
                user_role,
                note_id
            );
        }

        // Join the Socket.IO room
        let room = format!("doc_{}", doc_id);
        self.manager.join_room(sid, &room).await?;

        // Add user to Yjs document
        self.ydoc_manager.add_user(doc_id, sid).await?;

        // Get the current document state
        let state_update = self.ydoc_manager.get_state_as_update(doc_id).await?;

        // Get all active session IDs in the room
        let active_sessions = self.manager.get_room_sessions(&room).await;

        // Send the document state to the joining client
        let state_data = serde_json::json!({
            "document_id": doc_id,
            "state": state_update,
            "sessions": active_sessions,
        });

        self.emit_to_session(sid, "ydoc:document:state", state_data)
            .await?;

        // Notify other users about the new user
        let user_data = data
            .get("user_id")
            .and_then(|u| u.as_str())
            .unwrap_or("unknown");

        let join_notification = serde_json::json!({
            "document_id": doc_id,
            "user_id": user_data,
            "user_name": data.get("user_name").and_then(|n| n.as_str()).unwrap_or("Anonymous"),
            "user_color": data.get("user_color").and_then(|c| c.as_str()).unwrap_or("#000000"),
        });

        self.broadcast_to_room(&room, "ydoc:user:joined", join_notification, Some(sid))
            .await?;

        tracing::info!(
            "Session {} successfully joined Yjs document: {}",
            sid,
            doc_id
        );
        Ok(())
    }

    /// Handle Yjs document leave
    pub async fn handle_ydoc_leave(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let doc_id = data
            .get("document_id")
            .and_then(|d| d.as_str())
            .ok_or("Missing document_id")?;

        let room = format!("doc_{}", doc_id);
        self.manager.leave_room(sid, &room).await?;

        // Remove user from Yjs document
        self.ydoc_manager.remove_user(doc_id, sid).await?;

        // Check if document is now empty and clean up if needed
        let users = self.ydoc_manager.get_users(doc_id).await?;
        if users.is_empty() {
            tracing::info!("Document {} is now empty, clearing from memory", doc_id);
            self.ydoc_manager.clear_document(doc_id).await?;
        }

        // Notify other users
        let leave_notification = serde_json::json!({
            "document_id": doc_id,
            "user_id": data.get("user_id").and_then(|u| u.as_str()).unwrap_or("unknown"),
        });

        self.broadcast_to_room(&room, "ydoc:user:left", leave_notification, None)
            .await?;

        tracing::info!("Session {} left Yjs document: {}", sid, doc_id);
        Ok(())
    }

    /// Handle Yjs document update (broadcast to room)
    pub async fn handle_ydoc_update(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let doc_id = data
            .get("document_id")
            .and_then(|d| d.as_str())
            .ok_or("Missing document_id")?;

        // Extract the update bytes
        let update_array = data
            .get("update")
            .and_then(|u| u.as_array())
            .ok_or("Missing or invalid update array")?;

        let update_bytes: Vec<u8> = update_array
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u8))
            .collect();

        if update_bytes.is_empty() {
            return Err("Empty update".to_string());
        }

        // Store the update in Yjs manager
        self.ydoc_manager
            .append_update(doc_id, update_bytes)
            .await?;

        // Broadcast update to all other clients in the room
        let room = format!("doc_{}", doc_id);

        let broadcast_data = serde_json::json!({
            "document_id": doc_id,
            "user_id": data.get("user_id"),
            "update": data.get("update"),
            "socket_id": sid,
        });

        self.broadcast_to_room(&room, "ydoc:document:update", broadcast_data, Some(sid))
            .await?;

        tracing::debug!("Stored and broadcasted Yjs update for document: {}", doc_id);

        // Save to database for notes (debounced)
        if doc_id.starts_with("note:") {
            if let Some(note_data) = data.get("data") {
                let doc_id_clone = doc_id.to_string();
                let note_data_clone = note_data.clone();
                let db_clone = self.db.clone();

                // Spawn debounced save task
                tokio::spawn(async move {
                    // Wait 500ms before saving (debounce)
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                    if let Some(note_id) = doc_id_clone.strip_prefix("note:") {
                        use crate::models::note::NoteUpdateForm;
                        use crate::services::note::NoteService;

                        let note_service = NoteService::new(&db_clone);
                        let update_form = NoteUpdateForm {
                            title: None,
                            data: Some(note_data_clone),
                            meta: None,
                            access_control: None,
                        };

                        match note_service.update_note_by_id(note_id, &update_form).await {
                            Ok(_) => {
                                tracing::debug!(
                                    "Saved note {} to database after Yjs update",
                                    note_id
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to save note {} to database: {}",
                                    note_id,
                                    e
                                );
                            }
                        }
                    }
                });
            }
        }

        Ok(())
    }

    /// Handle Yjs document state request
    pub async fn handle_ydoc_state_request(
        &self,
        sid: &str,
        data: JsonValue,
    ) -> Result<(), String> {
        let doc_id = data
            .get("document_id")
            .and_then(|d| d.as_str())
            .ok_or("Missing document_id")?;

        // Check if session is in the room
        let room = format!("doc_{}", doc_id);
        let room_sessions = self.manager.get_room_sessions(&room).await;

        if !room_sessions.contains(&sid.to_string()) {
            tracing::warn!("Session {} not in room {}, cannot send state", sid, room);
            return Err("Not in document room".to_string());
        }

        // Check if document exists
        if !self.ydoc_manager.document_exists(doc_id).await? {
            tracing::warn!("Document {} not found", doc_id);
            // Send empty state for new document
            let state_data = serde_json::json!({
                "document_id": doc_id,
                "state": [],
                "sessions": room_sessions,
            });

            return self
                .emit_to_session(sid, "ydoc:document:state", state_data)
                .await;
        }

        // Get the document state as a full update
        let state_update = self.ydoc_manager.get_state_as_update(doc_id).await?;

        let state_data = serde_json::json!({
            "document_id": doc_id,
            "state": state_update,
            "sessions": room_sessions,
        });

        self.emit_to_session(sid, "ydoc:document:state", state_data)
            .await?;

        tracing::debug!("Sent Yjs state for document: {}", doc_id);
        Ok(())
    }

    /// Handle Yjs awareness update (broadcast to room)
    pub async fn handle_ydoc_awareness_update(
        &self,
        _sid: &str,
        data: JsonValue,
    ) -> Result<(), String> {
        let doc_id = data
            .get("document_id")
            .and_then(|d| d.as_str())
            .ok_or("Missing document_id")?;

        let room = format!("doc_{}", doc_id);

        // Broadcast awareness update to all clients in the room (including sender for awareness)
        // Awareness needs to be sent to all including sender for cursor sync
        let room_sids = self.manager.get_room_sessions(&room).await;

        let mut sent = 0;
        for session_sid in room_sids {
            if self
                .emit_to_session(&session_sid, "ydoc:awareness:update", data.clone())
                .await
                .is_ok()
            {
                sent += 1;
            }
        }

        tracing::debug!(
            "Broadcasted Yjs awareness update for document {} to {} clients",
            doc_id,
            sent
        );
        Ok(())
    }

    /// Get manager reference
    pub fn manager(&self) -> &SocketIOManager {
        &self.manager
    }

    /// Handle presence status update
    pub async fn handle_presence_status(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let session = self
            .manager
            .get_session(sid)
            .await
            .ok_or("Session not found")?;

        let user_id = session.user_id().ok_or("User not authenticated")?;

        let status_str = data
            .get("status")
            .and_then(|s| s.as_str())
            .ok_or("Missing status")?;

        let status = match status_str {
            "online" => crate::socketio::presence::PresenceStatus::Online,
            "away" => crate::socketio::presence::PresenceStatus::Away,
            "busy" => crate::socketio::presence::PresenceStatus::Busy,
            "offline" => crate::socketio::presence::PresenceStatus::Offline,
            _ => return Err("Invalid status".to_string()),
        };

        self.presence_manager.set_status(&user_id, status).await;
        self.metrics.record_event_received("presence:status").await;

        tracing::debug!(
            "User {} updated presence status to {:?}",
            user_id,
            status_str
        );
        Ok(())
    }

    /// Handle typing indicator start
    pub async fn handle_typing_start(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let session = self
            .manager
            .get_session(sid)
            .await
            .ok_or("Session not found")?;

        let user = session.user.ok_or("User not authenticated")?;
        let user_id = user
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or("Missing user ID")?;
        let user_name = user
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown");

        let room_id = data
            .get("room_id")
            .and_then(|r| r.as_str())
            .ok_or("Missing room_id")?;

        self.presence_manager
            .start_typing(user_id, user_name, room_id)
            .await;
        self.metrics.record_event_received("typing:start").await;

        // Broadcast typing indicator to room
        let typing_data = serde_json::json!({
            "user_id": user_id,
            "user_name": user_name,
            "room_id": room_id,
        });

        self.broadcast_to_room(room_id, "typing:start", typing_data, Some(sid))
            .await?;

        Ok(())
    }

    /// Handle typing indicator stop
    pub async fn handle_typing_stop(&self, sid: &str, data: JsonValue) -> Result<(), String> {
        let session = self
            .manager
            .get_session(sid)
            .await
            .ok_or("Session not found")?;

        let user_id = session.user_id().ok_or("User not authenticated")?;

        let room_id = data
            .get("room_id")
            .and_then(|r| r.as_str())
            .ok_or("Missing room_id")?;

        self.presence_manager.stop_typing(&user_id, room_id).await;
        self.metrics.record_event_received("typing:stop").await;

        // Broadcast typing stop to room
        let typing_data = serde_json::json!({
            "user_id": user_id,
            "room_id": room_id,
        });

        self.broadcast_to_room(room_id, "typing:stop", typing_data, Some(sid))
            .await?;

        Ok(())
    }

    /// Get presence for multiple users
    pub async fn handle_get_presences(
        &self,
        _sid: &str,
        data: JsonValue,
    ) -> Result<JsonValue, String> {
        let user_ids = data
            .get("user_ids")
            .and_then(|ids| ids.as_array())
            .ok_or("Missing user_ids array")?;

        let user_id_strings: Vec<String> = user_ids
            .iter()
            .filter_map(|id| id.as_str().map(|s| s.to_string()))
            .collect();

        let presences = self.presence_manager.get_presences(&user_id_strings).await;

        self.metrics.record_event_received("presence:get").await;

        Ok(serde_json::to_value(presences).unwrap_or(serde_json::json!({})))
    }
}

/// HTTP endpoint for emitting events from Rust backend
#[derive(Debug, Deserialize)]
pub struct EmitRequest {
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub room: Option<String>,
    pub event: String,
    pub data: JsonValue,
}

#[derive(Debug, Serialize)]
pub struct EmitResponse {
    pub status: String,
    pub sent: usize,
}

/// Handle emit endpoint
pub async fn handle_emit_request(
    event_handler: web::Data<EventHandler>,
    req: web::Json<EmitRequest>,
) -> Result<web::Json<EmitResponse>, actix_web::Error> {
    let sent = if let Some(user_id) = &req.user_id {
        // Emit to user
        event_handler
            .emit_to_user(user_id, &req.event, req.data.clone())
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?
    } else if let Some(session_id) = &req.session_id {
        // Emit to specific session
        event_handler
            .emit_to_session(session_id, &req.event, req.data.clone())
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;
        1
    } else if let Some(room) = &req.room {
        // Broadcast to room
        event_handler
            .broadcast_to_room(room, &req.event, req.data.clone(), None)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?
    } else {
        return Err(actix_web::error::ErrorBadRequest(
            "Must specify user_id, session_id, or room",
        ));
    };

    Ok(web::Json(EmitResponse {
        status: "ok".to_string(),
        sent,
    }))
}

/// Health check endpoint
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub sessions: usize,
    pub users: usize,
    pub rooms: usize,
}

pub async fn handle_health_check(
    event_handler: web::Data<EventHandler>,
) -> Result<web::Json<HealthResponse>, actix_web::Error> {
    let stats = event_handler.manager().get_stats().await;

    Ok(web::Json(HealthResponse {
        status: "ok".to_string(),
        sessions: *stats.get("sessions").unwrap_or(&0),
        users: *stats.get("users").unwrap_or(&0),
        rooms: *stats.get("rooms").unwrap_or(&0),
    }))
}
