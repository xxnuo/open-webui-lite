use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Python,
    Javascript,
    Shell,
    Rust,
}

impl Language {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Language::Python),
            "javascript" | "js" | "node" => Ok(Language::Javascript),
            "shell" | "sh" | "bash" => Ok(Language::Shell),
            "rust" | "rs" => Ok(Language::Rust),
            _ => Err(format!("Unsupported language: {}", s)),
        }
    }

    pub fn executor(&self) -> &str {
        match self {
            Language::Python => "python3",
            Language::Javascript => "node",
            Language::Shell => "/bin/sh",
            Language::Rust => "rustc",
        }
    }

    pub fn file_extension(&self) -> &str {
        match self {
            Language::Python => "py",
            Language::Javascript => "js",
            Language::Shell => "sh",
            Language::Rust => "rs",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExecuteRequest {
    #[validate(length(min = 1, max = 100000))]
    pub code: String,

    pub language: String,

    #[validate(range(min = 1, max = 300))]
    pub timeout: Option<u64>, // seconds, max 5 minutes

    pub env_vars: Option<Vec<EnvVar>>,

    pub files: Option<Vec<FileInput>>,

    // User/request identification
    pub user_id: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInput {
    pub name: String,
    pub content: String, // base64 encoded for binary files
    pub is_binary: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub execution_time_ms: u64,
    pub memory_used_mb: Option<f64>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Pending,
    Running,
    Success,
    Failed,
    Timeout,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub id: Uuid,
    pub language: Language,
    pub code: String,
    pub timeout: u64,
    pub env_vars: Vec<EnvVar>,
    pub files: Vec<FileInput>,
    pub user_id: Option<String>,
    pub request_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ExecutionContext {
    pub fn new(req: ExecuteRequest) -> Result<Self, String> {
        let language = Language::from_str(&req.language)?;

        Ok(Self {
            id: Uuid::new_v4(),
            language,
            code: req.code,
            timeout: req.timeout.unwrap_or(60),
            env_vars: req.env_vars.unwrap_or_default(),
            files: req.files.unwrap_or_default(),
            user_id: req.user_id,
            request_id: req.request_id,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub exit_code: i32,
    pub execution_time_ms: u64,
    pub memory_used_mb: Option<f64>,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
            result: None,
            exit_code: 0,
            execution_time_ms: 0,
            memory_used_mb: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub active_executions: usize,
    pub total_executions: u64,
    pub docker_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub max_execution_time: u64,
    pub max_memory_mb: u64,
    pub max_cpu_quota: u64,
    pub supported_languages: Vec<String>,
    pub rate_limit_per_minute: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub execution_id: Option<String>,
}
