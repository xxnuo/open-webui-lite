use crate::socketio::events::EventHandler;
use crate::socketio::manager::SocketIOManager;
/// Socket.IO Transport Layer
///
/// Handles both WebSocket and HTTP long-polling transports
///
/// This implementation follows Socket.IO Protocol v5 (used by Socket.IO v3+)
/// Key requirements:
/// - Client sends CONNECT packet first
/// - Server responds with CONNECT packet containing {sid: "..."}
/// - All namespaces (including default "/") require explicit CONNECT
use crate::socketio::protocol::{EnginePacket, EnginePacketType, SocketPacket, SocketPacketType};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_ws::Message as WsMessage;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Polling response queue - stores messages to be sent to polling clients
/// In a production system, this could be replaced with Redis
type PollingQueue = Arc<RwLock<HashMap<String, Vec<String>>>>;

lazy_static::lazy_static! {
    static ref POLLING_RESPONSES: PollingQueue = Arc::new(RwLock::new(HashMap::new()));
}

/// Queue a response for a polling session
async fn queue_polling_response(sid: &str, message: String) {
    let mut queue = POLLING_RESPONSES.write().await;
    queue
        .entry(sid.to_string())
        .or_insert_with(Vec::new)
        .push(message);
}

/// Get and clear queued responses for a polling session
async fn get_polling_responses(sid: &str) -> Vec<String> {
    let mut queue = POLLING_RESPONSES.write().await;
    queue.remove(sid).unwrap_or_default()
}

/// WebSocket transport handler
pub async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    event_handler: web::Data<EventHandler>,
) -> Result<HttpResponse, Error> {
    tracing::info!("WebSocket connection request from: {:?}", req.peer_addr());
    tracing::debug!("WebSocket headers: {:?}", req.headers());

    // Perform WebSocket handshake
    let (response, mut session, mut msg_stream) = match actix_ws::handle(&req, stream) {
        Ok(result) => {
            tracing::info!("WebSocket handshake successful");
            result
        }
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {}", e);
            return Err(e);
        }
    };

    // Generate session ID
    let sid = SocketIOManager::generate_sid();
    let sid_clone = sid.clone();
    let event_handler_clone = event_handler.get_ref().clone();
    let manager = event_handler.manager().clone();

    // Create session
    manager.create_session(&sid).await;

    // Create a channel for sending messages to this WebSocket
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Register the connection
    event_handler.register_connection(&sid, tx).await;

    // Send Engine.IO open packet
    let open_packet = EnginePacket::open(&sid, manager.ping_interval(), manager.ping_timeout());
    let open_encoded = open_packet.encode();
    tracing::info!("Sending Engine.IO OPEN: {}", open_encoded);
    let _ = session.text(open_encoded).await;

    // NOTE: In Socket.IO v5, we do NOT automatically send CONNECT
    // The client must send CONNECT first, then we respond with CONNECT containing {sid}
    // This is handled in the message loop below when we receive the CONNECT packet from client

    // Spawn a task to handle outgoing messages
    let sid_outgoing = sid.clone();
    let mut session_outgoing = session.clone();
    actix_web::rt::spawn(async move {
        while let Some(message) = rx.recv().await {
            if session_outgoing.text(message).await.is_err() {
                break;
            }
        }
        tracing::debug!("Outgoing message handler closed for {}", sid_outgoing);
    });

    // Spawn a task to handle incoming messages
    actix_web::rt::spawn(async move {
        let event_handler = event_handler_clone;
        let sid = sid_clone;
        let http_client = reqwest::Client::new();

        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                WsMessage::Text(text) => {
                    tracing::debug!("Received text message: {}", text);

                    // Parse Engine.IO packet
                    if let Ok(engine_packet) = EnginePacket::decode(&text.to_string()) {
                        match engine_packet.packet_type {
                            EnginePacketType::Ping => {
                                // Respond with pong
                                manager.update_ping(&sid).await;
                                let pong = EnginePacket::pong(engine_packet.data.clone());
                                let _ = session.text(pong.encode()).await;
                            }
                            EnginePacketType::Message => {
                                // Parse Socket.IO packet
                                let data_str = String::from_utf8_lossy(&engine_packet.data);
                                if let Ok(socket_packet) = SocketPacket::decode(&data_str) {
                                    // Handle Socket.IO packet
                                    handle_socket_packet(
                                        &event_handler,
                                        &sid,
                                        socket_packet,
                                        &http_client,
                                        &mut session,
                                    )
                                    .await;
                                }
                            }
                            EnginePacketType::Close => {
                                tracing::info!("Client {} requested close", sid);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                WsMessage::Binary(bytes) => {
                    tracing::debug!("Received binary message: {} bytes", bytes.len());
                    // Handle binary messages if needed
                }
                WsMessage::Ping(bytes) => {
                    let _ = session.pong(&bytes).await;
                }
                WsMessage::Close(_) => {
                    tracing::info!("Client {} closed connection", sid);
                    break;
                }
                _ => {}
            }
        }

        // Clean up session
        event_handler.unregister_connection(&sid).await;
        manager.remove_session(&sid).await;
        let _ = session.close(None).await;
    });

    Ok(response)
}

/// Handle Socket.IO packet
async fn handle_socket_packet(
    event_handler: &EventHandler,
    sid: &str,
    packet: SocketPacket,
    http_client: &reqwest::Client,
    session: &mut actix_ws::Session,
) {
    use crate::socketio::protocol::SocketPacketType;

    match packet.packet_type {
        SocketPacketType::Connect => {
            // Client is requesting connection to a namespace
            // In Socket.IO v5, we must respond with CONNECT containing {sid}
            tracing::info!(
                "Client {} connecting to namespace: {}",
                sid,
                packet.namespace
            );

            // Check if client sent auth data and authenticate immediately
            if let Some(ref auth_data) = packet.data {
                tracing::debug!("Auth data received during CONNECT: {:?}", auth_data);
                
                // Try to authenticate user with the auth data
                if let Some(auth_obj) = auth_data.get("auth") {
                    if let Some(token) = auth_obj.get("token").and_then(|t| t.as_str()) {
                        tracing::info!("Authenticating user during CONNECT with token");
                        
                        // Authenticate with backend
                        let auth_url = format!("{}/api/socketio/auth", event_handler.auth_endpoint());
                        
                        match http_client
                            .post(&auth_url)
                            .json(&serde_json::json!({"token": token}))
                            .send()
                            .await
                        {
                            Ok(response) if response.status().is_success() => {
                                if let Ok(user) = response.json::<serde_json::Value>().await {
                                    // Set session user immediately
                                    if let Err(e) = event_handler.manager().set_session_user(sid, user.clone()).await {
                                        tracing::error!("Failed to set session user: {}", e);
                                    } else {
                                        let user_id = user.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");
                                        tracing::info!("User {} authenticated during CONNECT on session {}", user_id, sid);
                                        
                                        // Update presence
                                        event_handler.presence_manager().user_online(user_id).await;
                                        
                                        // Auto-join user to their channels
                                        if let Err(e) = event_handler.auto_join_user_channels(sid, user_id).await {
                                            tracing::warn!("Failed to auto-join user {} to channels: {}", user_id, e);
                                        }
                                    }
                                }
                            }
                            Ok(response) => {
                                tracing::warn!("Authentication failed during CONNECT: {}", response.status());
                            }
                            Err(e) => {
                                tracing::error!("Auth request failed during CONNECT: {}", e);
                            }
                        }
                    }
                }
            }

            // Generate a Socket.IO session ID (different from Engine.IO sid)
            let socket_sid = SocketIOManager::generate_sid();

            // Send CONNECT response with sid
            let connect_response = SocketPacket::connect(&packet.namespace, Some(&socket_sid));
            let engine_msg = EnginePacket::message(connect_response.encode().into_bytes());
            tracing::info!("Sending CONNECT response: {}", engine_msg.encode());
            let _ = session.text(engine_msg.encode()).await;
        }
        SocketPacketType::Event => {
            if let Some((event, data)) = packet.get_event() {
                tracing::info!("Event from {}: {} - {:?}", sid, event, data);

                // Handle different event types
                let result = match event.as_str() {
                    "user-join" => event_handler
                        .handle_user_join(sid, data, http_client)
                        .await
                        .map(|_| ()),
                    "join-channels" => event_handler.handle_join_channels(sid, data).await,
                    "usage" => event_handler.handle_usage(sid, data).await,
                    "chat-events" => event_handler.handle_chat_event(sid, data).await,
                    "channel-events" => event_handler.handle_channel_event(sid, data).await,
                    "channel:join" => event_handler.handle_channel_join(sid, data).await,
                    "channel:leave" => event_handler.handle_channel_leave(sid, data).await,
                    "ydoc:document:join" => event_handler.handle_ydoc_join(sid, data).await,
                    "ydoc:document:leave" => event_handler.handle_ydoc_leave(sid, data).await,
                    "ydoc:document:update" => event_handler.handle_ydoc_update(sid, data).await,
                    "ydoc:document:state" => {
                        event_handler.handle_ydoc_state_request(sid, data).await
                    }
                    "ydoc:awareness:update" => {
                        event_handler.handle_ydoc_awareness_update(sid, data).await
                    }
                    "presence:status" => event_handler.handle_presence_status(sid, data).await,
                    "typing:start" => event_handler.handle_typing_start(sid, data).await,
                    "typing:stop" => event_handler.handle_typing_stop(sid, data).await,
                    "presence:get" => {
                        match event_handler.handle_get_presences(sid, data).await {
                            Ok(response) => {
                                // Send response back to client
                                let _ = event_handler
                                    .emit_to_session(sid, "presence:data", response)
                                    .await;
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    _ => {
                        tracing::debug!("Unknown event: {}", event);
                        Ok(())
                    }
                };

                if let Err(e) = result {
                    tracing::error!("Error handling event {}: {}", event, e);
                }
            }
        }
        SocketPacketType::Disconnect => {
            tracing::info!(
                "Client {} disconnecting from namespace {}",
                sid,
                packet.namespace
            );
        }
        _ => {
            tracing::debug!("Unhandled packet type: {:?}", packet.packet_type);
        }
    }
}

/// HTTP polling transport handler (for fallback)
pub async fn polling_handler(
    req: HttpRequest,
    body: web::Bytes,
    manager: web::Data<SocketIOManager>,
    event_handler: Option<web::Data<EventHandler>>,
) -> Result<HttpResponse, Error> {
    let query =
        web::Query::<std::collections::HashMap<String, String>>::from_query(req.query_string())
            .map_err(|_| actix_web::error::ErrorBadRequest("Invalid query parameters"))?;

    let transport = query.get("transport").map(|s| s.as_str());
    let sid = query.get("sid").map(|s| s.as_str());
    let eio = query.get("EIO").map(|s| s.as_str());

    // Check Engine.IO version
    if let Some(version) = eio {
        if version != "4" {
            tracing::warn!("Unsupported Engine.IO version: {}", version);
            return Ok(HttpResponse::BadRequest()
                .json(serde_json::json!({"error": "Unsupported Engine.IO version"})));
        }
    }

    let method = req.method();

    match (method.as_str(), transport, sid) {
        // GET request - initial connection or polling for messages
        ("GET", Some("polling"), None) | ("GET", None, None) => {
            // Initial polling request - open new session
            let sid = SocketIOManager::generate_sid();
            manager.create_session(&sid).await;
            tracing::info!("Created polling session: {}", sid);

            // Send Engine.IO OPEN packet only
            // In Socket.IO v5, client must send CONNECT first
            let open_packet =
                EnginePacket::open(&sid, manager.ping_interval(), manager.ping_timeout());

            Ok(HttpResponse::Ok()
                .content_type("text/plain; charset=UTF-8")
                .append_header(("Access-Control-Allow-Credentials", "true"))
                .body(open_packet.encode()))
        }
        ("GET", Some("polling"), Some(sid)) | ("GET", None, Some(sid)) => {
            // Polling request with session ID - client polling for messages
            if manager.get_session(sid).await.is_some() {
                manager.update_ping(sid).await;

                // Get queued messages for this session
                let messages = get_polling_responses(sid).await;

                if messages.is_empty() {
                    // No messages, return NOOP
                    Ok(HttpResponse::Ok()
                        .content_type("text/plain; charset=UTF-8")
                        .append_header(("Access-Control-Allow-Credentials", "true"))
                        .body("6")) // NOOP packet
                } else {
                    // Join messages with packet separator
                    let response_body = messages.join("\x1e");
                    Ok(HttpResponse::Ok()
                        .content_type("text/plain; charset=UTF-8")
                        .append_header(("Access-Control-Allow-Credentials", "true"))
                        .body(response_body))
                }
            } else {
                tracing::warn!("Polling session not found: {}", sid);
                Ok(HttpResponse::BadRequest()
                    .json(serde_json::json!({"error": "Session not found"})))
            }
        }
        // POST request - client sending messages
        ("POST", Some("polling"), Some(sid)) | ("POST", None, Some(sid)) => {
            if manager.get_session(sid).await.is_some() {
                manager.update_ping(sid).await;

                // Parse incoming messages
                let body_str = String::from_utf8_lossy(&body);
                tracing::debug!("Received polling POST from {}: {}", sid, body_str);

                // Handle messages if event_handler is provided
                if let Some(_handler) = event_handler {
                    let _http_client = reqwest::Client::new();

                    // Split by packet separator
                    for packet_str in body_str.split('\x1e') {
                        if packet_str.is_empty() {
                            continue;
                        }

                        // Parse Engine.IO packet
                        if let Ok(engine_packet) = EnginePacket::decode(packet_str) {
                            match engine_packet.packet_type {
                                EnginePacketType::Message => {
                                    // Parse Socket.IO packet
                                    let data_str = String::from_utf8_lossy(&engine_packet.data);
                                    if let Ok(socket_packet) = SocketPacket::decode(&data_str) {
                                        // Handle CONNECT specially for polling (need to queue response)
                                        if socket_packet.packet_type == SocketPacketType::Connect {
                                            tracing::info!(
                                                "Polling client {} connecting to namespace: {}",
                                                sid,
                                                socket_packet.namespace
                                            );

                                            // Check if client sent auth data and authenticate immediately
                                            if let Some(ref auth_data) = socket_packet.data {
                                                tracing::debug!("Auth data received during polling CONNECT: {:?}", auth_data);
                                                
                                                // Try to authenticate user with the auth data
                                                if let Some(auth_obj) = auth_data.get("auth") {
                                                    if let Some(token) = auth_obj.get("token").and_then(|t| t.as_str()) {
                                                        tracing::info!("Authenticating user during polling CONNECT with token");
                                                        
                                        // Authenticate with backend
                                        let auth_url = format!("{}/api/socketio/auth", _handler.auth_endpoint());
                                                        
                                                        match _http_client
                                                            .post(&auth_url)
                                                            .json(&serde_json::json!({"token": token}))
                                                            .send()
                                                            .await
                                                        {
                                                            Ok(response) if response.status().is_success() => {
                                                                if let Ok(user) = response.json::<serde_json::Value>().await {
                                                                    // Set session user immediately
                                                                    if let Err(e) = _handler.manager().set_session_user(sid, user.clone()).await {
                                                                        tracing::error!("Failed to set session user: {}", e);
                                                                    } else {
                                                                        let user_id = user.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");
                                                                        tracing::info!("User {} authenticated during polling CONNECT on session {}", user_id, sid);
                                                                        
                                                                        // Update presence
                                                                        _handler.presence_manager().user_online(user_id).await;
                                                                        
                                                                        // Auto-join user to their channels
                                                                        if let Err(e) = _handler.auto_join_user_channels(sid, user_id).await {
                                                                            tracing::warn!("Failed to auto-join user {} to channels: {}", user_id, e);
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            Ok(response) => {
                                                                tracing::warn!("Authentication failed during polling CONNECT: {}", response.status());
                                                            }
                                                            Err(e) => {
                                                                tracing::error!("Auth request failed during polling CONNECT: {}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // Generate Socket.IO session ID
                                            let socket_sid = SocketIOManager::generate_sid();

                                            // Create CONNECT response
                                            let connect_response = SocketPacket::connect(
                                                &socket_packet.namespace,
                                                Some(&socket_sid),
                                            );
                                            let engine_msg = EnginePacket::message(
                                                connect_response.encode().into_bytes(),
                                            );

                                            // Queue the response for next GET
                                            queue_polling_response(sid, engine_msg.encode()).await;
                                            tracing::info!(
                                                "Queued CONNECT response for polling session {}",
                                                sid
                                            );
                                        } else {
                                            // Handle other Socket.IO packets (but polling doesn't support this easily)
                                            tracing::debug!(
                                                "Received Socket.IO packet in polling: {:?}",
                                                socket_packet.packet_type
                                            );
                                        }
                                    }
                                }
                                EnginePacketType::Ping => {
                                    // Queue pong response
                                    let pong = EnginePacket::pong(engine_packet.data.clone());
                                    queue_polling_response(sid, pong.encode()).await;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                Ok(HttpResponse::Ok()
                    .content_type("text/plain; charset=UTF-8")
                    .append_header(("Access-Control-Allow-Credentials", "true"))
                    .body("ok"))
            } else {
                tracing::warn!("Polling session not found for POST: {}", sid);
                Ok(HttpResponse::BadRequest()
                    .json(serde_json::json!({"error": "Session not found"})))
            }
        }
        _ => {
            tracing::warn!(
                "Invalid polling request: method={}, transport={:?}, sid={:?}",
                method,
                transport,
                sid
            );
            Ok(HttpResponse::BadRequest()
                .json(serde_json::json!({"error": "Invalid transport parameters"})))
        }
    }
}

/// Emit event to a session
#[allow(dead_code)]
pub async fn emit_to_session(
    session: &mut actix_ws::Session,
    event: &str,
    data: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_packet = SocketPacket::event("/", event, data);
    let engine_packet = EnginePacket::message(socket_packet.encode().into_bytes());

    session.text(engine_packet.encode()).await?;
    Ok(())
}

/// Broadcast event to multiple sessions
#[allow(dead_code)]
pub async fn broadcast_to_sessions(_sids: Vec<String>, _event: &str, _data: serde_json::Value) {
    // This is now handled by EventHandler in events.rs
    // Keeping this as a placeholder for backward compatibility
}
