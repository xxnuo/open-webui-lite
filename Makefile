.PHONY: build build-backend build-frontend build-sidecar prepare run-app

BUILD_HOST := $(shell rustc -Vv | grep host | cut -d' ' -f2)

prepare-git:
	git submodule update --init --recursive
	mkdir -p build


prepare-frontend:
	cd backend && bun install

prepare-frontend-svelte:
	cd backend/svelte-frontend && bun install

prepare-backend:
	cd backend && bun install
	cd backend/rust-backend && cargo fetch

prepare-desktop:
	cd src-tauri && cargo fetch

build-frontend: prepare-frontend
	cd backend && bun run build

build-frontend-svelte: prepare-frontend-svelte
	cd backend/svelte-frontend && bun run build

# backend/rust-backend/src/static_files.rs
build-backend: build-frontend
	cd backend/rust-backend && cargo build --release
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite-${BUILD_HOST}

# Without static frontend
build-backend-slim:
	cd backend/rust-backend && cargo build --release --no-default-features
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite-slim-${BUILD_HOST}

# Without static frontend
run-backend-slim:
	cd backend/rust-backend && cargo run --no-default-features

run-backend:
	cd backend/rust-backend && cargo run

run-desktop:
	if [ ! -f build/open-webui-lite-${BUILD_HOST} ]; then $(MAKE) build-backend; fi
	cargo tauri dev

build-desktop:
	if [ ! -f build/open-webui-lite-${BUILD_HOST} ]; then $(MAKE) build-backend; fi
	cargo tauri build
