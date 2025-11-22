use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct Function {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub type_: String, // renamed to type_ because 'type' is a reserved keyword
    pub content: String,
    #[sqlx(skip)]
    #[serde(skip)]
    pub meta: Option<serde_json::Value>,
    #[sqlx(default)]
    pub meta_str: Option<String>,
    #[sqlx(skip)]
    #[serde(skip)]
    pub valves: Option<serde_json::Value>,
    #[sqlx(default)]
    pub valves_str: Option<String>,
    pub is_active: bool,
    pub is_global: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[allow(dead_code)]
impl Function {
    pub fn parse_meta(&mut self) {
        if let Some(ref meta_str) = self.meta_str {
            self.meta = serde_json::from_str(meta_str).ok();
        }
    }

    pub fn parse_valves(&mut self) {
        if let Some(ref valves_str) = self.valves_str {
            self.valves = serde_json::from_str(valves_str).ok();
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateFunctionRequest {
    pub id: String,
    pub name: String,
    pub type_: String,
    pub content: String,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct FunctionResponse {
    pub id: String,
    pub name: String,
    pub type_: String,
    pub is_active: bool,
    pub created_at: i64,
}

impl From<Function> for FunctionResponse {
    fn from(func: Function) -> Self {
        FunctionResponse {
            id: func.id,
            name: func.name,
            type_: func.type_,
            is_active: func.is_active,
            created_at: func.created_at,
        }
    }
}
