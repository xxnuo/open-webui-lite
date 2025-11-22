use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Feedback {
    pub id: String,
    pub user_id: String,
    pub version: i64,
    #[serde(rename = "type")]
    pub feedback_type: String,
    #[sqlx(skip)]
    pub data: Option<serde_json::Value>,
    #[sqlx(default)]
    pub data_str: Option<String>,
    #[sqlx(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    #[sqlx(skip)]
    pub snapshot: Option<serde_json::Value>,
    #[sqlx(default)]
    pub snapshot_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Feedback {
    pub fn parse_json_fields(&mut self) {
        if let Some(ref data_str) = self.data_str {
            self.data = serde_json::from_str(data_str).ok();
        }
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
        if let Some(ref snapshot_str) = self.snapshot_str {
            self.snapshot = serde_json::from_str(snapshot_str).ok();
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedbackModel {
    pub id: String,
    pub user_id: String,
    pub version: i64,
    #[serde(rename = "type")]
    pub feedback_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Feedback> for FeedbackModel {
    fn from(mut feedback: Feedback) -> Self {
        feedback.parse_json_fields();
        FeedbackModel {
            id: feedback.id,
            user_id: feedback.user_id,
            version: feedback.version,
            feedback_type: feedback.feedback_type,
            data: feedback.data,
            meta: feedback.meta,
            snapshot: feedback.snapshot,
            created_at: feedback.created_at,
            updated_at: feedback.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FeedbackForm {
    #[serde(rename = "type")]
    pub feedback_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct FeedbackResponse {
    pub id: String,
    pub user_id: String,
    pub version: i64,
    #[serde(rename = "type")]
    pub feedback_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}
