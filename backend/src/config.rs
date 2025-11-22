use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // Server
    pub host: String,
    pub port: u16,
    pub enable_random_port: bool,
    pub env: String,
    pub webui_secret_key: String,

    // Configuration Directory
    pub config_dir: String,

    // Database
    pub database_url: String,
    pub database_pool_size: u32,
    pub database_pool_max_overflow: u32,
    pub database_pool_timeout: u64,
    pub database_pool_recycle: u64,

    // Redis
    pub enable_redis: bool,
    pub redis_url: String,

    // Authentication
    pub jwt_expires_in: String,
    pub enable_signup: bool,
    pub enable_login_form: bool,
    pub enable_api_key: bool,
    pub enable_api_key_endpoint_restrictions: bool,
    pub api_key_allowed_endpoints: String,
    pub default_user_role: String,
    pub show_admin_details: bool,
    pub webui_url: String,
    pub pending_user_overlay_title: Option<String>,
    pub pending_user_overlay_content: Option<String>,
    pub response_watermark: Option<String>,

    // LDAP Authentication
    pub enable_ldap: bool,
    pub ldap_server_label: String,
    pub ldap_server_host: String,
    pub ldap_server_port: Option<i32>,
    pub ldap_attribute_for_username: String,
    pub ldap_attribute_for_mail: String,
    pub ldap_app_dn: String,
    pub ldap_app_password: String,
    pub ldap_search_base: String,
    pub ldap_search_filters: String,
    pub ldap_use_tls: bool,
    pub ldap_ca_cert_file: Option<String>,
    pub ldap_validate_cert: bool,
    pub ldap_ciphers: Option<String>,

    // SCIM 2.0
    pub scim_enabled: bool,
    pub scim_token: String,

    // CORS
    pub cors_allow_origin: String,

    // WebSocket
    pub enable_websocket_support: bool,
    pub websocket_manager: String,
    pub websocket_redis_url: Option<String>,

    // Features
    pub enable_openai_api: bool,
    pub enable_channels: bool,
    pub enable_image_generation: bool,
    pub enable_code_execution: bool,
    pub enable_web_search: bool,
    pub enable_admin_chat_access: bool,
    pub enable_admin_export: bool,
    pub enable_notes: bool,
    pub enable_community_sharing: bool,
    pub enable_message_rating: bool,
    pub bypass_admin_access_control: Option<bool>,

    // Storage
    pub upload_dir: String,
    pub cache_dir: String,
    pub static_dir: String,

    // Logging
    pub global_log_level: String,

    // OpenAI
    pub openai_api_base_url: String,
    pub openai_api_key: String,
    pub openai_api_base_urls: Vec<String>,
    pub openai_api_keys: Vec<String>,
    pub openai_api_configs: serde_json::Value,

    // Audio - TTS
    pub tts_openai_api_base_url: String,
    pub tts_openai_api_key: String,
    pub tts_api_key: String,
    pub tts_engine: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub tts_split_on: String,
    pub tts_azure_speech_region: String,
    pub tts_azure_speech_base_url: String,
    pub tts_azure_speech_output_format: String,

    // Audio - STT
    pub stt_openai_api_base_url: String,
    pub stt_openai_api_key: String,
    pub stt_engine: String,
    pub stt_model: String,
    pub stt_supported_content_types: Vec<String>,
    pub whisper_model: String,
    pub deepgram_api_key: String,
    pub audio_stt_azure_api_key: String,
    pub audio_stt_azure_region: String,
    pub audio_stt_azure_locales: String,
    pub audio_stt_azure_base_url: String,
    pub audio_stt_azure_max_speakers: String,

    // Image Generation - OpenAI
    pub images_openai_api_base_url: String,
    pub images_openai_api_version: String,
    pub images_openai_api_key: String,

    // Image Generation - Automatic1111
    pub automatic1111_base_url: String,
    pub automatic1111_api_auth: String,
    pub automatic1111_cfg_scale: Option<f64>,
    pub automatic1111_sampler: Option<String>,
    pub automatic1111_scheduler: Option<String>,

    // Image Generation - ComfyUI
    pub comfyui_base_url: String,
    pub comfyui_api_key: String,
    pub comfyui_workflow: String,
    pub comfyui_workflow_nodes: serde_json::Value,

    // Image Generation - Gemini
    pub images_gemini_api_base_url: String,
    pub images_gemini_api_key: String,

    pub image_generation_engine: String,
    pub enable_image_prompt_generation: bool,

    // RAG/Retrieval
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub rag_top_k: usize,
    pub rag_embedding_model: String,
    pub rag_embedding_engine: String,
    pub rag_openai_api_key: String,
    pub rag_openai_api_base_url: String,
    pub rag_template: String,
    pub rag_full_context: bool,
    pub bypass_embedding_and_retrieval: bool,
    pub enable_rag_hybrid_search: bool,
    pub top_k_reranker: i32,
    pub relevance_threshold: f64,
    pub hybrid_bm25_weight: f64,
    pub content_extraction_engine: String,
    pub pdf_extract_images: bool,
    pub rag_embedding_model_trust_remote_code: bool,
    pub rag_reranking_model_trust_remote_code: bool,

    // Sentence Transformers
    pub sentence_transformers_home: Option<String>,
    pub sentence_transformers_backend: String,
    pub sentence_transformers_model_kwargs: Option<String>,
    pub sentence_transformers_cross_encoder_backend: String,
    pub sentence_transformers_cross_encoder_model_kwargs: Option<String>,

    // RAG Embedding Prefixes
    pub rag_embedding_query_prefix: String,
    pub rag_embedding_content_prefix: String,
    pub rag_embedding_prefix_field_name: Option<String>,

    // Code Execution
    pub code_execution_engine: String,
    pub enable_pipeline_filters: bool,
    pub code_execution_jupyter_url: Option<String>,
    pub code_execution_jupyter_auth: Option<String>,
    pub code_execution_jupyter_auth_token: Option<String>,
    pub code_execution_jupyter_auth_password: Option<String>,
    pub code_execution_jupyter_timeout: Option<i32>,
    pub code_execution_sandbox_url: Option<String>,
    pub code_execution_sandbox_timeout: Option<i32>,
    pub code_execution_sandbox_enable_pool: Option<bool>,
    pub code_execution_sandbox_pool_size: Option<i32>,
    pub code_execution_sandbox_pool_max_reuse: Option<i32>,
    pub code_execution_sandbox_pool_max_age: Option<i32>,
    pub enable_code_interpreter: bool,
    pub code_interpreter_engine: String,
    pub code_interpreter_prompt_template: Option<String>,
    pub code_interpreter_jupyter_url: Option<String>,
    pub code_interpreter_jupyter_auth: Option<String>,
    pub code_interpreter_jupyter_auth_token: Option<String>,
    pub code_interpreter_jupyter_auth_password: Option<String>,
    pub code_interpreter_jupyter_timeout: Option<i32>,
    pub code_interpreter_sandbox_url: Option<String>,
    pub code_interpreter_sandbox_timeout: Option<i32>,

    // Webhooks
    pub webhook_url: Option<String>,

    // WebUI Settings
    pub webui_name: String,
    pub webui_auth: bool,
    pub default_models: String,
    pub model_order_list: Vec<String>,
    pub default_prompt_suggestions: serde_json::Value,
    pub banners: serde_json::Value,
    pub user_permissions: serde_json::Value,

    // Version and Updates
    pub enable_version_update_check: bool,

    // Task Configuration
    pub task_model: Option<String>,
    pub task_model_external: Option<String>,
    pub enable_search_query_generation: bool,
    pub enable_retrieval_query_generation: bool,
    pub enable_autocomplete_generation: bool,
    pub autocomplete_generation_input_max_length: i32,
    pub enable_tags_generation: bool,
    pub tags_generation_prompt_template: String,
    pub enable_title_generation: bool,
    pub title_generation_prompt_template: String,
    pub enable_follow_up_generation: bool,
    pub follow_up_generation_prompt_template: String,
    pub image_prompt_generation_prompt_template: String,
    pub query_generation_prompt_template: String,
    pub tools_function_calling_prompt_template: String,

    // User permissions
    pub enable_user_webhooks: bool,

    // Direct connections
    pub enable_direct_connections: bool,
    pub enable_base_models_cache: bool,

    // Tool Servers
    pub tool_server_connections: serde_json::Value,

    // Integrations
    pub enable_google_drive_integration: bool,
    pub enable_onedrive_integration: bool,

    // Evaluations
    pub enable_evaluation_arena_models: bool,
    pub evaluation_arena_models: serde_json::Value,
}

/// Mutable config wrapper for runtime updates
pub type MutableConfig = Arc<RwLock<Config>>;

impl Config {
    /// Expand tilde (~) to home directory in path
    fn expand_home_dir(path: &str) -> String {
        if path.starts_with("~/") {
            // Windows uses USERPROFILE, Unix uses HOME
            let home = if cfg!(windows) {
                env::var("USERPROFILE").or_else(|_| env::var("HOME"))
            } else {
                env::var("HOME")
            };
            
            if let Ok(home) = home {
                return path.replacen("~", &home, 1);
            }
        }
        path.to_string()
    }

    /// Get default database path
    fn get_default_database_url(config_dir: &str) -> String {
        let expanded_config_dir = Self::expand_home_dir(config_dir);
        let db_path = PathBuf::from(&expanded_config_dir).join("data.sqlite3");

        // Ensure config directory exists
        let _ = std::fs::create_dir_all(&expanded_config_dir);

        format!("sqlite://{}", db_path.to_string_lossy())
    }

    pub fn from_env() -> anyhow::Result<Self> {
        // Get config directory first
        let config_dir =
            env::var("CONFIG_DIR").unwrap_or_else(|_| "~/.config/open-coreui".to_string());

        // Check if random port is enabled
        let enable_random_port = env::var("ENABLE_RANDOM_PORT")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            .parse()
            .unwrap_or(false);

        // If random port is enabled, use 0 (OS will assign a random available port)
        let port = if enable_random_port {
            0
        } else {
            env::var("PORT")
                .unwrap_or_else(|_| "8168".to_string())
                .parse()
                .unwrap_or(8168)
        };

        Ok(Config {
            // Server
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port,
            enable_random_port,
            env: env::var("ENV").unwrap_or_else(|_| "production".to_string()),
            webui_secret_key: env::var("WEBUI_SECRET_KEY").unwrap_or_else(|_| {
                let key = uuid::Uuid::new_v4().to_string();
                eprintln!(
                    "Warning: WEBUI_SECRET_KEY not set, using generated key: {}",
                    key
                );
                key
            }),

            // Configuration Directory
            config_dir: config_dir.clone(),

            // Database
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| Self::get_default_database_url(&config_dir)),
            database_pool_size: env::var("DATABASE_POOL_SIZE")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            database_pool_max_overflow: env::var("DATABASE_POOL_MAX_OVERFLOW")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            database_pool_timeout: env::var("DATABASE_POOL_TIMEOUT")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            database_pool_recycle: env::var("DATABASE_POOL_RECYCLE")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),

            // Redis
            enable_redis: env::var("ENABLE_REDIS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),

            // Authentication
            jwt_expires_in: env::var("JWT_EXPIRES_IN").unwrap_or_else(|_| "168h".to_string()),
            enable_signup: env::var("ENABLE_SIGNUP")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_login_form: env::var("ENABLE_LOGIN_FORM")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_api_key: env::var("ENABLE_API_KEY")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_api_key_endpoint_restrictions: env::var("ENABLE_API_KEY_ENDPOINT_RESTRICTIONS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            api_key_allowed_endpoints: env::var("API_KEY_ALLOWED_ENDPOINTS").unwrap_or_default(),
            default_user_role: env::var("DEFAULT_USER_ROLE")
                .unwrap_or_else(|_| "pending".to_string()),
            show_admin_details: env::var("SHOW_ADMIN_DETAILS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            webui_url: env::var("WEBUI_URL")
                .unwrap_or_else(|_| {
                    let port = if enable_random_port { 0 } else { port };
                    format!("http://localhost:{}", port)
                }),
            pending_user_overlay_title: env::var("PENDING_USER_OVERLAY_TITLE").ok(),
            pending_user_overlay_content: env::var("PENDING_USER_OVERLAY_CONTENT").ok(),
            response_watermark: env::var("RESPONSE_WATERMARK").ok(),

            // LDAP Authentication
            enable_ldap: env::var("ENABLE_LDAP")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            ldap_server_label: env::var("LDAP_SERVER_LABEL")
                .unwrap_or_else(|_| "LDAP Server".to_string()),
            ldap_server_host: env::var("LDAP_SERVER_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            ldap_server_port: env::var("LDAP_SERVER_PORT")
                .ok()
                .and_then(|s| s.parse().ok()),
            ldap_attribute_for_username: env::var("LDAP_ATTRIBUTE_FOR_USERNAME")
                .unwrap_or_else(|_| "uid".to_string()),
            ldap_attribute_for_mail: env::var("LDAP_ATTRIBUTE_FOR_MAIL")
                .unwrap_or_else(|_| "mail".to_string()),
            ldap_app_dn: env::var("LDAP_APP_DN").unwrap_or_default(),
            ldap_app_password: env::var("LDAP_APP_PASSWORD").unwrap_or_default(),
            ldap_search_base: env::var("LDAP_SEARCH_BASE").unwrap_or_default(),
            ldap_search_filters: env::var("LDAP_SEARCH_FILTERS").unwrap_or_default(),
            ldap_use_tls: env::var("LDAP_USE_TLS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            ldap_ca_cert_file: env::var("LDAP_CA_CERT_FILE").ok(),
            ldap_validate_cert: env::var("LDAP_VALIDATE_CERT")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            ldap_ciphers: env::var("LDAP_CIPHERS").ok(),

            // SCIM 2.0
            scim_enabled: env::var("SCIM_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            scim_token: env::var("SCIM_TOKEN").unwrap_or_default(),

            // CORS
            cors_allow_origin: env::var("CORS_ALLOW_ORIGIN").unwrap_or_else(|_| "*".to_string()),

            // WebSocket
            enable_websocket_support: env::var("ENABLE_WEBSOCKET_SUPPORT")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            websocket_manager: env::var("WEBSOCKET_MANAGER")
                .unwrap_or_else(|_| "local".to_string()),
            websocket_redis_url: env::var("WEBSOCKET_REDIS_URL").ok(),

            // Features
            enable_openai_api: env::var("ENABLE_OPENAI_API")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_channels: env::var("ENABLE_CHANNELS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_image_generation: env::var("ENABLE_IMAGE_GENERATION")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_code_execution: env::var("ENABLE_CODE_EXECUTION")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_web_search: env::var("ENABLE_WEB_SEARCH")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_admin_chat_access: env::var("ENABLE_ADMIN_CHAT_ACCESS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_admin_export: env::var("ENABLE_ADMIN_EXPORT")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_notes: env::var("ENABLE_NOTES")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_community_sharing: env::var("ENABLE_COMMUNITY_SHARING")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_message_rating: env::var("ENABLE_MESSAGE_RATING")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            bypass_admin_access_control: env::var("BYPASS_ADMIN_ACCESS_CONTROL")
                .ok()
                .and_then(|s| s.parse().ok()),

            // Storage - all paths relative to config_dir for consistency
            upload_dir: env::var("UPLOAD_DIR").unwrap_or_else(|_| {
                let expanded_config_dir = Self::expand_home_dir(&config_dir);
                PathBuf::from(&expanded_config_dir)
                    .join("data")
                    .join("uploads")
                    .to_string_lossy()
                    .to_string()
            }),
            cache_dir: env::var("CACHE_DIR").unwrap_or_else(|_| {
                let expanded_config_dir = Self::expand_home_dir(&config_dir);
                PathBuf::from(&expanded_config_dir)
                    .join("data")
                    .join("cache")
                    .to_string_lossy()
                    .to_string()
            }),
            static_dir: env::var("STATIC_DIR").unwrap_or_else(|_| {
                let expanded_config_dir = Self::expand_home_dir(&config_dir);
                PathBuf::from(&expanded_config_dir)
                    .join("build")
                    .to_string_lossy()
                    .to_string()
            }),

            // Logging
            global_log_level: env::var("GLOBAL_LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string()),

            // OpenAI
            openai_api_base_url: env::var("OPENAI_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            openai_api_key: env::var("OPENAI_API_KEY").unwrap_or_default(),
            openai_api_base_urls: {
                let urls_str = env::var("OPENAI_API_BASE_URLS")
                    .or_else(|_| env::var("OPENAI_API_BASE_URL"))
                    .unwrap_or_default();

                if urls_str.is_empty() {
                    // No URLs configured - return empty vec
                    vec![]
                } else {
                    urls_str
                        .split(';')
                        .filter_map(|s| {
                            let trimmed = s.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_string())
                            }
                        })
                        .collect()
                }
            },
            openai_api_keys: {
                let keys_str = env::var("OPENAI_API_KEYS")
                    .or_else(|_| env::var("OPENAI_API_KEY"))
                    .unwrap_or_default();

                if keys_str.is_empty() {
                    // No keys configured - return empty vec
                    vec![]
                } else {
                    keys_str.split(';').map(|s| s.trim().to_string()).collect()
                }
            },
            openai_api_configs: serde_json::json!({}),

            // Audio - TTS
            tts_openai_api_base_url: env::var("TTS_OPENAI_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            tts_openai_api_key: env::var("TTS_OPENAI_API_KEY").unwrap_or_default(),
            tts_api_key: env::var("TTS_API_KEY").unwrap_or_default(),
            tts_engine: env::var("TTS_ENGINE").unwrap_or_else(|_| "openai".to_string()),
            tts_model: env::var("TTS_MODEL").unwrap_or_else(|_| "tts-1".to_string()),
            tts_voice: env::var("TTS_VOICE").unwrap_or_else(|_| "alloy".to_string()),
            tts_split_on: env::var("TTS_SPLIT_ON").unwrap_or_else(|_| "sentence".to_string()),
            tts_azure_speech_region: env::var("TTS_AZURE_SPEECH_REGION").unwrap_or_default(),
            tts_azure_speech_base_url: env::var("TTS_AZURE_SPEECH_BASE_URL").unwrap_or_default(),
            tts_azure_speech_output_format: env::var("TTS_AZURE_SPEECH_OUTPUT_FORMAT")
                .unwrap_or_else(|_| "audio-24khz-96kbitrate-mono-mp3".to_string()),

            // Audio - STT
            stt_openai_api_base_url: env::var("STT_OPENAI_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            stt_openai_api_key: env::var("STT_OPENAI_API_KEY").unwrap_or_default(),
            stt_engine: env::var("STT_ENGINE").unwrap_or_else(|_| "openai".to_string()),
            stt_model: env::var("STT_MODEL").unwrap_or_else(|_| "whisper-1".to_string()),
            stt_supported_content_types: env::var("STT_SUPPORTED_CONTENT_TYPES")
                .ok()
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_else(|| vec!["audio/*".to_string(), "video/webm".to_string()]),
            whisper_model: env::var("WHISPER_MODEL").unwrap_or_else(|_| "base".to_string()),
            deepgram_api_key: env::var("DEEPGRAM_API_KEY").unwrap_or_default(),
            audio_stt_azure_api_key: env::var("AUDIO_STT_AZURE_API_KEY").unwrap_or_default(),
            audio_stt_azure_region: env::var("AUDIO_STT_AZURE_REGION").unwrap_or_default(),
            audio_stt_azure_locales: env::var("AUDIO_STT_AZURE_LOCALES").unwrap_or_default(),
            audio_stt_azure_base_url: env::var("AUDIO_STT_AZURE_BASE_URL").unwrap_or_default(),
            audio_stt_azure_max_speakers: env::var("AUDIO_STT_AZURE_MAX_SPEAKERS")
                .unwrap_or_else(|_| "1".to_string()),

            // Image Generation - OpenAI
            images_openai_api_base_url: env::var("IMAGES_OPENAI_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            images_openai_api_version: env::var("IMAGES_OPENAI_API_VERSION")
                .unwrap_or_else(|_| "2024-02-01".to_string()),
            images_openai_api_key: env::var("IMAGES_OPENAI_API_KEY").unwrap_or_default(),

            // Image Generation - Automatic1111
            automatic1111_base_url: env::var("AUTOMATIC1111_BASE_URL").unwrap_or_default(),
            automatic1111_api_auth: env::var("AUTOMATIC1111_API_AUTH").unwrap_or_default(),
            automatic1111_cfg_scale: env::var("AUTOMATIC1111_CFG_SCALE")
                .ok()
                .and_then(|s| s.parse().ok()),
            automatic1111_sampler: env::var("AUTOMATIC1111_SAMPLER").ok(),
            automatic1111_scheduler: env::var("AUTOMATIC1111_SCHEDULER").ok(),

            // Image Generation - ComfyUI
            comfyui_base_url: env::var("COMFYUI_BASE_URL").unwrap_or_default(),
            comfyui_api_key: env::var("COMFYUI_API_KEY").unwrap_or_default(),
            comfyui_workflow: env::var("COMFYUI_WORKFLOW").unwrap_or_default(),
            comfyui_workflow_nodes: serde_json::json!([]),

            // Image Generation - Gemini
            images_gemini_api_base_url: env::var("IMAGES_GEMINI_API_BASE_URL")
                .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta".to_string()),
            images_gemini_api_key: env::var("IMAGES_GEMINI_API_KEY").unwrap_or_default(),

            image_generation_engine: env::var("IMAGE_GENERATION_ENGINE")
                .unwrap_or_else(|_| "openai".to_string()),
            enable_image_prompt_generation: env::var("ENABLE_IMAGE_PROMPT_GENERATION")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),

            // RAG/Retrieval
            chunk_size: env::var("CHUNK_SIZE")
                .unwrap_or_else(|_| "1500".to_string())
                .parse()
                .unwrap_or(1500),
            chunk_overlap: env::var("CHUNK_OVERLAP")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            rag_top_k: env::var("RAG_TOP_K")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            rag_embedding_model: env::var("RAG_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "sentence-transformers/all-MiniLM-L6-v2".to_string()),
            rag_embedding_engine: env::var("RAG_EMBEDDING_ENGINE")
                .unwrap_or_else(|_| "".to_string()),
            rag_openai_api_key: env::var("RAG_OPENAI_API_KEY")
                .or_else(|_| env::var("OPENAI_API_KEY"))
                .unwrap_or_default(),
            rag_openai_api_base_url: env::var("RAG_OPENAI_API_BASE_URL")
                .or_else(|_| env::var("OPENAI_API_BASE_URL"))
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            rag_template: env::var("RAG_TEMPLATE")
                .unwrap_or_else(|_| crate::utils::retrieval::DEFAULT_RAG_TEMPLATE.to_string()),
            rag_full_context: env::var("RAG_FULL_CONTEXT")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            bypass_embedding_and_retrieval: env::var("BYPASS_EMBEDDING_AND_RETRIEVAL")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_rag_hybrid_search: env::var("ENABLE_RAG_HYBRID_SEARCH")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            top_k_reranker: env::var("TOP_K_RERANKER")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            relevance_threshold: env::var("RELEVANCE_THRESHOLD")
                .unwrap_or_else(|_| "0.0".to_string())
                .parse()
                .unwrap_or(0.0),
            hybrid_bm25_weight: env::var("HYBRID_BM25_WEIGHT")
                .unwrap_or_else(|_| "0.5".to_string())
                .parse()
                .unwrap_or(0.5),
            content_extraction_engine: env::var("CONTENT_EXTRACTION_ENGINE")
                .unwrap_or_else(|_| "tika".to_string()),
            pdf_extract_images: env::var("PDF_EXTRACT_IMAGES")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            rag_embedding_model_trust_remote_code: env::var(
                "RAG_EMBEDDING_MODEL_TRUST_REMOTE_CODE",
            )
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true),
            rag_reranking_model_trust_remote_code: env::var(
                "RAG_RERANKING_MODEL_TRUST_REMOTE_CODE",
            )
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true),

            // Sentence Transformers
            sentence_transformers_home: env::var("SENTENCE_TRANSFORMERS_HOME").ok(),
            sentence_transformers_backend: env::var("SENTENCE_TRANSFORMERS_BACKEND")
                .unwrap_or_else(|_| "torch".to_string()),
            sentence_transformers_model_kwargs: env::var("SENTENCE_TRANSFORMERS_MODEL_KWARGS").ok(),
            sentence_transformers_cross_encoder_backend: env::var(
                "SENTENCE_TRANSFORMERS_CROSS_ENCODER_BACKEND",
            )
            .unwrap_or_else(|_| "torch".to_string()),
            sentence_transformers_cross_encoder_model_kwargs: env::var(
                "SENTENCE_TRANSFORMERS_CROSS_ENCODER_MODEL_KWARGS",
            )
            .ok(),

            // RAG Embedding Prefixes
            rag_embedding_query_prefix: env::var("RAG_EMBEDDING_QUERY_PREFIX").unwrap_or_default(),
            rag_embedding_content_prefix: env::var("RAG_EMBEDDING_CONTENT_PREFIX")
                .unwrap_or_default(),
            rag_embedding_prefix_field_name: env::var("RAG_EMBEDDING_PREFIX_FIELD_NAME").ok(),

            // Code Execution
            code_execution_engine: env::var("CODE_EXECUTION_ENGINE")
                .unwrap_or_else(|_| "python".to_string()),
            enable_pipeline_filters: env::var("ENABLE_PIPELINE_FILTERS")
                .unwrap_or_else(|_| "true".to_string())
                .to_lowercase()
                == "true",
            code_execution_jupyter_url: env::var("CODE_EXECUTION_JUPYTER_URL").ok(),
            code_execution_jupyter_auth: env::var("CODE_EXECUTION_JUPYTER_AUTH").ok(),
            code_execution_jupyter_auth_token: env::var("CODE_EXECUTION_JUPYTER_AUTH_TOKEN").ok(),
            code_execution_jupyter_auth_password: env::var("CODE_EXECUTION_JUPYTER_AUTH_PASSWORD")
                .ok(),
            code_execution_jupyter_timeout: env::var("CODE_EXECUTION_JUPYTER_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok()),
            code_execution_sandbox_url: env::var("CODE_EXECUTION_SANDBOX_URL")
                .ok()
                .or_else(|| Some("http://localhost:8090".to_string())),
            code_execution_sandbox_timeout: env::var("CODE_EXECUTION_SANDBOX_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .or(Some(60)),
            code_execution_sandbox_enable_pool: env::var("CODE_EXECUTION_SANDBOX_ENABLE_POOL")
                .ok()
                .and_then(|s| s.parse().ok()),
            code_execution_sandbox_pool_size: env::var("CODE_EXECUTION_SANDBOX_POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok()),
            code_execution_sandbox_pool_max_reuse: env::var(
                "CODE_EXECUTION_SANDBOX_POOL_MAX_REUSE",
            )
            .ok()
            .and_then(|s| s.parse().ok()),
            code_execution_sandbox_pool_max_age: env::var("CODE_EXECUTION_SANDBOX_POOL_MAX_AGE")
                .ok()
                .and_then(|s| s.parse().ok()),
            enable_code_interpreter: env::var("ENABLE_CODE_INTERPRETER")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            code_interpreter_engine: env::var("CODE_INTERPRETER_ENGINE")
                .unwrap_or_else(|_| "python".to_string()),
            code_interpreter_prompt_template: env::var("CODE_INTERPRETER_PROMPT_TEMPLATE").ok(),
            code_interpreter_jupyter_url: env::var("CODE_INTERPRETER_JUPYTER_URL").ok(),
            code_interpreter_jupyter_auth: env::var("CODE_INTERPRETER_JUPYTER_AUTH").ok(),
            code_interpreter_jupyter_auth_token: env::var("CODE_INTERPRETER_JUPYTER_AUTH_TOKEN")
                .ok(),
            code_interpreter_jupyter_auth_password: env::var(
                "CODE_INTERPRETER_JUPYTER_AUTH_PASSWORD",
            )
            .ok(),
            code_interpreter_jupyter_timeout: env::var("CODE_INTERPRETER_JUPYTER_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok()),
            code_interpreter_sandbox_url: env::var("CODE_INTERPRETER_SANDBOX_URL")
                .ok()
                .or_else(|| env::var("CODE_EXECUTION_SANDBOX_URL").ok())
                .or_else(|| Some("http://localhost:8090".to_string())),
            code_interpreter_sandbox_timeout: env::var("CODE_INTERPRETER_SANDBOX_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .or_else(|| {
                    env::var("CODE_EXECUTION_SANDBOX_TIMEOUT")
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .or(Some(60)),

            // Webhooks
            webhook_url: env::var("WEBHOOK_URL").ok(),

            // WebUI Settings
            webui_name: env::var("WEBUI_NAME").unwrap_or_else(|_| "Open WebUI".to_string()),
            webui_auth: env::var("WEBUI_AUTH")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            default_models: env::var("DEFAULT_MODELS").unwrap_or_default(),
            model_order_list: env::var("MODEL_ORDER_LIST")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
            default_prompt_suggestions: serde_json::json!([]),
            banners: serde_json::json!([]),
            user_permissions: serde_json::json!({}),

            // Version and Updates
            enable_version_update_check: env::var("ENABLE_VERSION_UPDATE_CHECK")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            // Task Configuration
            task_model: env::var("TASK_MODEL").ok(),
            task_model_external: env::var("TASK_MODEL_EXTERNAL").ok(),
            enable_search_query_generation: env::var("ENABLE_SEARCH_QUERY_GENERATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_retrieval_query_generation: env::var("ENABLE_RETRIEVAL_QUERY_GENERATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_autocomplete_generation: env::var("ENABLE_AUTOCOMPLETE_GENERATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            autocomplete_generation_input_max_length: env::var(
                "AUTOCOMPLETE_GENERATION_INPUT_MAX_LENGTH",
            )
            .unwrap_or_else(|_| "200".to_string())
            .parse()
            .unwrap_or(200),
            enable_tags_generation: env::var("ENABLE_TAGS_GENERATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            tags_generation_prompt_template: env::var("TAGS_GENERATION_PROMPT_TEMPLATE")
                .unwrap_or_else(|_| String::new()),
            enable_title_generation: env::var("ENABLE_TITLE_GENERATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            title_generation_prompt_template: env::var("TITLE_GENERATION_PROMPT_TEMPLATE")
                .unwrap_or_else(|_| String::new()),
            enable_follow_up_generation: env::var("ENABLE_FOLLOW_UP_GENERATION")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            follow_up_generation_prompt_template: env::var("FOLLOW_UP_GENERATION_PROMPT_TEMPLATE")
                .unwrap_or_else(|_| String::new()),
            image_prompt_generation_prompt_template: env::var(
                "IMAGE_PROMPT_GENERATION_PROMPT_TEMPLATE",
            )
            .unwrap_or_else(|_| String::new()),
            query_generation_prompt_template: env::var("QUERY_GENERATION_PROMPT_TEMPLATE")
                .unwrap_or_else(|_| String::new()),
            tools_function_calling_prompt_template: env::var(
                "TOOLS_FUNCTION_CALLING_PROMPT_TEMPLATE",
            )
            .unwrap_or_else(|_| String::new()),

            // User permissions
            enable_user_webhooks: env::var("ENABLE_USER_WEBHOOKS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            // Direct connections
            enable_direct_connections: env::var("ENABLE_DIRECT_CONNECTIONS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_base_models_cache: env::var("ENABLE_BASE_MODELS_CACHE")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),

            // Tool Servers
            tool_server_connections: serde_json::json!([]),

            // Evaluations
            enable_evaluation_arena_models: env::var("ENABLE_EVALUATION_ARENA_MODELS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            evaluation_arena_models: serde_json::json!([]),

            // Integrations
            enable_google_drive_integration: env::var("ENABLE_GOOGLE_DRIVE_INTEGRATION")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            enable_onedrive_integration: env::var("ENABLE_ONEDRIVE_INTEGRATION")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
        })
    }
}
