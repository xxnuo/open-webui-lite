use actix_web::{HttpRequest, HttpResponse};
use mime_guess::from_path;

#[cfg(feature = "embed-frontend")]
use rust_embed::RustEmbed;

// Embed the frontend build directory into the binary
// The path is relative to the Cargo.toml directory

// #[cfg(feature = "embed-frontend")]
// #[derive(RustEmbed)]
// #[folder = "../dist/"]
// pub struct FrontendAssets;

// If you want to embed the svelte frontend, uncomment the following line

#[cfg(feature = "embed-frontend")]
#[derive(RustEmbed)]
#[folder = "../svelte-frontend/build/"]
pub struct FrontendAssets;

// Dummy struct for slim build (no embedded frontend)
#[cfg(not(feature = "embed-frontend"))]
pub struct FrontendAssets;

#[cfg(not(feature = "embed-frontend"))]
impl FrontendAssets {
    pub fn get(_path: &str) -> Option<EmbeddedFile> {
        // Always return None in slim mode - no embedded files
        None
    }
}

// Dummy embedded file struct for slim build
#[cfg(not(feature = "embed-frontend"))]
pub struct EmbeddedFile {
    pub data: std::borrow::Cow<'static, [u8]>,
}

// Note: This should never be instantiated since get() always returns None,
// but we need it for type compatibility

// Re-export RustEmbed trait so other modules can use FrontendAssets::get()
#[cfg(feature = "embed-frontend")]
pub use rust_embed::RustEmbed as _;

/// Serve embedded static files with SPA fallback
/// This handler serves both static assets and handles SPA routing
pub async fn serve(req: HttpRequest) -> HttpResponse {
    #[cfg(feature = "embed-frontend")]
    {
        let mut path = req.path();

        // Remove leading slash
        path = path.trim_start_matches('/');

        // If path is empty (root), serve index.html
        if path.is_empty() {
            path = "index.html";
        }

        // Try to serve the requested file
        if let Some(content) = FrontendAssets::get(path) {
            let mime_type = from_path(path).first_or_octet_stream();

            return HttpResponse::Ok()
                .content_type(mime_type.as_ref())
                .body(content.data.into_owned());
        }

        // For SPA routing: if file not found and it doesn't look like an API request,
        // serve index.html to let the frontend router handle it
        // Exclude all backend routes from SPA fallback
        if !path.starts_with("api/")
            && !path.starts_with("openai/")
            && !path.starts_with("oauth/")
            && !path.starts_with("socket.io")
            && !path.starts_with("ws/")
            && !path.starts_with("cache/")
            && !path.starts_with("health")
            && path != "manifest.json"
            && path != "opensearch.xml"
            && path != "favicon.png"
            && path != "user.png"
        {
            if let Some(index) = FrontendAssets::get("index.html") {
                return HttpResponse::Ok()
                    .content_type("text/html")
                    .body(index.data.into_owned());
            }
        }

        // If it's an API request or index.html is not found, return 404
        HttpResponse::NotFound().body("404 Not Found")
    }

    #[cfg(not(feature = "embed-frontend"))]
    {
        // Slim build: no frontend embedded, return a simple API-only response
        let path = req.path();

        // For root or non-API routes, return a helpful message
        if path == "/" || path.is_empty() {
            return HttpResponse::Ok()
                .content_type("application/json")
                .body(r#"{"message":"Open WebUI API Backend (Slim Build - No Frontend)","version":"0.6.32","status":"running"}"#);
        }

        HttpResponse::NotFound()
            .content_type("application/json")
            .body(r#"{"error":"Frontend not embedded in this slim build. Please use a separate frontend or the full build."}"#)
    }
}
