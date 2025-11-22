/// Redis Adapter for Socket.IO
///
/// Enables horizontal scaling by using Redis pub/sub to broadcast events
/// across multiple server instances
use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Redis message for inter-server communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisMessage {
    pub server_id: String,
    pub message_type: RedisMessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RedisMessageType {
    Emit {
        user_id: Option<String>,
        session_id: Option<String>,
        room: Option<String>,
        event: String,
        data: JsonValue,
    },
    Broadcast {
        room: String,
        event: String,
        data: JsonValue,
        exclude_sid: Option<String>,
    },
    UserJoined {
        user_id: String,
        session_id: String,
    },
    UserLeft {
        user_id: String,
        session_id: String,
    },
}

use std::time::Duration;
use tokio::time::sleep;

/// Redis adapter for Socket.IO scaling
pub struct RedisAdapter {
    redis_client: redis::Client,
    server_id: String,
    channel: String,
    reconnect_attempts: usize,
    reconnect_delay: Duration,
}

impl RedisAdapter {
    /// Create a new Redis adapter
    #[allow(dead_code)]
    pub fn new(redis_url: &str, server_id: String) -> Result<Self, redis::RedisError> {
        let redis_client = redis::Client::open(redis_url)?;

        Ok(Self {
            redis_client,
            server_id,
            channel: "socketio:events".to_string(),
            reconnect_attempts: 10,
            reconnect_delay: Duration::from_secs(1),
        })
    }

    /// Create with custom reconnection settings
    pub fn new_with_config(
        redis_url: &str,
        server_id: String,
        reconnect_attempts: usize,
        reconnect_delay: Duration,
    ) -> Result<Self, redis::RedisError> {
        let redis_client = redis::Client::open(redis_url)?;

        Ok(Self {
            redis_client,
            server_id,
            channel: "socketio:events".to_string(),
            reconnect_attempts,
            reconnect_delay,
        })
    }

    /// Get the server ID
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Publish an event to Redis with automatic retry on failure
    pub async fn publish(&self, message: RedisMessage) -> Result<(), Box<dyn std::error::Error>> {
        let serialized = serde_json::to_string(&message)?;

        for attempt in 0..self.reconnect_attempts {
            match self.redis_client.get_multiplexed_async_connection().await {
                Ok(mut conn) => match conn.publish::<_, _, ()>(&self.channel, &serialized).await {
                    Ok(_) => {
                        tracing::debug!("Published message to Redis: {:?}", message.message_type);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to publish to Redis (attempt {}/{}): {}",
                            attempt + 1,
                            self.reconnect_attempts,
                            e
                        );

                        if attempt < self.reconnect_attempts - 1 {
                            sleep(self.reconnect_delay * (attempt as u32 + 1)).await;
                        }
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to connect to Redis (attempt {}/{}): {}",
                        attempt + 1,
                        self.reconnect_attempts,
                        e
                    );

                    if attempt < self.reconnect_attempts - 1 {
                        sleep(self.reconnect_delay * (attempt as u32 + 1)).await;
                    }
                }
            }
        }

        Err("Failed to publish to Redis after all retry attempts".into())
    }

    /// Subscribe to Redis channel and handle incoming messages with automatic reconnection
    pub async fn subscribe<F>(&self, mut handler: F) -> Result<(), String>
    where
        F: FnMut(RedisMessage) + Send + 'static,
    {
        loop {
            match self.subscribe_with_reconnect(&mut handler).await {
                Ok(_) => {
                    tracing::info!("Redis subscription ended normally");
                    break Ok(());
                }
                Err(e) => {
                    tracing::error!("Redis subscription error: {}, reconnecting...", e);
                    sleep(self.reconnect_delay).await;
                }
            }
        }
    }

    /// Internal subscribe method with connection handling
    async fn subscribe_with_reconnect<F>(&self, handler: &mut F) -> Result<(), String>
    where
        F: FnMut(RedisMessage) + Send + 'static,
    {
        let mut pubsub = self
            .redis_client
            .get_async_pubsub()
            .await
            .map_err(|e| e.to_string())?;
        pubsub
            .subscribe(&self.channel)
            .await
            .map_err(|e| e.to_string())?;

        tracing::info!("Subscribed to Redis channel: {}", self.channel);

        let mut stream = pubsub.on_message();

        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload().map_err(|e| e.to_string())?;

            match serde_json::from_str::<RedisMessage>(&payload) {
                Ok(redis_msg) => {
                    // Skip messages from this server
                    if redis_msg.server_id != self.server_id {
                        handler(redis_msg);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to parse Redis message: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Publish emit event
    pub async fn publish_emit(
        &self,
        user_id: Option<String>,
        session_id: Option<String>,
        room: Option<String>,
        event: String,
        data: JsonValue,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = RedisMessage {
            server_id: self.server_id.clone(),
            message_type: RedisMessageType::Emit {
                user_id,
                session_id,
                room,
                event,
                data,
            },
        };

        self.publish(message).await
    }

    /// Publish broadcast event
    pub async fn publish_broadcast(
        &self,
        room: String,
        event: String,
        data: JsonValue,
        exclude_sid: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = RedisMessage {
            server_id: self.server_id.clone(),
            message_type: RedisMessageType::Broadcast {
                room,
                event,
                data,
                exclude_sid,
            },
        };

        self.publish(message).await
    }

    /// Publish user joined event
    pub async fn publish_user_joined(
        &self,
        user_id: String,
        session_id: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = RedisMessage {
            server_id: self.server_id.clone(),
            message_type: RedisMessageType::UserJoined {
                user_id,
                session_id,
            },
        };

        self.publish(message).await
    }

    /// Publish user left event
    pub async fn publish_user_left(
        &self,
        user_id: String,
        session_id: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = RedisMessage {
            server_id: self.server_id.clone(),
            message_type: RedisMessageType::UserLeft {
                user_id,
                session_id,
            },
        };

        self.publish(message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_message_serialization() {
        let message = RedisMessage {
            server_id: "server-1".to_string(),
            message_type: RedisMessageType::Emit {
                user_id: Some("user-123".to_string()),
                session_id: None,
                room: None,
                event: "test-event".to_string(),
                data: serde_json::json!({"message": "hello"}),
            },
        };

        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: RedisMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.server_id, "server-1");
    }
}
