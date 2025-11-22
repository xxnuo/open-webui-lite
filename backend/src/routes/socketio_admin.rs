/// Socket.IO Admin and Monitoring API
///
/// Provides endpoints for monitoring and managing Socket.IO connections
use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::socketio::EventHandler;
use crate::error::AppError;
use crate::middleware::auth::{verify_admin, AuthUser};

/// Get Socket.IO health status
pub async fn get_health(
    event_handler: web::Data<EventHandler>,
) -> Result<HttpResponse, AppError> {
    let stats = event_handler.manager().get_stats().await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "sessions": stats.get("sessions").unwrap_or(&0),
        "users": stats.get("users").unwrap_or(&0),
        "rooms": stats.get("rooms").unwrap_or(&0),
    })))
}

/// Get detailed Socket.IO statistics
pub async fn get_stats(
    _req: HttpRequest,
    event_handler: web::Data<EventHandler>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    // TODO: Verify admin role
    
    let manager_stats = event_handler.manager().get_stats().await;
    let metrics = event_handler.metrics().get_all_metrics().await;
    let rate_limit_stats = event_handler.rate_limiter().get_stats().await;
    let presence_stats = event_handler.presence_manager().get_stats().await;
    let recovery_stats = event_handler.recovery_manager().get_stats().await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "sessions": manager_stats.get("sessions").unwrap_or(&0),
        "users": manager_stats.get("users").unwrap_or(&0),
        "rooms": manager_stats.get("rooms").unwrap_or(&0),
        "metrics": metrics,
        "rate_limit": {
            "active_users": rate_limit_stats.active_users,
            "active_sessions": rate_limit_stats.active_sessions,
            "total_queue_size": rate_limit_stats.total_queue_size,
        },
        "presence": {
            "total_users": presence_stats.total_users,
            "online_users": presence_stats.online_users,
            "away_users": presence_stats.away_users,
            "offline_users": presence_stats.offline_users,
            "typing_users": presence_stats.typing_users,
        },
        "recovery": {
            "active_states": recovery_stats.active_states,
            "total_buffered_messages": recovery_stats.total_buffered_messages,
        },
        "timestamp": chrono::Utc::now().timestamp(),
    })))
}

#[derive(Debug, Serialize)]
pub struct ActiveSession {
    pub id: String,
    pub user_id: Option<String>,
    pub connected_at: i64,
    pub last_ping: i64,
    pub rooms: Vec<String>,
}

/// Get active sessions (admin only)
pub async fn get_sessions(
    _req: HttpRequest,
    event_handler: web::Data<EventHandler>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    // TODO: Verify admin role and implement session listing
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "sessions": [],
        "total": 0,
    })))
}

#[derive(Debug, Deserialize)]
pub struct BroadcastRequest {
    pub room: Option<String>,
    pub event: String,
    pub data: JsonValue,
}

/// Broadcast an event (admin only)
pub async fn broadcast_event(
    _req: HttpRequest,
    event_handler: web::Data<EventHandler>,
    body: web::Json<BroadcastRequest>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    // TODO: Verify admin role
    
    let sent = if let Some(room) = &body.room {
        event_handler
            .broadcast_to_room(room, &body.event, body.data.clone(), None)
            .await
            .map_err(|e| AppError::InternalServerError(e))?
    } else {
        // Broadcast to all connected sessions
        0 // TODO: Implement global broadcast
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "sent": sent,
    })))
}

#[derive(Debug, Deserialize)]
pub struct DisconnectRequest {
    pub session_id: String,
    pub reason: Option<String>,
}

/// Force disconnect a session (admin only)
pub async fn disconnect_session(
    _req: HttpRequest,
    event_handler: web::Data<EventHandler>,
    body: web::Json<DisconnectRequest>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    // TODO: Verify admin role and implement force disconnect
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "message": "Session disconnected",
    })))
}

/// Get online users (admin only)
pub async fn get_online_users(
    _req: HttpRequest,
    event_handler: web::Data<EventHandler>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    // TODO: Verify admin role
    
    let online_users = event_handler.presence_manager().get_online_users().await;
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "users": online_users,
        "count": online_users.len(),
    })))
}

/// Get typing indicators for a room (admin only)
#[derive(Debug, Deserialize)]
pub struct RoomQuery {
    pub room_id: String,
}

pub async fn get_typing_indicators(
    _req: HttpRequest,
    event_handler: web::Data<EventHandler>,
    query: web::Query<RoomQuery>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    // TODO: Verify admin role
    
    let typing_users = event_handler
        .presence_manager()
        .get_typing_users(&query.room_id)
        .await;
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "room_id": query.room_id,
        "typing_users": typing_users,
        "count": typing_users.len(),
    })))
}

/// Get metrics endpoint (Prometheus-compatible format)
pub async fn get_metrics(
    event_handler: web::Data<EventHandler>,
) -> Result<HttpResponse, AppError> {
    let metrics = event_handler.metrics().get_all_metrics().await;
    
    // Convert to Prometheus text format
    let mut output = String::new();
    
    if let Some(connections) = metrics.get("connections") {
        if let Some(active) = connections.get("active") {
            output.push_str(&format!(
                "# HELP socketio_connections_active Active Socket.IO connections\n\
                 # TYPE socketio_connections_active gauge\n\
                 socketio_connections_active {}\n",
                active
            ));
        }
        if let Some(total) = connections.get("total") {
            output.push_str(&format!(
                "# HELP socketio_connections_total Total Socket.IO connections\n\
                 # TYPE socketio_connections_total counter\n\
                 socketio_connections_total {}\n",
                total
            ));
        }
    }
    
    if let Some(events) = metrics.get("events") {
        if let Some(total_received) = events.get("total_received") {
            output.push_str(&format!(
                "# HELP socketio_events_received_total Total events received\n\
                 # TYPE socketio_events_received_total counter\n\
                 socketio_events_received_total {}\n",
                total_received
            ));
        }
        if let Some(total_sent) = events.get("total_sent") {
            output.push_str(&format!(
                "# HELP socketio_events_sent_total Total events sent\n\
                 # TYPE socketio_events_sent_total counter\n\
                 socketio_events_sent_total {}\n",
                total_sent
            ));
        }
    }
    
    if let Some(latency) = metrics.get("latency") {
        if let Some(p95) = latency.get("p95_micros") {
            output.push_str(&format!(
                "# HELP socketio_latency_p95_microseconds P95 latency in microseconds\n\
                 # TYPE socketio_latency_p95_microseconds gauge\n\
                 socketio_latency_p95_microseconds {}\n",
                p95
            ));
        }
    }
    
    Ok(HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(output))
}

/// Configure routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/socketio/admin")
            .route("/health", web::get().to(get_health))
            .route("/stats", web::get().to(get_stats))
            .route("/sessions", web::get().to(get_sessions))
            .route("/broadcast", web::post().to(broadcast_event))
            .route("/disconnect", web::post().to(disconnect_session))
            .route("/presence/online", web::get().to(get_online_users))
            .route("/presence/typing", web::get().to(get_typing_indicators))
            .route("/metrics", web::get().to(get_metrics)),
    );
}

