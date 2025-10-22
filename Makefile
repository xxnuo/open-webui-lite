.PHONY: build build-backend build-frontend build-sidecar prepare run-app

BUILD_HOST := $(shell rustc -Vv | grep host | cut -d' ' -f2)

prepare:
	git submodule update --init --recursive
	cd frontend && bun install
	cd backend && bun install
	cd backend/rust-backend && cargo fetch
	cd src-tauri && cargo fetch
	mkdir -p build

build-frontend:
	cd frontend && bun run build

build-frontend-slim:
	cd backend && bun run build

build-backend:
	if [ ! -d frontend/build ]; then $(MAKE) build-frontend; fi
	cd backend/rust-backend && cargo build --release
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite-${BUILD_HOST}

build-backend-slim:
	if [ ! -d backend/build ]; then $(MAKE) build-frontend-slim; fi
	cd backend/rust-backend && cargo build --release --no-default-features
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite-slim-${BUILD_HOST}

run-backend:
	cd backend/rust-backend && cargo run

run-backend-slim:
	cd backend/rust-backend && cargo run --no-default-features

build-sidecar:
	$(MAKE) build-backend
	cp build/open-webui-lite-${BUILD_HOST} build/open-webui-lite-sidecar-${BUILD_HOST}

run-app:
	if [ ! -f build/open-webui-lite-sidecar-${BUILD_HOST} ]; then $(MAKE) build-sidecar; fi
	cargo tauri dev
