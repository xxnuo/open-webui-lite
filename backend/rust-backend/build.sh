#!/bin/bash
set -e

echo "🦀 Building Open WebUI Rust Backend..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed. Please install from https://rustup.rs/"
    exit 1
fi

# Clean previous build
echo "🧹 Cleaning previous build..."
cargo clean

# Format code
echo "📝 Formatting code..."
cargo fmt

# Run clippy for linting
echo "🔍 Running linter..."
cargo clippy -- -D warnings || echo "⚠️  Linter warnings found"

# Run tests
echo "🧪 Running tests..."
cargo test || echo "⚠️  Some tests failed"

# Build in release mode
echo "🔨 Building release binary..."
cargo build --release

echo "✅ Build complete!"
echo "📦 Binary location: target/release/open-webui-rust"
echo ""
echo "To run the server:"
echo "  ./target/release/open-webui-rust"
echo ""
echo "Or with environment file:"
echo "  export \$(cat .env | xargs) && ./target/release/open-webui-rust"

