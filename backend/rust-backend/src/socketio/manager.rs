/// Socket.IO Session and Room Manager
///
/// This manages:
/// - Sessions (sid -> user data)
/// - User pools (user_id -> [sids])
/// - Rooms (room_id -> [sids])
/// - Usage tracking (model_id -> {sid -> timestamp})
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A Socket.IO session
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user: Option<JsonValue>,
    pub rooms: HashSet<String>,
    pub connected_at: i64,
    pub last_ping: i64,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            user: None,
            rooms: HashSet::new(),
            connected_at: now,
            last_ping: now,
        }
    }

    pub fn user_id(&self) -> Option<String> {
        self.user
            .as_ref()
            .and_then(|u| u.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
    }
}

/// Socket.IO Manager
///
/// Thread-safe manager for all Socket.IO sessions, rooms, and connections
#[derive(Clone)]
pub struct SocketIOManager {
    /// Session pool: sid -> Session
    sessions: Arc<RwLock<HashMap<String, Session>>>,

    /// User pool: user_id -> [sids]
    user_pool: Arc<RwLock<HashMap<String, Vec<String>>>>,

    /// Room pool: room_id -> [sids]
    rooms: Arc<RwLock<HashMap<String, HashSet<String>>>>,

    /// Usage pool: model_id -> {sid -> timestamp}
    usage_pool: Arc<RwLock<HashMap<String, HashMap<String, i64>>>>,

    /// Configuration
    ping_interval: u64,
    ping_timeout: u64,
}

impl SocketIOManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            user_pool: Arc::new(RwLock::new(HashMap::new())),
            rooms: Arc::new(RwLock::new(HashMap::new())),
            usage_pool: Arc::new(RwLock::new(HashMap::new())),
            ping_interval: 25_000, // 25 seconds
            ping_timeout: 20_000,  // 20 seconds
        }
    }

    pub fn ping_interval(&self) -> u64 {
        self.ping_interval
    }

    pub fn ping_timeout(&self) -> u64 {
        self.ping_timeout
    }

    /// Generate a new session ID
    pub fn generate_sid() -> String {
        Uuid::new_v4().to_string()
    }

    /// Create a new session
    pub async fn create_session(&self, sid: &str) -> Session {
        let session = Session::new(sid.to_string());
        let mut sessions = self.sessions.write().await;
        sessions.insert(sid.to_string(), session.clone());
        tracing::info!("Created session: {}", sid);
        session
    }

    /// Get a session by ID
    pub async fn get_session(&self, sid: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(sid).cloned()
    }

    /// Update session user data
    pub async fn set_session_user(&self, sid: &str, user: JsonValue) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(sid)
            .ok_or_else(|| format!("Session not found: {}", sid))?;

        let user_id = user
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or("User ID not found")?
            .to_string();

        session.user = Some(user);

        // Add to user pool
        drop(sessions); // Release lock before acquiring another
        let mut user_pool = self.user_pool.write().await;
        user_pool
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(sid.to_string());

        tracing::info!("Authenticated session {} for user {}", sid, user_id);
        Ok(())
    }

    /// Remove a session
    pub async fn remove_session(&self, sid: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(sid) {
            tracing::info!("Removed session: {}", sid);

            // Remove from user pool
            if let Some(user_id) = session.user_id() {
                drop(sessions); // Release lock
                let mut user_pool = self.user_pool.write().await;
                if let Some(sids) = user_pool.get_mut(&user_id) {
                    sids.retain(|s| s != sid);
                    if sids.is_empty() {
                        user_pool.remove(&user_id);
                    }
                }
            } else {
                drop(sessions);
            }

            // Remove from all rooms
            let mut rooms = self.rooms.write().await;
            for (_room_id, sids) in rooms.iter_mut() {
                sids.remove(sid);
            }
            // Clean up empty rooms
            rooms.retain(|_, sids| !sids.is_empty());

            // Remove from usage pool
            let mut usage_pool = self.usage_pool.write().await;
            for (_model_id, usage) in usage_pool.iter_mut() {
                usage.remove(sid);
            }
            usage_pool.retain(|_, usage| !usage.is_empty());
        }
    }

    /// Join a room
    pub async fn join_room(&self, sid: &str, room: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(sid)
            .ok_or_else(|| format!("Session not found: {}", sid))?;

        session.rooms.insert(room.to_string());
        drop(sessions);

        let mut rooms = self.rooms.write().await;
        rooms
            .entry(room.to_string())
            .or_insert_with(HashSet::new)
            .insert(sid.to_string());

        tracing::debug!("Session {} joined room {}", sid, room);
        Ok(())
    }

    /// Leave a room
    pub async fn leave_room(&self, sid: &str, room: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(sid) {
            session.rooms.remove(room);
        }
        drop(sessions);

        let mut rooms = self.rooms.write().await;
        if let Some(sids) = rooms.get_mut(room) {
            sids.remove(sid);
            if sids.is_empty() {
                rooms.remove(room);
            }
        }

        tracing::debug!("Session {} left room {}", sid, room);
        Ok(())
    }

    /// Get all sessions in a room
    pub async fn get_room_sessions(&self, room: &str) -> Vec<String> {
        let rooms = self.rooms.read().await;
        rooms
            .get(room)
            .map(|sids| sids.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all sessions for a user
    pub async fn get_user_sessions(&self, user_id: &str) -> Vec<String> {
        let user_pool = self.user_pool.read().await;
        user_pool
            .get(user_id)
            .map(|sids| sids.clone())
            .unwrap_or_default()
    }

    /// Track usage
    pub async fn track_usage(&self, sid: &str, model_id: &str) {
        let now = chrono::Utc::now().timestamp();
        let mut usage_pool = self.usage_pool.write().await;
        usage_pool
            .entry(model_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(sid.to_string(), now);
        tracing::debug!("Tracked usage: {} for session {}", model_id, sid);
    }

    /// Update last ping time
    pub async fn update_ping(&self, sid: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(sid) {
            session.last_ping = chrono::Utc::now().timestamp();
        }
    }

    /// Get statistics
    pub async fn get_stats(&self) -> HashMap<String, usize> {
        let sessions = self.sessions.read().await;
        let user_pool = self.user_pool.read().await;
        let rooms = self.rooms.read().await;

        let mut stats = HashMap::new();
        stats.insert("sessions".to_string(), sessions.len());
        stats.insert("users".to_string(), user_pool.len());
        stats.insert("rooms".to_string(), rooms.len());

        stats
    }

    /// Clean up stale sessions (called periodically)
    pub async fn cleanup_stale_sessions(&self, timeout_seconds: i64) {
        let now = chrono::Utc::now().timestamp();
        let mut sessions_to_remove = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (sid, session) in sessions.iter() {
                if now - session.last_ping > timeout_seconds {
                    sessions_to_remove.push(sid.clone());
                }
            }
        }

        for sid in sessions_to_remove {
            tracing::warn!("Removing stale session: {}", sid);
            self.remove_session(&sid).await;
        }
    }
}

impl Default for SocketIOManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let manager = SocketIOManager::new();
        let sid = "test-sid";

        // Create session
        manager.create_session(sid).await;
        let session = manager.get_session(sid).await;
        assert!(session.is_some());

        // Set user
        let user = serde_json::json!({"id": "user-123", "email": "test@example.com"});
        manager.set_session_user(sid, user).await.unwrap();

        // Check user sessions
        let user_sessions = manager.get_user_sessions("user-123").await;
        assert_eq!(user_sessions.len(), 1);
        assert_eq!(user_sessions[0], sid);

        // Remove session
        manager.remove_session(sid).await;
        let session = manager.get_session(sid).await;
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_rooms() {
        let manager = SocketIOManager::new();
        let sid1 = "sid-1";
        let sid2 = "sid-2";

        manager.create_session(sid1).await;
        manager.create_session(sid2).await;

        // Join room
        manager.join_room(sid1, "room-a").await.unwrap();
        manager.join_room(sid2, "room-a").await.unwrap();

        let room_sessions = manager.get_room_sessions("room-a").await;
        assert_eq!(room_sessions.len(), 2);

        // Leave room
        manager.leave_room(sid1, "room-a").await.unwrap();
        let room_sessions = manager.get_room_sessions("room-a").await;
        assert_eq!(room_sessions.len(), 1);
    }
}
