use crate::error::{AppError, AppResult};
use crate::models::Claims;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

pub fn create_jwt(user_id: &str, secret: &str, expires_in: &str) -> AppResult<String> {
    let expiration = parse_duration(expires_in)?;
    let exp = Utc::now()
        .checked_add_signed(expiration)
        .ok_or_else(|| AppError::InternalServerError("Invalid expiration time".to_string()))?
        .timestamp();

    let claims = Claims {
        sub: user_id.to_string(),
        exp: Some(exp),
        iat: Some(Utc::now().timestamp()),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

pub fn verify_jwt(token: &str, secret: &str) -> AppResult<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}

pub fn parse_duration(duration_str: &str) -> AppResult<Duration> {
    let duration_str = duration_str.trim();

    if let Some(hours) = duration_str.strip_suffix('h') {
        let hours: i64 = hours
            .parse()
            .map_err(|_| AppError::BadRequest("Invalid duration format".to_string()))?;
        Ok(Duration::hours(hours))
    } else if let Some(days) = duration_str.strip_suffix('d') {
        let days: i64 = days
            .parse()
            .map_err(|_| AppError::BadRequest("Invalid duration format".to_string()))?;
        Ok(Duration::days(days))
    } else if let Some(minutes) = duration_str.strip_suffix('m') {
        let minutes: i64 = minutes
            .parse()
            .map_err(|_| AppError::BadRequest("Invalid duration format".to_string()))?;
        Ok(Duration::minutes(minutes))
    } else {
        // Default to hours
        let hours: i64 = duration_str
            .parse()
            .map_err(|_| AppError::BadRequest("Invalid duration format".to_string()))?;
        Ok(Duration::hours(hours))
    }
}

#[allow(dead_code)]
pub fn extract_bearer_token(auth_header: &str) -> Option<String> {
    if auth_header.starts_with("Bearer ") {
        Some(auth_header[7..].to_string())
    } else {
        None
    }
}
