pub mod limits;
pub mod seccomp;

use crate::config::Config;
use bollard::models::HostConfig;

pub struct SecurityConfig {
    pub memory_limit: i64, // bytes
    pub cpu_quota: i64,
    pub cpu_period: i64,
    pub pids_limit: i64,
    pub read_only_rootfs: bool,
    pub drop_capabilities: bool,
    pub network_disabled: bool,
}

impl SecurityConfig {
    pub fn from_config(config: &Config) -> Self {
        use tracing::debug;
        debug!(
            "SecurityConfig: read_only_root from config = {}",
            config.read_only_root
        );

        Self {
            memory_limit: (config.max_memory_mb * 1024 * 1024) as i64,
            cpu_quota: config.max_cpu_quota as i64,
            cpu_period: 100000, // Standard CPU period
            pids_limit: 50,     // Max 50 processes
            read_only_rootfs: config.read_only_root,
            drop_capabilities: config.drop_all_capabilities,
            network_disabled: config.network_mode == crate::config::NetworkMode::None,
        }
    }

    pub fn to_host_config(&self) -> HostConfig {
        let mut host_config = HostConfig::default();

        // Memory limits
        host_config.memory = Some(self.memory_limit);
        host_config.memory_swap = Some(self.memory_limit); // No swap

        // CPU limits
        host_config.cpu_quota = Some(self.cpu_quota);
        host_config.cpu_period = Some(self.cpu_period);

        // Process limits
        host_config.pids_limit = Some(self.pids_limit as i64);

        // Filesystem - read-only root with tmpfs for workspace
        host_config.readonly_rootfs = Some(self.read_only_rootfs);

        // Add tmpfs mount for /workspace if read-only rootfs is enabled
        if self.read_only_rootfs {
            use std::collections::HashMap;
            let mut tmpfs = HashMap::new();
            // Mount tmpfs with sandbox user ownership (uid=1000, gid=1000)
            tmpfs.insert(
                "/workspace".to_string(),
                "rw,nosuid,size=100m,uid=1000,gid=1000,mode=1777".to_string(),
            );
            host_config.tmpfs = Some(tmpfs);
        }

        // Network
        host_config.network_mode = Some(if self.network_disabled {
            "none".to_string()
        } else {
            "bridge".to_string()
        });

        // Drop all capabilities for maximum security
        if self.drop_capabilities {
            host_config.cap_drop = Some(vec!["ALL".to_string()]);
        }

        // Security options
        host_config.security_opt = Some(vec!["no-new-privileges".to_string()]);

        // Disable privileged mode
        host_config.privileged = Some(false);

        // No access to host devices
        host_config.devices = Some(vec![]);

        host_config
    }
}

/// Validate code for potential security issues
pub fn validate_code(code: &str, _language: &crate::models::Language) -> Result<(), String> {
    // Check code size
    if code.len() > 100_000 {
        return Err("Code size exceeds 100KB limit".to_string());
    }

    // Check for null bytes
    if code.contains('\0') {
        return Err("Code contains null bytes".to_string());
    }

    // Additional security checks can be added here
    // For example: detecting suspicious patterns, import restrictions, etc.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Language;

    #[test]
    fn test_validate_code_size() {
        let large_code = "x".repeat(100_001);
        assert!(validate_code(&large_code, &Language::Python).is_err());

        let normal_code = "print('hello')";
        assert!(validate_code(normal_code, &Language::Python).is_ok());
    }

    #[test]
    fn test_validate_null_bytes() {
        let code_with_null = "print('hello\0world')";
        assert!(validate_code(code_with_null, &Language::Python).is_err());
    }
}
