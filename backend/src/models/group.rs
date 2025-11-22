use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Group {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: String,
    #[sqlx(skip)]
    pub data: Option<serde_json::Value>,
    #[sqlx(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(skip)]
    pub permissions: Option<serde_json::Value>,
    #[sqlx(skip)]
    pub user_ids: Vec<String>,
    #[sqlx(default)]
    pub data_str: Option<String>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    #[sqlx(default)]
    pub permissions_str: Option<String>,
    #[sqlx(default)]
    pub user_ids_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Group {
    pub fn parse_json_fields(&mut self) {
        if let Some(ref data_str) = self.data_str {
            self.data = serde_json::from_str(data_str).ok();
        }
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
        if let Some(ref perms_str) = self.permissions_str {
            self.permissions = serde_json::from_str(perms_str).ok();
        }
        // Always parse user_ids_str, defaulting to empty array if None or parse fails
        self.user_ids = self
            .user_ids_str
            .as_ref()
            .and_then(|ids_str| serde_json::from_str(ids_str).ok())
            .unwrap_or_default();
    }
}

#[derive(Debug, Deserialize)]
pub struct GroupForm {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UserIdsForm {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct GroupUpdateForm {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    pub user_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Group> for GroupResponse {
    fn from(mut group: Group) -> Self {
        group.parse_json_fields();
        GroupResponse {
            id: group.id,
            user_id: group.user_id,
            name: group.name,
            description: group.description,
            permissions: group.permissions,
            data: group.data,
            meta: group.meta,
            user_ids: group.user_ids,
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}
