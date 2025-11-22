pub mod audio;
pub mod auth;
pub mod cache;
pub mod channels;
pub mod chats;
pub mod code_execution;
pub mod configs;
pub mod evaluations;
pub mod files;
pub mod folders;
pub mod functions;
pub mod groups;
pub mod images;
pub mod knowledge;
pub mod knowledge_vector; // Vector DB operations for knowledge
pub mod memories;
pub mod models;
pub mod notes;
pub mod openai;
pub mod pipelines;
pub mod prompts;
pub mod retrieval;
pub mod scim;
pub mod tasks;
pub mod tools;
pub mod users;
pub mod utils;

use actix_web::web;

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/audio").configure(audio::create_routes))
        .service(web::scope("/auths").configure(auth::create_routes))
        .service(web::scope("/api/v1").configure(cache::configure))
        .service(web::scope("/channels").configure(channels::create_routes))
        .service(web::scope("/chats").configure(chats::create_routes))
        .service(web::scope("/configs").configure(configs::create_routes))
        .service(web::scope("/evaluations").configure(evaluations::create_routes))
        .service(web::scope("/files").configure(files::create_routes))
        .service(web::scope("/folders").configure(folders::create_routes))
        .service(web::scope("/functions").configure(functions::create_routes))
        .service(web::scope("/groups").configure(groups::create_routes))
        .service(web::scope("/images").configure(images::create_routes))
        .service(web::scope("/knowledge").configure(knowledge::create_routes))
        .service(web::scope("/memories").configure(memories::create_routes))
        // Note: /models GET is handled in main.rs, nested routes handle POST/PUT/DELETE
        .service(web::scope("/models").configure(models::create_routes))
        .service(web::scope("/notes").configure(notes::create_routes))
        .service(web::scope("/pipelines").configure(pipelines::create_routes))
        .service(web::scope("/prompts").configure(prompts::create_routes))
        .service(web::scope("/retrieval").configure(retrieval::create_routes))
        .service(web::scope("/scim/v2").configure(scim::create_routes))
        .service(web::scope("/tasks").configure(tasks::create_routes))
        .service(web::scope("/tools").configure(tools::create_routes))
        .service(web::scope("/users").configure(users::create_routes))
        .service(web::scope("/utils").configure(utils::create_routes));
}
