use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub user_id: String,
    pub data: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateTagRequest {
    pub name: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct TagResponse {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

impl From<Tag> for TagResponse {
    fn from(tag: Tag) -> Self {
        TagResponse {
            id: tag.id,
            name: tag.name,
            created_at: tag.created_at,
        }
    }
}
