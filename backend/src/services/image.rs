use crate::config::Config;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>,
    pub n: Option<usize>,
    pub quality: Option<String>,
    pub style: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
    pub images: Vec<ImageData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageData {
    pub url: Option<String>,
    pub b64_json: Option<String>,
}

#[allow(dead_code)]
pub struct ImageService {
    client: Client,
    config: Config,
}

#[allow(dead_code)]
impl ImageService {
    pub fn new(config: Config) -> Self {
        ImageService {
            client: Client::new(),
            config,
        }
    }

    pub async fn generate_image(
        &self,
        request: ImageGenerationRequest,
    ) -> AppResult<ImageGenerationResponse> {
        let engine = if self.config.image_generation_engine.is_empty() {
            "openai"
        } else {
            self.config.image_generation_engine.as_str()
        };

        match engine {
            "openai" => self.openai_generate(request).await,
            "automatic1111" => self.automatic1111_generate(request).await,
            "comfyui" => self.comfyui_generate(request).await,
            _ => Err(AppError::BadRequest(
                "Unsupported image generation engine".to_string(),
            )),
        }
    }

    async fn openai_generate(
        &self,
        request: ImageGenerationRequest,
    ) -> AppResult<ImageGenerationResponse> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| AppError::InternalServerError("OpenAI API key not set".to_string()))?;

        let model = request.model.unwrap_or_else(|| "dall-e-3".to_string());
        let size = request.size.unwrap_or_else(|| "1024x1024".to_string());
        let n = request.n.unwrap_or(1);
        let quality = request.quality.unwrap_or_else(|| "standard".to_string());

        let response = self
            .client
            .post("https://api.openai.com/v1/images/generations")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": model,
                "prompt": request.prompt,
                "n": n,
                "size": size,
                "quality": quality,
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalServiceError(format!(
                "OpenAI image generation failed: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        let images = result["data"]
            .as_array()
            .ok_or_else(|| AppError::ExternalServiceError("No images in response".to_string()))?
            .iter()
            .map(|img| ImageData {
                url: img["url"].as_str().map(String::from),
                b64_json: img["b64_json"].as_str().map(String::from),
            })
            .collect();

        Ok(ImageGenerationResponse { images })
    }

    async fn automatic1111_generate(
        &self,
        request: ImageGenerationRequest,
    ) -> AppResult<ImageGenerationResponse> {
        let base_url = std::env::var("AUTOMATIC1111_URL")
            .unwrap_or_else(|_| "http://localhost:7860".to_string());

        let n = request.n.unwrap_or(1);

        // Parse size
        let (width, height) = if let Some(size) = request.size {
            let parts: Vec<&str> = size.split('x').collect();
            if parts.len() == 2 {
                (
                    parts[0].parse::<i32>().unwrap_or(512),
                    parts[1].parse::<i32>().unwrap_or(512),
                )
            } else {
                (512, 512)
            }
        } else {
            (512, 512)
        };

        let response = self
            .client
            .post(format!("{}/sdapi/v1/txt2img", base_url))
            .json(&serde_json::json!({
                "prompt": request.prompt,
                "negative_prompt": "",
                "batch_size": n,
                "width": width,
                "height": height,
                "steps": 20,
                "cfg_scale": 7,
                "sampler_name": "Euler a",
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "Automatic1111 generation failed: {}",
                response.status()
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        let images = result["images"]
            .as_array()
            .ok_or_else(|| AppError::ExternalServiceError("No images in response".to_string()))?
            .iter()
            .map(|img| ImageData {
                url: None,
                b64_json: img.as_str().map(String::from),
            })
            .collect();

        Ok(ImageGenerationResponse { images })
    }

    async fn comfyui_generate(
        &self,
        request: ImageGenerationRequest,
    ) -> AppResult<ImageGenerationResponse> {
        let base_url =
            std::env::var("COMFYUI_URL").unwrap_or_else(|_| "http://localhost:8188".to_string());

        // ComfyUI requires a workflow JSON
        // This is a simplified example - in practice you'd need a full workflow
        let workflow = serde_json::json!({
            "prompt": {
                "1": {
                    "inputs": {
                        "text": request.prompt,
                    },
                    "class_type": "CLIPTextEncode",
                },
                "2": {
                    "inputs": {
                        "samples": ["3", 0],
                        "vae": ["4", 0],
                    },
                    "class_type": "VAEDecode",
                },
                "3": {
                    "inputs": {
                        "seed": 12345,
                        "steps": 20,
                        "cfg": 7.0,
                        "sampler_name": "euler",
                        "scheduler": "normal",
                        "positive": ["1", 0],
                        "model": ["4", 0],
                    },
                    "class_type": "KSampler",
                },
                "4": {
                    "inputs": {
                        "ckpt_name": "model.safetensors",
                    },
                    "class_type": "CheckpointLoaderSimple",
                },
            }
        });

        let response = self
            .client
            .post(format!("{}/prompt", base_url))
            .json(&workflow)
            .send()
            .await
            .map_err(|e| AppError::ExternalServiceError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "ComfyUI generation failed: {}",
                response.status()
            )));
        }

        // ComfyUI workflow is more complex and requires polling for results
        // This is a simplified response
        Ok(ImageGenerationResponse {
            images: vec![ImageData {
                url: Some(format!("{}/view", base_url)),
                b64_json: None,
            }],
        })
    }
}
