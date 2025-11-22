use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub user_id: String,
    #[serde(rename = "type")]
    pub channel_type: Option<String>,
    #[sqlx(skip)]
    #[serde(skip)]
    pub data: Option<serde_json::Value>,
    #[sqlx(default)]
    pub data_str: Option<String>,
    #[sqlx(skip)]
    #[serde(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    #[sqlx(skip)]
    #[serde(skip)]
    pub access_control: Option<serde_json::Value>,
    #[sqlx(default)]
    pub access_control_str: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[allow(dead_code)]
impl Channel {
    pub fn parse_data(&mut self) {
        if let Some(ref data_str) = self.data_str {
            self.data = serde_json::from_str(data_str).ok();
        }
    }

    pub fn parse_meta(&mut self) {
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
    }

    pub fn parse_access_control(&mut self) {
        if let Some(ref ac_str) = self.access_control_str {
            self.access_control = serde_json::from_str(ac_str).ok();
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateChannelRequest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub data: Option<serde_json::Value>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ChannelResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub user_id: String,
    pub created_at: i64,
}

impl From<Channel> for ChannelResponse {
    fn from(channel: Channel) -> Self {
        ChannelResponse {
            id: channel.id,
            name: channel.name,
            description: channel.description,
            user_id: channel.user_id,
            created_at: channel.created_at,
        }
    }
}

// Channel Member structures
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelMember {
    pub id: String,
    pub channel_id: String,
    pub user_id: String,
    pub created_at: i64,
}
