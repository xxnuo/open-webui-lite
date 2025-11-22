use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Model {
    pub id: String,
    pub user_id: String,
    pub base_model_id: Option<String>,
    pub name: String,
    #[sqlx(json)]
    pub params: JsonValue,
    #[sqlx(json)]
    pub meta: Option<JsonValue>,
    #[sqlx(json)]
    pub access_control: Option<JsonValue>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateModelRequest {
    pub id: String,
    pub base_model_id: Option<String>,
    pub name: String,
    pub params: serde_json::Value,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelForm {
    pub id: String,
    #[serde(default)]
    pub base_model_id: Option<String>,
    pub name: String,
    #[serde(default = "default_params")]
    pub params: JsonValue,
    #[serde(default = "default_meta")]
    pub meta: JsonValue,
    #[serde(default)]
    pub access_control: Option<JsonValue>,
}

fn default_params() -> JsonValue {
    serde_json::json!({})
}

fn default_meta() -> JsonValue {
    serde_json::json!({})
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub id: String,
    pub user_id: String,
    pub base_model_id: Option<String>,
    pub name: String,
    pub params: JsonValue,
    pub meta: JsonValue,
    pub access_control: Option<JsonValue>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ModelUserResponse {
    pub id: String,
    pub user_id: String,
    pub base_model_id: Option<String>,
    pub name: String,
    pub params: JsonValue,
    pub meta: JsonValue,
    pub access_control: Option<JsonValue>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<serde_json::Value>,
}

impl From<Model> for ModelResponse {
    fn from(model: Model) -> Self {
        ModelResponse {
            id: model.id,
            user_id: model.user_id,
            base_model_id: model.base_model_id,
            name: model.name,
            params: model.params,
            meta: model.meta.unwrap_or_else(|| serde_json::json!({})),
            access_control: model.access_control,
            is_active: model.is_active,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

impl ModelUserResponse {
    pub fn from_model_and_user(model: Model, user: Option<serde_json::Value>) -> Self {
        ModelUserResponse {
            id: model.id,
            user_id: model.user_id.clone(),
            base_model_id: model.base_model_id,
            name: model.name,
            params: model.params,
            meta: model.meta.unwrap_or_else(|| serde_json::json!({})),
            access_control: model.access_control,
            is_active: model.is_active,
            created_at: model.created_at,
            updated_at: model.updated_at,
            user,
        }
    }
}
