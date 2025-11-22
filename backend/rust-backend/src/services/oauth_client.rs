use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientInfo {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub issuer: Option<String>,
    pub scope: Option<String>,
    pub auth_url: Option<String>,
    pub token_url: Option<String>,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// OAuth Client Manager for MCP OAuth 2.1 and other integrations
/// Manages OAuth clients for connecting to external services
pub struct OAuthClientManager {
    clients: Arc<RwLock<HashMap<String, OAuthClientInfo>>>,
    tokens: Arc<RwLock<HashMap<String, HashMap<String, OAuthToken>>>>, // user_id -> client_id -> token
    client: Client,
    db: Option<Database>,
}

impl OAuthClientManager {
    pub fn new(db: Option<Database>) -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            client: Client::new(),
            db,
        }
    }

    /// Register a new OAuth client
    pub async fn add_client(
        &self,
        client_id: String,
        client_info: OAuthClientInfo,
    ) -> AppResult<()> {
        self.clients
            .write()
            .await
            .insert(client_id.clone(), client_info);
        info!("Registered OAuth client: {}", client_id);
        Ok(())
    }

    /// Remove an OAuth client
    pub async fn remove_client(&self, client_id: &str) -> AppResult<()> {
        self.clients.write().await.remove(client_id);
        info!("Removed OAuth client: {}", client_id);
        Ok(())
    }

    /// Get OAuth client info
    pub async fn get_client(&self, client_id: &str) -> Option<OAuthClientInfo> {
        self.clients.read().await.get(client_id).cloned()
    }

    /// Get OAuth token for a user and client
    pub async fn get_oauth_token(
        &self,
        user_id: &str,
        client_id: &str,
        force_refresh: bool,
    ) -> AppResult<Option<OAuthToken>> {
        let tokens = self.tokens.read().await;

        if let Some(user_tokens) = tokens.get(user_id) {
            if let Some(token) = user_tokens.get(client_id) {
                // Check if token is expired
                if !force_refresh && !self.is_token_expired(token) {
                    return Ok(Some(token.clone()));
                }

                // Try to refresh the token
                if let Some(refresh_token) = token.refresh_token.clone() {
                    drop(tokens); // Release read lock before write
                    return match self
                        .refresh_access_token(user_id, client_id, &refresh_token)
                        .await
                    {
                        Ok(new_token) => Ok(Some(new_token)),
                        Err(e) => {
                            warn!("Failed to refresh token: {}", e);
                            Ok(None)
                        }
                    };
                }
            }
        }

        Ok(None)
    }

    /// Store OAuth token for a user and client
    pub async fn store_token(
        &self,
        user_id: &str,
        client_id: &str,
        token: OAuthToken,
    ) -> AppResult<()> {
        let mut tokens = self.tokens.write().await;
        tokens
            .entry(user_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(client_id.to_string(), token);

        debug!(
            "Stored OAuth token for user {} and client {}",
            user_id, client_id
        );
        Ok(())
    }

    /// Check if token is expired
    fn is_token_expired(&self, _token: &OAuthToken) -> bool {
        // TODO: Implement actual expiration checking
        // For now, assume not expired
        false
    }

    /// Refresh access token using refresh token
    async fn refresh_access_token(
        &self,
        user_id: &str,
        client_id: &str,
        refresh_token: &str,
    ) -> AppResult<OAuthToken> {
        let client_info = self
            .get_client(client_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("OAuth client not found: {}", client_id)))?;

        let token_url = client_info
            .token_url
            .ok_or_else(|| AppError::BadRequest("No token URL configured".to_string()))?;

        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ];

        if let Some(client_secret) = &client_info.client_secret {
            params.push(("client_id", &client_info.client_id));
            params.push(("client_secret", client_secret));
        }

        let response = self.client.post(&token_url).form(&params).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalServiceError(format!(
                "Token refresh failed: {} - {}",
                status, error_text
            )));
        }

        let new_token: OAuthToken = response.json().await?;

        // Store the new token
        self.store_token(user_id, client_id, new_token.clone())
            .await?;

        info!(
            "Refreshed OAuth token for user {} and client {}",
            user_id, client_id
        );

        Ok(new_token)
    }

    /// Generate authorization URL
    pub async fn get_authorization_url(
        &self,
        client_id: &str,
        redirect_uri: Option<&str>,
        state: Option<&str>,
        scope: Option<&str>,
    ) -> AppResult<String> {
        let client_info = self
            .get_client(client_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("OAuth client not found: {}", client_id)))?;

        let auth_url = client_info
            .auth_url
            .ok_or_else(|| AppError::BadRequest("No auth URL configured".to_string()))?;

        let redirect = redirect_uri
            .or(client_info.redirect_uri.as_deref())
            .ok_or_else(|| AppError::BadRequest("No redirect URI configured".to_string()))?;

        let scope_param = scope
            .or(client_info.scope.as_deref())
            .unwrap_or("openid profile email");

        let mut url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}",
            auth_url,
            urlencoding::encode(&client_info.client_id),
            urlencoding::encode(redirect),
            urlencoding::encode(scope_param)
        );

        if let Some(state) = state {
            url.push_str(&format!("&state={}", urlencoding::encode(state)));
        }

        Ok(url)
    }

    /// Exchange authorization code for access token
    pub async fn exchange_code_for_token(
        &self,
        user_id: &str,
        client_id: &str,
        code: &str,
        redirect_uri: Option<&str>,
    ) -> AppResult<OAuthToken> {
        let client_info = self
            .get_client(client_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("OAuth client not found: {}", client_id)))?;

        let token_url = client_info
            .token_url
            .ok_or_else(|| AppError::BadRequest("No token URL configured".to_string()))?;

        let redirect = redirect_uri
            .or(client_info.redirect_uri.as_deref())
            .ok_or_else(|| AppError::BadRequest("No redirect URI configured".to_string()))?;

        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", redirect.to_string()),
            ("client_id", client_info.client_id.clone()),
        ];

        if let Some(client_secret) = &client_info.client_secret {
            params.push(("client_secret", client_secret.clone()));
        }

        let response = self.client.post(&token_url).form(&params).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalServiceError(format!(
                "Token exchange failed: {} - {}",
                status, error_text
            )));
        }

        let token: OAuthToken = response.json().await?;

        // Store the token
        self.store_token(user_id, client_id, token.clone()).await?;

        info!(
            "Exchanged code for token for user {} and client {}",
            user_id, client_id
        );

        Ok(token)
    }

    /// Revoke OAuth token
    pub async fn revoke_token(&self, user_id: &str, client_id: &str) -> AppResult<()> {
        let mut tokens = self.tokens.write().await;

        if let Some(user_tokens) = tokens.get_mut(user_id) {
            user_tokens.remove(client_id);

            if user_tokens.is_empty() {
                tokens.remove(user_id);
            }
        }

        info!(
            "Revoked OAuth token for user {} and client {}",
            user_id, client_id
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_oauth_client_manager() {
        let manager = OAuthClientManager::new(None);

        let client_info = OAuthClientInfo {
            client_id: "test-client".to_string(),
            client_secret: Some("secret".to_string()),
            issuer: Some("https://auth.example.com".to_string()),
            scope: Some("openid profile".to_string()),
            auth_url: Some("https://auth.example.com/authorize".to_string()),
            token_url: Some("https://auth.example.com/token".to_string()),
            redirect_uri: Some("https://app.example.com/callback".to_string()),
        };

        manager
            .add_client("test-client".to_string(), client_info.clone())
            .await
            .unwrap();

        let retrieved = manager.get_client("test-client").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().client_id, "test-client");
    }

    #[tokio::test]
    async fn test_token_storage() {
        let manager = OAuthClientManager::new(None);

        let token = OAuthToken {
            access_token: "test-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: Some("refresh-token".to_string()),
            scope: Some("openid profile".to_string()),
        };

        manager
            .store_token("user1", "client1", token.clone())
            .await
            .unwrap();

        let retrieved = manager
            .get_oauth_token("user1", "client1", false)
            .await
            .unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().access_token, "test-token");
    }
}
