#![recursion_limit = "256"]

use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use tracing::{info, Level};
use tracing_subscriber;

mod api;
mod config;
mod container;
mod error;
mod executor;
mod models;
mod security;
mod state;

use config::Config;
use state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("🚀 Starting Sandbox Executor Service");

    // Load configuration
    let config = Config::from_env().expect("Failed to load configuration");
    info!("📋 Configuration loaded successfully");

    // Initialize container manager
    let container_manager = container::ContainerManager::new(&config)
        .await
        .expect("Failed to initialize container manager");

    info!("🐳 Container manager initialized");

    // Initialize container pool for fast execution
    let container_manager_arc = std::sync::Arc::new(container_manager);
    let config_arc = std::sync::Arc::new(config.clone());
    let container_pool =
        container::ContainerPool::new(container_manager_arc.clone(), config_arc.clone());

    // Pre-warm the pool with containers
    if let Err(e) = container_pool.initialize().await {
        tracing::warn!("Failed to initialize container pool: {}", e);
        info!("⚠️  Running without container pool (slower performance)");
    } else {
        info!("⚡ Container pool initialized (fast execution enabled)");
    }

    // Create a dummy ContainerManager for AppState (it will use the pool instead)
    let dummy_manager = container::ContainerManager::new(&config)
        .await
        .expect("Failed to create container manager");

    // Create application state
    let app_state = web::Data::new(AppState::new(config.clone(), dummy_manager, container_pool));

    let bind_addr = format!("{}:{}", config.host, config.port);
    info!("🌐 Starting HTTP server on {}", bind_addr);

    HttpServer::new(move || {
        // Configure CORS
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .app_data(app_state.clone())
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .configure(api::configure_routes)
    })
    .bind(&bind_addr)?
    .run()
    .await
}
