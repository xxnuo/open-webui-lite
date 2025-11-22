use unicode_segmentation::UnicodeSegmentation;

/// Configuration for text chunking
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Maximum chunk size in characters
    pub chunk_size: usize,
    /// Overlap between chunks in characters
    pub chunk_overlap: usize,
    /// Separator to use for splitting text
    pub separator: String,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 512,
            chunk_overlap: 50,
            separator: "\n\n".to_string(),
        }
    }
}

impl ChunkingConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        let chunk_size = std::env::var("CHUNK_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(512);

        let chunk_overlap = std::env::var("CHUNK_OVERLAP")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);

        Self {
            chunk_size,
            chunk_overlap,
            separator: "\n\n".to_string(),
        }
    }
}

/// Chunk text into smaller pieces with overlap
///
/// This function splits text into chunks of approximately `chunk_size` characters,
/// with `chunk_overlap` characters of overlap between consecutive chunks.
/// It attempts to split on sentence boundaries when possible.
pub fn chunk_text(text: &str, chunk_size: usize, chunk_overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }

    if text.len() <= chunk_size {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    // Split into sentences first
    let _sentences = split_into_sentences(text);

    while start < text.len() {
        let mut end = (start + chunk_size).min(text.len());

        // Try to end at a sentence boundary
        if end < text.len() {
            // Find the last sentence boundary before end
            let text_slice = &text[start..end];
            if let Some(boundary) = find_last_sentence_boundary(text_slice) {
                end = start + boundary + 1;
            }
        }

        // Ensure we make progress
        if end <= start {
            end = (start + chunk_size).min(text.len());
        }

        let chunk = text[start..end].trim().to_string();
        if !chunk.is_empty() {
            chunks.push(chunk);
        }

        // Move start forward, accounting for overlap
        if end >= text.len() {
            break;
        }
        start = end.saturating_sub(chunk_overlap);

        // Ensure we make progress
        if start >= end {
            start = end;
        }
    }

    chunks
}

/// Split text into sentences
fn split_into_sentences(text: &str) -> Vec<String> {
    let sentence_endings = ['.', '!', '?', '\n'];

    let mut sentences = Vec::new();
    let mut current = String::new();

    for grapheme in text.graphemes(true) {
        current.push_str(grapheme);

        if sentence_endings.iter().any(|&c| grapheme.contains(c)) {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }
    }

    // Add remaining text
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

/// Find the last sentence boundary in text
fn find_last_sentence_boundary(text: &str) -> Option<usize> {
    let sentence_endings = ['.', '!', '?', '\n'];

    text.char_indices()
        .rev()
        .find(|(_, c)| sentence_endings.contains(c))
        .map(|(i, _)| i)
}

/// Count tokens in text (approximate)
/// This is a simple approximation. For accurate token counting, use tiktoken-rs
pub fn count_tokens_approx(text: &str) -> usize {
    // Approximate: 1 token ≈ 4 characters for English text
    (text.len() as f64 / 4.0).ceil() as usize
}

/// Chunk text with token-based sizing
pub fn chunk_text_by_tokens(text: &str, max_tokens: usize, overlap_tokens: usize) -> Vec<String> {
    // Convert token counts to approximate character counts
    let chunk_size = max_tokens * 4;
    let chunk_overlap = overlap_tokens * 4;

    chunk_text(text, chunk_size, chunk_overlap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_short() {
        let text = "Short text.";
        let chunks = chunk_text(text, 100, 10);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Short text.");
    }

    #[test]
    fn test_chunk_text_long() {
        let text = "This is sentence one. This is sentence two. This is sentence three. This is sentence four.";
        let chunks = chunk_text(text, 40, 10);
        assert!(chunks.len() > 1);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_empty() {
        let text = "";
        let chunks = chunk_text(text, 100, 10);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_with_overlap() {
        let text = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let chunks = chunk_text(text, 30, 15);
        assert!(chunks.len() >= 2);

        // Check that there's overlap between consecutive chunks
        if chunks.len() >= 2 {
            let first_end = &chunks[0][chunks[0].len().saturating_sub(10)..];
            assert!(
                chunks[1].contains(first_end)
                    || first_end.contains(&chunks[1][..first_end.len().min(chunks[1].len())])
            );
        }
    }

    #[test]
    fn test_count_tokens_approx() {
        let text = "This is a test sentence.";
        let tokens = count_tokens_approx(text);
        assert!(tokens > 0);
        assert!(tokens < 20); // Reasonable range
    }

    #[test]
    fn test_split_into_sentences() {
        let text = "First sentence. Second sentence! Third sentence?";
        let sentences = split_into_sentences(text);
        assert_eq!(sentences.len(), 3);
    }
}
