# Build stage - Build the SvelteKit frontend
FROM node:20-slim AS frontend-builder

WORKDIR /app

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci

# Copy source files
COPY . .

# Build the frontend (creates static files in /app/build)
# Increase Node.js memory limit to avoid out-of-memory errors
ENV NODE_OPTIONS="--max-old-space-size=4096"
RUN npm run build

# Runtime stage - Serve the built static frontend with nginx
FROM nginx:alpine

# Copy built static files from builder
COPY --from=frontend-builder /app/build /usr/share/nginx/html

# Set default proxy timeout (can be overridden)
ENV NGINX_PROXY_TIMEOUT=300s

# Create nginx configuration for SPA with backend proxy
RUN echo 'server { \
    listen 8168; \
    server_name localhost; \
    root /usr/share/nginx/html; \
    index index.html; \
    client_max_body_size 100M; \
    \
    # Proxy API requests to Rust backend \
    location /api/ { \
        proxy_pass http://rust-backend:8168/api/; \
        proxy_http_version 1.1; \
        proxy_set_header Host $host; \
        proxy_set_header X-Real-IP $remote_addr; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $scheme; \
        proxy_buffering off; \
        proxy_request_buffering off; \
        proxy_read_timeout 300s; \
        proxy_connect_timeout 300s; \
        proxy_send_timeout 300s; \
    } \
    \
    # Proxy OpenAI API requests to Rust backend \
    location /openai/ { \
        proxy_pass http://rust-backend:8168/openai/; \
        proxy_http_version 1.1; \
        proxy_set_header Host $host; \
        proxy_set_header X-Real-IP $remote_addr; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $scheme; \
        proxy_buffering off; \
        proxy_request_buffering off; \
    } \
    \
    # Proxy OAuth requests to Rust backend \
    location /oauth/ { \
        proxy_pass http://rust-backend:8168/oauth/; \
        proxy_http_version 1.1; \
        proxy_set_header Host $host; \
        proxy_set_header X-Real-IP $remote_addr; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $scheme; \
    } \
    \
    # Proxy cache files to Rust backend \
    location /cache/ { \
        proxy_pass http://rust-backend:8168/cache/; \
        proxy_http_version 1.1; \
        proxy_set_header Host $host; \
        proxy_set_header X-Real-IP $remote_addr; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $scheme; \
    } \
    \
    # Socket.IO WebSocket proxy (native Rust implementation) \
    location /socket.io/ { \
        proxy_pass http://rust-backend:8168/socket.io/; \
        proxy_http_version 1.1; \
        proxy_set_header Upgrade $http_upgrade; \
        proxy_set_header Connection "upgrade"; \
        proxy_set_header Host $host; \
        proxy_set_header X-Real-IP $remote_addr; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $scheme; \
        proxy_buffering off; \
        proxy_read_timeout 86400; \
    } \
    \
    # SPA fallback \
    location / { \
        try_files $uri $uri/ /index.html; \
    } \
    \
    # Health check \
    location /health { \
        access_log off; \
        return 200 "healthy\\n"; \
        add_header Content-Type text/plain; \
    } \
}' > /etc/nginx/conf.d/default.conf

# Expose port 8168
EXPOSE 8168

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8168/health || exit 1

# Start nginx
CMD ["nginx", "-g", "daemon off;"]

