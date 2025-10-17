.PHONY: build build-backend build-frontend prepare

prepare:
	git submodule update --init --recursive
	cd frontend && bun install
	cd backend && bun install
	cd backend/rust-backend && cargo fetch
	mkdir -p build

build-frontend:
	cd frontend && bun run build

build-frontend-slim:
	cd backend && bun run build

build-backend: build-frontend
	cd backend/rust-backend && cargo build --release
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite

build-backend-slim: build-frontend-slim
	cd backend/rust-backend && cargo build --release --no-default-features
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite-slim

run-slim:
	cd backend/rust-backend && cargo run --no-default-features
