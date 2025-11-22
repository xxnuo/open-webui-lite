use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::HostConfig;
use bollard::service::ContainerCreateBody;
use bollard::Docker;
use bytes::Bytes;
use futures::StreamExt;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::config::Config as AppConfig;
use crate::error::{SandboxError, SandboxResult};
use crate::models::{ExecutionContext, ExecutionResult};
use crate::security::SecurityConfig;

pub struct ContainerManager {
    docker: Docker,
    config: AppConfig,
}

impl ContainerManager {
    pub async fn new(config: &AppConfig) -> SandboxResult<Self> {
        let docker = if config.docker_host.starts_with("unix://") {
            Docker::connect_with_unix(
                &config.docker_host.trim_start_matches("unix://"),
                120,
                bollard::API_DEFAULT_VERSION,
            )
        } else {
            Docker::connect_with_http(&config.docker_host, 120, bollard::API_DEFAULT_VERSION)
        }
        .map_err(|e| SandboxError::DockerConnectionFailed(e.to_string()))?;

        // Verify Docker connection
        match docker.ping().await {
            Ok(_) => info!("✓ Docker connection established"),
            Err(e) => {
                error!("✗ Docker connection failed: {}", e);
                return Err(SandboxError::DockerConnectionFailed(e.to_string()));
            }
        }

        Ok(Self {
            docker,
            config: config.clone(),
        })
    }

    pub async fn create_execution_container(
        &self,
        ctx: &ExecutionContext,
    ) -> SandboxResult<String> {
        let container_name = format!("sandbox-exec-{}", ctx.id);

        // Build security configuration
        let security_config = SecurityConfig::from_config(&self.config);
        let host_config = security_config.to_host_config();

        debug!(
            "Container readonly_rootfs setting: {:?}",
            host_config.readonly_rootfs
        );

        // Prepare environment variables
        let mut env_vars: Vec<String> = ctx
            .env_vars
            .iter()
            .map(|ev| format!("{}={}", ev.key, ev.value))
            .collect();

        // Add language-specific env vars
        env_vars.push("DEBIAN_FRONTEND=noninteractive".to_string());
        env_vars.push(format!("EXECUTION_TIMEOUT={}", ctx.timeout));

        // Build container config
        let mut container_config = ContainerCreateBody::default();
        container_config.image = Some(self.config.container_image.clone());
        container_config.hostname = Some(format!("sandbox-{}", &ctx.id.to_string()[..8]));
        container_config.user = Some("sandbox".to_string()); // Non-root user
        container_config.working_dir = Some("/workspace".to_string());
        container_config.env = Some(env_vars);
        container_config.attach_stdout = Some(true);
        container_config.attach_stderr = Some(true);
        container_config.tty = Some(false);
        container_config.open_stdin = Some(false);
        container_config.stop_timeout = Some(ctx.timeout as i64 + 5);
        container_config.host_config = Some(host_config);

        // Override the default CMD to keep the container running
        // This allows us to exec into it to write files and run code
        container_config.cmd = Some(vec!["sleep".to_string(), format!("{}", ctx.timeout + 10)]);

        debug!("Creating container: {}", container_name);

        let response = self
            .docker
            .create_container(
                Some(bollard::container::CreateContainerOptions {
                    name: container_name.clone(),
                    platform: None,
                }),
                container_config,
            )
            .await
            .map_err(|e| SandboxError::ContainerCreationFailed(e.to_string()))?;

        for warning in response.warnings {
            warn!("Container creation warning: {}", warning);
        }

        info!("✓ Container created: {}", response.id);
        Ok(response.id)
    }

    pub async fn start_container(&self, container_id: &str) -> SandboxResult<()> {
        debug!("Starting container: {}", container_id);

        self.docker
            .start_container(
                container_id,
                None::<bollard::container::StartContainerOptions<String>>,
            )
            .await
            .map_err(|e| SandboxError::ContainerStartFailed(e.to_string()))?;

        info!("✓ Container started: {}", container_id);
        Ok(())
    }

    pub async fn execute_in_container(
        &self,
        container_id: &str,
        ctx: &ExecutionContext,
    ) -> SandboxResult<ExecutionResult> {
        use std::time::Instant;
        let start_time = Instant::now();

        // Create a temporary script file with the code
        let script_content = self.prepare_execution_script(ctx)?;

        // Copy code to container
        self.copy_to_container(container_id, &script_content, ctx)
            .await?;

        // Execute the code
        let command = self.build_execution_command(ctx);

        debug!("Executing command in container: {:?}", command);

        let exec_config = bollard::exec::CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(command),
            user: Some("sandbox".to_string()),
            working_dir: Some("/workspace".to_string()),
            ..Default::default()
        };

        let exec_result = self
            .docker
            .create_exec(container_id, exec_config)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        // Start execution with timeout
        let timeout_duration = Duration::from_secs(ctx.timeout);

        let output = timeout(timeout_duration, async {
            let start_result = self
                .docker
                .start_exec(&exec_result.id, None::<bollard::exec::StartExecOptions>)
                .await
                .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

            let mut stdout = String::new();
            let mut stderr = String::new();

            if let bollard::exec::StartExecResults::Attached { mut output, .. } = start_result {
                while let Some(msg) = output.next().await {
                    match msg {
                        Ok(bollard::container::LogOutput::StdOut { message }) => {
                            stdout.push_str(&String::from_utf8_lossy(&message));
                        }
                        Ok(bollard::container::LogOutput::StdErr { message }) => {
                            stderr.push_str(&String::from_utf8_lossy(&message));
                        }
                        Err(e) => {
                            error!("Error reading output: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }

            Ok::<(String, String), SandboxError>((stdout, stderr))
        })
        .await;

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        let (stdout, stderr, exit_code) = match output {
            Ok(Ok((stdout, stderr))) => {
                // Get exit code
                let inspect = self.docker.inspect_exec(&exec_result.id).await.ok();
                let exit_code = inspect.and_then(|i| i.exit_code).unwrap_or(0);
                (stdout, stderr, exit_code as i32)
            }
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {
                // Timeout occurred
                warn!("Execution timeout for container: {}", container_id);
                return Err(SandboxError::ExecutionTimeout);
            }
        };

        // Get memory usage stats
        let memory_used_mb = self.get_memory_usage(container_id).await;

        Ok(ExecutionResult {
            stdout: stdout.trim().to_string(),
            stderr: stderr.trim().to_string(),
            result: None,
            exit_code,
            execution_time_ms,
            memory_used_mb,
        })
    }

    fn prepare_execution_script(&self, ctx: &ExecutionContext) -> SandboxResult<String> {
        Ok(ctx.code.clone())
    }

    async fn copy_to_container(
        &self,
        container_id: &str,
        content: &str,
        ctx: &ExecutionContext,
    ) -> SandboxResult<()> {
        let filename = format!("script.{}", ctx.language.file_extension());
        let filepath = format!("/workspace/{}", filename);

        debug!("Copying file {} to container {}", filepath, container_id);

        // Use exec to write the file content instead of upload_to_container
        // This avoids issues with read-only rootfs restrictions on the Docker API
        // Use 'tee' instead of shell redirection to avoid permission issues with cap-drop
        let exec_config = bollard::exec::CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(vec!["tee".to_string(), filepath]),
            user: Some("sandbox".to_string()),
            working_dir: Some("/workspace".to_string()),
            attach_stdin: Some(true),
            ..Default::default()
        };

        let exec_result = self
            .docker
            .create_exec(container_id, exec_config)
            .await
            .map_err(|e| {
                SandboxError::InternalError(format!("Failed to create exec for file write: {}", e))
            })?;

        // Start exec and write content to stdin
        let start_config = bollard::exec::StartExecOptions {
            detach: false,
            ..Default::default()
        };

        match self
            .docker
            .start_exec(&exec_result.id, Some(start_config))
            .await
        {
            Ok(bollard::exec::StartExecResults::Attached {
                mut output,
                mut input,
            }) => {
                use tokio::io::AsyncWriteExt;
                use tokio::time::{timeout, Duration};

                // Write the content to stdin
                input.write_all(content.as_bytes()).await.map_err(|e| {
                    SandboxError::InternalError(format!("Failed to write content: {}", e))
                })?;

                // Close stdin to signal completion
                drop(input);

                // Wait briefly for the exec to complete and check for errors
                // Use a short timeout to avoid blocking forever
                let mut has_error = false;
                let mut error_msg = String::new();

                let result = timeout(Duration::from_secs(2), async {
                    while let Some(msg) = output.next().await {
                        match msg {
                            Ok(bollard::container::LogOutput::StdErr { message }) => {
                                let stderr = String::from_utf8_lossy(&message);
                                if !stderr.trim().is_empty() {
                                    has_error = true;
                                    error_msg.push_str(&stderr);
                                    warn!("File write stderr: {}", stderr);
                                }
                            }
                            Ok(bollard::container::LogOutput::StdOut { .. }) => {
                                // tee writes to stdout, we can ignore it
                            }
                            Err(e) => {
                                return Err(SandboxError::InternalError(format!(
                                    "Error during file write: {}",
                                    e
                                )));
                            }
                            _ => {}
                        }
                    }
                    Ok::<(), SandboxError>(())
                })
                .await;

                // If timeout, that's okay - the file was probably written successfully
                match result {
                    Ok(Ok(())) => {
                        if has_error {
                            return Err(SandboxError::InternalError(format!(
                                "Failed to write file: {}",
                                error_msg
                            )));
                        }
                    }
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        // Timeout - assume success if no error so far
                        if has_error {
                            return Err(SandboxError::InternalError(format!(
                                "Failed to write file: {}",
                                error_msg
                            )));
                        }
                        debug!("File write timed out waiting for output, assuming success");
                    }
                }

                Ok(())
            }
            _ => Err(SandboxError::InternalError(
                "Failed to attach to exec".to_string(),
            )),
        }
    }

    fn build_execution_command(&self, ctx: &ExecutionContext) -> Vec<String> {
        let filename = format!("script.{}", ctx.language.file_extension());

        match ctx.language {
            crate::models::Language::Python => {
                vec!["python3".to_string(), filename]
            }
            crate::models::Language::Javascript => {
                vec!["node".to_string(), filename]
            }
            crate::models::Language::Shell => {
                vec!["/bin/sh".to_string(), filename]
            }
            crate::models::Language::Rust => {
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!("rustc {} -o /tmp/program && /tmp/program", filename),
                ]
            }
        }
    }

    async fn get_memory_usage(&self, container_id: &str) -> Option<f64> {
        match self
            .docker
            .stats(
                container_id,
                Some(bollard::container::StatsOptions {
                    stream: false,
                    one_shot: true,
                }),
            )
            .next()
            .await
        {
            Some(Ok(stats)) => {
                // Get memory usage from memory_stats
                if let Some(mem_stats) = stats.memory_stats {
                    if let Some(usage) = mem_stats.usage {
                        return Some(usage as f64 / 1024.0 / 1024.0); // Convert to MB
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub async fn stop_container(&self, container_id: &str) -> SandboxResult<()> {
        debug!("Stopping container: {}", container_id);

        // Try graceful stop first
        if let Err(e) = self
            .docker
            .stop_container(
                container_id,
                None::<bollard::container::StopContainerOptions>,
            )
            .await
        {
            warn!("Failed to stop container gracefully: {}", e);
            // Try to kill it
            if let Err(e) = self
                .docker
                .kill_container(
                    container_id,
                    None::<bollard::container::KillContainerOptions<String>>,
                )
                .await
            {
                error!("Failed to kill container: {}", e);
            }
        }

        info!("✓ Container stopped: {}", container_id);
        Ok(())
    }

    pub async fn remove_container(&self, container_id: &str) -> SandboxResult<()> {
        debug!("Removing container: {}", container_id);

        let options = bollard::container::RemoveContainerOptions {
            force: true,
            v: true,
            ..Default::default()
        };

        self.docker
            .remove_container(container_id, Some(options))
            .await
            .map_err(|e| {
                warn!("Failed to remove container {}: {}", container_id, e);
                SandboxError::ContainerCleanupFailed(e.to_string())
            })?;

        info!("✓ Container removed: {}", container_id);
        Ok(())
    }

    pub async fn cleanup_container(&self, container_id: &str) {
        let _ = self.stop_container(container_id).await;
        let _ = self.remove_container(container_id).await;
    }

    pub async fn check_docker_health(&self) -> SandboxResult<String> {
        match self.docker.version().await {
            Ok(version) => Ok(format!("Docker {}", version.version.unwrap_or_default())),
            Err(e) => Err(SandboxError::DockerConnectionFailed(e.to_string())),
        }
    }

    /// Check if a container exists and is running
    pub async fn check_container_status(&self, container_id: &str) -> SandboxResult<bool> {
        use bollard::query_parameters::InspectContainerOptions;

        match self
            .docker
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
        {
            Ok(info) => {
                // Check if container is running
                Ok(info.state.and_then(|s| s.running).unwrap_or(false))
            }
            Err(_) => Ok(false), // Container doesn't exist
        }
    }

    /// Execute a simple command without capturing all output (for cleanup operations)
    pub async fn exec_simple_command(
        &self,
        container_id: &str,
        command: Vec<String>,
    ) -> SandboxResult<()> {
        debug!(
            "Executing simple command in container {}: {:?}",
            container_id, command
        );

        let exec_config = CreateExecOptions {
            attach_stdout: Some(false),
            attach_stderr: Some(false),
            cmd: Some(command),
            user: Some("sandbox".to_string()),
            working_dir: Some("/workspace".to_string()),
            ..Default::default()
        };

        let exec_result = self
            .docker
            .create_exec(container_id, exec_config)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        self.docker
            .start_exec(&exec_result.id, None::<StartExecOptions>)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }
}
