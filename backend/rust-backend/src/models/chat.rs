use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Chat {
    pub id: String,
    pub user_id: String,
    pub title: String,
    #[sqlx(json)]
    pub chat: JsonValue,
    pub folder_id: Option<String>,
    pub archived: bool,
    pub pinned: Option<bool>,
    pub share_id: Option<String>,
    #[sqlx(json)]
    pub meta: Option<JsonValue>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateChatRequest {
    pub id: String,
    pub title: Option<String>,
    pub chat: serde_json::Value,
    pub folder_id: Option<String>,
    pub archived: Option<bool>,
    pub pinned: Option<bool>,
    pub share_id: Option<String>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChatRequest {
    pub title: Option<String>,
    pub chat: Option<serde_json::Value>,
    pub folder_id: Option<String>,
    pub archived: Option<bool>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub chat: JsonValue,
    pub folder_id: Option<String>,
    pub archived: bool,
    pub pinned: Option<bool>,
    pub share_id: Option<String>,
    pub meta: JsonValue,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Chat> for ChatResponse {
    fn from(chat: Chat) -> Self {
        ChatResponse {
            id: chat.id,
            user_id: chat.user_id,
            title: chat.title,
            chat: chat.chat,
            folder_id: chat.folder_id,
            archived: chat.archived,
            pinned: chat.pinned,
            share_id: chat.share_id,
            meta: chat.meta.unwrap_or_else(|| serde_json::json!({})),
            created_at: chat.created_at,
            updated_at: chat.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ChatListResponse {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub created_at: i64,
}

impl From<Chat> for ChatListResponse {
    fn from(chat: Chat) -> Self {
        ChatListResponse {
            id: chat.id,
            title: chat.title,
            updated_at: chat.updated_at,
            created_at: chat.created_at,
        }
    }
}
