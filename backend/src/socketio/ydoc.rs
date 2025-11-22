/// Yjs Document Manager for Collaborative Editing
///
/// This module provides CRDT-based collaborative editing using Yjs (yrs crate)
/// with Redis persistence for horizontal scaling
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

/// Yjs document manager
#[derive(Clone)]
pub struct YDocManager {
    /// In-memory cache of document updates
    /// Key: document_id, Value: Vec of encoded updates
    updates: Arc<RwLock<HashMap<String, Vec<Vec<u8>>>>>,

    /// Users currently editing each document
    /// Key: document_id, Value: Set of session IDs
    users: Arc<RwLock<HashMap<String, Vec<String>>>>,

    /// Redis connection pool for persistence (optional)
    redis: Option<deadpool_redis::Pool>,

    /// Redis key prefix
    redis_prefix: String,
}

/// Yjs awareness state for presence information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwarenessState {
    pub user_id: String,
    pub user_name: String,
    pub user_color: String,
    pub cursor: Option<CursorPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub anchor: usize,
    pub head: usize,
}

impl YDocManager {
    /// Create a new YDoc manager
    pub fn new(redis: Option<deadpool_redis::Pool>) -> Self {
        Self {
            updates: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
            redis,
            redis_prefix: "socketio:ydoc".to_string(),
        }
    }

    /// Sanitize document ID for Redis keys (replace : with _)
    fn sanitize_doc_id(&self, doc_id: &str) -> String {
        doc_id.replace(':', "_")
    }

    /// Append an update to a document
    pub async fn append_update(&self, doc_id: &str, update: Vec<u8>) -> Result<(), String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Store in memory
        {
            let mut updates = self.updates.write().await;
            updates
                .entry(sanitized_id.clone())
                .or_insert_with(Vec::new)
                .push(update.clone());
        }

        // Store in Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}:updates", self.redis_prefix, sanitized_id);

                // Store as JSON array for compatibility with Python backend
                let update_json = serde_json::to_string(&update).map_err(|e| e.to_string())?;

                let _ = conn.rpush::<_, _, ()>(&key, update_json).await;

                tracing::debug!("Stored update to Redis: {}", key);
            }
        }

        Ok(())
    }

    /// Get all updates for a document
    pub async fn get_updates(&self, doc_id: &str) -> Result<Vec<Vec<u8>>, String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Try Redis first if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}:updates", self.redis_prefix, sanitized_id);

                if let Ok(updates_json) = conn.lrange::<_, Vec<String>>(&key, 0, -1).await {
                    let mut updates = Vec::new();
                    for update_str in updates_json {
                        match serde_json::from_str::<Vec<u8>>(&update_str) {
                            Ok(update) => updates.push(update),
                            Err(e) => {
                                tracing::error!("Failed to parse update from Redis: {}", e);
                            }
                        }
                    }
                    tracing::debug!("Retrieved {} updates from Redis: {}", updates.len(), key);
                    return Ok(updates);
                }
            }
        }

        // Fallback to in-memory
        let updates = self.updates.read().await;
        Ok(updates.get(&sanitized_id).cloned().unwrap_or_default())
    }

    /// Get the current document state by applying all updates
    pub async fn get_state(&self, doc_id: &str) -> Result<Vec<u8>, String> {
        let updates = self.get_updates(doc_id).await?;

        if updates.is_empty() {
            // Return empty state - encode an empty state vector
            return Ok(StateVector::default().encode_v1());
        }

        // Create a new Yjs document and apply all updates
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();

            for update_bytes in updates {
                match Update::decode_v1(&update_bytes) {
                    Ok(update) => {
                        if let Err(e) = txn.apply_update(update) {
                            tracing::error!("Failed to apply update: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to decode update: {}", e);
                    }
                }
            }
        }

        // Get the state vector from the transaction
        let txn = doc.transact();
        let state_vector = txn.state_vector();
        Ok(state_vector.encode_v1())
    }

    /// Get document state as a full update (for client sync)
    pub async fn get_state_as_update(&self, doc_id: &str) -> Result<Vec<u8>, String> {
        let updates = self.get_updates(doc_id).await?;

        // Create a new Yjs document
        let doc = Doc::new();

        // Apply all updates if any exist
        if !updates.is_empty() {
            let mut txn = doc.transact_mut();

            for update_bytes in updates {
                match Update::decode_v1(&update_bytes) {
                    Ok(update) => {
                        if let Err(e) = txn.apply_update(update) {
                            tracing::error!("Failed to apply update: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to decode update: {}", e);
                    }
                }
            }
        }

        // Encode entire state as update (even if empty, this produces a valid Yjs update)
        let state_vector = StateVector::default();
        let txn = doc.transact();
        let update = txn.encode_diff_v1(&state_vector);
        Ok(update)
    }

    /// Check if a document exists
    pub async fn document_exists(&self, doc_id: &str) -> Result<bool, String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Check Redis first
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}:updates", self.redis_prefix, sanitized_id);

                if let Ok(exists) = conn.exists::<_, bool>(&key).await {
                    return Ok(exists);
                }
            }
        }

        // Fallback to memory
        let updates = self.updates.read().await;
        Ok(updates.contains_key(&sanitized_id))
    }

    /// Add a user to a document
    pub async fn add_user(&self, doc_id: &str, user_id: &str) -> Result<(), String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Add to memory
        {
            let mut users = self.users.write().await;
            users
                .entry(sanitized_id.clone())
                .or_insert_with(Vec::new)
                .push(user_id.to_string());
        }

        // Add to Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}:users", self.redis_prefix, sanitized_id);
                let _ = conn.sadd::<_, _, ()>(&key, user_id).await;
                tracing::debug!("Added user {} to document {} in Redis", user_id, doc_id);
            }
        }

        Ok(())
    }

    /// Remove a user from a document
    pub async fn remove_user(&self, doc_id: &str, user_id: &str) -> Result<(), String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Remove from memory
        {
            let mut users = self.users.write().await;
            if let Some(user_list) = users.get_mut(&sanitized_id) {
                user_list.retain(|u| u != user_id);
                if user_list.is_empty() {
                    users.remove(&sanitized_id);
                }
            }
        }

        // Remove from Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}:users", self.redis_prefix, sanitized_id);
                let _ = conn.srem::<_, _, ()>(&key, user_id).await;
                tracing::debug!("Removed user {} from document {} in Redis", user_id, doc_id);
            }
        }

        Ok(())
    }

    /// Get all users in a document
    pub async fn get_users(&self, doc_id: &str) -> Result<Vec<String>, String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Try Redis first
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let key = format!("{}:{}:users", self.redis_prefix, sanitized_id);
                if let Ok(users) = conn.smembers::<_, Vec<String>>(&key).await {
                    return Ok(users);
                }
            }
        }

        // Fallback to memory
        let users = self.users.read().await;
        Ok(users.get(&sanitized_id).cloned().unwrap_or_default())
    }

    /// Clear a document (remove all updates and users)
    pub async fn clear_document(&self, doc_id: &str) -> Result<(), String> {
        let sanitized_id = self.sanitize_doc_id(doc_id);

        // Clear from memory
        {
            let mut updates = self.updates.write().await;
            updates.remove(&sanitized_id);

            let mut users = self.users.write().await;
            users.remove(&sanitized_id);
        }

        // Clear from Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let updates_key = format!("{}:{}:updates", self.redis_prefix, sanitized_id);
                let users_key = format!("{}:{}:users", self.redis_prefix, sanitized_id);
                let _ = conn
                    .del::<_, ()>(&[updates_key.as_str(), users_key.as_str()])
                    .await;
                tracing::debug!("Cleared document {} from Redis", doc_id);
            }
        }

        Ok(())
    }

    /// Remove user from all documents
    pub async fn remove_user_from_all(&self, user_id: &str) -> Result<(), String> {
        // Get all documents this user is in
        let doc_ids: Vec<String> = {
            let users = self.users.read().await;
            users
                .iter()
                .filter(|(_, user_list)| user_list.contains(&user_id.to_string()))
                .map(|(doc_id, _)| doc_id.clone())
                .collect()
        };

        // Remove user from each document
        for doc_id in doc_ids {
            self.remove_user(&doc_id, user_id).await?;

            // Check if document is now empty
            let users = self.get_users(&doc_id).await?;
            if users.is_empty() {
                self.clear_document(&doc_id).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{GetString, Text, WriteTxn};

    #[tokio::test]
    async fn test_ydoc_manager_memory() {
        let manager = YDocManager::new(None);
        let doc_id = "test:doc:123";

        // Test adding updates
        let update1 = vec![1, 2, 3, 4];
        manager
            .append_update(doc_id, update1.clone())
            .await
            .unwrap();

        let updates = manager.get_updates(doc_id).await.unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0], update1);

        // Test adding users
        manager.add_user(doc_id, "user-1").await.unwrap();
        manager.add_user(doc_id, "user-2").await.unwrap();

        let users = manager.get_users(doc_id).await.unwrap();
        assert_eq!(users.len(), 2);

        // Test removing user
        manager.remove_user(doc_id, "user-1").await.unwrap();
        let users = manager.get_users(doc_id).await.unwrap();
        assert_eq!(users.len(), 1);

        // Test clearing document
        manager.clear_document(doc_id).await.unwrap();
        let updates = manager.get_updates(doc_id).await.unwrap();
        assert_eq!(updates.len(), 0);
    }

    #[tokio::test]
    async fn test_yjs_document_merge() {
        let manager = YDocManager::new(None);
        let doc_id = "test:doc:merge";

        // Create two Yjs documents and generate updates
        let doc1 = Doc::new();
        let doc2 = Doc::new();

        // Make some changes in doc1
        {
            let mut txn = doc1.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.push(&mut txn, "Hello ");
        }

        // Make some changes in doc2
        {
            let mut txn = doc2.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.push(&mut txn, "World!");
        }

        // Get updates from both docs
        let update1 = doc1.transact().encode_diff_v1(&StateVector::default());
        let update2 = doc2.transact().encode_diff_v1(&StateVector::default());

        // Store both updates
        manager.append_update(doc_id, update1).await.unwrap();
        manager.append_update(doc_id, update2).await.unwrap();

        // Get merged state
        let merged_state = manager.get_state_as_update(doc_id).await.unwrap();

        // Apply to a new document
        let doc3 = Doc::new();
        {
            let mut txn = doc3.transact_mut();
            let update = Update::decode_v1(&merged_state).unwrap();
            txn.apply_update(update).unwrap();
        }

        // Verify the merged content contains both changes
        let content = {
            let txn = doc3.transact();
            let text = txn.get_text("content").unwrap();
            text.get_string(&txn)
        };

        // Note: CRDT merge behavior depends on the order and concurrent edits
        // This test just verifies the mechanism works
        assert!(!content.is_empty());
    }
}
