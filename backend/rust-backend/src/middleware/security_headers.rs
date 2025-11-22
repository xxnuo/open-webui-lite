use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

/// Middleware that adds security headers to all responses
///
/// Security headers added:
/// - X-Content-Type-Options: nosniff (prevent MIME sniffing)
/// - X-Frame-Options: DENY (prevent clickjacking)
/// - X-XSS-Protection: 1; mode=block (XSS protection)
/// - Referrer-Policy: strict-origin-when-cross-origin (control referrer info)
/// - Permissions-Policy: Restrict browser features
/// - Content-Security-Policy: Restrict resource loading (if enabled)
///
/// Based on Python backend: backend/open_webui/utils/security_headers.py
pub struct SecurityHeaders;

impl<S, B> Transform<S, ServiceRequest> for SecurityHeaders
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = SecurityHeadersMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(SecurityHeadersMiddleware { service }))
    }
}

pub struct SecurityHeadersMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for SecurityHeadersMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);

        Box::pin(async move {
            let mut res = fut.await?;

            // Add security headers to the response
            let headers = res.headers_mut();
            add_security_headers(headers);

            Ok(res)
        })
    }
}

/// Add security headers to response headers
pub fn add_security_headers(headers: &mut actix_web::http::header::HeaderMap) {
    use actix_web::http::header::{HeaderName, HeaderValue};

    // Prevent MIME type sniffing
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );

    // Prevent clickjacking by not allowing the page to be framed
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );

    // Enable XSS protection in legacy browsers
    headers.insert(
        HeaderName::from_static("x-xss-protection"),
        HeaderValue::from_static("1; mode=block"),
    );

    // Control referrer information
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    // Restrict browser features
    // This helps prevent unauthorized access to camera, microphone, geolocation, etc.
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );

    // Content Security Policy (CSP)
    // Note: This is a basic CSP. For production, you may want to customize this based on your needs.
    // The Python backend has environment-specific CSP configuration.
    //
    // For now, we use a permissive policy that allows:
    // - Same-origin and inline scripts/styles (required for SPA)
    // - Images from any source (for user-uploaded content)
    // - Connections to any source (for API calls to various LLM providers)
    //
    // TODO: Make this configurable via environment variables or admin settings
    let csp_enabled = std::env::var("ENABLE_CSP")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase()
        == "true";

    if csp_enabled {
        // Strict CSP
        headers.insert(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'self'; \
                 script-src 'self' 'unsafe-inline' 'unsafe-eval'; \
                 style-src 'self' 'unsafe-inline'; \
                 img-src * data: blob:; \
                 font-src 'self' data:; \
                 connect-src *; \
                 media-src 'self' blob:; \
                 object-src 'none'; \
                 frame-ancestors 'none'; \
                 base-uri 'self'; \
                 form-action 'self'",
            ),
        );

        // HTTP Strict Transport Security (HSTS)
        // Only add if HTTPS is enabled
        let https_enabled = std::env::var("HTTPS_ENABLED")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        if https_enabled {
            // Enforce HTTPS for 1 year, including subdomains
            headers.insert(
                HeaderName::from_static("strict-transport-security"),
                HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};

    #[actix_web::test]
    async fn test_security_headers() {
        let app = test::init_service(App::new().wrap(SecurityHeaders).route(
            "/",
            web::get().to(|| async { HttpResponse::Ok().body("test") }),
        ))
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;

        // Check that security headers are present
        let headers = resp.headers();

        assert!(headers.contains_key("x-content-type-options"));
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");

        assert!(headers.contains_key("x-frame-options"));
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");

        assert!(headers.contains_key("x-xss-protection"));
        assert_eq!(headers.get("x-xss-protection").unwrap(), "1; mode=block");

        assert!(headers.contains_key("referrer-policy"));
        assert_eq!(
            headers.get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );

        assert!(headers.contains_key("permissions-policy"));
        assert_eq!(
            headers.get("permissions-policy").unwrap(),
            "geolocation=(), microphone=(), camera=()"
        );
    }
}
