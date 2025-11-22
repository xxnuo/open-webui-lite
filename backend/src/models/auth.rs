use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Auth {
    pub id: String,
    pub email: String,
    pub password: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SigninRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 1))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SignupRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8))]
    pub password: String,

    #[validate(length(min = 8))]
    pub password_confirmation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub token: String,
    pub token_type: String,
    pub expires_at: Option<i64>,
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub profile_image_url: String,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    #[serde(rename = "id")]
    pub sub: String, // User ID (Python JWT uses "id" field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>, // Expiration time (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>, // Issued at (optional)
}
