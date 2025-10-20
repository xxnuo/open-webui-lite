.PHONY: build build-backend build-frontend prepare

BUILD_HOST := $(shell rustc -Vv | grep host | cut -d' ' -f2)

prepare:
	git submodule update --init --recursive
	cd frontend && bun install
	cd backend && bun install
	cd backend/rust-backend && cargo fetch
	cd app && bun install
	cd app/src-tauri && cargo fetch
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

run-slim:
	cd backend/rust-backend && cargo run --no-default-features

run:
	cd backend/rust-backend && cargo run
