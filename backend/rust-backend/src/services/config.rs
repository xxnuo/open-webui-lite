use crate::{config::Config, db::Database, error::AppError, models::config::ConfigModel};
use serde_json::json;

/// Service for handling configuration persistence
pub struct ConfigService;

impl ConfigService {
    /// Load configuration from database and merge with environment config
    pub async fn load_from_db(db: &Database, mut config: Config) -> Result<Config, AppError> {
        // Try to load config from database
        match Self::get_latest_config(db).await {
            Ok(Some(config_model)) => {
                // Merge database config with environment config
                Self::merge_config(&mut config, &config_model.data);
                tracing::info!("Configuration loaded from database");
                Ok(config)
            }
            Ok(None) => {
                // No config in database, save current config
                tracing::info!("No configuration in database, initializing with defaults");
                Self::save_to_db(db, &config).await?;
                Ok(config)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to load config from database: {}, using environment config",
                    e
                );
                Ok(config)
            }
        }
    }

    /// Get the latest configuration from database
    pub async fn get_latest_config(db: &Database) -> Result<Option<ConfigModel>, AppError> {
        let result = sqlx::query_as::<_, ConfigModel>(
            "SELECT id, data, version, created_at, updated_at FROM config ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(db.pool())
        .await
        .map_err(|e| AppError::Database(e))?;

        Ok(result)
    }

    /// Save entire configuration to database
    pub async fn save_to_db(db: &Database, config: &Config) -> Result<(), AppError> {
        let config_json = Self::config_to_json(config);
        let config_json_str =
            serde_json::to_string(&config_json).unwrap_or_else(|_| "{}".to_string());

        // Try to update existing config, or insert new one
        let existing = Self::get_latest_config(db).await?;
        let now = crate::utils::time::current_timestamp_seconds();

        if let Some(existing_config) = existing {
            // Update existing config
            sqlx::query("UPDATE config SET data = $1, updated_at = $2 WHERE id = $3")
                .bind(&config_json_str)
                .bind(now)
                .bind(existing_config.id)
                .execute(db.pool())
                .await
                .map_err(|e| AppError::Database(e))?;
        } else {
            // Insert new config
            sqlx::query(
                "INSERT INTO config (data, version, created_at, updated_at) VALUES ($1, 0, $2, $3)",
            )
            .bind(&config_json_str)
            .bind(now)
            .bind(now)
            .execute(db.pool())
            .await
            .map_err(|e| AppError::Database(e))?;
        }

        tracing::info!("Configuration saved to database");
        Ok(())
    }

    /// Update specific configuration sections in database
    pub async fn update_section(
        db: &Database,
        section: &str,
        value: serde_json::Value,
    ) -> Result<(), AppError> {
        // Get existing config or create empty one
        let existing = Self::get_latest_config(db).await?;

        let config_json = if let Some(existing_config) = existing {
            // Merge with existing data
            let mut data = existing_config.data;
            if let Some(obj) = data.as_object_mut() {
                obj.insert(section.to_string(), value.clone());
            }
            data
        } else {
            // Create new config with just this section
            json!({ section: value.clone() })
        };

        // Upsert the config
        if let Ok(Some(existing_config)) = Self::get_latest_config(db).await {
            let now = crate::utils::time::current_timestamp_seconds();
            let data_str = serde_json::to_string(&config_json).unwrap_or_else(|_| "{}".to_string());
            sqlx::query("UPDATE config SET data = $1, updated_at = $2 WHERE id = $3")
                .bind(&data_str)
                .bind(now)
                .bind(existing_config.id)
                .execute(db.pool())
                .await
                .map_err(|e| AppError::Database(e))?;
        } else {
            let now = crate::utils::time::current_timestamp_seconds();
            let data_str = serde_json::to_string(&config_json).unwrap_or_else(|_| "{}".to_string());
            sqlx::query(
                "INSERT INTO config (data, version, created_at, updated_at) VALUES ($1, 0, $2, $3)",
            )
            .bind(&data_str)
            .bind(now)
            .bind(now)
            .execute(db.pool())
            .await
            .map_err(|e| AppError::Database(e))?;
        }

        Ok(())
    }

    /// Convert Config struct to JSON for database storage
    fn config_to_json(config: &Config) -> serde_json::Value {
        json!({
            "direct": {
                "enable": config.enable_direct_connections
            },
            "connections": {
                "enable_direct_connections": config.enable_direct_connections,
                "enable_base_models_cache": config.enable_base_models_cache
            },
            "openai": {
                "enable": config.enable_openai_api,
                "api_keys": config.openai_api_keys,
                "api_base_urls": config.openai_api_base_urls,
                "api_configs": config.openai_api_configs
            },
            "features": {
                "enable_channels": config.enable_channels,
                "enable_notes": config.enable_notes,
                "enable_image_generation": config.enable_image_generation,
                "enable_code_execution": config.enable_code_execution,
                "enable_code_interpreter": config.enable_code_interpreter,
                "enable_web_search": config.enable_web_search,
                "enable_admin_chat_access": config.enable_admin_chat_access,
                "enable_admin_export": config.enable_admin_export,
                "enable_community_sharing": config.enable_community_sharing,
                "enable_message_rating": config.enable_message_rating
            },
            "models": {
                "default_models": config.default_models,
                "model_order_list": config.model_order_list
            },
            "code_execution": {
                "engine": config.code_execution_engine,
                "jupyter_url": config.code_execution_jupyter_url,
                "jupyter_auth": config.code_execution_jupyter_auth,
                "jupyter_auth_token": config.code_execution_jupyter_auth_token,
                "jupyter_auth_password": config.code_execution_jupyter_auth_password,
                "jupyter_timeout": config.code_execution_jupyter_timeout,
                "sandbox_url": config.code_execution_sandbox_url,
                "sandbox_timeout": config.code_execution_sandbox_timeout
            },
            "code_interpreter": {
                "engine": config.code_interpreter_engine,
                "prompt_template": config.code_interpreter_prompt_template,
                "jupyter_url": config.code_interpreter_jupyter_url,
                "jupyter_auth": config.code_interpreter_jupyter_auth,
                "jupyter_auth_token": config.code_interpreter_jupyter_auth_token,
                "jupyter_auth_password": config.code_interpreter_jupyter_auth_password,
                "jupyter_timeout": config.code_interpreter_jupyter_timeout,
                "sandbox_url": config.code_interpreter_sandbox_url,
                "sandbox_timeout": config.code_interpreter_sandbox_timeout
            },
            "ui": {
                "banners": config.banners,
                "default_prompt_suggestions": config.default_prompt_suggestions
            },
            "tool_servers": {
                "connections": config.tool_server_connections
            }
        })
    }

    /// Merge database config into the runtime config
    fn merge_config(config: &mut Config, db_data: &serde_json::Value) {
        // Helper to safely get values from nested JSON
        let get_bool = |path: &[&str], default: bool| -> bool {
            let mut current = db_data;
            for key in path {
                if let Some(obj) = current.get(key) {
                    current = obj;
                } else {
                    return default;
                }
            }
            current.as_bool().unwrap_or(default)
        };

        let get_string = |path: &[&str], default: String| -> String {
            let mut current = db_data;
            for key in path {
                if let Some(obj) = current.get(key) {
                    current = obj;
                } else {
                    return default;
                }
            }
            current.as_str().unwrap_or(&default).to_string()
        };

        let get_vec_string = |path: &[&str], default: Vec<String>| -> Vec<String> {
            let mut current = db_data;
            for key in path {
                if let Some(obj) = current.get(key) {
                    current = obj;
                } else {
                    return default;
                }
            }
            current
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or(default)
        };

        let get_json = |path: &[&str], default: serde_json::Value| -> serde_json::Value {
            let mut current = db_data;
            for key in path {
                if let Some(obj) = current.get(key) {
                    current = obj;
                } else {
                    return default;
                }
            }
            current.clone()
        };

        let get_option_string = |path: &[&str]| -> Option<String> {
            let mut current = db_data;
            for key in path {
                if let Some(obj) = current.get(key) {
                    current = obj;
                } else {
                    return None;
                }
            }
            current.as_str().map(|s| s.to_string())
        };

        let get_option_i32 = |path: &[&str]| -> Option<i32> {
            let mut current = db_data;
            for key in path {
                if let Some(obj) = current.get(key) {
                    current = obj;
                } else {
                    return None;
                }
            }
            current.as_i64().map(|i| i as i32)
        };

        // Merge Direct Connections
        config.enable_direct_connections =
            get_bool(&["direct", "enable"], config.enable_direct_connections);
        config.enable_direct_connections = get_bool(
            &["connections", "enable_direct_connections"],
            config.enable_direct_connections,
        );
        config.enable_base_models_cache = get_bool(
            &["connections", "enable_base_models_cache"],
            config.enable_base_models_cache,
        );

        // Merge OpenAI config
        config.enable_openai_api = get_bool(&["openai", "enable"], config.enable_openai_api);
        config.openai_api_keys =
            get_vec_string(&["openai", "api_keys"], config.openai_api_keys.clone());
        config.openai_api_base_urls = get_vec_string(
            &["openai", "api_base_urls"],
            config.openai_api_base_urls.clone(),
        );
        config.openai_api_configs = get_json(
            &["openai", "api_configs"],
            config.openai_api_configs.clone(),
        );

        // Merge Admin config
        config.show_admin_details =
            get_bool(&["admin", "show_admin_details"], config.show_admin_details);
        config.webui_url = get_string(&["admin", "webui_url"], config.webui_url.clone());
        config.enable_signup = get_bool(&["admin", "enable_signup"], config.enable_signup);
        config.enable_api_key = get_bool(&["admin", "enable_api_key"], config.enable_api_key);
        config.enable_api_key_endpoint_restrictions = get_bool(
            &["admin", "enable_api_key_endpoint_restrictions"],
            config.enable_api_key_endpoint_restrictions,
        );
        config.api_key_allowed_endpoints = get_string(
            &["admin", "api_key_allowed_endpoints"],
            config.api_key_allowed_endpoints.clone(),
        );
        config.default_user_role = get_string(
            &["admin", "default_user_role"],
            config.default_user_role.clone(),
        );
        config.jwt_expires_in =
            get_string(&["admin", "jwt_expires_in"], config.jwt_expires_in.clone());
        config.enable_user_webhooks = get_bool(
            &["admin", "enable_user_webhooks"],
            config.enable_user_webhooks,
        );
        config.pending_user_overlay_title =
            get_option_string(&["admin", "pending_user_overlay_title"])
                .or(config.pending_user_overlay_title.clone());
        config.pending_user_overlay_content =
            get_option_string(&["admin", "pending_user_overlay_content"])
                .or(config.pending_user_overlay_content.clone());
        config.response_watermark = get_option_string(&["admin", "response_watermark"])
            .or(config.response_watermark.clone());

        // Merge Features (admin settings override features)
        config.enable_channels = get_bool(
            &["admin", "enable_channels"],
            get_bool(&["features", "enable_channels"], config.enable_channels),
        );
        config.enable_notes = get_bool(
            &["admin", "enable_notes"],
            get_bool(&["features", "enable_notes"], config.enable_notes),
        );
        config.enable_community_sharing = get_bool(
            &["admin", "enable_community_sharing"],
            get_bool(
                &["features", "enable_community_sharing"],
                config.enable_community_sharing,
            ),
        );
        config.enable_message_rating = get_bool(
            &["admin", "enable_message_rating"],
            get_bool(
                &["features", "enable_message_rating"],
                config.enable_message_rating,
            ),
        );

        config.enable_image_generation = get_bool(
            &["features", "enable_image_generation"],
            config.enable_image_generation,
        );
        config.enable_code_execution = get_bool(
            &["features", "enable_code_execution"],
            config.enable_code_execution,
        );
        config.enable_code_interpreter = get_bool(
            &["features", "enable_code_interpreter"],
            config.enable_code_interpreter,
        );
        config.enable_web_search =
            get_bool(&["features", "enable_web_search"], config.enable_web_search);
        config.enable_admin_chat_access = get_bool(
            &["features", "enable_admin_chat_access"],
            config.enable_admin_chat_access,
        );
        config.enable_admin_export = get_bool(
            &["features", "enable_admin_export"],
            config.enable_admin_export,
        );

        // Merge Models
        config.default_models =
            get_string(&["models", "default_models"], config.default_models.clone());
        config.model_order_list = get_vec_string(
            &["models", "model_order_list"],
            config.model_order_list.clone(),
        );

        // Merge Code Execution
        config.code_execution_engine = get_string(
            &["code_execution", "engine"],
            config.code_execution_engine.clone(),
        );
        config.code_execution_jupyter_url = get_option_string(&["code_execution", "jupyter_url"]);
        config.code_execution_jupyter_auth = get_option_string(&["code_execution", "jupyter_auth"]);
        config.code_execution_jupyter_auth_token =
            get_option_string(&["code_execution", "jupyter_auth_token"]);
        config.code_execution_jupyter_auth_password =
            get_option_string(&["code_execution", "jupyter_auth_password"]);
        config.code_execution_jupyter_timeout =
            get_option_i32(&["code_execution", "jupyter_timeout"]);
        config.code_execution_sandbox_url = get_option_string(&["code_execution", "sandbox_url"])
            .or(config.code_execution_sandbox_url.clone());
        config.code_execution_sandbox_timeout =
            get_option_i32(&["code_execution", "sandbox_timeout"])
                .or(config.code_execution_sandbox_timeout);

        // Merge Code Interpreter
        config.code_interpreter_engine = get_string(
            &["code_interpreter", "engine"],
            config.code_interpreter_engine.clone(),
        );
        config.code_interpreter_prompt_template =
            get_option_string(&["code_interpreter", "prompt_template"]);
        config.code_interpreter_jupyter_url =
            get_option_string(&["code_interpreter", "jupyter_url"]);
        config.code_interpreter_jupyter_auth =
            get_option_string(&["code_interpreter", "jupyter_auth"]);
        config.code_interpreter_jupyter_auth_token =
            get_option_string(&["code_interpreter", "jupyter_auth_token"]);
        config.code_interpreter_jupyter_auth_password =
            get_option_string(&["code_interpreter", "jupyter_auth_password"]);
        config.code_interpreter_jupyter_timeout =
            get_option_i32(&["code_interpreter", "jupyter_timeout"]);
        config.code_interpreter_sandbox_url =
            get_option_string(&["code_interpreter", "sandbox_url"])
                .or(config.code_interpreter_sandbox_url.clone());
        config.code_interpreter_sandbox_timeout =
            get_option_i32(&["code_interpreter", "sandbox_timeout"])
                .or(config.code_interpreter_sandbox_timeout);

        // Merge UI
        config.banners = get_json(&["ui", "banners"], config.banners.clone());
        config.default_prompt_suggestions = get_json(
            &["ui", "default_prompt_suggestions"],
            config.default_prompt_suggestions.clone(),
        );

        // Merge Tool Servers
        config.tool_server_connections = get_json(
            &["tool_servers", "connections"],
            config.tool_server_connections.clone(),
        );
    }
}
