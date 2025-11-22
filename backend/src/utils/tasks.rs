use futures::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub item_id: Option<String>,
    pub status: TaskStatus,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[allow(dead_code)]
pub struct TaskManager {
    // In-memory task storage
    tasks: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
    // Item ID -> Task IDs mapping
    item_tasks: Arc<RwLock<HashMap<String, Vec<String>>>>,
    // Redis pool (optional)
    redis: Option<deadpool_redis::Pool>,
    // Redis URL for pub/sub
    redis_url: Option<String>,
    // Redis key prefix
    redis_key_prefix: String,
}

#[allow(dead_code)]
impl TaskManager {
    pub fn new(
        redis: Option<deadpool_redis::Pool>,
        redis_url: Option<String>,
        redis_key_prefix: String,
    ) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            item_tasks: Arc::new(RwLock::new(HashMap::new())),
            redis,
            redis_url,
            redis_key_prefix,
        }
    }

    /// Create a new task
    pub async fn create_task<F>(&self, future: F, item_id: Option<String>) -> AppResult<String>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let task_id = Uuid::new_v4().to_string();

        // Create the tokio task
        let task_handle = tokio::spawn(future);

        // Store in memory
        self.tasks
            .write()
            .await
            .insert(task_id.clone(), task_handle);

        // Store item mapping
        if let Some(item_id) = &item_id {
            let mut item_tasks = self.item_tasks.write().await;
            item_tasks
                .entry(item_id.clone())
                .or_insert_with(Vec::new)
                .push(task_id.clone());
        }

        // Store in Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let tasks_key = format!("{}:tasks", self.redis_key_prefix);
                let item_value = item_id.as_deref().unwrap_or("");

                // Store task -> item_id mapping
                let _: Result<(), redis::RedisError> =
                    conn.hset(&tasks_key, &task_id, item_value).await;

                // Store item -> task mapping
                if let Some(item_id) = &item_id {
                    let item_tasks_key =
                        format!("{}:tasks:item:{}", self.redis_key_prefix, item_id);
                    let _: Result<(), redis::RedisError> =
                        conn.sadd(&item_tasks_key, &task_id).await;
                }
            }
        }

        info!("Created task {} for item {:?}", task_id, item_id);
        Ok(task_id)
    }

    /// Stop a task by ID
    pub async fn stop_task(&self, task_id: &str) -> AppResult<()> {
        // Find the item_id for this task
        let item_id = {
            let item_tasks = self.item_tasks.read().await;
            item_tasks
                .iter()
                .find(|(_, tasks)| tasks.contains(&task_id.to_string()))
                .map(|(id, _)| id.clone())
        };

        // Stop local task
        if let Some(handle) = self.tasks.write().await.remove(task_id) {
            handle.abort();
            info!("Stopped local task {}", task_id);
        }

        // Send stop command via Redis pub/sub if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let pubsub_channel = format!("{}:tasks:commands", self.redis_key_prefix);
                let command = json!({
                    "action": "stop",
                    "task_id": task_id,
                });

                let _: Result<(), redis::RedisError> =
                    conn.publish(&pubsub_channel, command.to_string()).await;

                info!("Published stop command for task {} to Redis", task_id);
            }
        }

        // Clean up task data
        self.cleanup_task(task_id, item_id).await?;

        Ok(())
    }

    /// Cleanup task data after completion
    async fn cleanup_task(&self, task_id: &str, item_id: Option<String>) -> AppResult<()> {
        // Remove from memory
        self.tasks.write().await.remove(task_id);

        // Remove from item mapping
        if let Some(item_id) = &item_id {
            let mut item_tasks = self.item_tasks.write().await;
            if let Some(tasks) = item_tasks.get_mut(item_id) {
                tasks.retain(|id| id != task_id);
                if tasks.is_empty() {
                    item_tasks.remove(item_id);
                }
            }
        }

        // Clean up Redis if available
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let tasks_key = format!("{}:tasks", self.redis_key_prefix);

                // Get item_id from Redis if not provided
                let redis_item_id: Result<Option<String>, redis::RedisError> =
                    conn.hget(&tasks_key, task_id).await;

                let item_id_to_clean = item_id.or_else(|| redis_item_id.ok().flatten());

                // Remove task from hash
                let _: Result<(), redis::RedisError> = conn.hdel(&tasks_key, task_id).await;

                // Remove from item set
                if let Some(item_id) = item_id_to_clean {
                    if !item_id.is_empty() {
                        let item_tasks_key =
                            format!("{}:tasks:item:{}", self.redis_key_prefix, item_id);
                        let _: Result<(), redis::RedisError> =
                            conn.srem(&item_tasks_key, task_id).await;
                    }
                }
            }
        }

        Ok(())
    }

    /// List all active tasks
    pub async fn list_tasks(&self) -> AppResult<Vec<String>> {
        // Get local tasks
        let local_tasks: Vec<String> = self.tasks.read().await.keys().cloned().collect();

        // If Redis is available, also get remote tasks
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let tasks_key = format!("{}:tasks", self.redis_key_prefix);
                let redis_tasks: Result<Vec<String>, redis::RedisError> =
                    conn.hkeys(&tasks_key).await;

                if let Ok(tasks) = redis_tasks {
                    // Combine local and Redis tasks (deduplicated)
                    let mut all_tasks: Vec<String> = local_tasks;
                    for task in tasks {
                        if !all_tasks.contains(&task) {
                            all_tasks.push(task);
                        }
                    }
                    return Ok(all_tasks);
                }
            }
        }

        Ok(local_tasks)
    }

    /// List tasks for a specific item (e.g., chat, note)
    pub async fn list_tasks_by_item(&self, item_id: &str) -> AppResult<Vec<String>> {
        // Get local tasks
        let local_tasks = self
            .item_tasks
            .read()
            .await
            .get(item_id)
            .cloned()
            .unwrap_or_default();

        // If Redis is available, also get remote tasks
        if let Some(redis) = &self.redis {
            if let Ok(mut conn) = redis.get().await {
                let item_tasks_key = format!("{}:tasks:item:{}", self.redis_key_prefix, item_id);
                let redis_tasks: Result<Vec<String>, redis::RedisError> =
                    conn.smembers(&item_tasks_key).await;

                if let Ok(tasks) = redis_tasks {
                    // Combine local and Redis tasks (deduplicated)
                    let mut all_tasks = local_tasks;
                    for task in tasks {
                        if !all_tasks.contains(&task) {
                            all_tasks.push(task);
                        }
                    }
                    return Ok(all_tasks);
                }
            }
        }

        Ok(local_tasks)
    }

    /// Stop all tasks for a specific item
    pub async fn stop_item_tasks(&self, item_id: &str) -> AppResult<()> {
        let task_ids = self.list_tasks_by_item(item_id).await?;

        for task_id in task_ids {
            if let Err(e) = self.stop_task(&task_id).await {
                warn!("Failed to stop task {}: {}", task_id, e);
            }
        }

        Ok(())
    }

    /// Start listening to Redis pub/sub commands
    pub async fn start_redis_listener(&self) -> AppResult<()> {
        if let Some(redis) = &self.redis {
            let pubsub_channel = format!("{}:tasks:commands", self.redis_key_prefix);

            info!("Starting Redis task command listener on {}", pubsub_channel);

            let tasks = self.tasks.clone();
            let _redis_clone = redis.clone();
            let _redis_key_prefix = self.redis_key_prefix.clone();

            // Get a dedicated connection for pubsub by creating a new client
            let redis_url = self
                .redis_url
                .clone()
                .ok_or_else(|| AppError::Redis("Redis URL not configured".to_string()))?;

            let client =
                redis::Client::open(redis_url).map_err(|e| AppError::Redis(e.to_string()))?;

            tokio::spawn(async move {
                let mut pubsub = match client.get_async_pubsub().await {
                    Ok(pubsub) => pubsub,
                    Err(e) => {
                        error!("Failed to get Redis pubsub connection: {}", e);
                        return;
                    }
                };

                if let Err(e) = pubsub.subscribe(&pubsub_channel).await {
                    error!("Failed to subscribe to Redis channel: {}", e);
                    return;
                }

                let mut message_stream = pubsub.on_message();
                loop {
                    match message_stream.next().await {
                        Some(msg) => {
                            let payload: String = match msg.get_payload() {
                                Ok(p) => p,
                                Err(e) => {
                                    error!("Failed to get message payload: {}", e);
                                    continue;
                                }
                            };

                            match serde_json::from_str::<serde_json::Value>(&payload) {
                                Ok(command) => {
                                    if let Some("stop") =
                                        command.get("action").and_then(|v| v.as_str())
                                    {
                                        if let Some(task_id) =
                                            command.get("task_id").and_then(|v| v.as_str())
                                        {
                                            // Stop local task if it exists
                                            if let Some(handle) =
                                                tasks.write().await.remove(task_id)
                                            {
                                                handle.abort();
                                                info!("Stopped task {} via Redis command", task_id);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse command JSON: {}", e);
                                }
                            }
                        }
                        None => {
                            warn!("Redis pub/sub connection closed");
                            break;
                        }
                    }
                }
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_manager_create() {
        let manager = TaskManager::new(None, None, "test".to_string());

        let task_id = manager
            .create_task(
                async {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                },
                None,
            )
            .await
            .unwrap();

        assert!(!task_id.is_empty());

        let tasks = manager.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0], task_id);
    }

    #[tokio::test]
    async fn test_task_manager_stop() {
        let manager = TaskManager::new(None, None, "test".to_string());

        let task_id = manager
            .create_task(
                async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                },
                None,
            )
            .await
            .unwrap();

        manager.stop_task(&task_id).await.unwrap();

        let tasks = manager.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_task_manager_item_tasks() {
        let manager = TaskManager::new(None, None, "test".to_string());
        let item_id = "test-item".to_string();

        let task_id1 = manager
            .create_task(
                async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                },
                Some(item_id.clone()),
            )
            .await
            .unwrap();

        let task_id2 = manager
            .create_task(
                async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                },
                Some(item_id.clone()),
            )
            .await
            .unwrap();

        let item_tasks = manager.list_tasks_by_item(&item_id).await.unwrap();
        assert_eq!(item_tasks.len(), 2);
        assert!(item_tasks.contains(&task_id1));
        assert!(item_tasks.contains(&task_id2));

        manager.stop_item_tasks(&item_id).await.unwrap();

        let item_tasks = manager.list_tasks_by_item(&item_id).await.unwrap();
        assert_eq!(item_tasks.len(), 0);
    }
}
