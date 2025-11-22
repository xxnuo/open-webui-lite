use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Knowledge {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    pub data: Option<JsonValue>,
    #[sqlx(default)]
    pub data_str: Option<String>,
    #[sqlx(skip)]
    pub meta: Option<JsonValue>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    #[sqlx(skip)]
    pub access_control: Option<JsonValue>,
    #[sqlx(default)]
    pub access_control_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Knowledge {
    pub fn parse_json_fields(&mut self) {
        if let Some(ref data_str) = self.data_str {
            self.data = serde_json::from_str(data_str).ok();
        }
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
        if let Some(ref ac_str) = self.access_control_str {
            self.access_control = serde_json::from_str(ac_str).ok();
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateKnowledgeRequest {
    pub name: String,
    pub description: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub data: Option<JsonValue>,
    pub meta: Option<JsonValue>,
    pub access_control: Option<JsonValue>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeUserResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub data: Option<JsonValue>,
    pub meta: Option<JsonValue>,
    pub access_control: Option<JsonValue>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeFilesResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub data: Option<JsonValue>,
    pub meta: Option<JsonValue>,
    pub access_control: Option<JsonValue>,
    pub created_at: i64,
    pub updated_at: i64,
    pub files: Vec<serde_json::Value>,
}

impl From<Knowledge> for KnowledgeResponse {
    fn from(knowledge: Knowledge) -> Self {
        KnowledgeResponse {
            id: knowledge.id,
            user_id: knowledge.user_id,
            name: knowledge.name,
            description: knowledge.description,
            data: knowledge.data,
            meta: knowledge.meta,
            access_control: knowledge.access_control,
            created_at: knowledge.created_at,
            updated_at: knowledge.updated_at,
        }
    }
}

impl KnowledgeUserResponse {
    pub fn from_knowledge_and_user(
        knowledge: Knowledge,
        user: Option<serde_json::Value>,
        files: Option<Vec<serde_json::Value>>,
    ) -> Self {
        KnowledgeUserResponse {
            id: knowledge.id,
            user_id: knowledge.user_id,
            name: knowledge.name,
            description: knowledge.description,
            data: knowledge.data,
            meta: knowledge.meta,
            access_control: knowledge.access_control,
            created_at: knowledge.created_at,
            updated_at: knowledge.updated_at,
            user,
            files,
        }
    }
}

impl KnowledgeFilesResponse {
    pub fn from_knowledge_and_files(knowledge: Knowledge, files: Vec<serde_json::Value>) -> Self {
        KnowledgeFilesResponse {
            id: knowledge.id,
            user_id: knowledge.user_id,
            name: knowledge.name,
            description: knowledge.description,
            data: knowledge.data,
            meta: knowledge.meta,
            access_control: knowledge.access_control,
            created_at: knowledge.created_at,
            updated_at: knowledge.updated_at,
            files,
        }
    }
}
