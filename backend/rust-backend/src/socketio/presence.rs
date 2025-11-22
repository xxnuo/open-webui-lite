/// Presence System for Socket.IO
///
/// Tracks user online/offline status, typing indicators, and last seen timestamps
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// User presence status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Online,
    Away,
    Busy,
    Offline,
}

/// User presence information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPresence {
    pub user_id: String,
    pub status: PresenceStatus,
    pub last_seen: u64, // Unix timestamp
    #[serde(skip)]
    #[serde(default = "default_instant")]
    pub last_activity: Instant, // For internal tracking (not serialized)
    pub custom_status: Option<String>,
    pub session_count: usize, // Number of active sessions
}

fn default_instant() -> Instant {
    Instant::now()
}

/// Typing indicator state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingIndicator {
    pub user_id: String,
    pub user_name: String,
    pub room_id: String,
    pub started_at: u64, // Unix timestamp
    #[serde(skip, default = "default_instant")]
    pub expires_at: Instant, // Internal expiry tracking
}

/// Presence manager
pub struct PresenceManager {
    /// User presence data
    presences: Arc<RwLock<HashMap<String, UserPresence>>>,

    /// Active typing indicators
    typing_indicators: Arc<RwLock<HashMap<String, Vec<TypingIndicator>>>>,

    /// Configuration
    config: PresenceConfig,
}

/// Presence configuration
#[derive(Debug, Clone)]
pub struct PresenceConfig {
    /// Time after which user is marked as away (no activity)
    pub away_timeout: Duration,

    /// Time after which typing indicator expires
    pub typing_timeout: Duration,

    /// Time after which presence data is cleaned up (offline users)
    pub cleanup_timeout: Duration,
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self {
            away_timeout: Duration::from_secs(300),     // 5 minutes
            typing_timeout: Duration::from_secs(5),     // 5 seconds
            cleanup_timeout: Duration::from_secs(3600), // 1 hour
        }
    }
}

impl PresenceManager {
    pub fn new(config: PresenceConfig) -> Self {
        Self {
            presences: Arc::new(RwLock::new(HashMap::new())),
            typing_indicators: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get current unix timestamp
    fn now_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Mark user as online (new session)
    pub async fn user_online(&self, user_id: &str) {
        let mut presences = self.presences.write().await;

        let presence = presences
            .entry(user_id.to_string())
            .or_insert_with(|| UserPresence {
                user_id: user_id.to_string(),
                status: PresenceStatus::Online,
                last_seen: Self::now_timestamp(),
                last_activity: Instant::now(),
                custom_status: None,
                session_count: 0,
            });

        presence.session_count += 1;
        presence.status = PresenceStatus::Online;
        presence.last_seen = Self::now_timestamp();
        presence.last_activity = Instant::now();

        tracing::debug!(
            "User {} is online (sessions: {})",
            user_id,
            presence.session_count
        );
    }

    /// Mark user as offline (session ended)
    pub async fn user_offline(&self, user_id: &str) -> bool {
        let mut presences = self.presences.write().await;

        if let Some(presence) = presences.get_mut(user_id) {
            if presence.session_count > 0 {
                presence.session_count -= 1;
            }

            if presence.session_count == 0 {
                presence.status = PresenceStatus::Offline;
                presence.last_seen = Self::now_timestamp();
                tracing::debug!("User {} is offline", user_id);
                return true; // User went fully offline
            } else {
                tracing::debug!(
                    "User {} still has {} active sessions",
                    user_id,
                    presence.session_count
                );
            }
        }

        false // User still has active sessions or not found
    }

    /// Update user activity (prevents away status)
    pub async fn update_activity(&self, user_id: &str) {
        let mut presences = self.presences.write().await;

        if let Some(presence) = presences.get_mut(user_id) {
            presence.last_activity = Instant::now();
            if presence.status == PresenceStatus::Away {
                presence.status = PresenceStatus::Online;
            }
        }
    }

    /// Set custom status for a user
    pub async fn set_custom_status(&self, user_id: &str, status: Option<String>) {
        let mut presences = self.presences.write().await;

        if let Some(presence) = presences.get_mut(user_id) {
            presence.custom_status = status;
        }
    }

    /// Set user status manually
    pub async fn set_status(&self, user_id: &str, status: PresenceStatus) {
        let mut presences = self.presences.write().await;

        if let Some(presence) = presences.get_mut(user_id) {
            presence.status = status;
            presence.last_activity = Instant::now();
        }
    }

    /// Get user presence
    pub async fn get_presence(&self, user_id: &str) -> Option<UserPresence> {
        let presences = self.presences.read().await;
        presences.get(user_id).cloned()
    }

    /// Get multiple user presences
    pub async fn get_presences(&self, user_ids: &[String]) -> HashMap<String, UserPresence> {
        let presences = self.presences.read().await;

        user_ids
            .iter()
            .filter_map(|id| presences.get(id).map(|p| (id.clone(), p.clone())))
            .collect()
    }

    /// Get all online users
    pub async fn get_online_users(&self) -> Vec<UserPresence> {
        let presences = self.presences.read().await;

        presences
            .values()
            .filter(|p| p.status == PresenceStatus::Online || p.status == PresenceStatus::Away)
            .cloned()
            .collect()
    }

    /// Start typing indicator
    pub async fn start_typing(&self, user_id: &str, user_name: &str, room_id: &str) {
        let mut indicators = self.typing_indicators.write().await;

        let room_indicators = indicators
            .entry(room_id.to_string())
            .or_insert_with(Vec::new);

        // Remove existing indicator for this user if any
        room_indicators.retain(|ind| ind.user_id != user_id);

        // Add new indicator
        room_indicators.push(TypingIndicator {
            user_id: user_id.to_string(),
            user_name: user_name.to_string(),
            room_id: room_id.to_string(),
            started_at: Self::now_timestamp(),
            expires_at: Instant::now() + self.config.typing_timeout,
        });

        tracing::debug!("User {} started typing in room {}", user_id, room_id);
    }

    /// Stop typing indicator
    pub async fn stop_typing(&self, user_id: &str, room_id: &str) {
        let mut indicators = self.typing_indicators.write().await;

        if let Some(room_indicators) = indicators.get_mut(room_id) {
            room_indicators.retain(|ind| ind.user_id != user_id);

            if room_indicators.is_empty() {
                indicators.remove(room_id);
            }
        }

        tracing::debug!("User {} stopped typing in room {}", user_id, room_id);
    }

    /// Get typing users in a room
    pub async fn get_typing_users(&self, room_id: &str) -> Vec<TypingIndicator> {
        let mut indicators = self.typing_indicators.write().await;

        if let Some(room_indicators) = indicators.get_mut(room_id) {
            let now = Instant::now();

            // Remove expired indicators
            room_indicators.retain(|ind| ind.expires_at > now);

            if room_indicators.is_empty() {
                indicators.remove(room_id);
                Vec::new()
            } else {
                room_indicators.clone()
            }
        } else {
            Vec::new()
        }
    }

    /// Update away statuses based on inactivity
    pub async fn update_away_statuses(&self) {
        let mut presences = self.presences.write().await;
        let now = Instant::now();

        for presence in presences.values_mut() {
            if presence.status == PresenceStatus::Online {
                let inactive_duration = now.duration_since(presence.last_activity);

                if inactive_duration >= self.config.away_timeout {
                    presence.status = PresenceStatus::Away;
                    tracing::debug!("User {} marked as away due to inactivity", presence.user_id);
                }
            }
        }
    }

    /// Clean up old presence data
    pub async fn cleanup_old_presences(&self) {
        let mut presences = self.presences.write().await;
        let now = Instant::now();

        presences.retain(|user_id, presence| {
            if presence.status == PresenceStatus::Offline {
                let inactive_duration = now.duration_since(presence.last_activity);

                if inactive_duration >= self.config.cleanup_timeout {
                    tracing::debug!("Cleaning up presence data for user {}", user_id);
                    return false;
                }
            }
            true
        });
    }

    /// Clean up expired typing indicators
    pub async fn cleanup_typing_indicators(&self) {
        let mut indicators = self.typing_indicators.write().await;
        let now = Instant::now();

        // Remove expired indicators from all rooms
        for (room_id, room_indicators) in indicators.iter_mut() {
            let initial_count = room_indicators.len();
            room_indicators.retain(|ind| ind.expires_at > now);

            let removed = initial_count - room_indicators.len();
            if removed > 0 {
                tracing::debug!(
                    "Removed {} expired typing indicators from room {}",
                    removed,
                    room_id
                );
            }
        }

        // Remove empty rooms
        indicators.retain(|_, room_indicators| !room_indicators.is_empty());
    }

    /// Get statistics
    pub async fn get_stats(&self) -> PresenceStats {
        let presences = self.presences.read().await;
        let indicators = self.typing_indicators.read().await;

        let online_count = presences
            .values()
            .filter(|p| p.status == PresenceStatus::Online)
            .count();

        let away_count = presences
            .values()
            .filter(|p| p.status == PresenceStatus::Away)
            .count();

        let offline_count = presences
            .values()
            .filter(|p| p.status == PresenceStatus::Offline)
            .count();

        let typing_count: usize = indicators.values().map(|v| v.len()).sum();

        PresenceStats {
            total_users: presences.len(),
            online_users: online_count,
            away_users: away_count,
            offline_users: offline_count,
            typing_users: typing_count,
        }
    }
}

impl Default for PresenceManager {
    fn default() -> Self {
        Self::new(PresenceConfig::default())
    }
}

/// Presence statistics
#[derive(Debug, Clone, Serialize)]
pub struct PresenceStats {
    pub total_users: usize,
    pub online_users: usize,
    pub away_users: usize,
    pub offline_users: usize,
    pub typing_users: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_presence_lifecycle() {
        let manager = PresenceManager::default();

        // User goes online
        manager.user_online("user-1").await;
        let presence = manager.get_presence("user-1").await.unwrap();
        assert_eq!(presence.status, PresenceStatus::Online);
        assert_eq!(presence.session_count, 1);

        // User goes offline
        manager.user_offline("user-1").await;
        let presence = manager.get_presence("user-1").await.unwrap();
        assert_eq!(presence.status, PresenceStatus::Offline);
        assert_eq!(presence.session_count, 0);
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let manager = PresenceManager::default();

        // User opens multiple sessions
        manager.user_online("user-1").await;
        manager.user_online("user-1").await;

        let presence = manager.get_presence("user-1").await.unwrap();
        assert_eq!(presence.session_count, 2);

        // Close one session
        let offline = manager.user_offline("user-1").await;
        assert!(!offline); // Still has sessions

        let presence = manager.get_presence("user-1").await.unwrap();
        assert_eq!(presence.status, PresenceStatus::Online);
        assert_eq!(presence.session_count, 1);

        // Close last session
        let offline = manager.user_offline("user-1").await;
        assert!(offline); // Now fully offline
    }

    #[tokio::test]
    async fn test_typing_indicators() {
        let manager = PresenceManager::default();

        // Start typing
        manager.start_typing("user-1", "Alice", "room-1").await;

        let typing = manager.get_typing_users("room-1").await;
        assert_eq!(typing.len(), 1);
        assert_eq!(typing[0].user_id, "user-1");

        // Stop typing
        manager.stop_typing("user-1", "room-1").await;

        let typing = manager.get_typing_users("room-1").await;
        assert_eq!(typing.len(), 0);
    }

    #[tokio::test]
    async fn test_custom_status() {
        let manager = PresenceManager::default();

        manager.user_online("user-1").await;
        manager
            .set_custom_status("user-1", Some("In a meeting".to_string()))
            .await;

        let presence = manager.get_presence("user-1").await.unwrap();
        assert_eq!(presence.custom_status, Some("In a meeting".to_string()));
    }
}
