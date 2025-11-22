use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Message {
    pub id: String,
    #[sqlx(default)]
    pub chat_id: Option<String>,
    #[sqlx(default)]
    pub channel_id: Option<String>,
    pub user_id: String,
    pub content: String,
    #[sqlx(default)]
    pub role: Option<String>,
    #[sqlx(default)]
    pub model: Option<String>,
    #[sqlx(default)]
    pub reply_to_id: Option<String>,
    #[sqlx(default)]
    pub parent_id: Option<String>,
    #[sqlx(skip)]
    #[serde(skip)]
    pub data: Option<serde_json::Value>,
    #[sqlx(default)]
    pub data_str: Option<String>,
    #[sqlx(skip)]
    #[serde(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Message {
    pub fn parse_meta(&mut self) {
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
    }

    pub fn parse_data(&mut self) {
        if let Some(ref data_str) = self.data_str {
            self.data = serde_json::from_str(data_str).ok();
        }
    }

    pub fn get_meta(&self) -> Option<serde_json::Value> {
        self.meta.clone().or_else(|| {
            self.meta_str
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
        })
    }

    pub fn get_data(&self) -> Option<serde_json::Value> {
        self.data.clone().or_else(|| {
            self.data_str
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
        })
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateMessageRequest {
    pub id: String,
    pub chat_id: Option<String>,
    pub channel_id: Option<String>,
    pub content: String,
    pub role: Option<String>,
    pub model: Option<String>,
    pub reply_to_id: Option<String>,
    pub parent_id: Option<String>,
    pub data: Option<serde_json::Value>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct MessageForm {
    pub content: String,
    #[serde(default)]
    pub reply_to_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<crate::models::user::UserNameResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Message> for MessageResponse {
    fn from(mut msg: Message) -> Self {
        msg.parse_meta();
        msg.parse_data();
        let data = msg.get_data();
        let meta = msg.get_meta();
        MessageResponse {
            id: msg.id,
            chat_id: msg.chat_id,
            channel_id: msg.channel_id,
            user_id: msg.user_id,
            user: None, // Will be populated by service layer
            reply_to_id: msg.reply_to_id,
            parent_id: msg.parent_id,
            content: msg.content,
            role: msg.role,
            model: msg.model,
            data,
            meta,
            created_at: msg.created_at,
            updated_at: msg.updated_at,
        }
    }
}

// Message Reaction structures
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageReaction {
    pub id: String,
    pub user_id: String,
    pub message_id: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct Reaction {
    pub name: String,
    pub user_ids: Vec<String>,
    pub count: usize,
}
