use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, warn};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[allow(dead_code)]
impl WebhookPayload {
    pub fn new(event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            event_type: event_type.into(),
            data,
            timestamp: Some(chrono::Utc::now().timestamp()),
        }
    }

    pub fn user_signup(username: &str, email: Option<&str>) -> Self {
        Self::new(
            "user.signup",
            json!({
                "username": username,
                "email": email,
            }),
        )
    }

    pub fn user_signin(username: &str, email: Option<&str>) -> Self {
        Self::new(
            "user.signin",
            json!({
                "username": username,
                "email": email,
            }),
        )
    }

    pub fn chat_created(chat_id: &str, user_id: &str, title: Option<&str>) -> Self {
        Self::new(
            "chat.created",
            json!({
                "chat_id": chat_id,
                "user_id": user_id,
                "title": title,
            }),
        )
    }

    pub fn message_created(chat_id: &str, message_id: &str, user_id: &str, content: &str) -> Self {
        Self::new(
            "message.created",
            json!({
                "chat_id": chat_id,
                "message_id": message_id,
                "user_id": user_id,
                "content": content,
            }),
        )
    }
}

/// Post webhook to configured URL
#[allow(dead_code)]
pub async fn post_webhook(webhook_url: &str, payload: WebhookPayload) -> Result<(), AppError> {
    if webhook_url.is_empty() {
        debug!("Webhook URL is empty, skipping webhook post");
        return Ok(());
    }

    debug!(
        "Posting webhook to {} with payload: {:?}",
        webhook_url, payload
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    match client.post(webhook_url).json(&payload).send().await {
        Ok(response) => {
            if response.status().is_success() {
                debug!("Webhook posted successfully: {}", response.status());
                Ok(())
            } else {
                warn!(
                    "Webhook post returned non-success status: {}",
                    response.status()
                );
                // Don't fail the request if webhook fails
                Ok(())
            }
        }
        Err(e) => {
            error!("Failed to post webhook: {}", e);
            // Don't fail the request if webhook fails
            Ok(())
        }
    }
}

/// Post user webhook (user-specific webhook URL)
#[allow(dead_code)]
pub async fn post_user_webhook(
    webhook_url: &str,
    user_id: &str,
    payload: WebhookPayload,
) -> Result<(), AppError> {
    if webhook_url.is_empty() {
        return Ok(());
    }

    let mut enriched_payload = payload;

    // Add user_id to payload data
    if let Some(data_obj) = enriched_payload.data.as_object_mut() {
        data_obj.insert("user_id".to_string(), json!(user_id));
    }

    post_webhook(webhook_url, enriched_payload).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_payload_creation() {
        let payload = WebhookPayload::user_signup("testuser", Some("test@example.com"));

        assert_eq!(payload.event_type, "user.signup");
        assert!(payload.timestamp.is_some());
        assert_eq!(payload.data["username"], "testuser");
        assert_eq!(payload.data["email"], "test@example.com");
    }

    #[test]
    fn test_chat_created_payload() {
        let payload = WebhookPayload::chat_created("chat123", "user456", Some("Test Chat"));

        assert_eq!(payload.event_type, "chat.created");
        assert_eq!(payload.data["chat_id"], "chat123");
        assert_eq!(payload.data["user_id"], "user456");
        assert_eq!(payload.data["title"], "Test Chat");
    }
}
