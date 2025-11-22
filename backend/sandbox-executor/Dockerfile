# Dockerfile for the sandbox-executor service itself
FROM rust:1.88-bookworm AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user
RUN useradd -m -u 1001 -s /bin/bash sandboxsvc

# Copy binary from builder
COPY --from=builder /app/target/release/sandbox-executor /usr/local/bin/

# Create necessary directories
RUN mkdir -p /var/log/sandbox-executor && \
    chown -R sandboxsvc:sandboxsvc /var/log/sandbox-executor

# Switch to non-root user
USER sandboxsvc

EXPOSE 8090

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8090/api/v1/health || exit 1

CMD ["sandbox-executor"]

