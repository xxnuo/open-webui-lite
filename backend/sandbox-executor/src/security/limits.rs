/// Resource limit validation and enforcement
use crate::error::{SandboxError, SandboxResult};

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory_bytes: u64,
    pub max_cpu_time_seconds: u64,
    pub max_file_size_bytes: u64,
    pub max_processes: u32,
}

impl ResourceLimits {
    pub fn new(max_memory_mb: u64, max_cpu_time_seconds: u64) -> Self {
        Self {
            max_memory_bytes: max_memory_mb * 1024 * 1024,
            max_cpu_time_seconds,
            max_file_size_bytes: 10 * 1024 * 1024, // 10MB max file size
            max_processes: 50,
        }
    }

    pub fn validate_memory(&self, requested_mb: u64) -> SandboxResult<()> {
        let requested_bytes = requested_mb * 1024 * 1024;
        if requested_bytes > self.max_memory_bytes {
            return Err(SandboxError::ResourceLimitExceeded(format!(
                "Memory limit exceeded: requested {}MB, max {}MB",
                requested_mb,
                self.max_memory_bytes / 1024 / 1024
            )));
        }
        Ok(())
    }

    pub fn validate_timeout(&self, timeout_seconds: u64) -> SandboxResult<()> {
        if timeout_seconds > self.max_cpu_time_seconds {
            return Err(SandboxError::ResourceLimitExceeded(format!(
                "Timeout limit exceeded: requested {}s, max {}s",
                timeout_seconds, self.max_cpu_time_seconds
            )));
        }
        Ok(())
    }

    pub fn validate_code_size(&self, code_size: usize) -> SandboxResult<()> {
        if code_size > 100_000 {
            return Err(SandboxError::CodeTooLarge);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_validation() {
        let limits = ResourceLimits::new(512, 60);
        assert!(limits.validate_memory(256).is_ok());
        assert!(limits.validate_memory(1024).is_err());
    }

    #[test]
    fn test_timeout_validation() {
        let limits = ResourceLimits::new(512, 60);
        assert!(limits.validate_timeout(30).is_ok());
        assert!(limits.validate_timeout(120).is_err());
    }

    #[test]
    fn test_code_size_validation() {
        let limits = ResourceLimits::new(512, 60);
        assert!(limits.validate_code_size(1000).is_ok());
        assert!(limits.validate_code_size(200_000).is_err());
    }
}
