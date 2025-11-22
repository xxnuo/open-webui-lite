use crate::error::AppError;
use crate::models::User;
use crate::services::user::UserService;
use crate::utils::auth::verify_jwt;
use crate::AppState;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    error::Error as ActixError,
    http::header,
    web, HttpMessage,
};
use futures::future::{ready, LocalBoxFuture, Ready};
use std::rc::Rc;

#[derive(Clone)]
pub struct AuthUser {
    pub user: User,
}

#[allow(dead_code)]
impl AuthUser {
    #[allow(dead_code)]
    pub fn id(&self) -> &str {
        &self.user.id
    }
}

impl std::ops::Deref for AuthUser {
    type Target = User;

    fn deref(&self) -> &Self::Target {
        &self.user
    }
}

// Extractor for AuthUser from request extensions
impl actix_web::FromRequest for AuthUser {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let result = req
            .extensions()
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()));

        ready(result)
    }
}

// Auth middleware factory
pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();

        Box::pin(async move {
            // Extract state
            let state = req
                .app_data::<web::Data<AppState>>()
                .ok_or_else(|| AppError::InternalServerError("App state not found".to_string()))?;

            // Try to extract token from Authorization header first
            let token = if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
                if let Ok(auth_str) = auth_header.to_str() {
                    auth_str.strip_prefix("Bearer ").map(|s| s.to_string())
                } else {
                    None
                }
            } else {
                None
            };

            // If no Authorization header, try to get token from cookie
            let token = token
                .or_else(|| req.cookie("token").map(|c| c.value().to_string()))
                .ok_or_else(|| AppError::Unauthorized("Missing authorization token".to_string()))?;

            // Check if it's an API key (starts with sk-)
            let user = if token.starts_with("sk-") {
                let config = state.config.read().unwrap();
                if !config.enable_api_key {
                    return Err(AppError::Forbidden("API keys are disabled".to_string()).into());
                }
                drop(config); // Release the lock

                let user_service = UserService::new(&state.db);
                user_service
                    .get_user_by_api_key(&token)
                    .await
                    .map_err(|e| AppError::from(e))?
                    .ok_or_else(|| AppError::Unauthorized("Invalid API key".to_string()))?
            } else {
                // Otherwise, verify JWT token
                let config = state.config.read().unwrap();
                let webui_secret_key = config.webui_secret_key.clone();
                drop(config); // Release the lock

                let claims = verify_jwt(&token, &webui_secret_key).map_err(|e| {
                    // Token verification failed (expired or invalid)
                    tracing::debug!("JWT verification failed: {:?}", e);
                    AppError::Unauthorized("Invalid or expired token".to_string())
                })?;

                // Check token expiration explicitly
                if let Some(exp) = claims.exp {
                    let now = chrono::Utc::now().timestamp();
                    if now > exp {
                        tracing::debug!("Token expired at {}, current time {}", exp, now);
                        return Err(AppError::Unauthorized("Token expired".to_string()).into());
                    }
                }

                let user_service = UserService::new(&state.db);
                user_service
                    .get_user_by_id(&claims.sub)
                    .await
                    .map_err(|e| AppError::from(e))?
                    .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?
            };

            // Insert user into request extensions
            req.extensions_mut().insert(AuthUser { user });

            let res = service.call(req).await?;
            Ok(res)
        })
    }
}

// Admin middleware factory
pub struct AdminMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AdminMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type InitError = ();
    type Transform = AdminMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AdminMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct AdminMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AdminMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();

        Box::pin(async move {
            // Extract state
            let state = req
                .app_data::<web::Data<AppState>>()
                .ok_or_else(|| AppError::InternalServerError("App state not found".to_string()))?;

            // Try to extract token from Authorization header first
            let token = if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
                if let Ok(auth_str) = auth_header.to_str() {
                    auth_str.strip_prefix("Bearer ").map(|s| s.to_string())
                } else {
                    None
                }
            } else {
                None
            };

            // If no Authorization header, try to get token from cookie
            let token = token
                .or_else(|| req.cookie("token").map(|c| c.value().to_string()))
                .ok_or_else(|| AppError::Unauthorized("Missing authorization token".to_string()))?;

            // Check if it's an API key (starts with sk-)
            let user = if token.starts_with("sk-") {
                let config = state.config.read().unwrap();
                if !config.enable_api_key {
                    return Err(AppError::Forbidden("API keys are disabled".to_string()).into());
                }
                drop(config); // Release the lock

                let user_service = UserService::new(&state.db);
                user_service
                    .get_user_by_api_key(&token)
                    .await
                    .map_err(|e| AppError::from(e))?
                    .ok_or_else(|| AppError::Unauthorized("Invalid API key".to_string()))?
            } else {
                // Otherwise, verify JWT token
                let config = state.config.read().unwrap();
                let webui_secret_key = config.webui_secret_key.clone();
                drop(config); // Release the lock

                let claims = verify_jwt(&token, &webui_secret_key).map_err(|e| {
                    // Token verification failed (expired or invalid)
                    tracing::debug!("JWT verification failed: {:?}", e);
                    AppError::Unauthorized("Invalid or expired token".to_string())
                })?;

                // Check token expiration explicitly
                if let Some(exp) = claims.exp {
                    let now = chrono::Utc::now().timestamp();
                    if now > exp {
                        tracing::debug!("Token expired at {}, current time {}", exp, now);
                        return Err(AppError::Unauthorized("Token expired".to_string()).into());
                    }
                }

                let user_service = UserService::new(&state.db);
                user_service
                    .get_user_by_id(&claims.sub)
                    .await
                    .map_err(|e| AppError::from(e))?
                    .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?
            };

            // Check if user is admin
            if user.role != "admin" {
                return Err(AppError::Forbidden("Admin access required".to_string()).into());
            }

            // Insert user into request extensions
            req.extensions_mut().insert(AuthUser { user });

            let res = service.call(req).await?;
            Ok(res)
        })
    }
}
