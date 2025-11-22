# Build stage
FROM rust:1.88-slim as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/.fingerprint/open-webui-rust-* target/release/open-webui-rust* target/release/deps/open_webui_rust-*

# Copy source code
COPY src ./src
COPY migrations ./migrations
COPY examples ./examples

# Build application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/open-webui-rust /app/open-webui-rust

# Copy migrations
COPY --from=builder /app/migrations /app/migrations

# Create data directory
RUN mkdir -p /app/data/uploads

# Create a wrapper script to ensure errors are visible
RUN echo '#!/bin/sh\n\
set -e\n\
echo "Starting Open WebUI Rust Backend..."\n\
echo "DATABASE_URL: $DATABASE_URL"\n\
echo "ENABLE_REDIS: $ENABLE_REDIS"\n\
echo "REDIS_URL: $REDIS_URL"\n\
echo "RUST_LOG: $RUST_LOG"\n\
# Disable output buffering\n\
export RUST_BACKTRACE=1\n\
export RUST_LOG=${RUST_LOG:-info}\n\
echo "Executing binary..."\n\
/app/open-webui-rust 2>&1 || { echo "Binary exited with code $?"; exit $?; }' > /app/start.sh && chmod +x /app/start.sh

# Expose port
EXPOSE 8168

# Run the wrapper script
CMD ["/app/start.sh"]

