use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use futures_util::StreamExt as _;
use serde::Deserialize;
use std::io::Write;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::middleware::auth::{AdminMiddleware, AuthUser};
use crate::models::file::FileResponse;
use crate::services::file::FileService;

#[derive(Debug, Deserialize)]
pub struct FileContentForm {
    pub content: String,
}

// GET / - List files
async fn list_files(
    db: web::Data<Database>,
    user: AuthUser,
    query: web::Query<ContentQuery>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let mut files = if user.role == "admin" {
        service.get_all_files().await?
    } else {
        service.get_files_by_user_id(&user.id).await?
    };

    // Remove content from data if not requested
    if !query.content.unwrap_or(true) {
        for file in &mut files {
            file.parse_json_fields();
            if let Some(ref mut data) = file.data {
                if let Some(obj) = data.as_object_mut() {
                    obj.remove("content");
                }
            }
        }
    }

    let responses: Vec<FileResponse> = files.into_iter().map(|f| f.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

#[derive(Debug, Deserialize)]
struct ContentQuery {
    content: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    filename: String,
    content: Option<bool>,
}

// GET /search - Search files
async fn search_files(
    db: web::Data<Database>,
    user: AuthUser,
    query: web::Query<SearchQuery>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let files = if user.role == "admin" {
        service.get_all_files().await?
    } else {
        service.get_files_by_user_id(&user.id).await?
    };

    // Filter files by filename pattern (simple case-insensitive contains for now)
    let pattern = query.filename.to_lowercase();
    let mut matching_files: Vec<_> = files
        .into_iter()
        .filter(|f| f.filename.to_lowercase().contains(&pattern))
        .collect();

    if matching_files.is_empty() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "No files found matching the pattern"
        })));
    }

    // Remove content from data if not requested
    if !query.content.unwrap_or(true) {
        for file in &mut matching_files {
            file.parse_json_fields();
            if let Some(ref mut data) = file.data {
                if let Some(obj) = data.as_object_mut() {
                    obj.remove("content");
                }
            }
        }
    }

    let responses: Vec<FileResponse> = matching_files.into_iter().map(|f| f.into()).collect();
    Ok(HttpResponse::Ok().json(responses))
}

// POST / - Upload file
async fn upload_file(
    state: web::Data<crate::AppState>,
    db: web::Data<Database>,
    user: AuthUser,
    mut payload: Multipart,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let mut filename = String::new();
    let mut file_data = Vec::new();

    // Process multipart fields
    while let Some(field) = payload.next().await {
        let mut field =
            field.map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?;

        let content_disposition = field.content_disposition();
        let field_name = content_disposition
            .and_then(|cd| cd.get_name())
            .unwrap_or("");

        if field_name == "file" {
            filename = content_disposition
                .and_then(|cd| cd.get_filename())
                .unwrap_or("unnamed")
                .to_string();

            // Read file data
            while let Some(chunk) = field.next().await {
                let chunk =
                    chunk.map_err(|e| AppError::BadRequest(format!("Chunk read error: {}", e)))?;
                file_data.extend_from_slice(&chunk);
            }
        }
    }

    if filename.is_empty() || file_data.is_empty() {
        return Err(AppError::BadRequest("No file uploaded".to_string()));
    }

    // Generate file ID
    let file_id = uuid::Uuid::new_v4().to_string();

    // Get upload directory from config
    let config = state.config.read().unwrap();
    let upload_dir = std::path::PathBuf::from(&config.upload_dir);
    drop(config);
    
    // Create upload directory if it doesn't exist
    std::fs::create_dir_all(&upload_dir)
        .map_err(|e| AppError::BadRequest(format!("Failed to create upload directory: {}", e)))?;

    // Save file to disk
    let file_path = upload_dir.join(&file_id);
    let mut f = std::fs::File::create(&file_path)
        .map_err(|e| AppError::BadRequest(format!("Failed to create file: {}", e)))?;
    f.write_all(&file_data)
        .map_err(|e| AppError::BadRequest(format!("Failed to write file: {}", e)))?;

    // Calculate file hash
    let hash = format!("{:x}", md5::compute(&file_data));

    // Create file metadata
    let meta = serde_json::json!({
        "source": "upload",
        "size": file_data.len(),
        "content_type": mime_guess::from_path(&filename).first_or_octet_stream().to_string(),
    });

    // Create file record in database
    let file = service
        .create_file(&file_id, &user.id, &filename, &hash, Some(meta))
        .await?;

    Ok(HttpResponse::Ok().json(FileResponse::from(file)))
}

// DELETE /all - Delete all files (admin only)
async fn delete_all_files(db: web::Data<Database>, _user: AuthUser) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    // TODO: Delete from storage
    // TODO: Reset vector DB

    service.delete_all_files().await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "All files deleted successfully"
    })))
}

// GET /{id} - Get file by ID
async fn get_file(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let file = file.unwrap();

    // Check access: owner, admin, or has knowledge base access
    if file.user_id != user.id && user.role != "admin" {
        // TODO: Check knowledge base access via has_access_to_file
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let response: FileResponse = file.into();
    Ok(HttpResponse::Ok().json(response))
}

// GET /{id}/process/status - Get file process status
async fn get_file_process_status(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let mut file = file.unwrap();

    // Check access
    if file.user_id != user.id && user.role != "admin" {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    file.parse_json_fields();

    let status = if let Some(ref data) = file.data {
        data.get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("pending")
            .to_string()
    } else {
        "pending".to_string()
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": status
    })))
}

// GET /{id}/data/content - Get file data content
async fn get_file_data_content(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let mut file = file.unwrap();

    // Check access
    if file.user_id != user.id && user.role != "admin" {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    file.parse_json_fields();

    let content = if let Some(ref data) = file.data {
        data.get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "content": content
    })))
}

// POST /{id}/data/content/update - Update file data content
async fn update_file_data_content(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
    form: web::Json<FileContentForm>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let mut file = file.unwrap();

    // Check access
    if file.user_id != user.id && user.role != "admin" {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    file.parse_json_fields();

    // Update data with new content
    let mut data = file.data.unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = data.as_object_mut() {
        obj.insert("content".to_string(), serde_json::json!(form.content));
    }

    let updated_file = service.update_file_data(&file_id, data.clone()).await?;

    // TODO: Process file content via retrieval system

    let content = if let Some(ref data_val) = data.as_object() {
        data_val
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "content": content
    })))
}

// GET /{id}/content - Get file content (download)
async fn get_file_content(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let file = file.unwrap();

    // Check access
    if file.user_id != user.id && user.role != "admin" {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    // TODO: Implement file download from storage
    // This requires storage provider integration and FileResponse streaming

    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "detail": "File download not yet implemented - requires storage integration"
    })))
}

// POST /{id}/update - Update file metadata
async fn update_file(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
    form: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let file = file.unwrap();

    // Check access
    if file.user_id != user.id && user.role != "admin" {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    // Update metadata
    let meta = form.into_inner();
    let updated_file = service.update_file_metadata(&file_id, meta).await?;

    let response: FileResponse = updated_file.into();
    Ok(HttpResponse::Ok().json(response))
}

// DELETE /{id} - Delete file
async fn delete_file(
    db: web::Data<Database>,
    user: AuthUser,
    file_id: web::Path<String>,
) -> AppResult<HttpResponse> {
    let service = FileService::new(&db);

    let file = service.get_file_by_id(&file_id).await?;

    if file.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    let file = file.unwrap();

    // Check access
    if file.user_id != user.id && user.role != "admin" {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "File not found"
        })));
    }

    service.delete_file(&file_id).await?;

    // TODO: Delete from storage
    // TODO: Delete from vector DB collection

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "File deleted successfully"
    })))
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/files")
            .route("", web::get().to(list_files))
            .route("/search", web::get().to(search_files))
            .route("", web::post().to(upload_file))
            .route("/{id}", web::get().to(get_file))
            .route(
                "/{id}/process/status",
                web::get().to(get_file_process_status),
            )
            .route("/{id}/data/content", web::get().to(get_file_data_content))
            .route(
                "/{id}/data/content/update",
                web::post().to(update_file_data_content),
            )
            .route("/{id}/content", web::get().to(get_file_content))
            .route("/{id}/update", web::post().to(update_file))
            .route("/{id}", web::delete().to(delete_file)),
    )
    .service(
        web::resource("/files/all")
            .wrap(AdminMiddleware)
            .route(web::delete().to(delete_all_files)),
    );
}
