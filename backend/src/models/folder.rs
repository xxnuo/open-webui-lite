use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Folder {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub is_expanded: Option<bool>,
    pub items_str: Option<String>,
    pub meta_str: Option<String>,
    pub data_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[sqlx(skip)]
    pub items: Option<serde_json::Value>,
    #[sqlx(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(skip)]
    pub data: Option<serde_json::Value>,
}

#[allow(dead_code)]
impl Folder {
    pub fn parse_json_fields(&mut self) {
        if let Some(ref items_str) = self.items_str {
            self.items = serde_json::from_str(items_str).ok();
        }
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
        if let Some(ref data_str) = self.data_str {
            self.data = serde_json::from_str(data_str).ok();
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FolderForm {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct FolderUpdateForm {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct FolderParentIdForm {
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FolderIsExpandedForm {
    pub is_expanded: bool,
}

#[derive(Debug, Serialize)]
pub struct FolderMetadataResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FolderNameIdResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    pub parent_id: Option<String>, // Don't skip - frontend expects null, not undefined
    pub is_expanded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Folder> for FolderNameIdResponse {
    fn from(mut folder: Folder) -> Self {
        folder.parse_json_fields();
        FolderNameIdResponse {
            id: folder.id,
            name: folder.name,
            meta: folder.meta,
            parent_id: folder.parent_id,
            is_expanded: folder.is_expanded.unwrap_or(false),
            data: folder.data,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FolderModel {
    pub id: String,
    pub user_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub is_expanded: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Folder> for FolderModel {
    fn from(mut folder: Folder) -> Self {
        folder.parse_json_fields();
        FolderModel {
            id: folder.id,
            user_id: folder.user_id,
            name: folder.name,
            parent_id: folder.parent_id,
            items: folder.items,
            meta: folder.meta,
            data: folder.data,
            is_expanded: folder.is_expanded.unwrap_or(false),
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        }
    }
}
