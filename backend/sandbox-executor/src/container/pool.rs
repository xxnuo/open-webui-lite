use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::container::ContainerManager;
use crate::error::{SandboxError, SandboxResult};
use crate::models::{ExecutionContext, ExecutionResult, Language};

/// A container in the pool with metadata
#[derive(Debug, Clone)]
pub struct PooledContainer {
    pub id: String,
    pub language: Language,
    pub created_at: Instant,
    pub last_used: Instant,
    pub execution_count: u32,
}

/// Container pool for reusing containers across executions
pub struct ContainerPool {
    manager: Arc<ContainerManager>,
    config: Arc<Config>,
    pools: Arc<Mutex<PoolsByLanguage>>,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug)]
struct PoolsByLanguage {
    python: VecDeque<PooledContainer>,
    javascript: VecDeque<PooledContainer>,
    shell: VecDeque<PooledContainer>,
    rust: VecDeque<PooledContainer>,
}

impl PoolsByLanguage {
    fn new() -> Self {
        Self {
            python: VecDeque::new(),
            javascript: VecDeque::new(),
            shell: VecDeque::new(),
            rust: VecDeque::new(),
        }
    }

    fn get_pool_mut(&mut self, language: &Language) -> &mut VecDeque<PooledContainer> {
        match language {
            Language::Python => &mut self.python,
            Language::Javascript => &mut self.javascript,
            Language::Shell => &mut self.shell,
            Language::Rust => &mut self.rust,
        }
    }

    fn total_size(&self) -> usize {
        self.python.len() + self.javascript.len() + self.shell.len() + self.rust.len()
    }
}

impl ContainerPool {
    pub fn new(manager: Arc<ContainerManager>, config: Arc<Config>) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executions));

        Self {
            manager,
            config,
            pools: Arc::new(Mutex::new(PoolsByLanguage::new())),
            semaphore,
        }
    }

    /// Initialize the pool with warm containers
    pub async fn initialize(&self) -> SandboxResult<()> {
        if !self.config.enable_container_pool {
            info!("Container pool is disabled");
            return Ok(());
        }

        info!(
            "Initializing container pool (size: {} per language)...",
            self.config.pool_size_per_language
        );

        let languages = vec![
            (Language::Python, self.config.enable_python),
            (Language::Javascript, self.config.enable_javascript),
            (Language::Shell, self.config.enable_shell),
            (Language::Rust, self.config.enable_rust),
        ];

        for (language, enabled) in languages {
            if enabled {
                for _ in 0..self.config.pool_size_per_language {
                    if let Err(e) = self.create_pooled_container(&language).await {
                        warn!("Failed to pre-create container for {:?}: {}", language, e);
                    }
                }
            }
        }

        let pools = self.pools.lock().await;
        info!(
            "Container pool initialized: {} containers ready",
            pools.total_size()
        );

        Ok(())
    }

    /// Execute code using a pooled container
    pub async fn execute_with_pool(
        &self,
        ctx: &ExecutionContext,
    ) -> SandboxResult<ExecutionResult> {
        // Acquire semaphore to limit concurrent executions
        let _permit =
            self.semaphore.acquire().await.map_err(|e| {
                SandboxError::InternalError(format!("Failed to acquire permit: {}", e))
            })?;

        // Try to get a container from the pool
        let container = self.acquire_container(&ctx.language).await?;

        debug!(
            "Using pooled container {} (age: {}s, uses: {})",
            container.id,
            container.created_at.elapsed().as_secs(),
            container.execution_count
        );

        // Execute the code
        let result = self.manager.execute_in_container(&container.id, ctx).await;

        // Handle result and return container to pool
        match result {
            Ok(exec_result) => {
                // Clean the container state before returning to pool
                if let Err(e) = self.clean_container(&container.id).await {
                    warn!("Failed to clean container {}: {}", container.id, e);
                    // Don't return to pool, it will be cleaned up
                    self.manager.cleanup_container(&container.id).await;
                } else {
                    // Return to pool with updated metadata
                    self.return_container(container).await;
                }

                Ok(exec_result)
            }
            Err(e) => {
                // On error, don't return to pool - remove it
                warn!("Execution failed in container {}: {}", container.id, e);
                self.manager.cleanup_container(&container.id).await;
                Err(e)
            }
        }
    }

    /// Acquire a container from the pool or create a new one
    async fn acquire_container(&self, language: &Language) -> SandboxResult<PooledContainer> {
        let mut pools = self.pools.lock().await;
        let pool = pools.get_pool_mut(language);

        // Try to get from pool
        while let Some(mut container) = pool.pop_front() {
            // Check if container is still valid
            if self.is_container_valid(&container).await {
                // Update metadata
                container.last_used = Instant::now();
                container.execution_count += 1;
                return Ok(container);
            } else {
                // Invalid container, clean it up
                debug!("Container {} is no longer valid, removing", container.id);
                self.manager.cleanup_container(&container.id).await;
            }
        }

        // No valid container in pool, create a new one
        drop(pools); // Release lock before creating
        self.create_pooled_container(language).await
    }

    /// Create a new container for the pool
    async fn create_pooled_container(&self, language: &Language) -> SandboxResult<PooledContainer> {
        use chrono::Utc;

        // Create a dummy execution context just for container creation
        let ctx = ExecutionContext {
            id: Uuid::new_v4(),
            code: "# Pool initialization".to_string(),
            language: language.clone(),
            timeout: self.config.max_execution_time,
            env_vars: vec![],
            files: vec![],
            user_id: None,
            request_id: None,
            created_at: Utc::now(),
        };

        let container_id = self.manager.create_execution_container(&ctx).await?;
        self.manager.start_container(&container_id).await?;

        let now = Instant::now();
        let container = PooledContainer {
            id: container_id.clone(),
            language: language.clone(),
            created_at: now,
            last_used: now,
            execution_count: 0,
        };

        debug!(
            "Created new pooled container {} for {:?}",
            container_id, language
        );

        Ok(container)
    }

    /// Check if a container is still valid and usable
    async fn is_container_valid(&self, container: &PooledContainer) -> bool {
        let max_age = Duration::from_secs(self.config.pool_max_container_age_seconds);
        let max_reuse = self.config.pool_max_container_reuse;

        // Check age
        if container.created_at.elapsed() > max_age {
            debug!(
                "Container {} exceeded max age ({:?})",
                container.id, max_age
            );
            return false;
        }

        // Check reuse count
        if container.execution_count >= max_reuse {
            debug!(
                "Container {} exceeded max reuse count ({})",
                container.id, max_reuse
            );
            return false;
        }

        // Check if container still exists and is running
        // This is a quick check via Docker API
        match self.manager.check_container_status(&container.id).await {
            Ok(true) => true,
            Ok(false) => {
                debug!("Container {} is not running", container.id);
                false
            }
            Err(e) => {
                warn!("Failed to check container {} status: {}", container.id, e);
                false
            }
        }
    }

    /// Clean container state after execution
    async fn clean_container(&self, container_id: &str) -> SandboxResult<()> {
        // Remove all files from /workspace
        let cleanup_script = "find /workspace -mindepth 1 -delete 2>/dev/null || true";

        self.manager
            .exec_simple_command(
                container_id,
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    cleanup_script.to_string(),
                ],
            )
            .await?;

        debug!("Cleaned container {}", container_id);
        Ok(())
    }

    /// Return container to the pool
    async fn return_container(&self, container: PooledContainer) {
        let max_pool_size = self.config.pool_size_per_language * 2; // Allow 2x normal size
        let language = container.language.clone();
        let container_id = container.id.clone();

        let mut pools = self.pools.lock().await;
        let pool = pools.get_pool_mut(&language);

        // Only keep up to a certain number of containers per language
        if pool.len() < max_pool_size {
            pool.push_back(container);
            debug!(
                "Returned container to {:?} pool (size: {})",
                language,
                pool.len()
            );
        } else {
            // Pool is full, clean up this container
            debug!(
                "Pool for {:?} is full, cleaning up container {}",
                language, container_id
            );
            self.manager.cleanup_container(&container_id).await;
        }
    }

    /// Maintenance task to keep pool healthy
    pub async fn maintain_pool(&self) {
        let max_age = Duration::from_secs(self.config.pool_max_container_age_seconds);
        let mut pools = self.pools.lock().await;

        // Remove expired containers and replenish
        for language in &[
            Language::Python,
            Language::Javascript,
            Language::Shell,
            Language::Rust,
        ] {
            let pool = pools.get_pool_mut(language);
            let mut to_remove = Vec::new();

            // Check each container
            for (idx, container) in pool.iter().enumerate() {
                if container.created_at.elapsed() > max_age {
                    to_remove.push(idx);
                }
            }

            // Remove expired containers (in reverse to maintain indices)
            for idx in to_remove.into_iter().rev() {
                if let Some(container) = pool.remove(idx) {
                    debug!("Removing expired container {}", container.id);
                    self.manager.cleanup_container(&container.id).await;
                }
            }
        }

        let total = pools.total_size();
        debug!("Pool maintenance complete. Total containers: {}", total);
    }

    /// Shutdown the pool and cleanup all containers
    pub async fn shutdown(&self) {
        info!("Shutting down container pool...");

        let mut pools = self.pools.lock().await;

        for language in &[
            Language::Python,
            Language::Javascript,
            Language::Shell,
            Language::Rust,
        ] {
            let pool = pools.get_pool_mut(language);
            while let Some(container) = pool.pop_front() {
                debug!("Cleaning up pooled container {}", container.id);
                self.manager.cleanup_container(&container.id).await;
            }
        }

        info!("Container pool shutdown complete");
    }
}
