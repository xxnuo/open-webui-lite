use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct File {
    pub id: String,
    pub user_id: String,
    pub filename: String,
    pub path: Option<String>,
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
    pub hash: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl File {
    pub fn parse_json_fields(&mut self) {
        if let Some(ref data_str) = self.data_str {
            if let Ok(data) = serde_json::from_str(data_str) {
                self.data = Some(data);
            }
        }
        if let Some(ref meta_str) = self.meta_str {
            if let Ok(meta) = serde_json::from_str(meta_str) {
                self.meta = Some(meta);
            }
        }
        if let Some(ref ac_str) = self.access_control_str {
            if let Ok(ac) = serde_json::from_str(ac_str) {
                self.access_control = Some(ac);
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileResponse {
    pub id: String,
    pub user_id: String,
    pub filename: String,
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    pub meta: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<JsonValue>,
    pub hash: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<File> for FileResponse {
    fn from(mut file: File) -> Self {
        file.parse_json_fields();
        FileResponse {
            id: file.id,
            user_id: file.user_id,
            filename: file.filename,
            path: file.path,
            data: file.data,
            meta: file.meta,
            access_control: file.access_control,
            hash: file.hash,
            created_at: file.created_at,
            updated_at: file.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FileForm {
    pub id: String,
    pub filename: String,
    pub path: String,
    #[serde(default)]
    pub data: Option<JsonValue>,
    #[serde(default)]
    pub meta: Option<JsonValue>,
    #[serde(default)]
    pub access_control: Option<JsonValue>,
    pub hash: Option<String>,
}
