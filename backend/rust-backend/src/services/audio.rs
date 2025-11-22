use crate::config::Config;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TTSRequest {
    pub model: String,
    pub input: String,
    pub voice: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct STTRequest {
    pub model: String,
    pub language: Option<String>,
}

#[allow(dead_code)]
pub struct AudioService {
    client: Client,
    config: Config,
}

#[allow(dead_code)]
impl AudioService {
    pub fn new(config: Config) -> Self {
        AudioService {
            client: Client::new(),
            config,
        }
    }

    /// Text-to-Speech: Convert text to audio
    pub async fn text_to_speech(&self, request: TTSRequest) -> AppResult<Vec<u8>> {
        let engine = if self.config.tts_engine.is_empty() {
            "openai"
        } else {
            self.config.tts_engine.as_str()
        };

        match engine {
            "openai" => self.openai_tts(request).await,
            "azure" => self.azure_tts(request).await,
            "elevenlabs" => self.elevenlabs_tts(request).await,
            _ => Err(AppError::BadRequest("Unsupported TTS engine".to_string())),
        }
    }

    async fn openai_tts(&self, request: TTSRequest) -> AppResult<Vec<u8>> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| AppError::InternalServerError("OpenAI API key not set".to_string()))?;

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/speech")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": request.model,
                "input": request.input,
                "voice": request.voice,
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "OpenAI TTS failed: {}",
                response.status()
            )));
        }

        let audio_data = response
            .bytes()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?
            .to_vec();

        Ok(audio_data)
    }

    async fn azure_tts(&self, request: TTSRequest) -> AppResult<Vec<u8>> {
        let api_key = std::env::var("AZURE_SPEECH_KEY")
            .map_err(|_| AppError::InternalServerError("Azure Speech key not set".to_string()))?;
        let region = std::env::var("AZURE_SPEECH_REGION").map_err(|_| {
            AppError::InternalServerError("Azure Speech region not set".to_string())
        })?;

        let ssml = format!(
            r#"<speak version='1.0' xml:lang='en-US'><voice name='{}'>{}</voice></speak>"#,
            request.voice, request.input
        );

        let response = self
            .client
            .post(format!(
                "https://{}.tts.speech.microsoft.com/cognitiveservices/v1",
                region
            ))
            .header("Ocp-Apim-Subscription-Key", api_key)
            .header("Content-Type", "application/ssml+xml")
            .header(
                "X-Microsoft-OutputFormat",
                "audio-16khz-128kbitrate-mono-mp3",
            )
            .body(ssml)
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "Azure TTS failed: {}",
                response.status()
            )));
        }

        let audio_data = response
            .bytes()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?
            .to_vec();

        Ok(audio_data)
    }

    async fn elevenlabs_tts(&self, request: TTSRequest) -> AppResult<Vec<u8>> {
        let api_key = std::env::var("ELEVENLABS_API_KEY")
            .map_err(|_| AppError::InternalServerError("ElevenLabs API key not set".to_string()))?;

        let response = self
            .client
            .post(format!(
                "https://api.elevenlabs.io/v1/text-to-speech/{}",
                request.voice
            ))
            .header("xi-api-key", api_key)
            .json(&serde_json::json!({
                "text": request.input,
                "model_id": request.model,
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "ElevenLabs TTS failed: {}",
                response.status()
            )));
        }

        let audio_data = response
            .bytes()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?
            .to_vec();

        Ok(audio_data)
    }

    /// Speech-to-Text: Transcribe audio to text
    pub async fn speech_to_text(
        &self,
        audio_data: Vec<u8>,
        request: STTRequest,
    ) -> AppResult<String> {
        let engine = if self.config.stt_engine.is_empty() {
            "whisper"
        } else {
            self.config.stt_engine.as_str()
        };

        match engine {
            "whisper" | "openai" => self.openai_stt(audio_data, request).await,
            "azure" => self.azure_stt(audio_data, request).await,
            _ => Err(AppError::BadRequest("Unsupported STT engine".to_string())),
        }
    }

    async fn openai_stt(&self, audio_data: Vec<u8>, request: STTRequest) -> AppResult<String> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| AppError::InternalServerError("OpenAI API key not set".to_string()))?;

        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio_data).file_name("audio.mp3"),
            )
            .text("model", request.model);

        let form = if let Some(language) = request.language {
            form.text("language", language)
        } else {
            form
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "OpenAI STT failed: {}",
                response.status()
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        let text = result["text"]
            .as_str()
            .ok_or_else(|| AppError::ExternalServiceError("No transcription text".to_string()))?
            .to_string();

        Ok(text)
    }

    async fn azure_stt(&self, audio_data: Vec<u8>, _request: STTRequest) -> AppResult<String> {
        let api_key = std::env::var("AZURE_SPEECH_KEY")
            .map_err(|_| AppError::InternalServerError("Azure Speech key not set".to_string()))?;
        let region = std::env::var("AZURE_SPEECH_REGION").map_err(|_| {
            AppError::InternalServerError("Azure Speech region not set".to_string())
        })?;

        let response = self
            .client
            .post(format!(
                "https://{}.stt.speech.microsoft.com/speech/recognition/conversation/cognitiveservices/v1",
                region
            ))
            .header("Ocp-Apim-Subscription-Key", api_key)
            .header("Content-Type", "audio/wav")
            .body(audio_data)
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "Azure STT failed: {}",
                response.status()
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        let text = result["DisplayText"]
            .as_str()
            .ok_or_else(|| AppError::InternalServerError("No transcription text".to_string()))?
            .to_string();

        Ok(text)
    }
}
