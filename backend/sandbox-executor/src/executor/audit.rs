use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use tokio::sync::Mutex;
use tracing::error;

use crate::error::SandboxError;
use crate::models::{ExecutionContext, ExecutionResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: String,
    pub execution_id: String,
    pub event_type: AuditEventType,
    pub user_id: Option<String>,
    pub request_id: Option<String>,
    pub language: String,
    pub code_length: usize,
    pub execution_time_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    ExecutionStart,
    ExecutionComplete,
    ExecutionError,
    ExecutionTimeout,
}

pub struct AuditLogger {
    log_file: Mutex<std::fs::File>,
}

impl AuditLogger {
    pub fn new(log_path: &str) -> Result<Self, std::io::Error> {
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(log_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;

        Ok(Self {
            log_file: Mutex::new(file),
        })
    }

    pub async fn log_execution_start(&self, ctx: &ExecutionContext) {
        let entry = AuditLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            execution_id: ctx.id.to_string(),
            event_type: AuditEventType::ExecutionStart,
            user_id: ctx.user_id.clone(),
            request_id: ctx.request_id.clone(),
            language: format!("{:?}", ctx.language),
            code_length: ctx.code.len(),
            execution_time_ms: None,
            exit_code: None,
            success: true,
            error: None,
        };

        self.write_log_entry(entry).await;
    }

    pub async fn log_execution_complete(
        &self,
        ctx: &ExecutionContext,
        result: &ExecutionResult,
        success: bool,
    ) {
        let entry = AuditLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            execution_id: ctx.id.to_string(),
            event_type: AuditEventType::ExecutionComplete,
            user_id: ctx.user_id.clone(),
            request_id: ctx.request_id.clone(),
            language: format!("{:?}", ctx.language),
            code_length: ctx.code.len(),
            execution_time_ms: Some(result.execution_time_ms),
            exit_code: Some(result.exit_code),
            success,
            error: None,
        };

        self.write_log_entry(entry).await;
    }

    pub async fn log_execution_error(&self, ctx: &ExecutionContext, error: &SandboxError) {
        let event_type = match error {
            SandboxError::ExecutionTimeout => AuditEventType::ExecutionTimeout,
            _ => AuditEventType::ExecutionError,
        };

        let entry = AuditLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            execution_id: ctx.id.to_string(),
            event_type,
            user_id: ctx.user_id.clone(),
            request_id: ctx.request_id.clone(),
            language: format!("{:?}", ctx.language),
            code_length: ctx.code.len(),
            execution_time_ms: None,
            exit_code: None,
            success: false,
            error: Some(error.to_string()),
        };

        self.write_log_entry(entry).await;
    }

    async fn write_log_entry(&self, entry: AuditLogEntry) {
        if let Ok(json) = serde_json::to_string(&entry) {
            let mut file = self.log_file.lock().await;
            if let Err(e) = writeln!(file, "{}", json) {
                error!("Failed to write audit log: {}", e);
            }
            // Flush to ensure logs are written immediately
            let _ = file.flush();
        }
    }
}
