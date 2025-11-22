use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Prompt {
    pub command: String,
    pub user_id: String,
    pub title: String,
    pub content: String,
    pub timestamp: i64,
    #[sqlx(skip)]
    pub access_control: Option<serde_json::Value>,
    #[sqlx(default)]
    pub access_control_str: Option<String>,
}

impl Prompt {
    pub fn parse_access_control(&mut self) {
        if let Some(ref ac_str) = self.access_control_str {
            self.access_control = serde_json::from_str(ac_str).ok();
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PromptForm {
    pub command: String,
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct PromptModel {
    pub command: String,
    pub user_id: String,
    pub title: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
}

impl From<Prompt> for PromptModel {
    fn from(mut prompt: Prompt) -> Self {
        prompt.parse_access_control();
        PromptModel {
            command: prompt.command,
            user_id: prompt.user_id,
            title: prompt.title,
            content: prompt.content,
            timestamp: prompt.timestamp,
            access_control: prompt.access_control,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PromptUserResponse {
    pub command: String,
    pub user_id: String,
    pub title: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<serde_json::Value>,
}

impl PromptUserResponse {
    pub fn from_prompt_and_user(prompt: Prompt, user: Option<serde_json::Value>) -> Self {
        let model = PromptModel::from(prompt);
        PromptUserResponse {
            command: model.command,
            user_id: model.user_id,
            title: model.title,
            content: model.content,
            timestamp: model.timestamp,
            access_control: model.access_control,
            user,
        }
    }
}
