use chrono::Utc;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::container::{ContainerManager, ContainerPool};
use crate::error::{SandboxError, SandboxResult};
use crate::executor::audit::AuditLogger;
use crate::models::{ExecuteRequest, ExecuteResponse, ExecutionContext, ExecutionStatus, Language};
use crate::security::{limits::ResourceLimits, validate_code};

pub struct ExecutionEngine {
    container_manager: Arc<ContainerManager>,
    container_pool: Option<Arc<ContainerPool>>,
    config: Config,
    audit_logger: Option<AuditLogger>,
}

impl ExecutionEngine {
    pub fn new(
        container_manager: Arc<ContainerManager>,
        container_pool: Option<Arc<ContainerPool>>,
        config: Config,
    ) -> Self {
        let audit_logger = if config.enable_audit_log {
            match AuditLogger::new(&config.audit_log_path) {
                Ok(logger) => Some(logger),
                Err(e) => {
                    error!("Failed to initialize audit logger: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            container_manager,
            container_pool,
            config,
            audit_logger,
        }
    }

    pub async fn execute(&self, request: ExecuteRequest) -> SandboxResult<ExecuteResponse> {
        let created_at = Utc::now();

        // Validate request
        self.validate_request(&request)?;

        // Create execution context
        let ctx =
            ExecutionContext::new(request.clone()).map_err(|e| SandboxError::InvalidInput(e))?;

        info!(
            "Starting execution {} for language {:?}",
            ctx.id, ctx.language
        );

        // Log to audit
        if let Some(ref logger) = self.audit_logger {
            logger.log_execution_start(&ctx).await;
        }

        let execution_id = ctx.id.to_string();

        // Execute with proper error handling
        let result = self.execute_internal(&ctx).await;

        // Build response
        let response = match result {
            Ok(exec_result) => {
                info!(
                    "Execution {} completed successfully in {}ms",
                    execution_id, exec_result.execution_time_ms
                );

                if let Some(ref logger) = self.audit_logger {
                    logger
                        .log_execution_complete(&ctx, &exec_result, true)
                        .await;
                }

                ExecuteResponse {
                    execution_id: execution_id.clone(),
                    status: if exec_result.exit_code == 0 {
                        ExecutionStatus::Success
                    } else {
                        ExecutionStatus::Failed
                    },
                    stdout: exec_result.stdout,
                    stderr: exec_result.stderr,
                    result: exec_result.result,
                    execution_time_ms: exec_result.execution_time_ms,
                    memory_used_mb: exec_result.memory_used_mb,
                    exit_code: Some(exec_result.exit_code),
                    error: None,
                    created_at,
                    completed_at: Some(Utc::now()),
                }
            }
            Err(e) => {
                error!("Execution {} failed: {}", execution_id, e);

                if let Some(ref logger) = self.audit_logger {
                    logger.log_execution_error(&ctx, &e).await;
                }

                let status = match e {
                    SandboxError::ExecutionTimeout => ExecutionStatus::Timeout,
                    _ => ExecutionStatus::Failed,
                };

                ExecuteResponse {
                    execution_id: execution_id.clone(),
                    status,
                    stdout: String::new(),
                    stderr: String::new(),
                    result: None,
                    execution_time_ms: 0,
                    memory_used_mb: None,
                    exit_code: None,
                    error: Some(e.to_string()),
                    created_at,
                    completed_at: Some(Utc::now()),
                }
            }
        };

        Ok(response)
    }

    async fn execute_internal(
        &self,
        ctx: &ExecutionContext,
    ) -> SandboxResult<crate::models::ExecutionResult> {
        // Use container pool if available (much faster!)
        if let Some(ref pool) = self.container_pool {
            info!("Using container pool for execution (fast path)");
            return pool.execute_with_pool(ctx).await;
        }

        // Fallback to old method if pool is not available
        info!("Using direct container creation (slow path)");
        let mut container_id: Option<String> = None;

        let result = async {
            // Create container
            let id = self
                .container_manager
                .create_execution_container(ctx)
                .await?;
            container_id = Some(id.clone());

            // Start container
            self.container_manager.start_container(&id).await?;

            // Execute code in container
            let exec_result = self
                .container_manager
                .execute_in_container(&id, ctx)
                .await?;

            Ok(exec_result)
        }
        .await;

        // Cleanup container (always, even on error)
        if let Some(id) = container_id {
            if !self.config.keep_containers {
                self.container_manager.cleanup_container(&id).await;
            } else {
                info!("Container {} kept for debugging", id);
            }
        }

        result
    }

    fn validate_request(&self, request: &ExecuteRequest) -> SandboxResult<()> {
        // Validate language support
        let language =
            Language::from_str(&request.language).map_err(|e| SandboxError::InvalidInput(e))?;

        // Check if language is enabled
        let enabled = match language {
            Language::Python => self.config.enable_python,
            Language::Javascript => self.config.enable_javascript,
            Language::Shell => self.config.enable_shell,
            Language::Rust => self.config.enable_rust,
        };

        if !enabled {
            return Err(SandboxError::LanguageNotSupported(format!(
                "{:?} is not enabled",
                language
            )));
        }

        // Validate code
        validate_code(&request.code, &language).map_err(|e| SandboxError::InvalidInput(e))?;

        // Validate resource limits
        let limits = ResourceLimits::new(self.config.max_memory_mb, self.config.max_execution_time);

        limits.validate_code_size(request.code.len())?;

        if let Some(timeout) = request.timeout {
            limits.validate_timeout(timeout)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_language() {
        assert!(Language::from_str("python").is_ok());
        assert!(Language::from_str("javascript").is_ok());
        assert!(Language::from_str("invalid").is_err());
    }
}
