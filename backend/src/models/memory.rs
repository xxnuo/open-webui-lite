use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Memory {
    pub id: String,
    pub user_id: String,
    pub content: String,
    #[sqlx(json)]
    pub meta: Option<JsonValue>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateMemoryRequest {
    pub content: String,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct MemoryResponse {
    pub id: String,
    pub user_id: String,
    pub content: String,
    pub meta: Option<JsonValue>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Memory> for MemoryResponse {
    fn from(memory: Memory) -> Self {
        MemoryResponse {
            id: memory.id,
            user_id: memory.user_id,
            content: memory.content,
            meta: memory.meta,
            created_at: memory.created_at,
            updated_at: memory.updated_at,
        }
    }
}
