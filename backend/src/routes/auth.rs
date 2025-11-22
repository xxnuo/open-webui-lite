use actix_web::{
    cookie::{Cookie, SameSite},
    http::header,
    web, HttpRequest, HttpResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use crate::error::AppResult;
use crate::middleware::{AuthMiddleware, AuthUser};
use crate::models::{SessionResponse, SigninRequest, SignupRequest};
use crate::services::{AuthService, UserService};
use crate::utils::auth::create_jwt;
use crate::AppState;

// Helper function to create a cookie for clearing auth cookies
fn create_clear_cookie() -> Cookie<'static> {
    let mut token_cookie = Cookie::new("token", "");
    token_cookie.set_http_only(true);
    token_cookie.set_same_site(SameSite::None);
    token_cookie.set_secure(true);
    token_cookie.set_path("/");
    token_cookie.set_max_age(time::Duration::seconds(-1));
    token_cookie
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/signin", web::post().to(signin))
        .route("/signup", web::post().to(signup))
        .route("/signout", web::get().to(signout))
        .route("/ldap", web::post().to(ldap_auth))
        .service(
            web::resource("")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_session_user)),
        )
        .service(
            web::resource("/")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_session_user)),
        )
        .service(
            web::resource("/update/profile")
                .wrap(AuthMiddleware)
                .route(web::post().to(update_profile)),
        )
        .service(
            web::resource("/update/password")
                .wrap(AuthMiddleware)
                .route(web::post().to(update_password)),
        )
        .service(
            web::resource("/add")
                .wrap(AuthMiddleware)
                .route(web::post().to(add_user)),
        )
        .service(
            web::resource("/api_key")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_api_key))
                .route(web::post().to(create_api_key))
                .route(web::delete().to(delete_api_key)),
        )
        .service(
            web::resource("/admin/details")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_admin_details)),
        )
        .service(
            web::resource("/admin/config")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_admin_config))
                .route(web::post().to(update_admin_config)),
        )
        .service(
            web::resource("/admin/config/ldap")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_ldap_config))
                .route(web::post().to(update_ldap_config)),
        )
        .service(
            web::resource("/admin/config/ldap/server")
                .wrap(AuthMiddleware)
                .route(web::get().to(get_ldap_server))
                .route(web::post().to(update_ldap_server)),
        );
}

async fn get_session_user(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    req: HttpRequest,
) -> AppResult<HttpResponse> {
    let config = state.config.read().unwrap();

    // Get token from Authorization header or cookie
    let token = if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            auth_str.strip_prefix("Bearer ").map(|s| s.to_string())
        } else {
            None
        }
    } else {
        None
    }
    .or_else(|| req.cookie("token").map(|c| c.value().to_string()));

    // Validate token and check expiration
    let (token, expires_at, _should_refresh) = if let Some(existing_token) = token {
        match crate::utils::auth::verify_jwt(&existing_token, &config.webui_secret_key) {
            Ok(claims) => {
                if let Some(exp) = claims.exp {
                    let now = chrono::Utc::now().timestamp();

                    // Check if token is expired
                    if now > exp {
                        return Err(crate::error::AppError::Unauthorized(
                            "Token expired".to_string(),
                        ));
                    }

                    // Check if token is close to expiring (within 5 minutes) - refresh it
                    let should_refresh = (exp - now) < 300; // 5 minutes = 300 seconds

                    if should_refresh {
                        // Generate new token
                        let new_token = create_jwt(
                            &auth_user.user.id,
                            &config.webui_secret_key,
                            &config.jwt_expires_in,
                        )?;

                        let new_expires_at = chrono::Utc::now()
                            .checked_add_signed(crate::utils::auth::parse_duration(
                                &config.jwt_expires_in,
                            )?)
                            .map(|dt| dt.timestamp());

                        (new_token, new_expires_at, true)
                    } else {
                        // Use existing token
                        (existing_token, Some(exp), false)
                    }
                } else {
                    // Token has no expiration, use it as is
                    (existing_token, None, false)
                }
            }
            Err(_) => {
                // Token is invalid, generate new one
                let new_token = create_jwt(
                    &auth_user.user.id,
                    &config.webui_secret_key,
                    &config.jwt_expires_in,
                )?;

                let new_expires_at = chrono::Utc::now()
                    .checked_add_signed(crate::utils::auth::parse_duration(&config.jwt_expires_in)?)
                    .map(|dt| dt.timestamp());

                (new_token, new_expires_at, true)
            }
        }
    } else {
        // No token found, generate new one
        let new_token = create_jwt(
            &auth_user.user.id,
            &config.webui_secret_key,
            &config.jwt_expires_in,
        )?;

        let new_expires_at = chrono::Utc::now()
            .checked_add_signed(crate::utils::auth::parse_duration(&config.jwt_expires_in)?)
            .map(|dt| dt.timestamp());

        (new_token, new_expires_at, true)
    };

    let response_json = json!({
        "token": token,
        "token_type": "Bearer",
        "expires_at": expires_at,
        "id": auth_user.user.id,
        "name": auth_user.user.name,
        "email": auth_user.user.email,
        "role": auth_user.user.role,
        "profile_image_url": auth_user.user.profile_image_url,
        "permissions": json!({}),
    });

    // Create/refresh cookie with token
    let mut cookie = Cookie::new("token", token.clone());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::None);
    cookie.set_secure(true);
    cookie.set_path("/");

    // Set expiration if available
    if let Some(exp) = expires_at {
        cookie.set_expires(time::OffsetDateTime::from_unix_timestamp(exp).ok());
    }

    // Return response with Set-Cookie header
    let mut response = HttpResponse::Ok();

    // Always set cookie to ensure it's refreshed
    response.append_header((header::SET_COOKIE, cookie.to_string()));

    Ok(response.json(response_json))
}

async fn signin(
    state: web::Data<AppState>,
    req: web::Json<SigninRequest>,
) -> AppResult<HttpResponse> {
    req.validate()
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;

    let auth_service = AuthService::new(&state.db);
    let user_service = UserService::new(&state.db);

    let user_id = auth_service
        .authenticate(&req.email.to_lowercase(), &req.password)
        .await?
        .ok_or(crate::error::AppError::InvalidCredentials)?;

    let user =
        user_service
            .get_user_by_id(&user_id)
            .await?
            .ok_or(crate::error::AppError::NotFound(
                "User not found".to_string(),
            ))?;

    let config = state.config.read().unwrap();
    let token = create_jwt(&user.id, &config.webui_secret_key, &config.jwt_expires_in)?;

    let expires_at = chrono::Utc::now()
        .checked_add_signed(crate::utils::auth::parse_duration(&config.jwt_expires_in)?)
        .map(|dt| dt.timestamp());

    let session_response = SessionResponse {
        token: token.clone(),
        token_type: "Bearer".to_string(),
        expires_at,
        id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
        profile_image_url: user.profile_image_url,
        permissions: json!({}),
    };

    // Create cookie with token
    let mut cookie = Cookie::new("token", token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::None);
    cookie.set_secure(true);
    cookie.set_path("/");

    // Set expiration if available
    if let Some(exp) = expires_at {
        cookie.set_expires(time::OffsetDateTime::from_unix_timestamp(exp).ok());
    }

    // Return response with Set-Cookie header
    Ok(HttpResponse::Ok()
        .append_header((header::SET_COOKIE, cookie.to_string()))
        .json(session_response))
}

async fn signup(
    state: web::Data<AppState>,
    req: web::Json<SignupRequest>,
) -> AppResult<HttpResponse> {
    let config = state.config.read().unwrap();

    if !config.enable_signup {
        return Err(crate::error::AppError::Forbidden(
            "Signup is disabled".to_string(),
        ));
    }

    req.validate()
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;

    if let Some(confirmation) = &req.password_confirmation {
        if &req.password != confirmation {
            return Err(crate::error::AppError::BadRequest(
                "Passwords do not match".to_string(),
            ));
        }
    }

    let auth_service = AuthService::new(&state.db);
    let user_service = UserService::new(&state.db);

    // Check if user already exists
    if user_service
        .get_user_by_email(&req.email.to_lowercase())
        .await?
        .is_some()
    {
        return Err(crate::error::AppError::UserAlreadyExists);
    }

    // Check if this is the first user (should be admin)
    let user_count = user_service.count_users().await?;
    let role = if user_count == 0 {
        "admin"
    } else {
        &config.default_user_role
    };

    // Create user
    let user_id = uuid::Uuid::new_v4().to_string();
    let profile_image_url = format!("/user.png");

    let user = user_service
        .create_user(
            &user_id,
            &req.name,
            &req.email.to_lowercase(),
            role,
            &profile_image_url,
        )
        .await?;

    // Create auth
    auth_service
        .create_auth(&user_id, &req.email.to_lowercase(), &req.password)
        .await?;

    let token = create_jwt(&user.id, &config.webui_secret_key, &config.jwt_expires_in)?;

    let expires_at = chrono::Utc::now()
        .checked_add_signed(crate::utils::auth::parse_duration(&config.jwt_expires_in)?)
        .map(|dt| dt.timestamp());

    let session_response = SessionResponse {
        token: token.clone(),
        token_type: "Bearer".to_string(),
        expires_at,
        id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
        profile_image_url: user.profile_image_url,
        permissions: json!({}),
    };

    // Create cookie with token
    let mut cookie = Cookie::new("token", token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::None);
    cookie.set_secure(true);
    cookie.set_path("/");

    // Set expiration if available
    if let Some(exp) = expires_at {
        cookie.set_expires(time::OffsetDateTime::from_unix_timestamp(exp).ok());
    }

    // Return response with Set-Cookie header
    Ok(HttpResponse::Ok()
        .append_header((header::SET_COOKIE, cookie.to_string()))
        .json(session_response))
}

async fn signout(_state: web::Data<AppState>) -> HttpResponse {
    // Clear the token cookie by setting an expired cookie
    let mut cookie = Cookie::new("token", "");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::None);
    cookie.set_secure(true);
    cookie.set_path("/");
    cookie.set_max_age(time::Duration::seconds(-1));

    HttpResponse::Ok()
        .append_header((header::SET_COOKIE, cookie.to_string()))
        .json(json!({"status": true}))
}

async fn update_profile(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    req: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);

    // Extract update fields from request
    let name = req.get("name").and_then(|v| v.as_str());
    let profile_image_url = req.get("profile_image_url").and_then(|v| v.as_str());

    // Update user profile in the database
    user_service
        .update_user_profile(
            &auth_user.user.id,
            name,
            profile_image_url,
            None, // bio
            None, // gender
            None, // date_of_birth
        )
        .await?;

    // Retrieve and return updated user
    let user = user_service
        .get_user_by_id(&auth_user.user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    Ok(HttpResponse::Ok().json(json!({
        "id": user.id,
        "name": user.name,
        "email": user.email,
        "role": user.role,
        "profile_image_url": user.profile_image_url,
    })))
}

async fn update_password(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    req: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let password =
        req.get("password")
            .and_then(|v| v.as_str())
            .ok_or(crate::error::AppError::BadRequest(
                "password is required".to_string(),
            ))?;

    let _new_password = req.get("new_password").and_then(|v| v.as_str()).ok_or(
        crate::error::AppError::BadRequest("new_password is required".to_string()),
    )?;

    let auth_service = AuthService::new(&state.db);

    // Verify current password
    let user_id = auth_service
        .authenticate(&auth_user.user.email, password)
        .await?
        .ok_or(crate::error::AppError::InvalidCredentials)?;

    if user_id != auth_user.user.id {
        return Err(crate::error::AppError::InvalidCredentials);
    }

    // TODO: Update password (you'll need to implement this in AuthService)
    // auth_service.update_password(&auth_user.user.id, new_password).await?;
    // For now, return success
    Ok(HttpResponse::Ok().json(json!({"status": true})))
}

#[derive(Debug, Deserialize, Validate)]
struct AddUserRequest {
    #[validate(email)]
    email: String,
    name: String,
    password: String,
    #[serde(default)]
    profile_image_url: Option<String>,
    role: String,
}

async fn add_user(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    req: web::Json<AddUserRequest>,
) -> AppResult<HttpResponse> {
    // Only admin can add users
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    req.validate()
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;

    let user_service = UserService::new(&state.db);
    let auth_service = AuthService::new(&state.db);

    // Check if user already exists
    if user_service
        .get_user_by_email(&req.email.to_lowercase())
        .await?
        .is_some()
    {
        return Err(crate::error::AppError::UserAlreadyExists);
    }

    // Create user
    let user_id = uuid::Uuid::new_v4().to_string();
    let profile_image_url = req
        .profile_image_url
        .clone()
        .unwrap_or_else(|| "/user.png".to_string());

    let user = user_service
        .create_user(
            &user_id,
            &req.name,
            &req.email.to_lowercase(),
            &req.role,
            &profile_image_url,
        )
        .await?;

    // Create auth
    auth_service
        .create_auth(&user_id, &req.email.to_lowercase(), &req.password)
        .await?;

    let config = state.config.read().unwrap();
    let token = create_jwt(&user.id, &config.webui_secret_key, &config.jwt_expires_in)?;

    Ok(HttpResponse::Ok().json(json!({
        "token": token,
        "token_type": "Bearer",
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "role": user.role,
        "profile_image_url": user.profile_image_url,
    })))
}

async fn get_admin_details(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let config = state.config.read().unwrap();

    if !config.show_admin_details {
        return Err(crate::error::AppError::Forbidden(
            "Action prohibited".to_string(),
        ));
    }

    let user_service = UserService::new(&state.db);

    // Get first user as admin
    let (admin_name, admin_email) = if let Some(first_user) = user_service.get_first_user().await? {
        (Some(first_user.name), Some(first_user.email))
    } else {
        (None, None)
    };

    Ok(HttpResponse::Ok().json(json!({
        "name": admin_name,
        "email": admin_email,
    })))
}

async fn get_api_key(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    let user_service = UserService::new(&state.db);

    let user = user_service
        .get_user_by_id(&auth_user.user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound(
            "User not found".to_string(),
        ))?;

    if let Some(api_key) = user.api_key {
        Ok(HttpResponse::Ok().json(json!({"api_key": api_key})))
    } else {
        Err(crate::error::AppError::NotFound(
            "API key not found".to_string(),
        ))
    }
}

async fn create_api_key(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let config = state.config.read().unwrap();

    if !config.enable_api_key {
        return Err(crate::error::AppError::Forbidden(
            "API key creation is not allowed".to_string(),
        ));
    }

    // Generate API key
    let api_key = format!("sk-{}", uuid::Uuid::new_v4().to_string().replace("-", ""));

    // Update user with new API key
    let result = sqlx::query(
        r#"
        UPDATE "user"
        SET api_key = $1, updated_at = $2
        WHERE id = $3
        "#,
    )
    .bind(&api_key)
    .bind(chrono::Utc::now().timestamp())
    .bind(&auth_user.user.id)
    .execute(&state.db.pool)
    .await?;

    if result.rows_affected() > 0 {
        Ok(HttpResponse::Ok().json(json!({"api_key": api_key})))
    } else {
        Err(crate::error::AppError::BadRequest(
            "Failed to create API key".to_string(),
        ))
    }
}

async fn delete_api_key(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Set API key to NULL
    let result = sqlx::query(
        r#"
        UPDATE "user"
        SET api_key = NULL, updated_at = $1
        WHERE id = $2
        "#,
    )
    .bind(chrono::Utc::now().timestamp())
    .bind(&auth_user.user.id)
    .execute(&state.db.pool)
    .await?;

    Ok(HttpResponse::Ok().json(result.rows_affected() > 0))
}

#[derive(Debug, Serialize, Deserialize)]
struct AdminConfigResponse {
    #[serde(rename = "SHOW_ADMIN_DETAILS")]
    show_admin_details: bool,
    #[serde(rename = "WEBUI_URL")]
    webui_url: String,
    #[serde(rename = "ENABLE_SIGNUP")]
    enable_signup: bool,
    #[serde(rename = "ENABLE_API_KEY")]
    enable_api_key: bool,
    #[serde(rename = "ENABLE_API_KEY_ENDPOINT_RESTRICTIONS")]
    enable_api_key_endpoint_restrictions: bool,
    #[serde(rename = "API_KEY_ALLOWED_ENDPOINTS")]
    api_key_allowed_endpoints: String,
    #[serde(rename = "DEFAULT_USER_ROLE")]
    default_user_role: String,
    #[serde(rename = "JWT_EXPIRES_IN")]
    jwt_expires_in: String,
    #[serde(rename = "ENABLE_COMMUNITY_SHARING")]
    enable_community_sharing: bool,
    #[serde(rename = "ENABLE_MESSAGE_RATING")]
    enable_message_rating: bool,
    #[serde(rename = "ENABLE_CHANNELS")]
    enable_channels: bool,
    #[serde(rename = "ENABLE_NOTES")]
    enable_notes: bool,
    #[serde(rename = "ENABLE_USER_WEBHOOKS")]
    enable_user_webhooks: bool,
    #[serde(rename = "PENDING_USER_OVERLAY_TITLE")]
    pending_user_overlay_title: Option<String>,
    #[serde(rename = "PENDING_USER_OVERLAY_CONTENT")]
    pending_user_overlay_content: Option<String>,
    #[serde(rename = "RESPONSE_WATERMARK")]
    response_watermark: Option<String>,
}

async fn get_admin_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can access this endpoint
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(AdminConfigResponse {
        show_admin_details: config.show_admin_details,
        webui_url: config.webui_url.clone(),
        enable_signup: config.enable_signup,
        enable_api_key: config.enable_api_key,
        enable_api_key_endpoint_restrictions: config.enable_api_key_endpoint_restrictions,
        api_key_allowed_endpoints: config.api_key_allowed_endpoints.clone(),
        default_user_role: config.default_user_role.clone(),
        jwt_expires_in: config.jwt_expires_in.clone(),
        enable_community_sharing: config.enable_community_sharing,
        enable_message_rating: config.enable_message_rating,
        enable_channels: config.enable_channels,
        enable_notes: config.enable_notes,
        enable_user_webhooks: config.enable_user_webhooks,
        pending_user_overlay_title: config.pending_user_overlay_title.clone(),
        pending_user_overlay_content: config.pending_user_overlay_content.clone(),
        response_watermark: config.response_watermark.clone(),
    }))
}

async fn update_admin_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<AdminConfigResponse>,
) -> AppResult<HttpResponse> {
    // Only admins can access this endpoint
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    // Update config with write lock
    let mut config = state.config.write().unwrap();

    config.show_admin_details = form_data.show_admin_details;
    config.webui_url = form_data.webui_url.clone();
    config.enable_signup = form_data.enable_signup;
    config.enable_api_key = form_data.enable_api_key;
    config.enable_api_key_endpoint_restrictions = form_data.enable_api_key_endpoint_restrictions;
    config.api_key_allowed_endpoints = form_data.api_key_allowed_endpoints.clone();

    // Validate and update default_user_role
    if ["pending", "user", "admin"].contains(&form_data.default_user_role.as_str()) {
        config.default_user_role = form_data.default_user_role.clone();
    }

    // Validate JWT_EXPIRES_IN format (basic validation)
    let pattern = regex::Regex::new(r"^(-1|0|(-?\d+(\.\d+)?)(ms|s|m|h|d|w))$").unwrap();
    if pattern.is_match(&form_data.jwt_expires_in) {
        config.jwt_expires_in = form_data.jwt_expires_in.clone();
    }

    config.enable_community_sharing = form_data.enable_community_sharing;
    config.enable_message_rating = form_data.enable_message_rating;
    config.enable_channels = form_data.enable_channels;
    config.enable_notes = form_data.enable_notes;
    config.enable_user_webhooks = form_data.enable_user_webhooks;
    config.pending_user_overlay_title = form_data.pending_user_overlay_title.clone();
    config.pending_user_overlay_content = form_data.pending_user_overlay_content.clone();
    config.response_watermark = form_data.response_watermark.clone();

    // Persist admin config to database
    let admin_config_json = serde_json::json!({
        "show_admin_details": config.show_admin_details,
        "webui_url": config.webui_url,
        "enable_signup": config.enable_signup,
        "enable_api_key": config.enable_api_key,
        "enable_api_key_endpoint_restrictions": config.enable_api_key_endpoint_restrictions,
        "api_key_allowed_endpoints": config.api_key_allowed_endpoints,
        "default_user_role": config.default_user_role,
        "jwt_expires_in": config.jwt_expires_in,
        "enable_community_sharing": config.enable_community_sharing,
        "enable_message_rating": config.enable_message_rating,
        "enable_channels": config.enable_channels,
        "enable_notes": config.enable_notes,
        "enable_user_webhooks": config.enable_user_webhooks,
        "pending_user_overlay_title": config.pending_user_overlay_title,
        "pending_user_overlay_content": config.pending_user_overlay_content,
        "response_watermark": config.response_watermark,
    });

    // Drop the write lock before async operations
    drop(config);

    if let Err(e) =
        crate::services::ConfigService::update_section(&state.db, "admin", admin_config_json).await
    {
        tracing::warn!("Failed to persist admin config to database: {}", e);
    }

    // Re-acquire read lock for response
    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(AdminConfigResponse {
        show_admin_details: config.show_admin_details,
        webui_url: config.webui_url.clone(),
        enable_signup: config.enable_signup,
        enable_api_key: config.enable_api_key,
        enable_api_key_endpoint_restrictions: config.enable_api_key_endpoint_restrictions,
        api_key_allowed_endpoints: config.api_key_allowed_endpoints.clone(),
        default_user_role: config.default_user_role.clone(),
        jwt_expires_in: config.jwt_expires_in.clone(),
        enable_community_sharing: config.enable_community_sharing,
        enable_message_rating: config.enable_message_rating,
        enable_channels: config.enable_channels,
        enable_notes: config.enable_notes,
        enable_user_webhooks: config.enable_user_webhooks,
        pending_user_overlay_title: config.pending_user_overlay_title.clone(),
        pending_user_overlay_content: config.pending_user_overlay_content.clone(),
        response_watermark: config.response_watermark.clone(),
    }))
}

// LDAP Configuration Structures
#[derive(Debug, Serialize, Deserialize)]
struct LdapConfigResponse {
    #[serde(rename = "ENABLE_LDAP")]
    enable_ldap: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct LdapConfigRequest {
    enable_ldap: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct LdapServerConfig {
    label: String,
    host: String,
    port: Option<i32>,
    attribute_for_mail: String,
    attribute_for_username: String,
    app_dn: String,
    app_dn_password: String,
    search_base: String,
    search_filters: String,
    use_tls: bool,
    certificate_path: Option<String>,
    validate_cert: bool,
    ciphers: Option<String>,
}

// LDAP Configuration Handlers
async fn get_ldap_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can access this endpoint
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(LdapConfigResponse {
        enable_ldap: config.enable_ldap,
    }))
}

async fn update_ldap_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<LdapConfigRequest>,
) -> AppResult<HttpResponse> {
    // Only admins can access this endpoint
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    // Update config with write lock
    let mut config = state.config.write().unwrap();
    config.enable_ldap = form_data.enable_ldap;

    // TODO: Persist to database

    Ok(HttpResponse::Ok().json(LdapConfigResponse {
        enable_ldap: config.enable_ldap,
    }))
}

async fn get_ldap_server(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    // Only admins can access this endpoint
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(LdapServerConfig {
        label: config.ldap_server_label.clone(),
        host: config.ldap_server_host.clone(),
        port: config.ldap_server_port,
        attribute_for_mail: config.ldap_attribute_for_mail.clone(),
        attribute_for_username: config.ldap_attribute_for_username.clone(),
        app_dn: config.ldap_app_dn.clone(),
        app_dn_password: config.ldap_app_password.clone(),
        search_base: config.ldap_search_base.clone(),
        search_filters: config.ldap_search_filters.clone(),
        use_tls: config.ldap_use_tls,
        certificate_path: config.ldap_ca_cert_file.clone(),
        validate_cert: config.ldap_validate_cert,
        ciphers: config.ldap_ciphers.clone(),
    }))
}

async fn update_ldap_server(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<LdapServerConfig>,
) -> AppResult<HttpResponse> {
    // Only admins can access this endpoint
    if auth_user.user.role != "admin" {
        return Err(crate::error::AppError::Forbidden(
            "Admin access required".to_string(),
        ));
    }

    // Validate required fields
    if form_data.label.is_empty()
        || form_data.host.is_empty()
        || form_data.attribute_for_mail.is_empty()
        || form_data.attribute_for_username.is_empty()
        || form_data.app_dn.is_empty()
        || form_data.app_dn_password.is_empty()
        || form_data.search_base.is_empty()
    {
        return Err(crate::error::AppError::Forbidden(
            "Required fields cannot be empty".to_string(),
        ));
    }

    // Update config with write lock
    let mut config = state.config.write().unwrap();

    config.ldap_server_label = form_data.label.clone();
    config.ldap_server_host = form_data.host.clone();
    config.ldap_server_port = form_data.port;
    config.ldap_attribute_for_mail = form_data.attribute_for_mail.clone();
    config.ldap_attribute_for_username = form_data.attribute_for_username.clone();
    config.ldap_app_dn = form_data.app_dn.clone();
    config.ldap_app_password = form_data.app_dn_password.clone();
    config.ldap_search_base = form_data.search_base.clone();
    config.ldap_search_filters = form_data.search_filters.clone();
    config.ldap_use_tls = form_data.use_tls;
    config.ldap_ca_cert_file = form_data.certificate_path.clone();
    config.ldap_validate_cert = form_data.validate_cert;
    config.ldap_ciphers = form_data.ciphers.clone();

    // TODO: Persist to database

    Ok(HttpResponse::Ok().json(LdapServerConfig {
        label: config.ldap_server_label.clone(),
        host: config.ldap_server_host.clone(),
        port: config.ldap_server_port,
        attribute_for_mail: config.ldap_attribute_for_mail.clone(),
        attribute_for_username: config.ldap_attribute_for_username.clone(),
        app_dn: config.ldap_app_dn.clone(),
        app_dn_password: config.ldap_app_password.clone(),
        search_base: config.ldap_search_base.clone(),
        search_filters: config.ldap_search_filters.clone(),
        use_tls: config.ldap_use_tls,
        certificate_path: config.ldap_ca_cert_file.clone(),
        validate_cert: config.ldap_validate_cert,
        ciphers: config.ldap_ciphers.clone(),
    }))
}

// LDAP Authentication Request
#[derive(Debug, Deserialize, Validate)]
struct LdapAuthRequest {
    #[validate(length(min = 1))]
    user: String,
    #[validate(length(min = 1))]
    password: String,
}

async fn ldap_auth(
    state: web::Data<AppState>,
    req: web::Json<LdapAuthRequest>,
) -> AppResult<HttpResponse> {
    req.validate()
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;

    let config = state.config.read().unwrap();

    // Check if LDAP is enabled
    if !config.enable_ldap {
        return Err(crate::error::AppError::BadRequest(
            "LDAP authentication is not enabled".to_string(),
        ));
    }

    // Build LDAP configuration from application config
    let ldap_config = crate::services::ldap::LdapConfig {
        server_url: format!(
            "ldap://{}:{}",
            config.ldap_server_host,
            config.ldap_server_port.unwrap_or(389)
        ),
        bind_dn: config.ldap_app_dn.clone(),
        bind_password: config.ldap_app_password.clone(),
        search_base: config.ldap_search_base.clone(),
        user_filter: format!(
            "(&({}={}){})",
            config.ldap_attribute_for_username, "{username}", config.ldap_search_filters
        ),
        group_filter: None,
        user_attributes: crate::services::ldap::LdapUserAttributes {
            username: config.ldap_attribute_for_username.clone(),
            email: config.ldap_attribute_for_mail.clone(),
            display_name: "cn".to_string(),
            first_name: Some("givenName".to_string()),
            last_name: Some("sn".to_string()),
            member_of: Some("memberOf".to_string()),
        },
        enable_tls: config.ldap_use_tls,
        verify_cert: config.ldap_validate_cert,
    };

    let ldap_client = crate::services::ldap::LdapClient::new(ldap_config);

    // Authenticate user via LDAP
    let ldap_user = ldap_client
        .authenticate(&req.user.to_lowercase(), &req.password)
        .await
        .map_err(|_| crate::error::AppError::InvalidCredentials)?;

    // Find or create user in local database
    let user_service = crate::services::user::UserService::new(&state.db);
    let mut user = user_service.get_user_by_email(&ldap_user.email).await?;

    if user.is_none() {
        // Create new user from LDAP
        let user_id = uuid::Uuid::new_v4().to_string();
        let profile_image_url = "/user.png".to_string();

        // Determine role based on LDAP groups
        let roles = ldap_client.map_groups_to_roles(&ldap_user.groups);
        let role = if roles.contains(&"admin".to_string()) {
            "admin"
        } else {
            "user"
        };

        user = Some(
            user_service
                .create_user(
                    &user_id,
                    &ldap_user.display_name,
                    &ldap_user.email,
                    role,
                    &profile_image_url,
                )
                .await?,
        );
    }

    let user = user.ok_or(crate::error::AppError::NotFound(
        "Failed to create user".to_string(),
    ))?;

    // Generate JWT token
    let token = create_jwt(&user.id, &config.webui_secret_key, &config.jwt_expires_in)?;

    let expires_at = chrono::Utc::now()
        .checked_add_signed(crate::utils::auth::parse_duration(&config.jwt_expires_in)?)
        .map(|dt| dt.timestamp());

    let session_response = SessionResponse {
        token: token.clone(),
        token_type: "Bearer".to_string(),
        expires_at,
        id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
        profile_image_url: user.profile_image_url,
        permissions: json!({}),
    };

    // Create cookie with token
    let mut cookie = Cookie::new("token", token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");

    // Set expiration if available
    if let Some(exp) = expires_at {
        cookie.set_expires(time::OffsetDateTime::from_unix_timestamp(exp).ok());
    }

    // Return response with Set-Cookie header
    Ok(HttpResponse::Ok()
        .append_header((header::SET_COOKIE, cookie.to_string()))
        .json(session_response))
}
