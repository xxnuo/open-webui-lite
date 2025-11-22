/// Code Interpreter Middleware for Automatic Code Execution
/// This module detects code blocks in streaming chat responses and executes them automatically
/// Similar to Python backend's middleware.py code interpreter functionality
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    services::sandbox_executor::{SandboxExecuteResponse, SandboxExecutorClient},
    AppState,
};

/// Represents a code block detected in the response
#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
    pub full_block: String, // The complete ```language\ncode\n``` block
}

/// State machine for tracking code block detection
#[derive(Debug, Clone, PartialEq)]
enum CodeBlockState {
    Outside,
    OpeningBackticks,
    Language,
    Code,
}

/// Code block detector that processes streaming content
pub struct CodeBlockDetector {
    state: CodeBlockState,
    buffer: String,
    language_buffer: String,
    code_buffer: String,
    full_block_buffer: String,
    backtick_count: usize,
}

impl CodeBlockDetector {
    pub fn new() -> Self {
        Self {
            state: CodeBlockState::Outside,
            buffer: String::new(),
            language_buffer: String::new(),
            code_buffer: String::new(),
            full_block_buffer: String::new(),
            backtick_count: 0,
        }
    }

    /// Process a chunk of text and detect complete code blocks
    /// Returns detected code blocks and any remaining text outside blocks
    pub fn process_chunk(&mut self, chunk: &str) -> (Vec<CodeBlock>, String) {
        let mut detected_blocks = Vec::new();
        let mut output_text = String::new();

        for ch in chunk.chars() {
            self.buffer.push(ch);
            self.full_block_buffer.push(ch);

            match self.state {
                CodeBlockState::Outside => {
                    if ch == '`' {
                        self.backtick_count = 1;
                        self.state = CodeBlockState::OpeningBackticks;
                    } else {
                        output_text.push(ch);
                        self.full_block_buffer.clear();
                    }
                }
                CodeBlockState::OpeningBackticks => {
                    if ch == '`' {
                        self.backtick_count += 1;
                        if self.backtick_count == 3 {
                            // Found opening ```
                            self.state = CodeBlockState::Language;
                            self.language_buffer.clear();
                            self.code_buffer.clear();
                        }
                    } else if ch == '\n' || ch == '\r' {
                        // Not a code block, just backticks
                        output_text.push_str(&self.buffer);
                        self.buffer.clear();
                        self.full_block_buffer.clear();
                        self.state = CodeBlockState::Outside;
                    } else {
                        // Not a code block
                        output_text.push_str(&self.buffer);
                        self.buffer.clear();
                        self.full_block_buffer.clear();
                        self.state = CodeBlockState::Outside;
                    }
                }
                CodeBlockState::Language => {
                    if ch == '\n' || ch == '\r' {
                        // Language line complete, now reading code
                        self.state = CodeBlockState::Code;
                        self.backtick_count = 0;
                    } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                        self.language_buffer.push(ch);
                    }
                }
                CodeBlockState::Code => {
                    if ch == '`' {
                        self.backtick_count = 1;
                        // Don't add backticks to code yet, wait to see if it's closing
                    } else if self.backtick_count > 0 {
                        if ch == '`' {
                            self.backtick_count += 1;
                            if self.backtick_count == 3 {
                                // Found closing ```
                                let language = self.language_buffer.trim().to_lowercase();
                                let code = self.code_buffer.trim().to_string();

                                if !code.is_empty() && Self::is_executable_language(&language) {
                                    detected_blocks.push(CodeBlock {
                                        language,
                                        code,
                                        full_block: self.full_block_buffer.clone(),
                                    });
                                    debug!("✅ Detected code block: {}", self.language_buffer);
                                }

                                // Reset for next block
                                self.buffer.clear();
                                self.language_buffer.clear();
                                self.code_buffer.clear();
                                self.full_block_buffer.clear();
                                self.backtick_count = 0;
                                self.state = CodeBlockState::Outside;
                            }
                        } else {
                            // False alarm, add the backticks and continue
                            for _ in 0..self.backtick_count {
                                self.code_buffer.push('`');
                            }
                            self.code_buffer.push(ch);
                            self.backtick_count = 0;
                        }
                    } else {
                        self.code_buffer.push(ch);
                    }
                }
            }
        }

        (detected_blocks, output_text)
    }

    /// Check if a language should be executed
    fn is_executable_language(language: &str) -> bool {
        matches!(
            language,
            "python"
                | "py"
                | "javascript"
                | "js"
                | "bash"
                | "sh"
                | "shell"
                | "ruby"
                | "rb"
                | "php"
                | "go"
                | "rust"
                | "rs"
        )
    }

    /// Normalize language name to standard format
    pub fn normalize_language(language: &str) -> String {
        match language {
            "py" => "python".to_string(),
            "js" => "javascript".to_string(),
            "sh" | "shell" => "bash".to_string(),
            "rb" => "ruby".to_string(),
            "rs" => "rust".to_string(),
            _ => language.to_string(),
        }
    }
}

/// Execute a code block using the sandbox executor
pub async fn execute_code_block(
    code_block: &CodeBlock,
    sandbox_client: &Arc<SandboxExecutorClient>,
    user_id: &str,
    timeout: Option<i32>,
) -> Result<SandboxExecuteResponse, String> {
    let language = CodeBlockDetector::normalize_language(&code_block.language);

    info!(
        "🔒 Executing code block: {} ({} bytes)",
        language,
        code_block.code.len()
    );

    sandbox_client
        .execute_code(
            code_block.code.clone(),
            language,
            timeout.map(|t| t as u64),
            Some(user_id.to_string()),
            None,
        )
        .await
}

/// Format execution result for display in chat
pub fn format_execution_result(result: &SandboxExecuteResponse) -> String {
    let mut output = String::new();

    // Add execution metadata
    output.push_str(&format!(
        "\n**Code Execution Result** ({}ms)\n\n",
        result.execution_time_ms
    ));

    // Add stdout if present
    if !result.stdout.is_empty() {
        output.push_str("**Output:**\n```\n");
        output.push_str(&result.stdout);
        output.push_str("\n```\n\n");
    }

    // Add stderr if present
    if !result.stderr.is_empty() {
        output.push_str("**Errors:**\n```\n");
        output.push_str(&result.stderr);
        output.push_str("\n```\n\n");
    }

    // Add result if present
    if let Some(res) = &result.result {
        if !res.is_empty() {
            output.push_str("**Result:**\n```\n");
            output.push_str(res);
            output.push_str("\n```\n\n");
        }
    }

    // Add exit code if non-zero
    if let Some(exit_code) = result.exit_code {
        if exit_code != 0 {
            output.push_str(&format!("**Exit Code:** {}\n\n", exit_code));
        }
    }

    // Add error if present
    if let Some(error) = &result.error {
        output.push_str(&format!("**Error:** {}\n\n", error));
    }

    output
}

/// Check if code interpreter is enabled in config
pub fn is_code_interpreter_enabled(state: &actix_web::web::Data<AppState>) -> bool {
    let config = state.config.read().unwrap();
    config.enable_code_interpreter && config.code_interpreter_engine == "sandbox"
}

/// Get sandbox executor client from app state
/// This function now returns a client with the current sandbox URL from config
/// to support dynamic URL updates via admin settings
pub fn get_sandbox_client(
    state: &actix_web::web::Data<AppState>,
) -> Option<Arc<SandboxExecutorClient>> {
    let config = state.config.read().unwrap();
    let sandbox_url = config.code_interpreter_sandbox_url.clone()?;
    drop(config);

    // Check if existing client has the same URL, otherwise create a new one
    if let Some(existing_client) = state.sandbox_executor_client.as_ref() {
        if existing_client.base_url() == sandbox_url {
            return Some(existing_client.clone());
        }
    }

    // Create a new client with the current URL
    Some(Arc::new(SandboxExecutorClient::new(sandbox_url)))
}

/// Get code interpreter timeout from config
pub fn get_code_interpreter_timeout(state: &actix_web::web::Data<AppState>) -> Option<i32> {
    let config = state.config.read().unwrap();
    config.code_interpreter_sandbox_timeout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_simple_python_block() {
        let mut detector = CodeBlockDetector::new();
        let input = "Here is some code:\n```python\nprint('hello')\n```\nDone!";

        let (blocks, _) = detector.process_chunk(input);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "python");
        assert_eq!(blocks[0].code, "print('hello')");
    }

    #[test]
    fn test_detect_multiple_blocks() {
        let mut detector = CodeBlockDetector::new();
        let input =
            "```python\nprint('test')\n```\nSome text\n```javascript\nconsole.log('hi')\n```";

        let (blocks, _) = detector.process_chunk(input);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, "python");
        assert_eq!(blocks[1].language, "javascript");
    }

    #[test]
    fn test_detect_streaming_block() {
        let mut detector = CodeBlockDetector::new();

        // Simulate streaming chunks
        let chunk1 = "```py";
        let chunk2 = "thon\nprint";
        let chunk3 = "('hello')\n```";

        let (blocks1, _) = detector.process_chunk(chunk1);
        assert_eq!(blocks1.len(), 0);

        let (blocks2, _) = detector.process_chunk(chunk2);
        assert_eq!(blocks2.len(), 0);

        let (blocks3, _) = detector.process_chunk(chunk3);
        assert_eq!(blocks3.len(), 1);
        assert_eq!(blocks3[0].language, "python");
        assert_eq!(blocks3[0].code, "print('hello')");
    }

    #[test]
    fn test_language_normalization() {
        assert_eq!(CodeBlockDetector::normalize_language("py"), "python");
        assert_eq!(CodeBlockDetector::normalize_language("js"), "javascript");
        assert_eq!(CodeBlockDetector::normalize_language("sh"), "bash");
        assert_eq!(CodeBlockDetector::normalize_language("rb"), "ruby");
    }

    #[test]
    fn test_executable_languages() {
        assert!(CodeBlockDetector::is_executable_language("python"));
        assert!(CodeBlockDetector::is_executable_language("javascript"));
        assert!(CodeBlockDetector::is_executable_language("bash"));
        assert!(!CodeBlockDetector::is_executable_language("markdown"));
        assert!(!CodeBlockDetector::is_executable_language("text"));
    }
}
