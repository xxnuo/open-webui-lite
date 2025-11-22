use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Note {
    pub id: String,
    pub user_id: String,
    pub title: String,
    #[sqlx(skip)]
    pub data: Option<serde_json::Value>,
    #[sqlx(default)]
    pub data_str: Option<String>,
    #[sqlx(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    #[sqlx(skip)]
    pub access_control: Option<serde_json::Value>,
    #[sqlx(default)]
    pub access_control_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Note {
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
pub struct NoteForm {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct NoteUpdateForm {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct NoteModel {
    pub id: String,
    pub user_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Note> for NoteModel {
    fn from(mut note: Note) -> Self {
        note.parse_json_fields();
        NoteModel {
            id: note.id,
            user_id: note.user_id,
            title: note.title,
            data: note.data,
            meta: note.meta,
            access_control: note.access_control,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NoteUserResponse {
    pub id: String,
    pub user_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<serde_json::Value>,
}

impl NoteUserResponse {
    pub fn from_note_and_user(note: Note, user: Option<serde_json::Value>) -> Self {
        let model = NoteModel::from(note);

        // Ensure data has proper content structure for frontend
        let normalized_data = model.data.map(|mut data| {
            if let Some(obj) = data.as_object_mut() {
                // If content field doesn't exist or is incomplete, normalize it
                if !obj.contains_key("content") {
                    // Add default empty content structure
                    obj.insert(
                        "content".to_string(),
                        serde_json::json!({
                            "json": null,
                            "html": "",
                            "md": ""
                        }),
                    );
                } else if let Some(content) = obj.get_mut("content") {
                    // Ensure content has all required fields
                    if let Some(content_obj) = content.as_object_mut() {
                        if !content_obj.contains_key("json") {
                            content_obj.insert("json".to_string(), serde_json::Value::Null);
                        }
                        if !content_obj.contains_key("html") {
                            content_obj.insert(
                                "html".to_string(),
                                serde_json::Value::String("".to_string()),
                            );
                        }
                        if !content_obj.contains_key("md") {
                            content_obj.insert(
                                "md".to_string(),
                                serde_json::Value::String("".to_string()),
                            );
                        }
                    }
                }
            }
            data
        });

        NoteUserResponse {
            id: model.id,
            user_id: model.user_id,
            title: model.title,
            data: normalized_data,
            meta: model.meta,
            access_control: model.access_control,
            created_at: model.created_at,
            updated_at: model.updated_at,
            user,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NoteTitleIdResponse {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub created_at: i64,
}
