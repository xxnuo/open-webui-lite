use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tool {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub content: String,
    #[sqlx(skip)]
    #[serde(skip)]
    pub specs: serde_json::Value,
    #[sqlx(default)]
    pub specs_str: String,
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
    #[sqlx(skip)]
    #[serde(skip)]
    pub valves: Option<serde_json::Value>,
    #[sqlx(default)]
    pub valves_str: Option<String>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateToolRequest {
    pub id: String,
    pub name: String,
    pub content: String,
    pub specs: serde_json::Value,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ToolResponse {
    pub id: String,
    pub name: String,
    pub specs: serde_json::Value,
    pub is_active: bool,
    pub created_at: i64,
}

#[allow(dead_code)]
impl Tool {
    /// Parse specs from JSON string
    pub fn parse_specs(&mut self) {
        self.specs = serde_json::from_str(&self.specs_str)
            .unwrap_or(serde_json::Value::Object(Default::default()));
    }

    /// Parse meta from JSON string
    pub fn parse_meta(&mut self) {
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
    }

    /// Parse access_control from JSON string
    pub fn parse_access_control(&mut self) {
        if let Some(ref ac_str) = self.access_control_str {
            self.access_control = serde_json::from_str(ac_str).ok();
        }
    }

    /// Parse valves from JSON string
    pub fn parse_valves(&mut self) {
        if let Some(ref valves_str) = self.valves_str {
            self.valves = serde_json::from_str(valves_str).ok();
        }
    }

    /// Get specs as JSON Value
    pub fn get_specs(&self) -> serde_json::Value {
        self.specs.clone()
    }

    /// Get meta as JSON Value
    pub fn get_meta(&self) -> Option<serde_json::Value> {
        self.meta.clone().or_else(|| {
            self.meta_str
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
        })
    }

    /// Get access_control as JSON Value
    pub fn get_access_control(&self) -> Option<serde_json::Value> {
        self.access_control.clone().or_else(|| {
            self.access_control_str
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ToolUserResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub meta: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control: Option<serde_json::Value>,
    pub updated_at: i64,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_user_valves: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<serde_json::Value>,
}

impl ToolUserResponse {
    pub fn from_tool_and_user(
        tool: Tool,
        user: Option<serde_json::Value>,
        has_user_valves: Option<bool>,
    ) -> Self {
        let meta = tool
            .get_meta()
            .unwrap_or_else(|| serde_json::json!({"description": ""}));
        let access_control = tool.get_access_control();
        ToolUserResponse {
            id: tool.id.clone(),
            user_id: tool.user_id.clone(),
            name: tool.name.clone(),
            meta,
            access_control,
            updated_at: tool.updated_at,
            created_at: tool.created_at,
            has_user_valves,
            user,
        }
    }
}

impl From<Tool> for ToolResponse {
    fn from(mut tool: Tool) -> Self {
        tool.parse_specs();
        let specs = tool.get_specs();
        ToolResponse {
            id: tool.id,
            name: tool.name,
            specs,
            is_active: tool.is_active,
            created_at: tool.created_at,
        }
    }
}
