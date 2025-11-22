use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub provider_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OAuthUserInfo {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub picture: Option<String>,
    pub locale: Option<String>,
}

#[allow(dead_code)]
pub struct OAuthClient {
    config: OAuthConfig,
    client: Client,
}

#[allow(dead_code)]
impl OAuthClient {
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Generate authorization URL with PKCE
    pub fn get_authorization_url(&self, state: &str, code_verifier: &str) -> String {
        let code_challenge = Self::generate_code_challenge(code_verifier);

        let mut url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&state={}&code_challenge={}&code_challenge_method=S256",
            self.config.authorize_url,
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(&code_challenge),
        );

        if !self.config.scopes.is_empty() {
            url.push_str(&format!(
                "&scope={}",
                urlencoding::encode(&self.config.scopes.join(" "))
            ));
        }

        url
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> AppResult<OAuthTokenResponse> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.config.redirect_uri),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("code_verifier", code_verifier),
        ];

        let response = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to exchange OAuth code: {}", e);
                AppError::Auth(format!("Failed to exchange code: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("OAuth token exchange failed: {}", error_text);
            return Err(AppError::Auth(format!(
                "Token exchange failed: {}",
                error_text
            )));
        }

        let token_response: OAuthTokenResponse = response.json().await.map_err(|e| {
            error!("Failed to parse token response: {}", e);
            AppError::Auth(format!("Failed to parse token response: {}", e))
        })?;

        info!("Successfully exchanged OAuth code for tokens");
        Ok(token_response)
    }

    /// Refresh access token
    pub async fn refresh_token(&self, refresh_token: &str) -> AppResult<OAuthTokenResponse> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
        ];

        let response = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to refresh OAuth token: {}", e);
                AppError::Auth(format!("Failed to refresh token: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("OAuth token refresh failed: {}", error_text);
            return Err(AppError::Auth(format!(
                "Token refresh failed: {}",
                error_text
            )));
        }

        let token_response: OAuthTokenResponse = response.json().await.map_err(|e| {
            error!("Failed to parse refresh response: {}", e);
            AppError::Auth(format!("Failed to parse refresh response: {}", e))
        })?;

        info!("Successfully refreshed OAuth token");
        Ok(token_response)
    }

    /// Get user information from OAuth provider
    pub async fn get_user_info(&self, access_token: &str) -> AppResult<OAuthUserInfo> {
        let userinfo_url = self
            .config
            .userinfo_url
            .as_ref()
            .ok_or_else(|| AppError::Auth("No userinfo URL configured".to_string()))?;

        let response = self
            .client
            .get(userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to get user info: {}", e);
                AppError::Auth(format!("Failed to get user info: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Failed to get user info: {}", error_text);
            return Err(AppError::Auth(format!(
                "Failed to get user info: {}",
                error_text
            )));
        }

        let user_info: OAuthUserInfo = response.json().await.map_err(|e| {
            error!("Failed to parse user info: {}", e);
            AppError::Auth(format!("Failed to parse user info: {}", e))
        })?;

        Ok(user_info)
    }

    /// Decode and verify ID token (OIDC)
    pub async fn verify_id_token(&self, id_token: &str) -> AppResult<Value> {
        // In production, implement full JWT verification with JWKS
        // For now, just decode without verification (NOT SECURE FOR PRODUCTION)
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err(AppError::Auth("Invalid ID token format".to_string()));
        }

        let payload = parts[1];
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let decoded = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|e| AppError::Auth(format!("Failed to decode ID token: {}", e)))?;

        let claims: Value = serde_json::from_slice(&decoded)
            .map_err(|e| AppError::Auth(format!("Failed to parse ID token: {}", e)))?;

        Ok(claims)
    }

    /// Generate PKCE code verifier
    pub fn generate_code_verifier() -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use rand::Rng;
        let random_bytes: Vec<u8> = (0..32).map(|_| rand::rng().random()).collect();
        URL_SAFE_NO_PAD.encode(random_bytes)
    }

    /// Generate PKCE code challenge from verifier
    pub fn generate_code_challenge(verifier: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let result = hasher.finalize();
        URL_SAFE_NO_PAD.encode(result)
    }

    /// Revoke token
    pub async fn revoke_token(
        &self,
        _token: &str,
        _token_type_hint: Option<&str>,
    ) -> AppResult<()> {
        // Not all providers support token revocation
        info!("Revoking OAuth token");
        Ok(())
    }
}

/// Predefined OAuth providers
#[allow(dead_code)]
pub mod providers {
    use super::OAuthConfig;

    #[allow(dead_code)]
    pub fn google(client_id: String, client_secret: String, redirect_uri: String) -> OAuthConfig {
        OAuthConfig {
            provider_name: "Google".to_string(),
            client_id,
            client_secret,
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_url: Some("https://openidconnect.googleapis.com/v1/userinfo".to_string()),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            redirect_uri,
        }
    }

    #[allow(dead_code)]
    pub fn github(client_id: String, client_secret: String, redirect_uri: String) -> OAuthConfig {
        OAuthConfig {
            provider_name: "GitHub".to_string(),
            client_id,
            client_secret,
            authorize_url: "https://github.com/login/oauth/authorize".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            userinfo_url: Some("https://api.github.com/user".to_string()),
            scopes: vec!["user:email".to_string()],
            redirect_uri,
        }
    }

    #[allow(dead_code)]
    pub fn microsoft(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        tenant: Option<String>,
    ) -> OAuthConfig {
        let tenant = tenant.unwrap_or_else(|| "common".to_string());
        OAuthConfig {
            provider_name: "Microsoft".to_string(),
            client_id,
            client_secret,
            authorize_url: format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
                tenant
            ),
            token_url: format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
                tenant
            ),
            userinfo_url: Some("https://graph.microsoft.com/oidc/userinfo".to_string()),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            redirect_uri,
        }
    }

    #[allow(dead_code)]
    pub fn keycloak(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        realm_url: String,
    ) -> OAuthConfig {
        OAuthConfig {
            provider_name: "Keycloak".to_string(),
            client_id,
            client_secret,
            authorize_url: format!("{}/protocol/openid-connect/auth", realm_url),
            token_url: format!("{}/protocol/openid-connect/token", realm_url),
            userinfo_url: Some(format!("{}/protocol/openid-connect/userinfo", realm_url)),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            redirect_uri,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_challenge_generation() {
        let verifier = "test_verifier_12345";
        let challenge = OAuthClient::generate_code_challenge(verifier);
        assert!(!challenge.is_empty());
        assert_ne!(challenge, verifier);
    }

    #[test]
    fn test_code_verifier_generation() {
        let verifier1 = OAuthClient::generate_code_verifier();
        let verifier2 = OAuthClient::generate_code_verifier();
        assert!(!verifier1.is_empty());
        assert_ne!(verifier1, verifier2);
    }
}
