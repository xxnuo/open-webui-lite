#!/bin/bash
set -e

echo "🦀 Starting Open WebUI Rust Backend..."

# Check if .env exists
if [ ! -f .env ]; then
    echo "⚠️  .env file not found. Creating from env.example..."
    cp env.example .env
    echo "📝 Please edit .env with your configuration"
    exit 1
fi

# Load environment variables
export $(cat .env | xargs)

# Check if binary exists
if [ ! -f target/release/open-webui-rust ]; then
    echo "📦 Binary not found. Building..."
    ./build.sh
fi

# Create data directory if it doesn't exist
mkdir -p /app/data/uploads

# Run the server
echo "🚀 Starting server..."
./target/release/open-webui-rust

