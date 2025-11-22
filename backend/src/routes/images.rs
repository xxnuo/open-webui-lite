use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    middleware::{AuthMiddleware, AuthUser},
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIConfigForm {
    #[serde(rename = "OPENAI_API_BASE_URL")]
    openai_api_base_url: String,
    #[serde(rename = "OPENAI_API_VERSION")]
    openai_api_version: String,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Automatic1111ConfigForm {
    #[serde(rename = "AUTOMATIC1111_BASE_URL")]
    automatic1111_base_url: String,
    #[serde(rename = "AUTOMATIC1111_API_AUTH")]
    automatic1111_api_auth: String,
    #[serde(rename = "AUTOMATIC1111_CFG_SCALE")]
    automatic1111_cfg_scale: Option<f64>,
    #[serde(rename = "AUTOMATIC1111_SAMPLER")]
    automatic1111_sampler: Option<String>,
    #[serde(rename = "AUTOMATIC1111_SCHEDULER")]
    automatic1111_scheduler: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ComfyUIConfigForm {
    #[serde(rename = "COMFYUI_BASE_URL")]
    comfyui_base_url: String,
    #[serde(rename = "COMFYUI_API_KEY")]
    comfyui_api_key: String,
    #[serde(rename = "COMFYUI_WORKFLOW")]
    comfyui_workflow: String,
    #[serde(rename = "COMFYUI_WORKFLOW_NODES")]
    comfyui_workflow_nodes: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiConfigForm {
    #[serde(rename = "GEMINI_API_BASE_URL")]
    gemini_api_base_url: String,
    #[serde(rename = "GEMINI_API_KEY")]
    gemini_api_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImagesConfigResponse {
    enabled: bool,
    engine: String,
    prompt_generation: bool,
    openai: OpenAIConfigForm,
    automatic1111: Automatic1111ConfigForm,
    comfyui: ComfyUIConfigForm,
    gemini: GeminiConfigForm,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("/config", web::get().to(get_config))
            .route("/config/update", web::post().to(update_config))
            .route("/image/config", web::get().to(get_image_config))
            .route("/image/config/update", web::post().to(update_image_config))
            .route("/generations", web::post().to(generate_image))
            .route("/models", web::get().to(get_models)),
    );
}

#[derive(Debug, Deserialize)]
struct GenerateImageForm {
    model: String,
    prompt: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    n: Option<i32>,
    #[serde(default)]
    negative_prompt: Option<String>,
}

/// POST /generations - Generate image
async fn generate_image(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    _form_data: web::Json<GenerateImageForm>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_image_generation {
        return Err(AppError::Forbidden(
            "Image generation is not enabled".to_string(),
        ));
    }

    // TODO: Implement image generation based on configured engine
    // - OpenAI DALL-E
    // - Automatic1111
    // - ComfyUI
    // - Gemini

    Err(AppError::NotImplemented(
        "Image generation not yet implemented".to_string(),
    ))
}

/// GET /models - Get available image generation models
async fn get_models(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    if !config.enable_image_generation {
        return Err(AppError::Forbidden(
            "Image generation is not enabled".to_string(),
        ));
    }

    // Return models based on configured engine
    let models = match config.image_generation_engine.as_str() {
        "openai" => vec![
            serde_json::json!({"id": "dall-e-2", "name": "DALL-E 2"}),
            serde_json::json!({"id": "dall-e-3", "name": "DALL-E 3"}),
        ],
        "automatic1111" => {
            vec![serde_json::json!({"id": "stable-diffusion", "name": "Stable Diffusion"})]
        }
        "comfyui" => vec![serde_json::json!({"id": "comfyui", "name": "ComfyUI Workflow"})],
        "gemini" => vec![serde_json::json!({"id": "imagen", "name": "Imagen"})],
        _ => vec![],
    };

    Ok(HttpResponse::Ok().json(models))
}

async fn get_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(ImagesConfigResponse {
        enabled: config.enable_image_generation,
        engine: config.image_generation_engine.clone(),
        prompt_generation: config.enable_image_prompt_generation,
        openai: OpenAIConfigForm {
            openai_api_base_url: config.images_openai_api_base_url.clone(),
            openai_api_version: config.images_openai_api_version.clone(),
            openai_api_key: config.images_openai_api_key.clone(),
        },
        automatic1111: Automatic1111ConfigForm {
            automatic1111_base_url: config.automatic1111_base_url.clone(),
            automatic1111_api_auth: config.automatic1111_api_auth.clone(),
            automatic1111_cfg_scale: config.automatic1111_cfg_scale,
            automatic1111_sampler: config.automatic1111_sampler.clone(),
            automatic1111_scheduler: config.automatic1111_scheduler.clone(),
        },
        comfyui: ComfyUIConfigForm {
            comfyui_base_url: config.comfyui_base_url.clone(),
            comfyui_api_key: config.comfyui_api_key.clone(),
            comfyui_workflow: config.comfyui_workflow.clone(),
            comfyui_workflow_nodes: config.comfyui_workflow_nodes.clone(),
        },
        gemini: GeminiConfigForm {
            gemini_api_base_url: config.images_gemini_api_base_url.clone(),
            gemini_api_key: config.images_gemini_api_key.clone(),
        },
    }))
}

async fn get_image_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let _config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "MODEL": "dall-e-2",
        "IMAGE_SIZE": "512x512",
        "IMAGE_STEPS": 50,
    })))
}

#[derive(Debug, Deserialize)]
struct ImageConfigForm {
    #[serde(rename = "MODEL")]
    model: String,
    #[serde(rename = "IMAGE_SIZE")]
    image_size: String,
    #[serde(rename = "IMAGE_STEPS")]
    image_steps: i32,
}

async fn update_image_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<ImageConfigForm>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Persist to database
    let image_config_json = serde_json::json!({
        "model": form_data.model,
        "size": form_data.image_size,
        "steps": form_data.image_steps,
    });

    let _ =
        crate::services::ConfigService::update_section(&state.db, "image", image_config_json).await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "MODEL": form_data.model,
        "IMAGE_SIZE": form_data.image_size,
        "IMAGE_STEPS": form_data.image_steps,
    })))
}

async fn update_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<ImagesConfigResponse>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let mut config = state.config.write().unwrap();

    config.image_generation_engine = form_data.engine.clone();
    config.enable_image_generation = form_data.enabled;
    config.enable_image_prompt_generation = form_data.prompt_generation;

    // Update OpenAI config
    config.images_openai_api_base_url = form_data.openai.openai_api_base_url.clone();
    config.images_openai_api_version = form_data.openai.openai_api_version.clone();
    config.images_openai_api_key = form_data.openai.openai_api_key.clone();

    // Update Gemini config
    config.images_gemini_api_base_url = form_data.gemini.gemini_api_base_url.clone();
    config.images_gemini_api_key = form_data.gemini.gemini_api_key.clone();

    // Update Automatic1111 config
    config.automatic1111_base_url = form_data.automatic1111.automatic1111_base_url.clone();
    config.automatic1111_api_auth = form_data.automatic1111.automatic1111_api_auth.clone();
    config.automatic1111_cfg_scale = form_data.automatic1111.automatic1111_cfg_scale;
    config.automatic1111_sampler = form_data.automatic1111.automatic1111_sampler.clone();
    config.automatic1111_scheduler = form_data.automatic1111.automatic1111_scheduler.clone();

    // Update ComfyUI config
    config.comfyui_base_url = form_data.comfyui.comfyui_base_url.clone();
    config.comfyui_api_key = form_data.comfyui.comfyui_api_key.clone();
    config.comfyui_workflow = form_data.comfyui.comfyui_workflow.clone();
    config.comfyui_workflow_nodes = form_data.comfyui.comfyui_workflow_nodes.clone();

    // TODO: Persist to database

    Ok(HttpResponse::Ok().json(ImagesConfigResponse {
        enabled: config.enable_image_generation,
        engine: config.image_generation_engine.clone(),
        prompt_generation: config.enable_image_prompt_generation,
        openai: OpenAIConfigForm {
            openai_api_base_url: config.images_openai_api_base_url.clone(),
            openai_api_version: config.images_openai_api_version.clone(),
            openai_api_key: config.images_openai_api_key.clone(),
        },
        automatic1111: Automatic1111ConfigForm {
            automatic1111_base_url: config.automatic1111_base_url.clone(),
            automatic1111_api_auth: config.automatic1111_api_auth.clone(),
            automatic1111_cfg_scale: config.automatic1111_cfg_scale,
            automatic1111_sampler: config.automatic1111_sampler.clone(),
            automatic1111_scheduler: config.automatic1111_scheduler.clone(),
        },
        comfyui: ComfyUIConfigForm {
            comfyui_base_url: config.comfyui_base_url.clone(),
            comfyui_api_key: config.comfyui_api_key.clone(),
            comfyui_workflow: config.comfyui_workflow.clone(),
            comfyui_workflow_nodes: config.comfyui_workflow_nodes.clone(),
        },
        gemini: GeminiConfigForm {
            gemini_api_base_url: config.images_gemini_api_base_url.clone(),
            gemini_api_key: config.images_gemini_api_key.clone(),
        },
    }))
}
