use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use sqlx::FromRow;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub username: Option<String>,
    pub role: String,
    pub profile_image_url: String,
    pub bio: Option<String>,
    pub gender: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    #[sqlx(json)]
    pub info: Option<JsonValue>,
    #[sqlx(json)]
    pub settings: Option<JsonValue>,
    pub api_key: Option<String>,
    pub oauth_sub: Option<String>,
    pub last_active_at: i64,
    pub updated_at: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub name: String,
    pub email: String,
    pub username: Option<String>,
    pub role: String,
    pub profile_image_url: String,
    pub bio: Option<String>,
    pub gender: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub info: Option<JsonValue>,
    pub settings: Option<JsonValue>,
    pub last_active_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        UserResponse {
            id: user.id,
            name: user.name,
            email: user.email,
            username: user.username,
            role: user.role,
            profile_image_url: user.profile_image_url,
            bio: user.bio,
            gender: user.gender,
            date_of_birth: user.date_of_birth,
            info: user.info,
            settings: user.settings,
            last_active_at: user.last_active_at,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

/// Lightweight user response for channel messages and other contexts
/// where only basic user info is needed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserNameResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_image_url: Option<String>,
}

impl From<User> for UserNameResponse {
    fn from(user: User) -> Self {
        UserNameResponse {
            id: user.id,
            name: user.name,
            email: Some(user.email),
            profile_image_url: Some(user.profile_image_url),
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[allow(dead_code)]
pub struct CreateUserRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8))]
    pub password: String,

    pub role: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[allow(dead_code)]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub bio: Option<String>,
    pub gender: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub profile_image_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRoleRequest {
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserSettings {
    pub ui: Option<serde_json::Value>,
}
