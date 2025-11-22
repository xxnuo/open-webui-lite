/// Example demonstrating Sentence Transformers usage in the Rust backend
///
/// This example shows how to use the sentence transformers implementation
/// to generate embeddings locally, matching the Python backend functionality.
///
/// To run this example:
/// ```bash
/// cargo run --example sentence_transformers_example --features embeddings
/// ```
use open_webui_rust::retrieval::{EmbeddingFactory, EmbeddingFunction};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logs
    tracing_subscriber::fmt::init();

    println!("=== Sentence Transformers Example ===\n");

    // Example 1: Using the factory to create an embedding provider
    println!("1. Creating embedding provider from environment...");
    std::env::set_var("RAG_EMBEDDING_ENGINE", ""); // Empty = local sentence transformers
    std::env::set_var(
        "RAG_EMBEDDING_MODEL",
        "sentence-transformers/all-MiniLM-L6-v2",
    );

    let provider = EmbeddingFactory::from_env()?;
    println!("   Model: {}", provider.model_name());
    println!("   Dimension: {}", provider.dimension());

    // Example 2: Generate embeddings for simple texts
    println!("\n2. Generating embeddings for sample texts...");
    let texts = vec![
        "The quick brown fox jumps over the lazy dog".to_string(),
        "Machine learning is fascinating".to_string(),
        "Rust is a systems programming language".to_string(),
    ];

    let embeddings = provider.embed(texts.clone()).await?;
    println!("   Generated {} embeddings", embeddings.len());
    println!(
        "   Embedding shape: [{}, {}]",
        embeddings.len(),
        embeddings[0].len()
    );

    // Example 3: Using EmbeddingFunction with prefixes
    println!("\n3. Using EmbeddingFunction with query/content prefixes...");
    std::env::set_var("RAG_EMBEDDING_QUERY_PREFIX", "query: ");
    std::env::set_var("RAG_EMBEDDING_CONTENT_PREFIX", "passage: ");

    let embedding_fn = EmbeddingFunction::from_env()?;

    // Query embedding
    let query = vec!["What is machine learning?".to_string()];
    let query_embedding = embedding_fn.embed_query(query).await?;
    println!(
        "   Query embedding generated: dimension = {}",
        query_embedding[0].len()
    );

    // Content embeddings
    let content = vec![
        "Machine learning is a branch of artificial intelligence".to_string(),
        "The weather is nice today".to_string(),
    ];
    let content_embeddings = embedding_fn.embed_content(content).await?;
    println!(
        "   Content embeddings generated: {} embeddings",
        content_embeddings.len()
    );

    // Example 4: Calculate similarity
    println!("\n4. Calculating cosine similarity...");
    let similarity_1 = cosine_similarity(&query_embedding[0], &content_embeddings[0]);
    let similarity_2 = cosine_similarity(&query_embedding[0], &content_embeddings[1]);

    println!(
        "   Query vs 'Machine learning...' similarity: {:.4}",
        similarity_1
    );
    println!("   Query vs 'Weather...' similarity: {:.4}", similarity_2);
    println!("   → More relevant document has higher similarity! ✓");

    // Example 5: Different models
    println!("\n5. Testing with different models...");

    #[cfg(feature = "embeddings")]
    {
        use open_webui_rust::retrieval::EmbeddingFactory;

        let models = vec![
            "sentence-transformers/all-MiniLM-L6-v2",
            // Uncomment to test other models (requires downloading):
            // "sentence-transformers/paraphrase-MiniLM-L6-v2",
            // "BAAI/bge-small-en-v1.5",
        ];

        for model_name in models {
            println!("\n   Model: {}", model_name);
            match EmbeddingFactory::create_sentence_transformer(model_name.to_string(), false) {
                Ok(provider) => {
                    println!("   ✓ Loaded successfully");
                    println!("   ✓ Dimension: {}", provider.dimension());

                    let test_text = vec!["Hello, world!".to_string()];
                    match provider.embed(test_text).await {
                        Ok(emb) => println!("   ✓ Generated embedding of size {}", emb[0].len()),
                        Err(e) => println!("   ✗ Failed to generate embedding: {}", e),
                    }
                }
                Err(e) => println!("   ✗ Failed to load: {}", e),
            }
        }
    }

    println!("\n=== Example completed successfully! ===");
    Ok(())
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}
