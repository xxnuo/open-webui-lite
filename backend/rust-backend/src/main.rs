mod cache_manager;
mod config;
mod db;
mod error;
mod middleware;
mod models;
mod retrieval;
mod routes;
mod services;
mod socket;
mod socketio;
mod static_files;
mod utils;
mod websocket_chat;

use actix_cors::Cors;
use actix_files::Files;
use actix_web::{
    http::header,
    middleware::{Compress, Logger, NormalizePath},
    web, App, HttpRequest, HttpResponse, HttpServer,
};
use std::net::SocketAddr;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use crate::config::{Config, MutableConfig};
use crate::db::Database;
use crate::routes::create_routes;
use crate::services::sandbox_executor::SandboxExecutorClient;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: MutableConfig,
    pub redis: Option<deadpool_redis::Pool>,
    // Model cache: model_id -> model info (with urlIdx, etc.)
    pub models_cache: Arc<RwLock<std::collections::HashMap<String, serde_json::Value>>>,
    // Socket state for tracking sessions and users (Socket.IO-like functionality)
    pub socket_state: Option<socket::SocketState>,
    // Socket.IO event handler (native Rust implementation)
    pub socketio_handler: Option<Arc<socketio::EventHandler>>,
    // Shared HTTP client for better performance (connection pooling, TLS reuse)
    pub http_client: reqwest::Client,
    // Vector database client for RAG/knowledge base operations
    pub vector_db: Option<Arc<dyn retrieval::VectorDB>>,
    // Embedding provider for generating embeddings
    pub embedding_provider: Option<Arc<dyn retrieval::EmbeddingProvider>>,
    // Sandbox executor client for secure code execution
    pub sandbox_executor_client: Option<Arc<SandboxExecutorClient>>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    dotenvy::dotenv().ok();

    let log_level = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| "info".to_string())
        .parse()
        .unwrap_or(Level::INFO);

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Open WebUI Rust Backend");

    // Load configuration from environment
    let config = Config::from_env()?;
    info!("Configuration loaded from environment");

    // Initialize database
    let db = Database::new(&config.database_url).await?;
    info!("Database connected");

    // Run migrations
    db.run_migrations().await?;
    info!("Database migrations completed");

    // Load and merge config from database (PersistentConfig behavior)
    let config = services::ConfigService::load_from_db(&db, config).await?;
    info!("Configuration loaded and merged from database");

    // Initialize Redis if enabled
    let redis = if config.enable_redis {
        let redis_config = deadpool_redis::Config::from_url(&config.redis_url);
        let pool = redis_config.create_pool(Some(deadpool_redis::Runtime::Tokio1))?;
        info!("Redis connected");
        Some(pool)
    } else {
        None
    };

    // Initialize cache manager (memory-only, no Redis required)
    let cache_manager = cache_manager::CacheManager::init();
    cache_manager.start_cleanup_tasks();
    info!("Cache manager initialized and cleanup tasks started");

    // Initialize native Rust Socket.IO
    let socketio_enabled = std::env::var("ENABLE_SOCKETIO")
        .unwrap_or_else(|_| "true".to_string())
        .to_lowercase()
        == "true";

    let socketio_handler = if socketio_enabled {
        use crate::socketio::redis_adapter::RedisAdapter;
        use crate::socketio::{
            EventHandler, PresenceConfig, PresenceManager, RateLimitConfig, RateLimiter,
            RecoveryConfig, RecoveryManager, SocketIOManager, SocketIOMetrics, YDocManager,
        };

        let manager = SocketIOManager::new();
        // Use localhost for auth endpoint to avoid 0.0.0.0 issues
        let host_for_auth = if config.host == "0.0.0.0" {
            "127.0.0.1"
        } else {
            &config.host
        };
        let auth_endpoint = format!("http://{}:{}", host_for_auth, config.port);

        // Initialize metrics
        let metrics = SocketIOMetrics::new();

        // Initialize rate limiter
        let rate_limit_config = RateLimitConfig::default();
        let rate_limiter = Arc::new(RateLimiter::new(rate_limit_config));

        // Initialize presence manager
        let presence_config = PresenceConfig::default();
        let presence_manager = Arc::new(PresenceManager::new(presence_config));

        // Initialize recovery manager with Redis if available
        let recovery_config = RecoveryConfig::default();
        let recovery_manager = Arc::new(RecoveryManager::new(redis.clone(), recovery_config));

        // Initialize YDoc manager with Redis if available
        let ydoc_manager = YDocManager::new(redis.clone());

        // Initialize Redis adapter if SOCKETIO_REDIS_URL is set
        let redis_adapter = if let Ok(redis_url) = std::env::var("SOCKETIO_REDIS_URL") {
            let server_id = uuid::Uuid::new_v4().to_string();
            match RedisAdapter::new(&redis_url, server_id.clone()) {
                Ok(adapter) => {
                    let adapter_arc = Arc::new(adapter);

                    // Spawn Redis subscription handler
                    let adapter_sub = adapter_arc.clone();
                    tokio::spawn(async move {
                        // TODO: Handle incoming Redis messages and route to local sessions
                        if let Err(e) = adapter_sub
                            .subscribe(move |msg| {
                                tracing::debug!("Received Redis message: {:?}", msg);
                                // Route message to local connections
                            })
                            .await
                        {
                            tracing::error!("Redis subscription error: {:?}", e);
                        }
                    });

                    info!(
                        "✅ Redis adapter initialized for horizontal scaling (server: {})",
                        server_id
                    );
                    Some(adapter_arc)
                }
                Err(e) => {
                    tracing::error!("Failed to initialize Redis adapter: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let handler = EventHandler::new(
            manager.clone(),
            auth_endpoint,
            ydoc_manager,
            redis_adapter,
            metrics.clone(),
            rate_limiter.clone(),
            presence_manager.clone(),
            recovery_manager.clone(),
            db.clone(),
        );

        // Spawn background cleanup tasks
        let manager_cleanup = manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                // Clean up sessions that haven't pinged in 60 seconds (3x ping interval + timeout)
                manager_cleanup.cleanup_stale_sessions(60).await;
            }
        });

        // Spawn rate limiter cleanup task
        let rate_limiter_cleanup = rate_limiter.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(600)).await; // Every 10 minutes
                rate_limiter_cleanup.cleanup_old_buckets().await;
            }
        });

        // Spawn presence cleanup tasks
        let presence_cleanup = presence_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                presence_cleanup.update_away_statuses().await;
                presence_cleanup.cleanup_typing_indicators().await;
            }
        });

        let presence_cleanup2 = presence_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await; // Every hour
                presence_cleanup2.cleanup_old_presences().await;
            }
        });

        // Spawn recovery cleanup task
        let recovery_cleanup = recovery_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(300)).await; // Every 5 minutes
                recovery_cleanup.cleanup_old_states().await;
            }
        });

        info!("✅ Native Rust Socket.IO initialized with:");
        info!("   - Yjs collaborative editing");
        info!("   - Metrics and observability");
        info!("   - Rate limiting and backpressure");
        info!("   - Presence tracking and typing indicators");
        info!("   - Connection recovery");
        info!("   - Periodic cleanup tasks");
        Some(Arc::new(handler))
    } else {
        info!("⚠️  Socket.IO disabled");
        None
    };

    // Create Socket state with native handler
    let socket_state = if config.enable_websocket_support && socketio_handler.is_some() {
        use crate::socket::SocketState;
        Some(SocketState::new(socketio_handler.as_ref().unwrap().clone()))
    } else {
        None
    };

    // Create app state
    // Create shared HTTP client with connection pooling and optimized settings
    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(10) // Reuse connections
        .tcp_nodelay(true) // Disable Nagle's algorithm for real-time streaming
        .timeout(std::time::Duration::from_secs(300)) // 5 min default timeout
        .http2_keep_alive_interval(Some(std::time::Duration::from_secs(5)))
        .http2_keep_alive_while_idle(true)
        .build()?;

    tracing::info!("🌐 HTTP client initialized with connection pooling");

    // Initialize vector database if enabled
    let vector_db_enabled = std::env::var("ENABLE_RAG")
        .unwrap_or_else(|_| "true".to_string())
        .to_lowercase()
        == "true";

    let vector_db = if vector_db_enabled {
        match retrieval::VectorDBFactory::from_env().await {
            Ok(db) => {
                info!("✅ Vector database initialized successfully");
                Some(db)
            }
            Err(e) => {
                warn!("⚠️  Failed to initialize vector database: {}", e);
                warn!("   RAG features will be disabled. To enable:");
                warn!("   - Set VECTOR_DB=chroma (or other supported DB)");
                warn!("   - Set CHROMA_HTTP_HOST and CHROMA_HTTP_PORT");
                warn!("   - Ensure vector database is running");
                None
            }
        }
    } else {
        info!("⚠️  Vector database disabled (ENABLE_RAG=false)");
        None
    };

    // Initialize embedding provider if enabled
    let embedding_provider = if vector_db_enabled {
        let engine = config.rag_embedding_engine.to_lowercase();

        match engine.as_str() {
            "openai" => {
                let api_key = if !config.rag_openai_api_key.is_empty() {
                    Some(config.rag_openai_api_key.clone())
                } else {
                    None
                };
                let model = if !config.rag_embedding_model.is_empty() {
                    Some(config.rag_embedding_model.clone())
                } else {
                    None
                };

                match retrieval::embeddings::OpenAIEmbeddings::new(api_key, model) {
                    Ok(provider) => {
                        info!("✅ Embedding provider initialized (OpenAI)");
                        Some(Arc::new(provider) as Arc<dyn retrieval::EmbeddingProvider>)
                    }
                    Err(e) => {
                        warn!("⚠️  Failed to initialize embedding provider: {}", e);
                        warn!("   Set RAG_OPENAI_API_KEY or OPENAI_API_KEY to enable embeddings");
                        None
                    }
                }
            }
            "knoxchat" | "knox" => {
                let api_key = std::env::var("KNOXCHAT_API_KEY").ok();
                let base_url = std::env::var("KNOXCHAT_BASE_URL").ok();
                let model_name = if !config.rag_embedding_model.is_empty() {
                    Some(config.rag_embedding_model.clone())
                } else {
                    None
                };

                match retrieval::embeddings::KnoxChatEmbeddings::new(api_key, base_url, model_name)
                {
                    Ok(provider) => {
                        info!("✅ Embedding provider initialized (Knox Chat)");
                        Some(Arc::new(provider) as Arc<dyn retrieval::EmbeddingProvider>)
                    }
                    Err(e) => {
                        warn!(
                            "⚠️  Failed to initialize Knox Chat embedding provider: {}",
                            e
                        );
                        warn!("   Set KNOXCHAT_API_KEY to enable Knox Chat embeddings");
                        None
                    }
                }
            }
            "" | "local" | "sentence-transformers" => {
                // Local sentence transformers using Candle
                #[cfg(feature = "embeddings")]
                {
                    info!("🤖 Initializing local sentence transformers");
                    match retrieval::EmbeddingFactory::create_sentence_transformer(
                        config.rag_embedding_model.clone(),
                        false, // auto_update = false by default
                    ) {
                        Ok(provider) => {
                            info!(
                                "✅ Sentence transformer initialized: {}",
                                provider.model_name()
                            );
                            info!("   Dimension: {}", provider.dimension());
                            info!("   Engine: Local (Candle)");
                            Some(provider)
                        }
                        Err(e) => {
                            warn!("⚠️  Failed to initialize sentence transformers: {}", e);
                            warn!("   Model: {}", config.rag_embedding_model);
                            warn!("   Tip: Model will be downloaded from HuggingFace on first use");

                            // Try Knox Chat fallback if API key is available
                            if let Ok(knox_api_key) = std::env::var("KNOXCHAT_API_KEY") {
                                info!("ℹ️  Attempting Knox Chat fallback...");
                                match retrieval::embeddings::KnoxChatEmbeddings::new(
                                    Some(knox_api_key),
                                    None,
                                    Some("voyage-3.5".to_string()),
                                ) {
                                    Ok(provider) => {
                                        info!("✅ Knox Chat fallback initialized");
                                        Some(Arc::new(provider)
                                            as Arc<dyn retrieval::EmbeddingProvider>)
                                    }
                                    Err(e) => {
                                        warn!("⚠️  Knox Chat fallback failed: {}", e);
                                        None
                                    }
                                }
                            }
                            // Try OpenAI fallback if API key is available
                            else if !config.rag_openai_api_key.is_empty() {
                                info!("ℹ️  Attempting OpenAI fallback...");
                                match retrieval::embeddings::OpenAIEmbeddings::new(
                                    Some(config.rag_openai_api_key.clone()),
                                    Some("text-embedding-3-small".to_string()),
                                ) {
                                    Ok(provider) => {
                                        info!("✅ OpenAI fallback initialized");
                                        Some(Arc::new(provider)
                                            as Arc<dyn retrieval::EmbeddingProvider>)
                                    }
                                    Err(e) => {
                                        warn!("⚠️  OpenAI fallback also failed: {}", e);
                                        None
                                    }
                                }
                            } else {
                                None
                            }
                        }
                    }
                }
                #[cfg(not(feature = "embeddings"))]
                {
                    warn!("⚠️  Sentence transformers not compiled (embeddings feature disabled)");
                    warn!("   To enable local embeddings:");
                    warn!("   - Rebuild with: cargo build --release --features embeddings");
                    warn!("   Or use remote embeddings:");
                    warn!("   - Set RAG_EMBEDDING_ENGINE=knoxchat and KNOXCHAT_API_KEY");
                    warn!("   - Set RAG_EMBEDDING_ENGINE=openai and RAG_OPENAI_API_KEY");

                    // Try Knox Chat fallback if API key is available
                    if let Ok(knox_api_key) = std::env::var("KNOXCHAT_API_KEY") {
                        info!("ℹ️  Attempting Knox Chat fallback...");
                        match retrieval::embeddings::KnoxChatEmbeddings::new(
                            Some(knox_api_key),
                            None,
                            Some("voyage-3.5".to_string()),
                        ) {
                            Ok(provider) => {
                                info!("✅ Knox Chat fallback initialized");
                                Some(Arc::new(provider) as Arc<dyn retrieval::EmbeddingProvider>)
                            }
                            Err(e) => {
                                warn!("⚠️  Knox Chat fallback failed: {}", e);
                                None
                            }
                        }
                    }
                    // Try OpenAI fallback if API key is available
                    else if !config.rag_openai_api_key.is_empty() {
                        info!("ℹ️  Attempting OpenAI fallback...");
                        match retrieval::embeddings::OpenAIEmbeddings::new(
                            Some(config.rag_openai_api_key.clone()),
                            Some("text-embedding-3-small".to_string()),
                        ) {
                            Ok(provider) => {
                                info!("✅ OpenAI fallback initialized");
                                Some(Arc::new(provider) as Arc<dyn retrieval::EmbeddingProvider>)
                            }
                            Err(e) => {
                                warn!("⚠️  OpenAI fallback also failed: {}", e);
                                None
                            }
                        }
                    } else {
                        None
                    }
                }
            }
            _ => {
                warn!("⚠️  Unknown embedding engine: {}", engine);
                warn!("   Supported engines: openai, knoxchat, local, sentence-transformers, or '' (empty for local)");
                None
            }
        }
    } else {
        None
    };

    // Initialize sandbox executor client if enabled
    let sandbox_executor_client = if config.enable_code_execution {
        let sandbox_url = config
            .code_execution_sandbox_url
            .clone()
            .unwrap_or_else(|| "http://localhost:8090".to_string());

        info!("🔒 Initializing Sandbox Executor Client");
        info!("   URL: {}", sandbox_url);
        Some(Arc::new(SandboxExecutorClient::new(sandbox_url)))
    } else {
        info!("⚠️  Code execution is disabled");
        None
    };

    let state = web::Data::new(AppState {
        db: db.clone(),
        config: Arc::new(RwLock::new(config.clone())),
        redis: redis.clone(),
        models_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        socket_state,
        socketio_handler: socketio_handler.clone(),
        http_client,
        vector_db,
        embedding_provider,
        sandbox_executor_client,
    });

    // Start server
    let addr = SocketAddr::from((config.host.parse::<std::net::IpAddr>()?, config.port));
    let cors_allow_origin = config.cors_allow_origin.clone();
    let enable_random_port = config.enable_random_port;
    
    // Check if static directory exists
    let static_dir = config.static_dir.clone();
    let static_dir_exists = std::path::Path::new(&static_dir).exists();
    
    if static_dir_exists {
        info!("📁 Using external static files directory: {}", static_dir);
    } else {
        info!("📁 External static directory not found ({}), using embedded static files", static_dir);
    }

    // Display user-friendly address (replace 0.0.0.0 with localhost for display)
    let display_host = if config.host == "0.0.0.0" {
        "localhost"
    } else {
        &config.host
    };
    
    if enable_random_port {
        info!("🚀 Starting server with random port on host: {}", display_host);
    } else {
        info!("🚀 Server running at http://{}:{}", display_host, config.port);
    }

    let server = HttpServer::new(move || {
        // Create CORS middleware
        // NOTE: When credentials are needed (cookies/auth), we cannot use allow_any_origin()
        // Instead, we need to allow specific origins or use allowed_origin_fn to dynamically allow
        let cors = if cors_allow_origin == "*" {
            // When "*" is specified, allow any origin dynamically while supporting credentials
            Cors::default()
                .allowed_origin_fn(|_origin, _req_head| {
                    // Allow all origins when * is configured
                    true
                })
                .allow_any_method()
                .allow_any_header()
                .expose_headers(vec![header::SET_COOKIE])
                .supports_credentials()
                .max_age(3600)
        } else {
            let origins: Vec<&str> = cors_allow_origin.split(',').map(|s| s.trim()).collect();
            let mut cors = Cors::default();
            for origin in origins {
                cors = cors.allowed_origin(origin);
            }
            cors.allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"])
                .allowed_headers(vec![
                    header::CONTENT_TYPE,
                    header::AUTHORIZATION,
                    header::ACCEPT,
                    header::COOKIE,
                ])
                .expose_headers(vec![header::SET_COOKIE])
                .supports_credentials()
                .max_age(3600)
        };

        App::new()
            .app_data(state.clone())
            .wrap(cors)
            .wrap(Compress::default())
            .wrap(Logger::default())
            .wrap(NormalizePath::trim())
            .wrap(middleware::SecurityHeaders) // Security headers middleware
            // Health checks
            .route("/health", web::get().to(health_check))
            .route("/health/db", web::get().to(health_check_db))
            // Config and version
            .route("/api/config", web::get().to(get_app_config))
            .route("/api/version", web::get().to(get_app_version))
            .route(
                "/api/version/updates",
                web::get().to(get_app_latest_version),
            )
            // Models list endpoint (OpenAI compatible - returns {"data": [...]})
            .route("/api/models", web::get().to(get_models))
            .route("/api/models/base", web::get().to(get_base_models))
            // API routes (nested after specific routes to avoid conflicts)
            .service(web::scope("/api/v1").configure(create_routes))
            // OpenAI compatible API
            .service(web::scope("/openai").configure(routes::openai::create_routes))
            // Chat endpoints (legacy routes without /v1 prefix)
            .service(
                web::resource("/api/chat/completions")
                    .wrap(middleware::AuthMiddleware)
                    .route(web::post().to(chat_completions)),
            )
            .service(
                web::resource("/api/chat/completed")
                    .wrap(middleware::AuthMiddleware)
                    .route(web::post().to(chat_completed)),
            )
            // WebSocket endpoint for real-time chat streaming
            .service(
                web::resource("/api/ws/chat")
                    .route(web::get().to(websocket_chat::websocket_chat_handler)),
            )
            // Native Rust Socket.IO endpoints
            .configure(configure_socketio_routes)
            .service(
                web::resource("/api/chat/actions/{action_id}")
                    .wrap(middleware::AuthMiddleware)
                    .route(web::post().to(chat_action)),
            )
            // Embeddings endpoint (legacy route without /v1 prefix)
            .route("/api/embeddings", web::post().to(embeddings))
            // Task management
            .route("/api/tasks", web::get().to(list_tasks))
            .route("/api/tasks/stop/{task_id}", web::post().to(stop_task))
            .route(
                "/api/tasks/chat/{chat_id}",
                web::get().to(list_tasks_by_chat),
            )
            // Usage and webhook
            .route("/api/usage", web::get().to(get_usage))
            .route("/api/webhook", web::get().to(get_webhook))
            .route("/api/webhook", web::post().to(update_webhook))
            // OAuth integration endpoints (for MCP and other tools)
            .route(
                "/oauth/clients/{client_id}/authorize",
                web::get().to(oauth_client_authorize),
            )
            .route(
                "/oauth/clients/{client_id}/callback",
                web::get().to(oauth_client_callback),
            )
            // PWA manifest and opensearch
            .route("/manifest.json", web::get().to(get_manifest))
            .route("/opensearch.xml", web::get().to(get_opensearch))
            // Favicon
            .route("/favicon.png", web::get().to(serve_favicon))
            // Cache file serving
            .route("/cache/{path:.*}", web::get().to(serve_cache_file))
            // Serve default user avatar at root (for backward compatibility)
            .route("/user.png", web::get().to(serve_user_avatar))
            // Conditionally serve static files from external directory if it exists
            .configure({
                let static_dir = static_dir.clone();
                move |cfg| {
                    if static_dir_exists {
                        // Use external static directory if it exists
                        cfg.service(Files::new("/static", static_dir.clone()));
                    }
                }
            })
            // Serve embedded frontend static files as default service (SPA fallback)
            // This catches all unmatched routes and serves frontend or index.html
            .default_service(web::get().to(static_files::serve))
    })
    // CRITICAL: Disable keep-alive buffering for real-time streaming
    .keep_alive(actix_web::http::KeepAlive::Timeout(
        std::time::Duration::from_secs(75),
    ))
    // CRITICAL: Set client timeout high for long-running streams
    .client_request_timeout(std::time::Duration::from_secs(300));

    let server = server.bind(addr)?;
    
    // If random port is enabled, get the actual assigned port
    if enable_random_port {
        let addrs = server.addrs();
        let actual_addr = addrs.first()
            .ok_or_else(|| anyhow::anyhow!("Failed to get server address"))?;
        let display_host = if config.host == "0.0.0.0" {
            "localhost"
        } else {
            &config.host
        };
        info!("🚀 Server running at http://{}:{}", display_host, actual_addr.port());
    }
    
    server.run().await?;

    Ok(())
}

// Serve default user avatar
async fn serve_user_avatar(state: web::Data<AppState>) -> Result<HttpResponse, crate::error::AppError> {
    let config = state.config.read().unwrap();
    let static_dir = &config.static_dir;
    let user_avatar_path = std::path::Path::new(static_dir).join("user.png");
    
    // Try external file first
    if user_avatar_path.exists() {
        if let Ok(image_data) = std::fs::read(&user_avatar_path) {
            return Ok(HttpResponse::Ok()
                .content_type("image/png")
                .body(image_data));
        }
    }
    
    // Fall back to embedded file
    use crate::static_files::FrontendAssets;
    if let Some(content) = FrontendAssets::get("static/user.png") {
        return Ok(HttpResponse::Ok()
            .content_type("image/png")
            .body(content.data.into_owned()));
    }
    
    Err(crate::error::AppError::NotFound("User avatar not found".to_string()))
}

// Serve favicon
async fn serve_favicon(state: web::Data<AppState>) -> Result<HttpResponse, crate::error::AppError> {
    let config = state.config.read().unwrap();
    let static_dir = &config.static_dir;
    let favicon_path = std::path::Path::new(static_dir).join("favicon.png");
    
    // Try external file first
    if favicon_path.exists() {
        if let Ok(image_data) = std::fs::read(&favicon_path) {
            return Ok(HttpResponse::Ok()
                .content_type("image/png")
                .body(image_data));
        }
    }
    
    // Fall back to embedded file
    use crate::static_files::FrontendAssets;
    if let Some(content) = FrontendAssets::get("static/favicon.png") {
        return Ok(HttpResponse::Ok()
            .content_type("image/png")
            .body(content.data.into_owned()));
    }
    
    Err(crate::error::AppError::NotFound("Favicon not found".to_string()))
}

// Health check endpoints
async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": true }))
}

async fn health_check_db(
    state: web::Data<AppState>,
) -> Result<HttpResponse, crate::error::AppError> {
    sqlx::query("SELECT 1")
        .execute(state.db.pool())
        .await
        .map_err(|e| crate::error::AppError::Database(e))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": true })))
}

async fn get_app_config(state: web::Data<AppState>, req: HttpRequest) -> HttpResponse {
    use serde_json::json;

    // Get read lock on config
    let config = state.config.read().unwrap();

    // Try to get user from token (in Authorization header or cookie)
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(|s| s.to_string()))
        .or_else(|| req.cookie("token").map(|c| c.value().to_string()));

    // Get actual user from database if token is valid
    let user = if let Some(ref token) = token {
        match crate::utils::auth::verify_jwt(&token, &config.webui_secret_key) {
            Ok(claims) => {
                // Get user from database (properly async)
                let user_service = services::user::UserService::new(&state.db);
                user_service
                    .get_user_by_id(&claims.sub)
                    .await
                    .ok()
                    .flatten()
            }
            Err(_) => None,
        }
    } else {
        None
    };

    // Get actual user count from database (properly async)
    let user_service = services::user::UserService::new(&state.db);
    let user_count = user_service.get_user_count().await.unwrap_or(0);

    let onboarding = user.is_none() && user_count == 0;

    let mut response = json!({
        "status": true,
        "name": config.webui_name,
        "version": env!("CARGO_PKG_VERSION"),
        "default_locale": "en-US",
        "features": {
            "auth": config.webui_auth,
            "auth_trusted_header": false,
            "enable_signup": config.enable_signup,
            "enable_login_form": config.enable_login_form,
            "enable_api_key": config.enable_api_key,
            "enable_ldap": false,
            "enable_websocket": config.enable_websocket_support,
            "enable_version_update_check": config.enable_version_update_check,
            "enable_signup_password_confirmation": false,
        },
        "oauth": {
            "providers": {}
        }
    });

    if onboarding {
        response["onboarding"] = json!(true);
    }

    // Add authenticated user configuration
    if user.is_some() {
        response["features"]["enable_direct_connections"] = json!(config.enable_direct_connections);
        response["features"]["enable_channels"] = json!(config.enable_channels);
        response["features"]["enable_notes"] = json!(config.enable_notes);
        response["features"]["enable_web_search"] = json!(config.enable_web_search);
        response["features"]["enable_code_execution"] = json!(config.enable_code_execution);
        response["features"]["enable_code_interpreter"] = json!(config.enable_code_interpreter);
        response["features"]["enable_image_generation"] = json!(config.enable_image_generation);
        response["features"]["enable_autocomplete_generation"] =
            json!(config.enable_autocomplete_generation);
        response["features"]["enable_community_sharing"] = json!(config.enable_community_sharing);
        response["features"]["enable_message_rating"] = json!(config.enable_message_rating);
        response["features"]["enable_user_webhooks"] = json!(config.enable_user_webhooks);
        response["features"]["enable_admin_export"] = json!(config.enable_admin_export);
        response["features"]["enable_admin_chat_access"] = json!(config.enable_admin_chat_access);
        response["features"]["enable_google_drive_integration"] =
            json!(config.enable_google_drive_integration);
        response["features"]["enable_onedrive_integration"] =
            json!(config.enable_onedrive_integration);

        // Convert default_models to comma-separated string (or null if empty) to match Python backend
        response["default_models"] = if config.default_models.is_empty() {
            json!(null)
        } else {
            json!(&config.default_models)
        };
        response["default_prompt_suggestions"] = config.default_prompt_suggestions.clone();
        response["user_count"] = json!(user_count);

        response["code"] = json!({
            "engine": if config.enable_code_execution { "python" } else { "" }
        });

        response["audio"] = json!({
            "tts": {
                "engine": &config.tts_engine,
                "voice": &config.tts_voice,
                "split_on": &config.tts_split_on
            },
            "stt": {
                "engine": &config.stt_engine,
            }
        });

        response["file"] = json!({
            "max_size": 10485760, // 10MB default
            "max_count": 10,
            "image_compression": {
                "width": 1024,
                "height": 1024
            }
        });

        response["permissions"] = json!({
            "chat": {
                "deletion": true,
                "edit": true,
                "temporary": true
            }
        });

        response["google_drive"] = json!({
            "client_id": "",
            "api_key": ""
        });

        response["onedrive"] = json!({
            "client_id_personal": "",
            "client_id_business": "",
            "sharepoint_url": "",
            "sharepoint_tenant_id": ""
        });

        response["ui"] = json!({});
    }

    // Add cache-control headers to prevent browser caching of config
    // This ensures users always get fresh config data
    HttpResponse::Ok()
        .insert_header((
            "Cache-Control",
            "no-store, no-cache, must-revalidate, max-age=0",
        ))
        .insert_header(("Pragma", "no-cache"))
        .insert_header(("Expires", "0"))
        .json(response)
}

async fn get_app_version() -> HttpResponse {
    use serde_json::json;

    HttpResponse::Ok().json(json!({
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn get_app_latest_version() -> HttpResponse {
    use serde_json::json;

    // TODO: Implement actual version checking from GitHub API
    let current_version = env!("CARGO_PKG_VERSION");

    HttpResponse::Ok().json(json!({
        "current": current_version,
        "latest": current_version,
    }))
}

// Models endpoints
async fn get_models(
    state: web::Data<AppState>,
    auth_user: Option<middleware::AuthUser>,
) -> HttpResponse {
    use serde_json::json;

    // Get config for model service
    let config = state.config.read().unwrap().clone();
    let model_service = crate::services::models::ModelService::new(config.clone());

    // Fetch all models
    match model_service.get_all_models(&state.db).await {
        Ok(mut models) => {
            // Apply user-based filtering if authenticated
            if let Some(user) = auth_user {
                models =
                    model_service.filter_models_by_access(models, &user.user.id, &user.user.role);
            } else {
                // For unauthenticated users, only show public models
                models = model_service.filter_models_by_access(models, "", "guest");
            }

            // Apply model ordering if configured
            if !config.model_order_list.is_empty() {
                let order_map: std::collections::HashMap<String, usize> = config
                    .model_order_list
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.clone(), i))
                    .collect();

                models.sort_by(|a, b| {
                    let a_order = order_map.get(&a.id).copied().unwrap_or(usize::MAX);
                    let b_order = order_map.get(&b.id).copied().unwrap_or(usize::MAX);

                    a_order.cmp(&b_order).then_with(|| {
                        // If both have same order (or no order), sort by name
                        let a_name = a.name.as_deref().unwrap_or(&a.id);
                        let b_name = b.name.as_deref().unwrap_or(&b.id);
                        a_name.cmp(b_name)
                    })
                });
            }

            // Process model tags
            for model in &mut models {
                let mut tags = Vec::new();

                // Collect tags from info.meta.tags
                if let Some(info) = &model.info {
                    if let Some(meta) = &info.meta {
                        if let Some(meta_tags) = &meta.tags {
                            tags.extend(meta_tags.clone());
                        }
                    }
                }

                // Collect tags from model.tags
                if let Some(model_tags) = &model.tags {
                    tags.extend(model_tags.clone());
                }

                // Deduplicate tags
                tags.sort_by(|a, b| a.name.cmp(&b.name));
                tags.dedup_by(|a, b| a.name == b.name);

                model.tags = if tags.is_empty() { None } else { Some(tags) };
            }

            HttpResponse::Ok().json(json!({
                "data": models
            }))
        }
        Err(e) => {
            tracing::error!("Failed to fetch models: {}", e);
            HttpResponse::Ok().json(json!({
                "data": []
            }))
        }
    }
}

async fn get_base_models(state: web::Data<AppState>) -> HttpResponse {
    use serde_json::json;

    // Get config for model service
    let config = state.config.read().unwrap().clone();
    let model_service = crate::services::models::ModelService::new(config);

    // Fetch base models (no filtering or customization)
    match model_service.get_all_base_models(&state.db).await {
        Ok(models) => HttpResponse::Ok().json(json!({
            "data": models
        })),
        Err(e) => {
            tracing::error!("Failed to fetch base models: {}", e);
            HttpResponse::Ok().json(json!({
                "data": []
            }))
        }
    }
}

// Chat endpoints
async fn chat_completions(
    state: web::Data<AppState>,
    payload: web::Json<serde_json::Value>,
    auth_user: middleware::AuthUser,
) -> Result<HttpResponse, crate::error::AppError> {
    // Forward to OpenAI chat completions handler
    routes::openai::handle_chat_completions(state, auth_user, payload).await
}

// Configure Socket.IO routes
fn configure_socketio_routes(cfg: &mut web::ServiceConfig) {
    // Register Socket.IO endpoints
    // Note: NormalizePath middleware strips trailing slashes, so we need both variants
    cfg
        // With trailing slash (before normalization)
        .route("/socket.io/", web::get().to(socketio_connection_get))
        .route("/socket.io/", web::post().to(socketio_connection_post))
        // Without trailing slash (after normalization)
        .route("/socket.io", web::get().to(socketio_connection_get))
        .route("/socket.io", web::post().to(socketio_connection_post))
        // Alternative path with /ws prefix (for compatibility)
        .route("/ws/socket.io/", web::get().to(socketio_connection_get))
        .route("/ws/socket.io/", web::post().to(socketio_connection_post))
        .route("/ws/socket.io", web::get().to(socketio_connection_get))
        .route("/ws/socket.io", web::post().to(socketio_connection_post))
        // Other endpoints
        .route("/api/socketio/emit", web::post().to(socketio_native_emit))
        .route("/api/socketio/health", web::get().to(socketio_health))
        .route("/api/socketio/auth", web::post().to(socketio_auth));
}

// Socket.IO GET connection handler (WebSocket upgrade or polling GET)
async fn socketio_connection_get(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Check if Socket.IO is enabled
    let handler = match &state.socketio_handler {
        Some(h) => h,
        None => {
            tracing::error!("Socket.IO not enabled but connection attempted");
            return Ok(HttpResponse::ServiceUnavailable()
                .json(serde_json::json!({"error": "Socket.IO not enabled"})));
        }
    };

    // Check transport type from query parameters
    let query =
        web::Query::<std::collections::HashMap<String, String>>::from_query(req.query_string())
            .unwrap_or(web::Query(std::collections::HashMap::new()));

    let transport = query.get("transport").map(|s| s.as_str());
    let eio = query.get("EIO");
    let sid = query.get("sid");

    tracing::info!(
        "Socket.IO GET request - transport: {:?}, EIO: {:?}, sid: {:?}, path: {}, headers: {:?}",
        transport,
        eio,
        sid,
        req.path(),
        req.headers()
    );

    // Check if this is a WebSocket upgrade request
    let is_websocket_upgrade = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_websocket_upgrade || transport == Some("websocket") {
        // WebSocket transport
        tracing::info!("Handling WebSocket upgrade for Socket.IO");
        let handler_data = web::Data::new(handler.as_ref().clone());
        socketio::transport::websocket_handler(req, stream, handler_data).await
    } else {
        // HTTP polling transport (GET - initial connection or polling for messages)
        tracing::info!("Handling HTTP polling GET for Socket.IO");
        let manager_data = web::Data::new(handler.manager().clone());
        let handler_data = Some(web::Data::new(handler.as_ref().clone()));
        socketio::transport::polling_handler(req, web::Bytes::new(), manager_data, handler_data)
            .await
    }
}

// Socket.IO POST connection handler (polling POST - client sending messages)
async fn socketio_connection_post(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Check if Socket.IO is enabled
    let handler = match &state.socketio_handler {
        Some(h) => h,
        None => {
            return Ok(HttpResponse::ServiceUnavailable()
                .json(serde_json::json!({"error": "Socket.IO not enabled"})));
        }
    };

    // HTTP polling POST transport (client sending messages)
    let manager_data = web::Data::new(handler.manager().clone());
    let handler_data = Some(web::Data::new(handler.as_ref().clone()));
    socketio::transport::polling_handler(req, body, manager_data, handler_data).await
}

// Native Socket.IO emit endpoint (for Rust backend to emit events)
async fn socketio_native_emit(
    state: web::Data<AppState>,
    payload: web::Json<socketio::events::EmitRequest>,
) -> Result<web::Json<socketio::events::EmitResponse>, actix_web::Error> {
    if let Some(ref handler) = state.socketio_handler {
        let handler_data = web::Data::new(handler.as_ref().clone());
        socketio::events::handle_emit_request(handler_data, payload).await
    } else {
        Err(actix_web::error::ErrorServiceUnavailable(
            "Socket.IO not enabled",
        ))
    }
}

// Native Socket.IO health check
async fn socketio_health(
    state: web::Data<AppState>,
) -> Result<web::Json<socketio::events::HealthResponse>, actix_web::Error> {
    if let Some(ref handler) = state.socketio_handler {
        let handler_data = web::Data::new(handler.as_ref().clone());
        socketio::events::handle_health_check(handler_data).await
    } else {
        Err(actix_web::error::ErrorServiceUnavailable(
            "Socket.IO not enabled",
        ))
    }
}

// Socket.IO authentication endpoint
async fn socketio_auth(
    state: web::Data<AppState>,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, actix_web::Error> {
    use serde_json::json;

    // Extract token from request
    let token = payload
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Missing token"))?;

    // Verify JWT token
    let config = state.config.read().unwrap();
    let claims = match crate::utils::auth::verify_jwt(token, &config.webui_secret_key) {
        Ok(claims) => claims,
        Err(_) => {
            return Ok(HttpResponse::Unauthorized().json(json!({
                "error": "Invalid token"
            })));
        }
    };

    // Get user from database
    let user_service = services::user::UserService::new(&state.db);
    let user = match user_service.get_user_by_id(&claims.sub).await {
        Ok(Some(user)) => user,
        _ => {
            return Ok(HttpResponse::NotFound().json(json!({
                "error": "User not found"
            })));
        }
    };

    // Return user data
    Ok(HttpResponse::Ok().json(json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "role": user.role,
        "profile_image_url": user.profile_image_url,
    })))
}

async fn chat_completed(
    _state: web::Data<AppState>,
    _payload: web::Json<serde_json::Value>,
) -> HttpResponse {
    use serde_json::json;

    // TODO: Implement chat completed handler
    HttpResponse::Ok().json(json!({
        "status": "ok"
    }))
}

async fn chat_action(
    _state: web::Data<AppState>,
    _action_id: web::Path<String>,
    _payload: web::Json<serde_json::Value>,
) -> HttpResponse {
    use serde_json::json;

    // TODO: Implement chat action handler
    HttpResponse::Ok().json(json!({
        "status": "ok"
    }))
}

// Embeddings endpoint
async fn embeddings(
    state: web::Data<AppState>,
    payload: web::Json<serde_json::Value>,
    auth_user: Option<middleware::AuthUser>,
) -> Result<HttpResponse, crate::error::AppError> {
    use serde_json::json;

    let mut form_data = payload.into_inner();

    // Get model ID from request
    let model_id = form_data
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("Model ID required".to_string()))?
        .to_string();

    // Get config and fetch models
    let config = state.config.read().unwrap().clone();
    let model_service = crate::services::models::ModelService::new(config.clone());

    let all_models = model_service.get_all_models(&state.db).await?;

    // Find the requested model
    let model = all_models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| crate::error::AppError::NotFound("Model not found".to_string()))?;

    // Check access control
    if let Some(ref user) = auth_user {
        if user.user.role != "admin" {
            if !model_service.check_model_access(model, &user.user.id, &user.user.role) {
                return Err(crate::error::AppError::Forbidden(
                    "Access denied to this model".to_string(),
                ));
            }
        }
    }

    // Determine which backend to use based on model type
    let is_openai_model = model.owned_by == "openai"
        || model.owned_by == "azure"
        || (model.pipeline.is_none() && model.owned_by != "custom");

    if is_openai_model {
        // Use OpenAI embeddings endpoint
        // Get the URL index from model info
        let url_idx = model
            .info
            .as_ref()
            .and_then(|info| info.params.as_ref())
            .and_then(|params| params.get("urlIdx"))
            .and_then(|idx| idx.as_u64())
            .unwrap_or(0) as usize;

        // Get OpenAI config
        if url_idx >= config.openai_api_base_urls.len() {
            return Err(crate::error::AppError::BadRequest(
                "Invalid URL index for model".to_string(),
            ));
        }

        let base_url = &config.openai_api_base_urls[url_idx];
        let api_key = config
            .openai_api_keys
            .get(url_idx)
            .cloned()
            .unwrap_or_default();

        // Prepare embedding request
        let embedding_url = format!("{}/embeddings", base_url);

        let mut request = state.http_client.post(&embedding_url);

        if !api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        // Send request
        let response = request
            .json(&form_data)
            .send()
            .await
            .map_err(|e| crate::error::AppError::ExternalServiceError(e.to_string()))?;

        let status = response.status();
        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| crate::error::AppError::ExternalServiceError(e.to_string()))?;

        if !status.is_success() {
            return Ok(HttpResponse::build(
                actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap(),
            )
            .json(response_body));
        }

        Ok(HttpResponse::Ok().json(response_body))
    } else if let Some(pipeline) = &model.pipeline {
        // Handle pipeline/function models
        if pipeline.pipeline_type.as_deref() == Some("pipe") {
            // TODO: Implement function/pipeline embedding generation
            // This would require calling the Python function module
            Err(crate::error::AppError::NotImplemented(
                "Pipeline embeddings not yet implemented".to_string(),
            ))
        } else {
            Err(crate::error::AppError::BadRequest(
                "Invalid model type for embeddings".to_string(),
            ))
        }
    } else {
        // Custom model - try to use underlying base model
        if let Some(info) = &model.info {
            if let Some(params) = &info.params {
                if let Some(base_model_id) = params.get("base_model_id").and_then(|v| v.as_str()) {
                    // Use Box::pin to handle recursive async call
                    form_data["model"] = json!(base_model_id);
                    return Box::pin(embeddings(state, web::Json(form_data), auth_user)).await;
                }
            }
        }

        Err(crate::error::AppError::NotImplemented(
            "Custom model embeddings require base_model_id".to_string(),
        ))
    }
}

// Task management
async fn list_tasks(_state: web::Data<AppState>) -> HttpResponse {
    use serde_json::json;

    // TODO: Implement task listing from Redis
    HttpResponse::Ok().json(json!({
        "tasks": []
    }))
}

async fn stop_task(
    state: web::Data<AppState>,
    task_id: web::Path<String>,
) -> Result<HttpResponse, crate::error::AppError> {
    use serde_json::json;

    // Emit cancel event through Socket.IO
    // The task_id is the chat_id in our Socket.IO streaming implementation
    if let Some(ref socket_state) = state.socket_state {
        // Send cancel event to all connected clients for this chat
        // This will trigger the frontend to stop listening for streaming events
        tracing::info!("Sending stop signal for chat: {}", task_id.as_str());

        // Emit a chat:tasks:cancel event that the frontend listens for
        let event_emitter = crate::socket::get_event_emitter(
            socket_state.clone(),
            "system".to_string(), // system user for admin actions
            Some(task_id.to_string()),
            None,
            None,
        );

        event_emitter(json!({
            "type": "chat:tasks:cancel"
        }))
        .await;
    }

    Ok(HttpResponse::Ok().json(json!({
        "status": true,
        "message": format!("Stop signal sent for {}", task_id.as_str())
    })))
}

async fn list_tasks_by_chat(
    _state: web::Data<AppState>,
    _chat_id: web::Path<String>,
) -> HttpResponse {
    use serde_json::json;

    // TODO: Implement task listing by chat ID
    HttpResponse::Ok().json(json!({
        "task_ids": []
    }))
}

// Usage and webhook
async fn get_usage(_state: web::Data<AppState>) -> HttpResponse {
    use serde_json::json;

    // TODO: Implement usage tracking (models in use, active users)
    HttpResponse::Ok().json(json!({
        "model_ids": [],
        "user_ids": []
    }))
}

async fn get_webhook(state: web::Data<AppState>) -> HttpResponse {
    use serde_json::json;

    let config = state.config.read().unwrap();
    HttpResponse::Ok().json(json!({
        "url": config.webhook_url.as_deref().unwrap_or("")
    }))
}

async fn update_webhook(
    _state: web::Data<AppState>,
    _payload: web::Json<serde_json::Value>,
) -> HttpResponse {
    use serde_json::json;

    // TODO: Implement webhook URL update
    HttpResponse::Ok().json(json!({
        "url": ""
    }))
}

// OAuth integration endpoints
async fn oauth_client_authorize(
    _state: web::Data<AppState>,
    _client_id: web::Path<String>,
) -> Result<HttpResponse, crate::error::AppError> {
    // TODO: Implement OAuth client authorization
    Err(crate::error::AppError::NotImplemented(
        "OAuth client authorization not implemented yet".to_string(),
    ))
}

async fn oauth_client_callback(
    _state: web::Data<AppState>,
    _client_id: web::Path<String>,
) -> Result<HttpResponse, crate::error::AppError> {
    use serde_json::json;

    // TODO: Implement OAuth client callback
    Ok(HttpResponse::Ok().json(json!({
        "status": "ok"
    })))
}

// PWA manifest
async fn get_manifest(_state: web::Data<AppState>) -> HttpResponse {
    use serde_json::json;

    let webui_name = "Open WebUI";

    HttpResponse::Ok().json(json!({
        "name": webui_name,
        "short_name": webui_name,
        "description": format!("{} is an open, extensible, user-friendly interface for AI that adapts to your workflow.", webui_name),
        "start_url": "/",
        "display": "standalone",
        "background_color": "#343541",
        "icons": [
            {
                "src": "/static/logo.png",
                "type": "image/png",
                "sizes": "500x500",
                "purpose": "any",
            },
            {
                "src": "/static/logo.png",
                "type": "image/png",
                "sizes": "500x500",
                "purpose": "maskable",
            },
        ],
    }))
}

// OpenSearch XML
async fn get_opensearch(state: web::Data<AppState>) -> HttpResponse {
    let config = state.config.read().unwrap();
    let webui_name = &config.webui_name;
    let webui_url = &config.webui_url;

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/" xmlns:moz="http://www.mozilla.org/2006/browser/search/">
  <ShortName>{}</ShortName>
  <Description>Search {}</Description>
  <InputEncoding>UTF-8</InputEncoding>
  <Image width="16" height="16" type="image/x-icon">{}/static/favicon.png</Image>
  <Url type="text/html" method="get" template="{}/?q={{searchTerms}}"/>
  <moz:SearchForm>{}</moz:SearchForm>
</OpenSearchDescription>"#,
        webui_name, webui_name, webui_url, webui_url, webui_url
    );

    HttpResponse::Ok().content_type("application/xml").body(xml)
}

// Cache file serving
async fn serve_cache_file(
    _state: web::Data<AppState>,
    _path: web::Path<String>,
) -> Result<HttpResponse, crate::error::AppError> {
    // TODO: Implement cache file serving with path traversal protection
    Err(crate::error::AppError::NotFound(
        "File not found".to_string(),
    ))
}
