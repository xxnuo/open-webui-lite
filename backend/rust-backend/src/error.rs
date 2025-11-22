use actix_web::{
    cookie::{Cookie, SameSite},
    http::{header, StatusCode},
    HttpResponse, ResponseError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    InternalServerError(String),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("User already exists")]
    UserAlreadyExists,

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("External service error: {0}")]
    ExternalServiceError(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Redis pool error: {0}")]
    RedisPool(String),

    #[error("Request timeout: {0}")]
    Timeout(String),

    #[error("Too many requests: {0}")]
    TooManyRequests(String),
}

#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub detail: String,
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status, error_message) = match self {
            AppError::Database(ref e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
            AppError::Redis(ref e) => {
                tracing::error!("Redis error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Redis error".to_string())
            }
            AppError::Auth(ref e) => (StatusCode::UNAUTHORIZED, e.clone()),
            AppError::Validation(ref e) => (StatusCode::BAD_REQUEST, e.clone()),
            AppError::NotFound(ref e) => (StatusCode::NOT_FOUND, e.clone()),
            AppError::Unauthorized(ref e) => (StatusCode::UNAUTHORIZED, e.clone()),
            AppError::Forbidden(ref e) => (StatusCode::FORBIDDEN, e.clone()),
            AppError::BadRequest(ref e) => (StatusCode::BAD_REQUEST, e.clone()),
            AppError::InternalServerError(ref e) => {
                tracing::error!("Internal server error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.clone())
            }
            AppError::Jwt(ref e) => {
                tracing::error!("JWT error: {:?}", e);
                (StatusCode::UNAUTHORIZED, "Invalid token".to_string())
            }
            AppError::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "Invalid credentials".to_string())
            }
            AppError::UserAlreadyExists => {
                (StatusCode::BAD_REQUEST, "User already exists".to_string())
            }
            AppError::Conflict(ref e) => (StatusCode::CONFLICT, e.clone()),
            AppError::Io(ref e) => {
                tracing::error!("IO error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "IO error".to_string())
            }
            AppError::NotImplemented(ref e) => (StatusCode::NOT_IMPLEMENTED, e.clone()),
            AppError::ExternalServiceError(ref e) => {
                tracing::error!("External service error: {:?}", e);
                (StatusCode::BAD_GATEWAY, e.clone())
            }
            AppError::Internal(ref e) => {
                tracing::error!("Internal error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.clone())
            }
            AppError::Http(ref e) => {
                tracing::error!("HTTP error: {:?}", e);
                (StatusCode::BAD_GATEWAY, "HTTP request failed".to_string())
            }
            AppError::RedisPool(ref e) => {
                tracing::error!("Redis pool error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Redis pool error".to_string(),
                )
            }
            AppError::Timeout(ref e) => {
                tracing::error!("Request timeout: {:?}", e);
                (StatusCode::GATEWAY_TIMEOUT, e.clone())
            }
            AppError::TooManyRequests(ref e) => (StatusCode::TOO_MANY_REQUESTS, e.clone()),
        };

        let body = ErrorResponse {
            detail: error_message,
        };

        // Build response with CORS headers to ensure they're always present
        // even when errors occur in middleware before CORS middleware processes the response
        let mut response_builder = HttpResponse::build(status);
        response_builder
            .insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
            .insert_header((header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true"))
            .insert_header((
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, PUT, DELETE, PATCH, OPTIONS",
            ))
            .insert_header((
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                "Content-Type, Authorization, Accept, Cookie",
            ))
            .insert_header((header::ACCESS_CONTROL_EXPOSE_HEADERS, "Set-Cookie"));

        // Clear auth cookies on authentication errors (matching Python backend behavior)
        if matches!(
            self,
            AppError::Auth(_)
                | AppError::Unauthorized(_)
                | AppError::Jwt(_)
                | AppError::InvalidCredentials
        ) {
            let mut token_cookie = Cookie::new("token", "");
            token_cookie.set_http_only(true);
            token_cookie.set_same_site(SameSite::None);
            token_cookie.set_secure(true);
            token_cookie.set_path("/");
            token_cookie.set_max_age(time::Duration::seconds(-1));

            response_builder.insert_header((header::SET_COOKIE, token_cookie.to_string()));
        }

        response_builder.json(body)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Redis(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Auth(_) => StatusCode::UNAUTHORIZED,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Jwt(_) => StatusCode::UNAUTHORIZED,
            AppError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            AppError::UserAlreadyExists => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            AppError::ExternalServiceError(_) => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Http(_) => StatusCode::BAD_GATEWAY,
            AppError::RedisPool(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

// Implement From for redis pool errors
impl From<deadpool_redis::PoolError> for AppError {
    fn from(err: deadpool_redis::PoolError) -> Self {
        AppError::RedisPool(err.to_string())
    }
}
