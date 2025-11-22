use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, AppResult},
    middleware::{AdminMiddleware, AuthMiddleware, AuthUser},
    models::feedback::{FeedbackForm, FeedbackModel},
    services::{FeedbackService, UserService},
    AppState,
};

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("/config", web::get().to(get_config))
            .route("/config", web::post().to(update_config))
            .service(
                web::scope("/feedbacks")
                    .service(
                        web::resource("/all")
                            .wrap(AdminMiddleware)
                            .route(web::get().to(get_all_feedbacks))
                            .route(web::delete().to(delete_all_feedbacks)),
                    )
                    .service(
                        web::resource("/all/export")
                            .wrap(AdminMiddleware)
                            .route(web::get().to(export_all_feedbacks)),
                    )
                    .route("/user", web::get().to(get_user_feedbacks))
                    .route("", web::delete().to(delete_user_feedbacks)),
            )
            .route("/feedback", web::post().to(create_feedback))
            .route("/feedback/{id}", web::get().to(get_feedback_by_id))
            .route("/feedback/{id}", web::post().to(update_feedback_by_id))
            .route("/feedback/{id}", web::delete().to(delete_feedback_by_id)),
    );
}

#[derive(Debug, Serialize, Deserialize)]
struct EvaluationConfig {
    #[serde(rename = "ENABLE_EVALUATION_ARENA_MODELS")]
    enable_evaluation_arena_models: bool,
    #[serde(rename = "EVALUATION_ARENA_MODELS")]
    evaluation_arena_models: serde_json::Value,
}

/// GET /config - Get evaluation config (admin only)
async fn get_config(state: web::Data<AppState>, auth_user: AuthUser) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(EvaluationConfig {
        enable_evaluation_arena_models: config.enable_evaluation_arena_models,
        evaluation_arena_models: config.evaluation_arena_models.clone(),
    }))
}

#[derive(Debug, Deserialize)]
struct UpdateConfigForm {
    #[serde(rename = "ENABLE_EVALUATION_ARENA_MODELS")]
    enable_evaluation_arena_models: Option<bool>,
    #[serde(rename = "EVALUATION_ARENA_MODELS")]
    evaluation_arena_models: Option<serde_json::Value>,
}

/// POST /config - Update evaluation config (admin only)
async fn update_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<UpdateConfigForm>,
) -> AppResult<HttpResponse> {
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Update in-memory config
    {
        let mut config = state.config.write().unwrap();
        if let Some(enable) = form_data.enable_evaluation_arena_models {
            config.enable_evaluation_arena_models = enable;
        }
        if let Some(ref models) = form_data.evaluation_arena_models {
            config.evaluation_arena_models = models.clone();
        }
    }

    let config = state.config.read().unwrap();
    Ok(HttpResponse::Ok().json(EvaluationConfig {
        enable_evaluation_arena_models: config.enable_evaluation_arena_models,
        evaluation_arena_models: config.evaluation_arena_models.clone(),
    }))
}

#[derive(Debug, Serialize)]
struct FeedbackUserResponse {
    #[serde(flatten)]
    feedback: FeedbackModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<serde_json::Value>,
}

/// GET /feedbacks/all - Get all feedbacks (admin only)
async fn get_all_feedbacks(
    state: web::Data<AppState>,
    _auth_user: AuthUser, // AdminMiddleware already checked
) -> AppResult<HttpResponse> {
    let feedback_service = FeedbackService::new(&state.db);
    let user_service = UserService::new(&state.db);

    let feedbacks = feedback_service.get_all_feedbacks().await?;

    let mut feedback_list = Vec::new();
    for feedback in feedbacks {
        let user = user_service.get_user_by_id(&feedback.user_id).await?;
        let user_json = user.map(|u| {
            serde_json::json!({
                "id": u.id,
                "name": u.name,
                "email": u.email,
                "role": u.role
            })
        });

        feedback_list.push(FeedbackUserResponse {
            feedback: FeedbackModel::from(feedback),
            user: user_json,
        });
    }

    Ok(HttpResponse::Ok().json(feedback_list))
}

/// DELETE /feedbacks/all - Delete all feedbacks (admin only)
async fn delete_all_feedbacks(
    state: web::Data<AppState>,
    _auth_user: AuthUser, // AdminMiddleware already checked
) -> AppResult<HttpResponse> {
    let feedback_service = FeedbackService::new(&state.db);
    let success = feedback_service.delete_all_feedbacks().await?;
    Ok(HttpResponse::Ok().json(success))
}

/// GET /feedbacks/all/export - Export all feedbacks (admin only)
async fn export_all_feedbacks(
    state: web::Data<AppState>,
    _auth_user: AuthUser, // AdminMiddleware already checked
) -> AppResult<HttpResponse> {
    let feedback_service = FeedbackService::new(&state.db);
    let feedbacks = feedback_service.get_all_feedbacks().await?;
    let feedback_models: Vec<FeedbackModel> =
        feedbacks.into_iter().map(FeedbackModel::from).collect();
    Ok(HttpResponse::Ok().json(feedback_models))
}

/// GET /feedbacks/user - Get current user's feedbacks
async fn get_user_feedbacks(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let feedback_service = FeedbackService::new(&state.db);
    let feedbacks = feedback_service
        .get_feedbacks_by_user_id(&auth_user.user.id)
        .await?;
    let feedback_models: Vec<FeedbackModel> =
        feedbacks.into_iter().map(FeedbackModel::from).collect();
    Ok(HttpResponse::Ok().json(feedback_models))
}

/// DELETE /feedbacks - Delete current user's feedbacks
async fn delete_user_feedbacks(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> AppResult<HttpResponse> {
    let feedback_service = FeedbackService::new(&state.db);
    let success = feedback_service
        .delete_feedbacks_by_user_id(&auth_user.user.id)
        .await?;
    Ok(HttpResponse::Ok().json(success))
}

/// POST /feedback - Create feedback
async fn create_feedback(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<FeedbackForm>,
) -> AppResult<HttpResponse> {
    let feedback_service = FeedbackService::new(&state.db);
    let feedback = feedback_service
        .insert_new_feedback(&auth_user.user.id, &form_data)
        .await?;

    Ok(HttpResponse::Ok().json(FeedbackModel::from(feedback)))
}

/// GET /feedback/{id} - Get feedback by ID
async fn get_feedback_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let feedback_id = path.into_inner();
    let feedback_service = FeedbackService::new(&state.db);

    let feedback = if auth_user.user.role == "admin" {
        feedback_service.get_feedback_by_id(&feedback_id).await?
    } else {
        feedback_service
            .get_feedback_by_id_and_user_id(&feedback_id, &auth_user.user.id)
            .await?
    };

    let feedback = feedback.ok_or_else(|| AppError::NotFound("Feedback not found".to_string()))?;

    Ok(HttpResponse::Ok().json(FeedbackModel::from(feedback)))
}

/// POST /feedback/{id} - Update feedback by ID
async fn update_feedback_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<String>,
    form_data: web::Json<FeedbackForm>,
) -> AppResult<HttpResponse> {
    let feedback_id = path.into_inner();
    let feedback_service = FeedbackService::new(&state.db);

    let feedback = if auth_user.user.role == "admin" {
        feedback_service
            .update_feedback_by_id(&feedback_id, &form_data)
            .await?
    } else {
        feedback_service
            .update_feedback_by_id_and_user_id(&feedback_id, &auth_user.user.id, &form_data)
            .await?
    };

    Ok(HttpResponse::Ok().json(FeedbackModel::from(feedback)))
}

/// DELETE /feedback/{id} - Delete feedback by ID
async fn delete_feedback_by_id(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let feedback_id = path.into_inner();
    let feedback_service = FeedbackService::new(&state.db);

    let success = if auth_user.user.role == "admin" {
        feedback_service.delete_feedback_by_id(&feedback_id).await?
    } else {
        feedback_service
            .delete_feedback_by_id_and_user_id(&feedback_id, &auth_user.user.id)
            .await?
    };

    if !success {
        return Err(AppError::NotFound("Feedback not found".to_string()));
    }

    Ok(HttpResponse::Ok().json(success))
}
