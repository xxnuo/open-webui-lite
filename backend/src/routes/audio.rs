use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::AppError,
    middleware::{AuthMiddleware, AuthUser},
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
struct TTSConfigForm {
    #[serde(rename = "OPENAI_API_BASE_URL")]
    openai_api_base_url: String,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: String,
    #[serde(rename = "API_KEY")]
    api_key: String,
    #[serde(rename = "ENGINE")]
    engine: String,
    #[serde(rename = "MODEL")]
    model: String,
    #[serde(rename = "VOICE")]
    voice: String,
    #[serde(rename = "SPLIT_ON")]
    split_on: String,
    #[serde(rename = "AZURE_SPEECH_REGION")]
    azure_speech_region: String,
    #[serde(rename = "AZURE_SPEECH_BASE_URL")]
    azure_speech_base_url: String,
    #[serde(rename = "AZURE_SPEECH_OUTPUT_FORMAT")]
    azure_speech_output_format: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct STTConfigForm {
    #[serde(rename = "OPENAI_API_BASE_URL")]
    openai_api_base_url: String,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: String,
    #[serde(rename = "ENGINE")]
    engine: String,
    #[serde(rename = "MODEL")]
    model: String,
    #[serde(rename = "SUPPORTED_CONTENT_TYPES", default)]
    supported_content_types: Vec<String>,
    #[serde(rename = "WHISPER_MODEL")]
    whisper_model: String,
    #[serde(rename = "DEEPGRAM_API_KEY")]
    deepgram_api_key: String,
    #[serde(rename = "AZURE_API_KEY")]
    azure_api_key: String,
    #[serde(rename = "AZURE_REGION")]
    azure_region: String,
    #[serde(rename = "AZURE_LOCALES")]
    azure_locales: String,
    #[serde(rename = "AZURE_BASE_URL")]
    azure_base_url: String,
    #[serde(rename = "AZURE_MAX_SPEAKERS")]
    azure_max_speakers: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AudioConfigResponse {
    tts: TTSConfigForm,
    stt: STTConfigForm,
}

#[derive(Debug, Deserialize)]
struct SpeechRequest {
    input: String,
    model: Option<String>,
    voice: Option<String>,
    response_format: Option<String>,
}

#[derive(Debug, Serialize)]
struct ModelInfo {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct VoiceInfo {
    id: String,
    name: String,
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AuthMiddleware)
            .route("/config", web::get().to(get_audio_config))
            .route("/config/update", web::post().to(update_audio_config))
            .route("/speech", web::post().to(speech))
            .route("/transcriptions", web::post().to(transcriptions))
            .route("/models", web::get().to(get_models))
            .route("/voices", web::get().to(get_voices)),
    );
}

async fn get_audio_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Only admins can access this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(AudioConfigResponse {
        tts: TTSConfigForm {
            openai_api_base_url: config.tts_openai_api_base_url.clone(),
            openai_api_key: config.tts_openai_api_key.clone(),
            api_key: config.tts_api_key.clone(),
            engine: config.tts_engine.clone(),
            model: config.tts_model.clone(),
            voice: config.tts_voice.clone(),
            split_on: config.tts_split_on.clone(),
            azure_speech_region: config.tts_azure_speech_region.clone(),
            azure_speech_base_url: config.tts_azure_speech_base_url.clone(),
            azure_speech_output_format: config.tts_azure_speech_output_format.clone(),
        },
        stt: STTConfigForm {
            openai_api_base_url: config.stt_openai_api_base_url.clone(),
            openai_api_key: config.stt_openai_api_key.clone(),
            engine: config.stt_engine.clone(),
            model: config.stt_model.clone(),
            supported_content_types: config.stt_supported_content_types.clone(),
            whisper_model: config.whisper_model.clone(),
            deepgram_api_key: config.deepgram_api_key.clone(),
            azure_api_key: config.audio_stt_azure_api_key.clone(),
            azure_region: config.audio_stt_azure_region.clone(),
            azure_locales: config.audio_stt_azure_locales.clone(),
            azure_base_url: config.audio_stt_azure_base_url.clone(),
            azure_max_speakers: config.audio_stt_azure_max_speakers.clone(),
        },
    }))
}

async fn update_audio_config(
    state: web::Data<AppState>,
    auth_user: AuthUser,
    form_data: web::Json<AudioConfigResponse>,
) -> Result<HttpResponse, AppError> {
    // Only admins can update this
    if auth_user.user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    let mut config = state.config.write().unwrap();

    // Update TTS config
    config.tts_openai_api_base_url = form_data.tts.openai_api_base_url.clone();
    config.tts_openai_api_key = form_data.tts.openai_api_key.clone();
    config.tts_api_key = form_data.tts.api_key.clone();
    config.tts_engine = form_data.tts.engine.clone();
    config.tts_model = form_data.tts.model.clone();
    config.tts_voice = form_data.tts.voice.clone();
    config.tts_split_on = form_data.tts.split_on.clone();
    config.tts_azure_speech_region = form_data.tts.azure_speech_region.clone();
    config.tts_azure_speech_base_url = form_data.tts.azure_speech_base_url.clone();
    config.tts_azure_speech_output_format = form_data.tts.azure_speech_output_format.clone();

    // Update STT config
    config.stt_openai_api_base_url = form_data.stt.openai_api_base_url.clone();
    config.stt_openai_api_key = form_data.stt.openai_api_key.clone();
    config.stt_engine = form_data.stt.engine.clone();
    config.stt_model = form_data.stt.model.clone();
    config.stt_supported_content_types = form_data.stt.supported_content_types.clone();
    config.whisper_model = form_data.stt.whisper_model.clone();
    config.deepgram_api_key = form_data.stt.deepgram_api_key.clone();
    config.audio_stt_azure_api_key = form_data.stt.azure_api_key.clone();
    config.audio_stt_azure_region = form_data.stt.azure_region.clone();
    config.audio_stt_azure_locales = form_data.stt.azure_locales.clone();
    config.audio_stt_azure_base_url = form_data.stt.azure_base_url.clone();
    config.audio_stt_azure_max_speakers = form_data.stt.azure_max_speakers.clone();

    // Persist to database
    let audio_config_json = serde_json::json!({
        "tts": {
            "openai_api_base_url": config.tts_openai_api_base_url,
            "openai_api_key": config.tts_openai_api_key,
            "api_key": config.tts_api_key,
            "engine": config.tts_engine,
            "model": config.tts_model,
            "voice": config.tts_voice,
            "split_on": config.tts_split_on,
            "azure_speech_region": config.tts_azure_speech_region,
            "azure_speech_base_url": config.tts_azure_speech_base_url,
            "azure_speech_output_format": config.tts_azure_speech_output_format,
        },
        "stt": {
            "openai_api_base_url": config.stt_openai_api_base_url,
            "openai_api_key": config.stt_openai_api_key,
            "engine": config.stt_engine,
            "model": config.stt_model,
            "supported_content_types": config.stt_supported_content_types,
            "whisper_model": config.whisper_model,
            "deepgram_api_key": config.deepgram_api_key,
            "azure_api_key": config.audio_stt_azure_api_key,
            "azure_region": config.audio_stt_azure_region,
            "azure_locales": config.audio_stt_azure_locales,
            "azure_base_url": config.audio_stt_azure_base_url,
            "azure_max_speakers": config.audio_stt_azure_max_speakers,
        },
    });

    drop(config);

    let _ =
        crate::services::ConfigService::update_section(&state.db, "audio", audio_config_json).await;

    let config = state.config.read().unwrap();

    Ok(HttpResponse::Ok().json(AudioConfigResponse {
        tts: TTSConfigForm {
            openai_api_base_url: config.tts_openai_api_base_url.clone(),
            openai_api_key: config.tts_openai_api_key.clone(),
            api_key: config.tts_api_key.clone(),
            engine: config.tts_engine.clone(),
            model: config.tts_model.clone(),
            voice: config.tts_voice.clone(),
            split_on: config.tts_split_on.clone(),
            azure_speech_region: config.tts_azure_speech_region.clone(),
            azure_speech_base_url: config.tts_azure_speech_base_url.clone(),
            azure_speech_output_format: config.tts_azure_speech_output_format.clone(),
        },
        stt: STTConfigForm {
            openai_api_base_url: config.stt_openai_api_base_url.clone(),
            openai_api_key: config.stt_openai_api_key.clone(),
            engine: config.stt_engine.clone(),
            model: config.stt_model.clone(),
            supported_content_types: config.stt_supported_content_types.clone(),
            whisper_model: config.whisper_model.clone(),
            deepgram_api_key: config.deepgram_api_key.clone(),
            azure_api_key: config.audio_stt_azure_api_key.clone(),
            azure_region: config.audio_stt_azure_region.clone(),
            azure_locales: config.audio_stt_azure_locales.clone(),
            azure_base_url: config.audio_stt_azure_base_url.clone(),
            azure_max_speakers: config.audio_stt_azure_max_speakers.clone(),
        },
    }))
}

// Speech (TTS) endpoint - proxies to configured TTS engine
async fn speech(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    payload: web::Json<SpeechRequest>,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    // Forward to configured TTS engine (OpenAI-compatible endpoint)
    if config.tts_engine == "openai" {
        let client = reqwest::Client::new();

        let tts_payload = json!({
            "model": payload.model.as_ref().unwrap_or(&config.tts_model),
            "input": payload.input,
            "voice": payload.voice.as_ref().unwrap_or(&config.tts_voice),
            "response_format": payload.response_format.as_deref().unwrap_or("mp3"),
        });

        let response = client
            .post(format!("{}/audio/speech", config.tts_openai_api_base_url))
            .header(
                "Authorization",
                format!("Bearer {}", config.tts_openai_api_key),
            )
            .json(&tts_payload)
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(format!("TTS request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalServiceError(format!(
                "TTS engine error: {}",
                error_text
            )));
        }

        let audio_bytes = response.bytes().await.map_err(|e| {
            AppError::ExternalServiceError(format!("Failed to read TTS response: {}", e))
        })?;

        return Ok(HttpResponse::Ok()
            .content_type("audio/mpeg")
            .body(audio_bytes));
    }

    // TODO: Implement other TTS engines (elevenlabs, azure, transformers)
    Err(AppError::NotImplemented(format!(
        "TTS engine '{}' not yet implemented",
        config.tts_engine
    )))
}

// Transcriptions (STT) endpoint - proxies to configured STT engine
async fn transcriptions(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
    _payload: actix_multipart::Multipart,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    // TODO: Implement file upload handling with multipart
    // TODO: Forward to configured STT engine
    // For now, return a placeholder

    if config.stt_engine == "openai" {
        // Forward to OpenAI-compatible STT endpoint
        // This requires proper multipart form handling
        return Err(AppError::NotImplemented(
            "STT endpoint not fully implemented yet".to_string(),
        ));
    }

    Err(AppError::NotImplemented(format!(
        "STT engine '{}' not yet implemented",
        config.stt_engine
    )))
}

// Get available TTS models
async fn get_models(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    let mut models = Vec::new();

    match config.tts_engine.as_str() {
        "openai" => {
            // Try to fetch from custom endpoint if not official OpenAI
            if !config
                .tts_openai_api_base_url
                .starts_with("https://api.openai.com")
            {
                let client = reqwest::Client::new();
                if let Ok(response) = client
                    .get(format!("{}/audio/models", config.tts_openai_api_base_url))
                    .send()
                    .await
                {
                    if let Ok(data) = response.json::<serde_json::Value>().await {
                        if let Some(model_list) = data["models"].as_array() {
                            models = model_list
                                .iter()
                                .filter_map(|m| {
                                    m["id"].as_str().map(|id| ModelInfo {
                                        id: id.to_string(),
                                        name: m["name"].as_str().map(|s| s.to_string()),
                                    })
                                })
                                .collect();
                        }
                    }
                }
            }

            // Fallback to defaults if no models found
            if models.is_empty() {
                models = vec![
                    ModelInfo {
                        id: "tts-1".to_string(),
                        name: None,
                    },
                    ModelInfo {
                        id: "tts-1-hd".to_string(),
                        name: None,
                    },
                ];
            }
        }
        "elevenlabs" => {
            // Fetch from Elevenlabs API
            let client = reqwest::Client::new();
            if let Ok(response) = client
                .get("https://api.elevenlabs.io/v1/models")
                .header("xi-api-key", &config.tts_api_key)
                .header("Content-Type", "application/json")
                .send()
                .await
            {
                if let Ok(data) = response.json::<serde_json::Value>().await {
                    if let Some(model_list) = data.as_array() {
                        models = model_list
                            .iter()
                            .filter_map(|m| {
                                Some(ModelInfo {
                                    id: m["model_id"].as_str()?.to_string(),
                                    name: Some(m["name"].as_str()?.to_string()),
                                })
                            })
                            .collect();
                    }
                }
            }
        }
        _ => {
            // Return empty for other engines
        }
    }

    Ok(HttpResponse::Ok().json(json!({ "models": models })))
}

// Get available TTS voices
async fn get_voices(
    state: web::Data<AppState>,
    _auth_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let config = state.config.read().unwrap();

    let mut voices = Vec::new();

    match config.tts_engine.as_str() {
        "openai" => {
            // Try to fetch from custom endpoint if not official OpenAI
            if !config
                .tts_openai_api_base_url
                .starts_with("https://api.openai.com")
            {
                let client = reqwest::Client::new();
                if let Ok(response) = client
                    .get(format!("{}/audio/voices", config.tts_openai_api_base_url))
                    .send()
                    .await
                {
                    if let Ok(data) = response.json::<serde_json::Value>().await {
                        if let Some(voice_list) = data["voices"].as_array() {
                            voices = voice_list
                                .iter()
                                .filter_map(|v| {
                                    Some(VoiceInfo {
                                        id: v["id"].as_str()?.to_string(),
                                        name: v["name"].as_str()?.to_string(),
                                    })
                                })
                                .collect();
                        }
                    }
                }
            }

            // Fallback to defaults if no voices found
            if voices.is_empty() {
                voices = vec![
                    VoiceInfo {
                        id: "alloy".to_string(),
                        name: "alloy".to_string(),
                    },
                    VoiceInfo {
                        id: "echo".to_string(),
                        name: "echo".to_string(),
                    },
                    VoiceInfo {
                        id: "fable".to_string(),
                        name: "fable".to_string(),
                    },
                    VoiceInfo {
                        id: "onyx".to_string(),
                        name: "onyx".to_string(),
                    },
                    VoiceInfo {
                        id: "nova".to_string(),
                        name: "nova".to_string(),
                    },
                    VoiceInfo {
                        id: "shimmer".to_string(),
                        name: "shimmer".to_string(),
                    },
                ];
            }
        }
        "elevenlabs" => {
            // Fetch from Elevenlabs API
            let client = reqwest::Client::new();
            if let Ok(response) = client
                .get("https://api.elevenlabs.io/v1/voices")
                .header("xi-api-key", &config.tts_api_key)
                .header("Content-Type", "application/json")
                .send()
                .await
            {
                if let Ok(data) = response.json::<serde_json::Value>().await {
                    if let Some(voice_list) = data["voices"].as_array() {
                        voices = voice_list
                            .iter()
                            .filter_map(|v| {
                                Some(VoiceInfo {
                                    id: v["voice_id"].as_str()?.to_string(),
                                    name: v["name"].as_str()?.to_string(),
                                })
                            })
                            .collect();
                    }
                }
            }
        }
        "azure" => {
            // Fetch from Azure Cognitive Services
            let region = config.tts_azure_speech_region.clone();
            let base_url = config.tts_azure_speech_base_url.clone();
            let url = if !base_url.is_empty() {
                format!("{}/cognitiveservices/voices/list", base_url)
            } else {
                format!(
                    "https://{}.tts.speech.microsoft.com/cognitiveservices/voices/list",
                    region
                )
            };

            let client = reqwest::Client::new();
            if let Ok(response) = client
                .get(&url)
                .header("Ocp-Apim-Subscription-Key", &config.tts_api_key)
                .send()
                .await
            {
                if let Ok(voice_list) = response.json::<Vec<serde_json::Value>>().await {
                    voices = voice_list
                        .iter()
                        .filter_map(|v| {
                            let short_name = v["ShortName"].as_str()?.to_string();
                            let display_name = v["DisplayName"].as_str()?;
                            Some(VoiceInfo {
                                id: short_name.clone(),
                                name: format!("{} ({})", display_name, short_name),
                            })
                        })
                        .collect();
                }
            }
        }
        _ => {
            // Return empty for other engines
        }
    }

    Ok(HttpResponse::Ok().json(json!({ "voices": voices })))
}
