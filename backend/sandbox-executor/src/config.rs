use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub docker_host: String,

    // Security settings
    pub max_execution_time: u64, // seconds
    pub max_memory_mb: u64,      // MB
    pub max_cpu_quota: u64,      // CPU quota (100000 = 1 CPU)
    pub max_disk_mb: u64,        // MB
    pub max_concurrent_executions: usize,

    // Rate limiting
    pub rate_limit_per_minute: u32,
    pub rate_limit_burst: u32,

    // Container settings
    pub container_image: String,
    pub network_mode: NetworkMode,
    pub read_only_root: bool,
    pub drop_all_capabilities: bool,

    // Execution settings
    pub enable_streaming: bool,
    pub keep_containers: bool, // For debugging

    // Container Pool settings
    pub enable_container_pool: bool,
    pub pool_size_per_language: usize,
    pub pool_max_container_reuse: u32,
    pub pool_max_container_age_seconds: u64,

    // Language support
    pub enable_python: bool,
    pub enable_javascript: bool,
    pub enable_shell: bool,
    pub enable_rust: bool,

    // Audit & logging
    pub enable_audit_log: bool,
    pub audit_log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    None,
    Bridge,
    Host,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8090,
            docker_host: "unix:///var/run/docker.sock".to_string(),

            // Security defaults - very restrictive
            max_execution_time: 60,
            max_memory_mb: 512,
            max_cpu_quota: 100000, // 1 CPU
            max_disk_mb: 100,
            max_concurrent_executions: 10,

            // Rate limiting - 30 requests per minute with burst of 10
            rate_limit_per_minute: 30,
            rate_limit_burst: 10,

            // Container defaults
            container_image: "sandbox-runtime:latest".to_string(),
            network_mode: NetworkMode::None,
            read_only_root: false, // TODO: Enable with proper tmpfs configuration
            drop_all_capabilities: true,

            // Execution defaults
            enable_streaming: true,
            keep_containers: false,

            // Container Pool defaults
            enable_container_pool: true,
            pool_size_per_language: 3,
            pool_max_container_reuse: 50,
            pool_max_container_age_seconds: 600, // 10 minutes

            // Language support - all enabled by default
            enable_python: true,
            enable_javascript: true,
            enable_shell: true,
            enable_rust: true,

            // Audit
            enable_audit_log: true,
            audit_log_path: "./logs/audit.log".to_string(),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        dotenvy::dotenv().ok();

        let mut config = Config::default();

        // Load from environment variables
        if let Ok(host) = env::var("SANDBOX_HOST") {
            config.host = host;
        }

        if let Ok(port) = env::var("SANDBOX_PORT") {
            config.port = port.parse().map_err(|e| format!("Invalid port: {}", e))?;
        }

        if let Ok(docker_host) = env::var("DOCKER_HOST") {
            config.docker_host = docker_host;
        }

        if let Ok(max_time) = env::var("MAX_EXECUTION_TIME") {
            config.max_execution_time = max_time
                .parse()
                .map_err(|e| format!("Invalid max_execution_time: {}", e))?;
        }

        if let Ok(max_mem) = env::var("MAX_MEMORY_MB") {
            config.max_memory_mb = max_mem
                .parse()
                .map_err(|e| format!("Invalid max_memory_mb: {}", e))?;
        }

        if let Ok(max_cpu) = env::var("MAX_CPU_QUOTA") {
            config.max_cpu_quota = max_cpu
                .parse()
                .map_err(|e| format!("Invalid max_cpu_quota: {}", e))?;
        }

        if let Ok(max_disk) = env::var("MAX_DISK_MB") {
            config.max_disk_mb = max_disk
                .parse()
                .map_err(|e| format!("Invalid max_disk_mb: {}", e))?;
        }

        if let Ok(concurrent) = env::var("MAX_CONCURRENT_EXECUTIONS") {
            config.max_concurrent_executions = concurrent
                .parse()
                .map_err(|e| format!("Invalid max_concurrent_executions: {}", e))?;
        }

        if let Ok(rate) = env::var("RATE_LIMIT_PER_MINUTE") {
            config.rate_limit_per_minute = rate
                .parse()
                .map_err(|e| format!("Invalid rate_limit: {}", e))?;
        }

        if let Ok(burst) = env::var("RATE_LIMIT_BURST") {
            config.rate_limit_burst = burst
                .parse()
                .map_err(|e| format!("Invalid rate_limit_burst: {}", e))?;
        }

        if let Ok(image) = env::var("CONTAINER_IMAGE") {
            config.container_image = image;
        }

        if let Ok(network) = env::var("NETWORK_MODE") {
            config.network_mode = match network.to_lowercase().as_str() {
                "none" => NetworkMode::None,
                "bridge" => NetworkMode::Bridge,
                "host" => NetworkMode::Host,
                _ => return Err(format!("Invalid network mode: {}", network)),
            };
        }

        if let Ok(readonly) = env::var("READ_ONLY_ROOT") {
            config.read_only_root = readonly
                .parse()
                .map_err(|e| format!("Invalid read_only_root: {}", e))?;
        }

        if let Ok(drop_caps) = env::var("DROP_ALL_CAPABILITIES") {
            config.drop_all_capabilities = drop_caps
                .parse()
                .map_err(|e| format!("Invalid drop_all_capabilities: {}", e))?;
        }

        if let Ok(streaming) = env::var("ENABLE_STREAMING") {
            config.enable_streaming = streaming
                .parse()
                .map_err(|e| format!("Invalid enable_streaming: {}", e))?;
        }

        if let Ok(keep) = env::var("KEEP_CONTAINERS") {
            config.keep_containers = keep
                .parse()
                .map_err(|e| format!("Invalid keep_containers: {}", e))?;
        }

        // Container Pool settings
        if let Ok(enable_pool) = env::var("ENABLE_CONTAINER_POOL") {
            config.enable_container_pool = enable_pool
                .parse()
                .map_err(|e| format!("Invalid enable_container_pool: {}", e))?;
        }

        if let Ok(pool_size) = env::var("POOL_SIZE_PER_LANGUAGE") {
            config.pool_size_per_language = pool_size
                .parse()
                .map_err(|e| format!("Invalid pool_size_per_language: {}", e))?;
        }

        if let Ok(max_reuse) = env::var("POOL_MAX_CONTAINER_REUSE") {
            config.pool_max_container_reuse = max_reuse
                .parse()
                .map_err(|e| format!("Invalid pool_max_container_reuse: {}", e))?;
        }

        if let Ok(max_age) = env::var("POOL_MAX_CONTAINER_AGE_SECONDS") {
            config.pool_max_container_age_seconds = max_age
                .parse()
                .map_err(|e| format!("Invalid pool_max_container_age_seconds: {}", e))?;
        }

        // Language support
        if let Ok(python) = env::var("ENABLE_PYTHON") {
            config.enable_python = python
                .parse()
                .map_err(|e| format!("Invalid enable_python: {}", e))?;
        }

        if let Ok(js) = env::var("ENABLE_JAVASCRIPT") {
            config.enable_javascript = js
                .parse()
                .map_err(|e| format!("Invalid enable_javascript: {}", e))?;
        }

        if let Ok(shell) = env::var("ENABLE_SHELL") {
            config.enable_shell = shell
                .parse()
                .map_err(|e| format!("Invalid enable_shell: {}", e))?;
        }

        if let Ok(rust) = env::var("ENABLE_RUST") {
            config.enable_rust = rust
                .parse()
                .map_err(|e| format!("Invalid enable_rust: {}", e))?;
        }

        // Audit
        if let Ok(audit) = env::var("ENABLE_AUDIT_LOG") {
            config.enable_audit_log = audit
                .parse()
                .map_err(|e| format!("Invalid enable_audit_log: {}", e))?;
        }

        if let Ok(audit_path) = env::var("AUDIT_LOG_PATH") {
            config.audit_log_path = audit_path;
        }

        Ok(config)
    }
}
