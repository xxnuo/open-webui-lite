# Command Line Interface (CLI) Documentation

This document describes the environment variables available for configuring the Open WebUI Lite backend server.

## Server Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `HOST` | `0.0.0.0` | Server host address |
| `PORT` | `8080` | Server port number |
| `ENABLE_RANDOM_PORT` | `false` | Enable random port assignment (OS will assign an available port) |
| `ENV` | `production` | Environment mode |
| `WEBUI_SECRET_KEY` | Auto-generated UUID | Secret key for WebUI session management |

## Configuration Directory

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `CONFIG_DIR` | `~/.config/open-webui-lite` | Configuration and data directory path |

## Database Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `DATABASE_URL` | `sqlite://{CONFIG_DIR}/data.sqlite3` | Database connection URL |
| `DATABASE_POOL_SIZE` | `10` | Database connection pool size |
| `DATABASE_POOL_MAX_OVERFLOW` | `10` | Maximum overflow connections |
| `DATABASE_POOL_TIMEOUT` | `30` | Connection timeout in seconds |
| `DATABASE_POOL_RECYCLE` | `3600` | Connection recycle time in seconds |

## Redis Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_REDIS` | `false` | Enable Redis support |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection URL |

## Authentication

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `JWT_EXPIRES_IN` | `168h` | JWT token expiration time |
| `ENABLE_SIGNUP` | `true` | Enable user registration |
| `ENABLE_LOGIN_FORM` | `true` | Enable login form |
| `ENABLE_API_KEY` | `true` | Enable API key authentication |
| `ENABLE_API_KEY_ENDPOINT_RESTRICTIONS` | `false` | Enable API key endpoint restrictions |
| `API_KEY_ALLOWED_ENDPOINTS` | `` | Comma-separated list of allowed endpoints for API keys |
| `DEFAULT_USER_ROLE` | `pending` | Default role for new users |
| `SHOW_ADMIN_DETAILS` | `true` | Show admin details |
| `WEBUI_URL` | `http://localhost:8080` | WebUI URL |
| `PENDING_USER_OVERLAY_TITLE` | - | Title for pending user overlay |
| `PENDING_USER_OVERLAY_CONTENT` | - | Content for pending user overlay |
| `RESPONSE_WATERMARK` | - | Watermark for responses |

## LDAP Authentication

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_LDAP` | `false` | Enable LDAP authentication |
| `LDAP_SERVER_LABEL` | `LDAP Server` | LDAP server label |
| `LDAP_SERVER_HOST` | `localhost` | LDAP server host |
| `LDAP_SERVER_PORT` | - | LDAP server port |
| `LDAP_ATTRIBUTE_FOR_USERNAME` | `uid` | LDAP attribute for username |
| `LDAP_ATTRIBUTE_FOR_MAIL` | `mail` | LDAP attribute for email |
| `LDAP_APP_DN` | `` | LDAP application DN |
| `LDAP_APP_PASSWORD` | `` | LDAP application password |
| `LDAP_SEARCH_BASE` | `` | LDAP search base |
| `LDAP_SEARCH_FILTERS` | `` | LDAP search filters |
| `LDAP_USE_TLS` | `true` | Use TLS for LDAP connection |
| `LDAP_CA_CERT_FILE` | - | Path to LDAP CA certificate file |
| `LDAP_VALIDATE_CERT` | `true` | Validate LDAP certificate |
| `LDAP_CIPHERS` | - | LDAP cipher suite |

## SCIM 2.0

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `SCIM_ENABLED` | `false` | Enable SCIM 2.0 support |
| `SCIM_TOKEN` | `` | SCIM authentication token |

## CORS Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `CORS_ALLOW_ORIGIN` | `*` | CORS allowed origins |

## WebSocket Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_WEBSOCKET_SUPPORT` | `true` | Enable WebSocket support |
| `WEBSOCKET_MANAGER` | `local` | WebSocket manager type |
| `WEBSOCKET_REDIS_URL` | - | Redis URL for WebSocket manager |

## Features

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_OPENAI_API` | `true` | Enable OpenAI API compatibility |
| `ENABLE_CHANNELS` | `false` | Enable channels feature |
| `ENABLE_IMAGE_GENERATION` | `false` | Enable image generation |
| `ENABLE_CODE_EXECUTION` | `false` | Enable code execution |
| `ENABLE_WEB_SEARCH` | `false` | Enable web search |
| `ENABLE_ADMIN_CHAT_ACCESS` | `true` | Enable admin chat access |
| `ENABLE_ADMIN_EXPORT` | `true` | Enable admin export |
| `ENABLE_NOTES` | `true` | Enable notes feature |
| `ENABLE_COMMUNITY_SHARING` | `true` | Enable community sharing |
| `ENABLE_MESSAGE_RATING` | `true` | Enable message rating |
| `BYPASS_ADMIN_ACCESS_CONTROL` | - | Bypass admin access control |

## Storage Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `UPLOAD_DIR` | `/app/data/uploads` | Upload directory path |
| `CACHE_DIR` | `/app/data/cache` | Cache directory path |
| `STATIC_DIR` | `{CONFIG_DIR}/build` | Static files directory path |

## Logging

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `GLOBAL_LOG_LEVEL` | `INFO` | Global log level (DEBUG, INFO, WARN, ERROR) |

## OpenAI Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `OPENAI_API_BASE_URL` | `https://api.openai.com/v1` | OpenAI API base URL |
| `OPENAI_API_KEY` | `` | OpenAI API key |
| `OPENAI_API_BASE_URLS` | `` | Semicolon-separated list of OpenAI API base URLs |
| `OPENAI_API_KEYS` | `` | Semicolon-separated list of OpenAI API keys |

## Audio - Text-to-Speech (TTS)

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `TTS_OPENAI_API_BASE_URL` | `https://api.openai.com/v1` | TTS OpenAI API base URL |
| `TTS_OPENAI_API_KEY` | `` | TTS OpenAI API key |
| `TTS_API_KEY` | `` | TTS API key |
| `TTS_ENGINE` | `openai` | TTS engine |
| `TTS_MODEL` | `tts-1` | TTS model |
| `TTS_VOICE` | `alloy` | TTS voice |
| `TTS_SPLIT_ON` | `sentence` | TTS split on |
| `TTS_AZURE_SPEECH_REGION` | `` | Azure Speech region |
| `TTS_AZURE_SPEECH_BASE_URL` | `` | Azure Speech base URL |
| `TTS_AZURE_SPEECH_OUTPUT_FORMAT` | `audio-24khz-96kbitrate-mono-mp3` | Azure Speech output format |

## Audio - Speech-to-Text (STT)

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `STT_OPENAI_API_BASE_URL` | `https://api.openai.com/v1` | STT OpenAI API base URL |
| `STT_OPENAI_API_KEY` | `` | STT OpenAI API key |
| `STT_ENGINE` | `openai` | STT engine |
| `STT_MODEL` | `whisper-1` | STT model |
| `STT_SUPPORTED_CONTENT_TYPES` | `audio/*,video/webm` | Comma-separated list of supported content types |
| `WHISPER_MODEL` | `base` | Whisper model |
| `DEEPGRAM_API_KEY` | `` | Deepgram API key |
| `AUDIO_STT_AZURE_API_KEY` | `` | Azure STT API key |
| `AUDIO_STT_AZURE_REGION` | `` | Azure STT region |
| `AUDIO_STT_AZURE_LOCALES` | `` | Azure STT locales |
| `AUDIO_STT_AZURE_BASE_URL` | `` | Azure STT base URL |
| `AUDIO_STT_AZURE_MAX_SPEAKERS` | `1` | Azure STT max speakers |

## Image Generation - OpenAI

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `IMAGES_OPENAI_API_BASE_URL` | `https://api.openai.com/v1` | Image generation OpenAI API base URL |
| `IMAGES_OPENAI_API_VERSION` | `2024-02-01` | Image generation OpenAI API version |
| `IMAGES_OPENAI_API_KEY` | `` | Image generation OpenAI API key |

## Image Generation - Automatic1111

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `AUTOMATIC1111_BASE_URL` | `` | Automatic1111 base URL |
| `AUTOMATIC1111_API_AUTH` | `` | Automatic1111 API authentication |
| `AUTOMATIC1111_CFG_SCALE` | - | Automatic1111 CFG scale |
| `AUTOMATIC1111_SAMPLER` | - | Automatic1111 sampler |
| `AUTOMATIC1111_SCHEDULER` | - | Automatic1111 scheduler |

## Image Generation - ComfyUI

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `COMFYUI_BASE_URL` | `` | ComfyUI base URL |
| `COMFYUI_API_KEY` | `` | ComfyUI API key |
| `COMFYUI_WORKFLOW` | `` | ComfyUI workflow |

## Image Generation - Gemini

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `IMAGES_GEMINI_API_BASE_URL` | `https://generativelanguage.googleapis.com/v1beta` | Gemini API base URL |
| `IMAGES_GEMINI_API_KEY` | `` | Gemini API key |

## Image Generation - General

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `IMAGE_GENERATION_ENGINE` | `openai` | Image generation engine |
| `ENABLE_IMAGE_PROMPT_GENERATION` | `false` | Enable image prompt generation |

## RAG/Retrieval Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `CHUNK_SIZE` | `1500` | Document chunk size |
| `CHUNK_OVERLAP` | `100` | Document chunk overlap |
| `RAG_TOP_K` | `5` | Top K results for RAG |
| `RAG_EMBEDDING_MODEL` | `sentence-transformers/all-MiniLM-L6-v2` | RAG embedding model |
| `RAG_EMBEDDING_ENGINE` | `` | RAG embedding engine |
| `RAG_OPENAI_API_KEY` | Falls back to `OPENAI_API_KEY` | RAG OpenAI API key |
| `RAG_OPENAI_API_BASE_URL` | Falls back to `OPENAI_API_BASE_URL` | RAG OpenAI API base URL |
| `RAG_TEMPLATE` | Default template | RAG template |
| `RAG_FULL_CONTEXT` | `false` | Use full context for RAG |
| `BYPASS_EMBEDDING_AND_RETRIEVAL` | `false` | Bypass embedding and retrieval |
| `ENABLE_RAG_HYBRID_SEARCH` | `false` | Enable RAG hybrid search |
| `TOP_K_RERANKER` | `5` | Top K for reranker |
| `RELEVANCE_THRESHOLD` | `0.0` | Relevance threshold |
| `HYBRID_BM25_WEIGHT` | `0.5` | Hybrid BM25 weight |
| `CONTENT_EXTRACTION_ENGINE` | `tika` | Content extraction engine |
| `PDF_EXTRACT_IMAGES` | `false` | Extract images from PDFs |
| `RAG_EMBEDDING_MODEL_TRUST_REMOTE_CODE` | `true` | Trust remote code for embedding model |
| `RAG_RERANKING_MODEL_TRUST_REMOTE_CODE` | `true` | Trust remote code for reranking model |

## Sentence Transformers

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `SENTENCE_TRANSFORMERS_HOME` | - | Sentence Transformers home directory |
| `SENTENCE_TRANSFORMERS_BACKEND` | `torch` | Sentence Transformers backend |
| `SENTENCE_TRANSFORMERS_MODEL_KWARGS` | - | Sentence Transformers model kwargs (JSON) |
| `SENTENCE_TRANSFORMERS_CROSS_ENCODER_BACKEND` | `torch` | Cross encoder backend |
| `SENTENCE_TRANSFORMERS_CROSS_ENCODER_MODEL_KWARGS` | - | Cross encoder model kwargs (JSON) |

## RAG Embedding Prefixes

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `RAG_EMBEDDING_QUERY_PREFIX` | `` | Prefix for query embeddings |
| `RAG_EMBEDDING_CONTENT_PREFIX` | `` | Prefix for content embeddings |
| `RAG_EMBEDDING_PREFIX_FIELD_NAME` | - | Field name for embedding prefix |

## Code Execution

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `CODE_EXECUTION_ENGINE` | `python` | Code execution engine |
| `ENABLE_PIPELINE_FILTERS` | `true` | Enable pipeline filters |
| `CODE_EXECUTION_JUPYTER_URL` | - | Jupyter server URL for code execution |
| `CODE_EXECUTION_JUPYTER_AUTH` | - | Jupyter authentication method |
| `CODE_EXECUTION_JUPYTER_AUTH_TOKEN` | - | Jupyter authentication token |
| `CODE_EXECUTION_JUPYTER_AUTH_PASSWORD` | - | Jupyter authentication password |
| `CODE_EXECUTION_JUPYTER_TIMEOUT` | - | Jupyter execution timeout |
| `CODE_EXECUTION_SANDBOX_URL` | `http://localhost:8090` | Sandbox server URL |
| `CODE_EXECUTION_SANDBOX_TIMEOUT` | `60` | Sandbox execution timeout |
| `CODE_EXECUTION_SANDBOX_ENABLE_POOL` | - | Enable sandbox connection pool |
| `CODE_EXECUTION_SANDBOX_POOL_SIZE` | - | Sandbox connection pool size |
| `CODE_EXECUTION_SANDBOX_POOL_MAX_REUSE` | - | Sandbox connection pool max reuse |
| `CODE_EXECUTION_SANDBOX_POOL_MAX_AGE` | - | Sandbox connection pool max age |

## Code Interpreter

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_CODE_INTERPRETER` | `false` | Enable code interpreter |
| `CODE_INTERPRETER_ENGINE` | `python` | Code interpreter engine |
| `CODE_INTERPRETER_PROMPT_TEMPLATE` | - | Code interpreter prompt template |
| `CODE_INTERPRETER_JUPYTER_URL` | - | Jupyter server URL for code interpreter |
| `CODE_INTERPRETER_JUPYTER_AUTH` | - | Jupyter authentication method |
| `CODE_INTERPRETER_JUPYTER_AUTH_TOKEN` | - | Jupyter authentication token |
| `CODE_INTERPRETER_JUPYTER_AUTH_PASSWORD` | - | Jupyter authentication password |
| `CODE_INTERPRETER_JUPYTER_TIMEOUT` | - | Jupyter execution timeout |
| `CODE_INTERPRETER_SANDBOX_URL` | Falls back to `CODE_EXECUTION_SANDBOX_URL` | Sandbox server URL |
| `CODE_INTERPRETER_SANDBOX_TIMEOUT` | Falls back to `CODE_EXECUTION_SANDBOX_TIMEOUT` | Sandbox execution timeout |

## Webhooks

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `WEBHOOK_URL` | - | Webhook URL |

## WebUI Settings

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `WEBUI_NAME` | `Open WebUI` | WebUI name |
| `WEBUI_AUTH` | `true` | Enable WebUI authentication |
| `DEFAULT_MODELS` | `` | Default models |
| `MODEL_ORDER_LIST` | `` | Comma-separated list of model order |

## Version and Updates

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_VERSION_UPDATE_CHECK` | `true` | Enable version update check |

## Task Configuration

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `TASK_MODEL` | - | Task model |
| `TASK_MODEL_EXTERNAL` | - | External task model |
| `ENABLE_SEARCH_QUERY_GENERATION` | `true` | Enable search query generation |
| `ENABLE_RETRIEVAL_QUERY_GENERATION` | `true` | Enable retrieval query generation |
| `ENABLE_AUTOCOMPLETE_GENERATION` | `true` | Enable autocomplete generation |
| `AUTOCOMPLETE_GENERATION_INPUT_MAX_LENGTH` | `200` | Autocomplete generation input max length |
| `ENABLE_TAGS_GENERATION` | `true` | Enable tags generation |
| `TAGS_GENERATION_PROMPT_TEMPLATE` | `` | Tags generation prompt template |
| `ENABLE_TITLE_GENERATION` | `true` | Enable title generation |
| `TITLE_GENERATION_PROMPT_TEMPLATE` | `` | Title generation prompt template |
| `ENABLE_FOLLOW_UP_GENERATION` | `true` | Enable follow-up generation |
| `FOLLOW_UP_GENERATION_PROMPT_TEMPLATE` | `` | Follow-up generation prompt template |
| `IMAGE_PROMPT_GENERATION_PROMPT_TEMPLATE` | `` | Image prompt generation prompt template |
| `QUERY_GENERATION_PROMPT_TEMPLATE` | `` | Query generation prompt template |
| `TOOLS_FUNCTION_CALLING_PROMPT_TEMPLATE` | `` | Tools function calling prompt template |

## User Permissions

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_USER_WEBHOOKS` | `true` | Enable user webhooks |

## Direct Connections

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_DIRECT_CONNECTIONS` | `false` | Enable direct connections |
| `ENABLE_BASE_MODELS_CACHE` | `true` | Enable base models cache |

## Integrations

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_GOOGLE_DRIVE_INTEGRATION` | `false` | Enable Google Drive integration |
| `ENABLE_ONEDRIVE_INTEGRATION` | `false` | Enable OneDrive integration |

## Evaluations

| Environment Variable | Default Value | Description |
|---------------------|---------------|-------------|
| `ENABLE_EVALUATION_ARENA_MODELS` | `false` | Enable evaluation arena models |

## Usage Example

```bash
# Basic usage with custom port
PORT=3000 ./open-webui-lite-linux-x86_64

# With OpenAI API configuration
OPENAI_API_KEY=sk-xxx OPENAI_API_BASE_URL=https://api.openai.com/v1 ./open-webui-lite-linux-x86_64

# With custom configuration directory
CONFIG_DIR=~/my-config ./open-webui-lite-linux-x86_64

# Enable random port
ENABLE_RANDOM_PORT=true ./open-webui-lite-linux-x86_64

# Multiple environment variables
HOST=127.0.0.1 PORT=8888 WEBUI_NAME="My WebUI" ./open-webui-lite-linux-x86_64
```

