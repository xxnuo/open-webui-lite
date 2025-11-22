pub mod handlers;
pub mod routes;

use actix_web::web;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/health", web::get().to(handlers::health_check))
            .route("/config", web::get().to(handlers::get_config))
            .route("/execute", web::post().to(handlers::execute_code))
            .route("/stats", web::get().to(handlers::get_stats)),
    );
}
