use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use std::fmt;

#[derive(Debug)]
pub enum SandboxError {
    // Execution errors
    ExecutionTimeout,
    ExecutionFailed(String),
    LanguageNotSupported(String),
    CodeTooLarge,

    // Container errors
    ContainerCreationFailed(String),
    ContainerStartFailed(String),
    ContainerCleanupFailed(String),
    DockerConnectionFailed(String),

    // Security errors
    RateLimitExceeded,
    ResourceLimitExceeded(String),
    InvalidInput(String),

    // System errors
    InternalError(String),
    ConfigurationError(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::ExecutionTimeout => write!(f, "Code execution timed out"),
            SandboxError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            SandboxError::LanguageNotSupported(lang) => {
                write!(f, "Language not supported: {}", lang)
            }
            SandboxError::CodeTooLarge => write!(f, "Code size exceeds maximum allowed"),

            SandboxError::ContainerCreationFailed(msg) => {
                write!(f, "Failed to create container: {}", msg)
            }
            SandboxError::ContainerStartFailed(msg) => {
                write!(f, "Failed to start container: {}", msg)
            }
            SandboxError::ContainerCleanupFailed(msg) => {
                write!(f, "Failed to cleanup container: {}", msg)
            }
            SandboxError::DockerConnectionFailed(msg) => {
                write!(f, "Docker connection failed: {}", msg)
            }

            SandboxError::RateLimitExceeded => {
                write!(f, "Rate limit exceeded, please try again later")
            }
            SandboxError::ResourceLimitExceeded(msg) => {
                write!(f, "Resource limit exceeded: {}", msg)
            }
            SandboxError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),

            SandboxError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            SandboxError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for SandboxError {}

impl ResponseError for SandboxError {
    fn status_code(&self) -> StatusCode {
        match self {
            SandboxError::ExecutionTimeout => StatusCode::REQUEST_TIMEOUT,
            SandboxError::ExecutionFailed(_) => StatusCode::BAD_REQUEST,
            SandboxError::LanguageNotSupported(_) => StatusCode::BAD_REQUEST,
            SandboxError::CodeTooLarge => StatusCode::PAYLOAD_TOO_LARGE,

            SandboxError::ContainerCreationFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SandboxError::ContainerStartFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SandboxError::ContainerCleanupFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SandboxError::DockerConnectionFailed(_) => StatusCode::SERVICE_UNAVAILABLE,

            SandboxError::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            SandboxError::ResourceLimitExceeded(_) => StatusCode::BAD_REQUEST,
            SandboxError::InvalidInput(_) => StatusCode::BAD_REQUEST,

            SandboxError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SandboxError::ConfigurationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status_code = self.status_code();
        let error_message = self.to_string();

        HttpResponse::build(status_code).json(serde_json::json!({
            "error": format!("{:?}", self),
            "message": error_message,
        }))
    }
}

impl From<bollard::errors::Error> for SandboxError {
    fn from(err: bollard::errors::Error) -> Self {
        SandboxError::DockerConnectionFailed(err.to_string())
    }
}

impl From<std::io::Error> for SandboxError {
    fn from(err: std::io::Error) -> Self {
        SandboxError::InternalError(err.to_string())
    }
}

pub type SandboxResult<T> = Result<T, SandboxError>;
