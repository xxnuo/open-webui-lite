use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Database;
use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::memory::MemoryResponse;
use crate::services::memory::MemoryService;

#[derive(Debug, Deserialize)]
pub struct AddMemoryForm {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemoryForm {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryMemoryForm {
    pub content: String,
    #[serde(default = "default_k")]
    pub k: i64,
}

fn default_k() -> i64 {
    1
}

#[derive(Debug, Serialize)]
pub struct QueryMemoryResult {
    pub results: Vec<MemoryResponse>,
}

// GET /ef - Get embeddings (testing endpoint)
async fn get_embeddings(_user: AuthUser) -> AppResult<HttpResponse> {
    // TODO: Implement embedding function integration
    // For now, return a placeholder
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "result": "Embedding function not yet implemented"
    })))
}

// GET / - Get memories by user
async fn get_memories(db: web::Data<Database>, user: AuthUser) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);
    let memories = service.get_memories_by_user_id(&user.id).await?;

    let responses: Vec<MemoryResponse> = memories.into_iter().map(|m| m.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

// POST /add - Add memory with vector upsert
async fn add_memory(
    db: web::Data<Database>,
    user: AuthUser,
    form: web::Json<AddMemoryForm>,
) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);

    let memory_id = Uuid::new_v4().to_string();
    let memory = service
        .create_memory(&memory_id, &user.id, &form.content, None)
        .await?;

    // TODO: Implement vector DB upsert
    // VECTOR_DB_CLIENT.upsert(
    //     collection_name=f"user-memory-{user.id}",
    //     items=[{
    //         "id": memory.id,
    //         "text": memory.content,
    //         "vector": EMBEDDING_FUNCTION(memory.content),
    //         "metadata": {"created_at": memory.created_at},
    //     }],
    // )

    let response: MemoryResponse = memory.into();
    Ok(HttpResponse::Ok().json(response))
}

// POST /query - Query memories with vector search
async fn query_memory(
    db: web::Data<Database>,
    user: AuthUser,
    form: web::Json<QueryMemoryForm>,
) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);

    // First check if user has any memories
    let memories = service.get_memories_by_user_id(&user.id).await?;
    if memories.is_empty() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "No memories found for user"
        })));
    }

    // TODO: Implement vector DB search
    // For now, do a simple text search
    let results = service
        .query_memories(&user.id, &form.content, form.k)
        .await?;

    let responses: Vec<MemoryResponse> = results.into_iter().map(|m| m.into()).collect();
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "results": responses
    })))
}

// POST /reset - Reset memory from vector DB
async fn reset_memory(db: web::Data<Database>, user: AuthUser) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);

    // TODO: Implement vector DB operations
    // VECTOR_DB_CLIENT.delete_collection(f"user-memory-{user.id}")

    let _memories = service.get_memories_by_user_id(&user.id).await?;

    // TODO: Re-index all memories in vector DB
    // VECTOR_DB_CLIENT.upsert(
    //     collection_name=f"user-memory-{user.id}",
    //     items=[...],
    // )

    Ok(HttpResponse::Ok().json(true))
}

// DELETE /delete/user - Delete all memories by user ID
async fn delete_memories_by_user(
    db: web::Data<Database>,
    user: AuthUser,
) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);

    service.delete_memories_by_user_id(&user.id).await?;

    // TODO: Delete vector collection
    // VECTOR_DB_CLIENT.delete_collection(f"user-memory-{user.id}")

    Ok(HttpResponse::Ok().json(true))
}

// POST /{memory_id}/update - Update memory by ID
async fn update_memory(
    db: web::Data<Database>,
    user: AuthUser,
    memory_id: web::Path<String>,
    form: web::Json<UpdateMemoryForm>,
) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);

    // First verify the memory exists and belongs to the user
    let existing = service.get_memory_by_id(&memory_id).await?;
    if existing.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "Memory not found"
        })));
    }

    let existing = existing.unwrap();
    if existing.user_id != user.id {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "Memory not found"
        })));
    }

    let memory = service
        .update_memory(&memory_id, form.content.as_deref(), None)
        .await?;

    // TODO: Update vector DB
    // if form.content.is_some() {
    //     VECTOR_DB_CLIENT.upsert(...)
    // }

    let response: MemoryResponse = memory.into();
    Ok(HttpResponse::Ok().json(response))
}

// DELETE /{memory_id} - Delete memory by ID
async fn delete_memory(
    db: web::Data<Database>,
    user: AuthUser,
    memory_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = MemoryService::new(&db);

    // First verify the memory exists and belongs to the user
    let existing = service.get_memory_by_id(&memory_id).await?;
    if existing.is_none() {
        return Ok(HttpResponse::Ok().json(false));
    }

    let existing = existing.unwrap();
    if existing.user_id != user.id {
        return Ok(HttpResponse::Ok().json(false));
    }

    service.delete_memory(&memory_id).await?;

    // TODO: Delete from vector DB
    // VECTOR_DB_CLIENT.delete(
    //     collection_name=f"user-memory-{user.id}",
    //     ids=[memory_id]
    // )

    Ok(HttpResponse::Ok().json(true))
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/memories")
            .route("/ef", web::get().to(get_embeddings))
            .route("", web::get().to(get_memories))
            .route("/add", web::post().to(add_memory))
            .route("/query", web::post().to(query_memory))
            .route("/reset", web::post().to(reset_memory))
            .route("/delete/user", web::delete().to(delete_memories_by_user))
            .route("/{memory_id}/update", web::post().to(update_memory))
            .route("/{memory_id}", web::delete().to(delete_memory)),
    );
}
