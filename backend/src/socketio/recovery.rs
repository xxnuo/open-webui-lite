/// Connection Recovery for Socket.IO
///
/// Handles session persistence, reconnection, and state recovery
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Recovery token for reconnection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryToken {
    pub session_id: String,
    pub user_id: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub secret: String,
}

impl RecoveryToken {
    pub fn new(session_id: String, user_id: String, ttl_seconds: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let secret = uuid::Uuid::new_v4().to_string();

        Self {
            session_id,
            user_id,
            created_at: now,
            expires_at: now + ttl_seconds,
            secret,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now > self.expires_at
    }

    pub fn to_string(&self) -> String {
        format!("{}:{}:{}", self.session_id, self.user_id, self.secret)
    }

    pub fn from_string(token: &str) -> Option<Self> {
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() != 3 {
            return None;
        }

        // This is a simplified version - in production, you'd validate the token
        // against stored data
        Some(Self {
            session_id: parts[0].to_string(),
            user_id: parts[1].to_string(),
            secret: parts[2].to_string(),
            created_at: 0,
            expires_at: 0,
        })
    }
}

/// Buffered message for missed events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferedMessage {
    pub event: String,
    pub data: JsonValue,
    pub timestamp: u64,
}

/// Session recovery state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub session_id: String,
    pub user_id: String,
    pub rooms: Vec<String>,
    pub buffered_messages: VecDeque<BufferedMessage>,
    pub last_seen: u64,
}

impl RecoveryState {
    pub fn new(session_id: String, user_id: String) -> Self {
        Self {
            session_id,
            user_id,
            rooms: Vec::new(),
            buffered_messages: VecDeque::new(),
            last_seen: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

/// Connection recovery manager
pub struct RecoveryManager {
    /// In-memory recovery states
    states: Arc<RwLock<std::collections::HashMap<String, RecoveryState>>>,

    /// Redis connection pool for persistence
    redis: Option<deadpool_redis::Pool>,

    /// Redis key prefix
    redis_prefix: String,

    /// Configuration
    config: RecoveryConfig,
}

/// Recovery configuration
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum messages to buffer per session
    pub max_buffered_messages: usize,

    /// How long to keep recovery state (seconds)
    pub state_ttl: u64,

    /// How long recovery tokens are valid (seconds)
    pub token_ttl: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_buffered_messages: 100,
            state_ttl: 300, // 5 minutes
            token_ttl: 300, // 5 minutes
        }
    }
}

impl RecoveryManager {
    pub fn new(redis: Option<deadpool_redis::Pool>, config: RecoveryConfig) -> Self {
        Self {
            states: Arc::new(RwLock::new(std::collections::HashMap::new())),
            redis,
            redis_prefix: "socketio:recovery".to_string(),
            config,
        }
    }

    /// Save session state for recovery
    pub async fn save_state(
        &self,
        session_id: &str,
        user_id: &str,
        rooms: Vec<String>,
    ) -> Result<(), String> {
        let mut state = RecoveryState::new(session_id.to_string(), user_id.to_string());
        state.rooms = rooms;

        // Save to memory
        {
            let mut states = self.states.write().await;
            states.insert(session_id.to_string(), state.clone());
        }

        // Save to Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}", self.redis_prefix, session_id);
                let serialized = serde_json::to_string(&state).map_err(|e| e.to_string())?;
                let _ = conn
                    .set_ex::<_, _, ()>(&key, serialized, self.config.state_ttl)
                    .await;
                tracing::debug!("Saved recovery state for session {} to Redis", session_id);
            }
        }

        Ok(())
    }

    /// Buffer a message for a disconnected session
    pub async fn buffer_message(
        &self,
        session_id: &str,
        event: String,
        data: JsonValue,
    ) -> Result<(), String> {
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(session_id) {
            let message = BufferedMessage {
                event,
                data,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            state.buffered_messages.push_back(message);

            // Limit buffer size
            while state.buffered_messages.len() > self.config.max_buffered_messages {
                state.buffered_messages.pop_front();
            }

            tracing::debug!(
                "Buffered message for session {} (total: {})",
                session_id,
                state.buffered_messages.len()
            );

            Ok(())
        } else {
            Err(format!(
                "No recovery state found for session {}",
                session_id
            ))
        }
    }

    /// Generate a recovery token for a session
    pub async fn generate_token(&self, session_id: &str, user_id: &str) -> Result<String, String> {
        let token = RecoveryToken::new(
            session_id.to_string(),
            user_id.to_string(),
            self.config.token_ttl,
        );

        // Store token in Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:token:{}", self.redis_prefix, session_id);
                let serialized = serde_json::to_string(&token).map_err(|e| e.to_string())?;
                let _ = conn
                    .set_ex::<_, _, ()>(&key, serialized, self.config.token_ttl)
                    .await;
            }
        }

        Ok(token.to_string())
    }

    /// Validate a recovery token
    pub async fn validate_token(&self, token_str: &str) -> Result<RecoveryToken, String> {
        let token = RecoveryToken::from_string(token_str)
            .ok_or_else(|| "Invalid token format".to_string())?;

        // Verify token against Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:token:{}", self.redis_prefix, token.session_id);

                if let Ok(stored) = conn.get::<_, Option<String>>(&key).await {
                    if let Some(stored_json) = stored {
                        let stored_token: RecoveryToken = serde_json::from_str(&stored_json)
                            .map_err(|e| format!("Failed to parse token: {}", e))?;

                        if stored_token.secret != token.secret {
                            return Err("Invalid token secret".to_string());
                        }

                        if stored_token.is_expired() {
                            return Err("Token expired".to_string());
                        }

                        return Ok(stored_token);
                    } else {
                        return Err("Token not found".to_string());
                    }
                }
            }
        }
        // Without Redis or on error, just check basic format
        Ok(token)
    }

    /// Recover session state
    pub async fn recover_state(&self, session_id: &str) -> Result<RecoveryState, String> {
        // Try Redis first
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}", self.redis_prefix, session_id);

                if let Ok(stored) = conn.get::<_, Option<String>>(&key).await {
                    if let Some(stored_json) = stored {
                        let state: RecoveryState = serde_json::from_str(&stored_json)
                            .map_err(|e| format!("Failed to parse state: {}", e))?;

                        tracing::info!(
                            "Recovered state for session {} from Redis ({} buffered messages, {} rooms)",
                            session_id,
                            state.buffered_messages.len(),
                            state.rooms.len()
                        );

                        return Ok(state);
                    }
                }
            }
        }

        // Fallback to memory
        let states = self.states.read().await;
        if let Some(state) = states.get(session_id) {
            Ok(state.clone())
        } else {
            Err(format!(
                "No recovery state found for session {}",
                session_id
            ))
        }
    }

    /// Remove recovery state
    pub async fn remove_state(&self, session_id: &str) -> Result<(), String> {
        // Remove from memory
        {
            let mut states = self.states.write().await;
            states.remove(session_id);
        }

        // Remove from Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let state_key = format!("{}:{}", self.redis_prefix, session_id);
                let token_key = format!("{}:token:{}", self.redis_prefix, session_id);
                let _ = conn
                    .del::<_, ()>(&[state_key.as_str(), token_key.as_str()])
                    .await;
            }
        }

        Ok(())
    }

    /// Clean up old recovery states
    pub async fn cleanup_old_states(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut states = self.states.write().await;

        let initial_count = states.len();
        states.retain(|session_id, state| {
            let age = now - state.last_seen;
            if age > self.config.state_ttl {
                tracing::debug!("Cleaning up old recovery state for session {}", session_id);
                false
            } else {
                true
            }
        });

        let removed = initial_count - states.len();
        if removed > 0 {
            tracing::info!("Cleaned up {} old recovery states", removed);
        }
    }

    /// Get statistics
    pub async fn get_stats(&self) -> RecoveryStats {
        let states = self.states.read().await;

        let total_states = states.len();
        let total_buffered: usize = states.values().map(|s| s.buffered_messages.len()).sum();

        RecoveryStats {
            active_states: total_states,
            total_buffered_messages: total_buffered,
        }
    }
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new(None, RecoveryConfig::default())
    }
}

/// Recovery statistics
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryStats {
    pub active_states: usize,
    pub total_buffered_messages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_recovery_state() {
        let manager = RecoveryManager::default();

        // Save state
        manager
            .save_state("session-1", "user-1", vec!["room-a".to_string()])
            .await
            .unwrap();

        // Recover state
        let state = manager.recover_state("session-1").await.unwrap();
        assert_eq!(state.session_id, "session-1");
        assert_eq!(state.user_id, "user-1");
        assert_eq!(state.rooms.len(), 1);
    }

    #[tokio::test]
    async fn test_buffer_messages() {
        let manager = RecoveryManager::default();

        // Save initial state
        manager
            .save_state("session-1", "user-1", vec![])
            .await
            .unwrap();

        // Buffer messages
        for i in 0..5 {
            manager
                .buffer_message(
                    "session-1",
                    "test-event".to_string(),
                    serde_json::json!({"index": i}),
                )
                .await
                .unwrap();
        }

        // Recover and check messages
        let state = manager.recover_state("session-1").await.unwrap();
        assert_eq!(state.buffered_messages.len(), 5);
    }

    #[tokio::test]
    async fn test_recovery_token() {
        let manager = RecoveryManager::default();

        // Generate token
        let token_str = manager.generate_token("session-1", "user-1").await.unwrap();

        // Validate token (without Redis, basic validation)
        let token = RecoveryToken::from_string(&token_str).unwrap();
        assert_eq!(token.session_id, "session-1");
        assert_eq!(token.user_id, "user-1");
    }
}
