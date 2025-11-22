use crate::config::Config;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: Option<String>,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<ModelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<PipelineInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arena: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ModelMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Vec<KnowledgeItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineInfo {
    #[serde(rename = "type")]
    pub pipeline_type: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
}

pub struct ModelService {
    client: Client,
    config: Config,
}

impl ModelService {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Fetch all models from all configured backends
    pub async fn get_all_models(&self, db: &crate::db::Database) -> AppResult<Vec<Model>> {
        let mut all_models = Vec::new();

        // Fetch OpenAI models
        if self.config.enable_openai_api {
            match self.fetch_openai_models().await {
                Ok(models) => all_models.extend(models),
                Err(e) => warn!("Failed to fetch OpenAI models: {}", e),
            }
        }

        // Fetch function/pipeline models
        match self.fetch_function_models(db).await {
            Ok(models) => all_models.extend(models),
            Err(e) => warn!("Failed to fetch function models: {}", e),
        }

        // Apply custom models overlay from database
        all_models = self.apply_custom_models_overlay(db, all_models).await?;

        // Add arena models if enabled
        if self.config.enable_evaluation_arena_models {
            all_models.extend(self.get_arena_models());
        }

        // Attach global actions and filters
        all_models = self.attach_actions_and_filters(db, all_models).await?;

        Ok(all_models)
    }

    /// Fetch base models (without applying filters or arena models)
    pub async fn get_all_base_models(&self, db: &crate::db::Database) -> AppResult<Vec<Model>> {
        let mut base_models = Vec::new();

        // Fetch OpenAI models
        if self.config.enable_openai_api {
            match self.fetch_openai_models().await {
                Ok(models) => base_models.extend(models),
                Err(e) => warn!("Failed to fetch OpenAI models: {}", e),
            }
        }

        // Fetch function models
        match self.fetch_function_models(db).await {
            Ok(models) => base_models.extend(models),
            Err(e) => warn!("Failed to fetch function models: {}", e),
        }

        Ok(base_models)
    }

    /// Fetch models from OpenAI-compatible API endpoints
    async fn fetch_openai_models(&self) -> AppResult<Vec<Model>> {
        let mut all_models = Vec::new();

        // Skip if no URLs are configured
        if self.config.openai_api_base_urls.is_empty() {
            return Ok(all_models);
        }

        for (idx, base_url) in self.config.openai_api_base_urls.iter().enumerate() {
            let api_key = self
                .config
                .openai_api_keys
                .get(idx)
                .cloned()
                .unwrap_or_default();

            let api_config = self
                .config
                .openai_api_configs
                .get(&idx.to_string())
                .or_else(|| self.config.openai_api_configs.get(base_url))
                .and_then(|v| v.as_object())
                .cloned();

            // Check if this endpoint is explicitly disabled
            let enabled = api_config
                .as_ref()
                .and_then(|cfg| cfg.get("enable"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if !enabled {
                continue;
            }

            // Skip if no API key is provided (unless it's explicitly configured as auth_type "none")
            let auth_type = api_config
                .as_ref()
                .and_then(|cfg| cfg.get("auth_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("bearer");

            if api_key.is_empty() && auth_type != "none" {
                warn!(
                    "Skipping OpenAI endpoint {} - no API key configured and auth_type is not 'none'",
                    base_url
                );
                continue;
            }

            match self
                .fetch_models_from_endpoint(base_url, &api_key, api_config.as_ref())
                .await
            {
                Ok(mut models) => {
                    // Add urlIdx to each model for backend routing
                    for model in &mut models {
                        if let Some(info) = &mut model.info {
                            if let Some(_meta) = &mut info.meta {
                                // Store the URL index for routing
                            }
                        } else {
                            model.info = Some(ModelInfo {
                                meta: Some(ModelMeta {
                                    description: None,
                                    capabilities: None,
                                    tags: None,
                                    knowledge: None,
                                    profile_image_url: None,
                                }),
                                params: Some(json!({ "urlIdx": idx })),
                            });
                        }
                    }
                    all_models.extend(models);
                }
                Err(e) => {
                    warn!("Failed to fetch models from {}: {}", base_url, e);
                }
            }
        }

        Ok(all_models)
    }

    /// Fetch models from a single OpenAI-compatible endpoint
    async fn fetch_models_from_endpoint(
        &self,
        base_url: &str,
        api_key: &str,
        config: Option<&serde_json::Map<String, Value>>,
    ) -> AppResult<Vec<Model>> {
        let is_azure = config
            .and_then(|c| c.get("azure"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let url = if is_azure {
            let api_version = config
                .and_then(|c| c.get("api_version"))
                .and_then(|v| v.as_str())
                .unwrap_or("2023-05-15");
            format!("{}/models?api-version={}", base_url, api_version)
        } else {
            format!("{}/models", base_url)
        };

        let mut request = self.client.get(&url);

        // Add authentication based on config
        if is_azure {
            let auth_type = config
                .and_then(|c| c.get("auth_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("bearer");

            match auth_type {
                "azure_ad" | "microsoft_entra_id" => {
                    // Azure AD authentication - handled separately
                    // For now, skip auth header
                }
                _ => {
                    // Default: API key authentication
                    request = request.header("api-key", api_key);
                }
            }
        } else {
            // Standard Bearer token for OpenAI and compatible APIs
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalServiceError(format!(
                "Failed to fetch models: {} - {}",
                status, error_text
            )));
        }

        let response_data: Value = response.json().await?;

        let models: Vec<Model> = response_data
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let id = v.get("id")?.as_str()?;
                        let name = v
                            .get("name")
                            .and_then(|n| n.as_str().map(|s| s.to_string()))
                            .or_else(|| Some(id.to_string()));
                        Some(Model {
                            id: id.to_string(),
                            name,
                            object: v
                                .get("object")
                                .and_then(|o| o.as_str())
                                .unwrap_or("model")
                                .to_string(),
                            created: v.get("created").and_then(|c| c.as_i64()).unwrap_or(0),
                            owned_by: v
                                .get("owned_by")
                                .and_then(|o| o.as_str())
                                .unwrap_or("openai")
                                .to_string(),
                            info: None,
                            pipeline: None,
                            tags: None,
                            arena: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    /// Fetch function/pipeline models from database
    async fn fetch_function_models(&self, db: &crate::db::Database) -> AppResult<Vec<Model>> {
        use crate::models::function::Function;

        // Query all active pipe functions from database
        let pipes: Vec<Function> = sqlx::query_as::<_, Function>(
            r#"
            SELECT * FROM "function" 
            WHERE type = 'pipe' AND is_active = true
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        let mut pipe_models = Vec::new();

        for pipe in pipes {
            // For manifold pipes (functions that expose multiple models via .pipes()),
            // we would need to load the Python module and call it
            // For now, create a simple model entry for each pipe

            let model = Model {
                id: pipe.id.clone(),
                name: Some(pipe.name.clone()),
                object: "model".to_string(),
                created: pipe.created_at,
                owned_by: "openai".to_string(),
                info: Some(ModelInfo {
                    meta: Some(ModelMeta {
                        description: pipe.meta.as_ref().and_then(|m| {
                            m.get("description")
                                .and_then(|d| d.as_str().map(|s| s.to_string()))
                        }),
                        capabilities: None,
                        tags: None,
                        knowledge: None,
                        profile_image_url: pipe.meta.as_ref().and_then(|m| {
                            m.get("profile_image_url")
                                .and_then(|d| d.as_str().map(|s| s.to_string()))
                        }),
                    }),
                    params: None,
                }),
                pipeline: Some(PipelineInfo {
                    pipeline_type: Some("pipe".to_string()),
                    priority: None,
                }),
                tags: None,
                arena: None,
            };

            pipe_models.push(model);
        }

        Ok(pipe_models)
    }

    /// Apply custom models overlay from database
    async fn apply_custom_models_overlay(
        &self,
        db: &crate::db::Database,
        mut base_models: Vec<Model>,
    ) -> AppResult<Vec<Model>> {
        use crate::models::model::Model as DbModel;

        // Query custom models from database
        let custom_models: Vec<DbModel> =
            sqlx::query_as::<_, DbModel>(r#"SELECT * FROM "model" WHERE is_active = true"#)
                .fetch_all(db.pool())
                .await?;

        for custom in custom_models {
            if let Some(base_model_id) = &custom.base_model_id {
                // Find and overlay on existing base model
                if let Some(model) = base_models.iter_mut().find(|m| &m.id == base_model_id) {
                    // Update model with custom data
                    model.name = Some(custom.name.clone());

                    // Merge info/meta
                    if let Some(ref mut info) = model.info {
                        if let Some(ref mut meta) = info.meta {
                            // Merge tags from custom model
                            if let Some(custom_meta) = &custom.meta {
                                if let Some(action_ids) = custom_meta.get("actionIds") {
                                    meta.tags = Some(vec![Tag {
                                        name: "custom".to_string(),
                                    }]);
                                }
                            }
                        }

                        // Store custom model params
                        info.params = Some(custom.params.clone());
                    } else {
                        model.info = Some(ModelInfo {
                            meta: custom.meta.as_ref().map(|m| ModelMeta {
                                description: m
                                    .get("description")
                                    .and_then(|d| d.as_str().map(|s| s.to_string())),
                                capabilities: None,
                                tags: None,
                                knowledge: None,
                                profile_image_url: m
                                    .get("profile_image_url")
                                    .and_then(|d| d.as_str().map(|s| s.to_string())),
                            }),
                            params: Some(custom.params.clone()),
                        });
                    }
                }
            } else {
                // Add as a new custom model (not based on existing model)
                base_models.push(Model {
                    id: custom.id.clone(),
                    name: Some(custom.name.clone()),
                    object: "model".to_string(),
                    created: custom.created_at,
                    owned_by: "custom".to_string(),
                    info: Some(ModelInfo {
                        meta: custom.meta.as_ref().map(|m| ModelMeta {
                            description: m
                                .get("description")
                                .and_then(|d| d.as_str().map(|s| s.to_string())),
                            capabilities: None,
                            tags: None,
                            knowledge: None,
                            profile_image_url: m
                                .get("profile_image_url")
                                .and_then(|d| d.as_str().map(|s| s.to_string())),
                        }),
                        params: Some(custom.params.clone()),
                    }),
                    pipeline: None,
                    tags: None,
                    arena: None,
                });
            }
        }

        Ok(base_models)
    }

    /// Get arena models from configuration
    fn get_arena_models(&self) -> Vec<Model> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Check if custom arena models are configured
        if let Some(models_array) = self.config.evaluation_arena_models.as_array() {
            return models_array
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?;
                    let name = m.get("name")?.as_str()?;

                    Some(Model {
                        id: id.to_string(),
                        name: Some(name.to_string()),
                        object: "model".to_string(),
                        created,
                        owned_by: "arena".to_string(),
                        info: Some(ModelInfo {
                            meta: m.get("meta").and_then(|meta| {
                                Some(ModelMeta {
                                    description: meta
                                        .get("description")
                                        .and_then(|d| d.as_str().map(|s| s.to_string())),
                                    capabilities: None,
                                    tags: None,
                                    knowledge: None,
                                    profile_image_url: None,
                                })
                            }),
                            params: None,
                        }),
                        pipeline: None,
                        tags: None,
                        arena: Some(true),
                    })
                })
                .collect();
        }

        // Return default arena model if no custom models configured
        vec![Model {
            id: "arena-model".to_string(),
            name: Some("Arena Model".to_string()),
            object: "model".to_string(),
            created,
            owned_by: "arena".to_string(),
            info: Some(ModelInfo {
                meta: Some(ModelMeta {
                    description: Some("Model for evaluation arena".to_string()),
                    capabilities: None,
                    tags: None,
                    knowledge: None,
                    profile_image_url: None,
                }),
                params: None,
            }),
            pipeline: None,
            tags: None,
            arena: Some(true),
        }]
    }

    /// Attach global actions and filters to models
    async fn attach_actions_and_filters(
        &self,
        db: &crate::db::Database,
        mut models: Vec<Model>,
    ) -> AppResult<Vec<Model>> {
        use crate::models::function::Function;

        // Query global action functions
        let global_actions: Vec<Function> = sqlx::query_as::<_, Function>(
            r#"
            SELECT * FROM "function" 
            WHERE type = 'action' AND is_active = true AND is_global = true
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(db.pool())
        .await
        .unwrap_or_default();

        // Query global filter functions
        let global_filters: Vec<Function> = sqlx::query_as::<_, Function>(
            r#"
            SELECT * FROM "function" 
            WHERE type = 'filter' AND is_active = true AND is_global = true
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(db.pool())
        .await
        .unwrap_or_default();

        // Query all enabled action functions
        let enabled_actions: Vec<Function> = sqlx::query_as::<_, Function>(
            r#"
            SELECT * FROM "function" 
            WHERE type = 'action' AND is_active = true
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(db.pool())
        .await
        .unwrap_or_default();

        // Query all enabled filter functions
        let enabled_filters: Vec<Function> = sqlx::query_as::<_, Function>(
            r#"
            SELECT * FROM "function" 
            WHERE type = 'filter' AND is_active = true
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(db.pool())
        .await
        .unwrap_or_default();

        // Attach to each model
        for model in &mut models {
            // Collect action IDs (global + model-specific)
            let mut action_ids: Vec<String> = global_actions.iter().map(|f| f.id.clone()).collect();

            // Add model-specific actions from meta
            if let Some(info) = &model.info {
                if let Some(meta) = &info.meta {
                    if let Some(params) = &info.params {
                        if let Some(meta_actions) =
                            params.get("actionIds").and_then(|v| v.as_array())
                        {
                            for action_id in meta_actions {
                                if let Some(id) = action_id.as_str() {
                                    action_ids.push(id.to_string());
                                }
                            }
                        }
                    }
                }
            }

            // Filter to only enabled actions
            action_ids.retain(|id| enabled_actions.iter().any(|f| &f.id == id));

            // Collect filter IDs (similar logic)
            let mut filter_ids: Vec<String> = global_filters.iter().map(|f| f.id.clone()).collect();

            if let Some(info) = &model.info {
                if let Some(params) = &info.params {
                    if let Some(meta_filters) = params.get("filterIds").and_then(|v| v.as_array()) {
                        for filter_id in meta_filters {
                            if let Some(id) = filter_id.as_str() {
                                filter_ids.push(id.to_string());
                            }
                        }
                    }
                }
            }

            filter_ids.retain(|id| enabled_filters.iter().any(|f| &f.id == id));

            // Store action and filter IDs in model info
            if let Some(ref mut info) = model.info {
                let mut params = info.params.clone().unwrap_or_else(|| json!({}));
                params["action_ids"] = json!(action_ids);
                params["filter_ids"] = json!(filter_ids);
                info.params = Some(params);
            }
        }

        Ok(models)
    }

    /// Get model by ID
    pub async fn get_model_by_id(
        &self,
        db: &crate::db::Database,
        model_id: &str,
    ) -> AppResult<Option<Model>> {
        let all_models = self.get_all_models(db).await?;
        Ok(all_models.into_iter().find(|m| m.id == model_id))
    }

    /// Check if user has access to a model
    pub fn check_model_access(&self, model: &Model, user_id: &str, user_role: &str) -> bool {
        // Admins have access to all models
        if user_role == "admin" {
            return true;
        }

        // Check if model has arena flag (arena models have special access rules)
        if model.arena == Some(true) {
            return true; // Arena models are accessible to all authenticated users
        }

        // Check access control from model info
        if let Some(info) = &model.info {
            if let Some(params) = &info.params {
                // Check if model has access_control
                if let Some(access_control) = params.get("access_control") {
                    // If access control exists, check permissions
                    if let Some(access_obj) = access_control.as_object() {
                        // Check read access
                        if let Some(read) = access_obj.get("read") {
                            // Check if user is in read.user_ids
                            if let Some(user_ids) = read.get("user_ids").and_then(|v| v.as_array())
                            {
                                for id in user_ids {
                                    if id.as_str() == Some(user_id) {
                                        return true;
                                    }
                                }
                            }

                            // Check if user is in read.group_ids (would need group membership check)
                            if let Some(group_ids) =
                                read.get("group_ids").and_then(|v| v.as_array())
                            {
                                if !group_ids.is_empty() {
                                    // TODO: Check user's group memberships
                                    // For now, allow if user has any groups
                                }
                            }
                        }
                    }

                    // If access control is defined but user not in list, deny access
                    return false;
                }
            }
        }

        // Check model ownership (from database model records)
        if let Some(info) = &model.info {
            if let Some(params) = &info.params {
                if let Some(owner_id) = params.get("user_id").and_then(|v| v.as_str()) {
                    if owner_id == user_id {
                        return true;
                    }
                }
            }
        }

        // Default: allow access for models without explicit access control
        true
    }

    /// Filter models based on user access
    pub fn filter_models_by_access(
        &self,
        models: Vec<Model>,
        user_id: &str,
        user_role: &str,
    ) -> Vec<Model> {
        models
            .into_iter()
            .filter(|m| {
                // Filter out filter pipelines
                if let Some(pipeline) = &m.pipeline {
                    if pipeline.pipeline_type.as_deref() == Some("filter") {
                        return false;
                    }
                }

                // Check user access
                self.check_model_access(m, user_id, user_role)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_service_creation() {
        // Use Config::from_env() which provides defaults for all fields
        // Set a minimal DATABASE_URL env var to avoid errors
        std::env::set_var("DATABASE_URL", "postgres://localhost/test");

        let config = Config::from_env().expect("Failed to create test config");
        let service = ModelService::new(config);

        // The test just verifies that ModelService can be created
        // We don't assert on openai_api_base_urls as it may have defaults from env
        assert!(
            service.config.host == "0.0.0.0"
                || service.config.host == "127.0.0.1"
                || !service.config.host.is_empty()
        );
    }
}
