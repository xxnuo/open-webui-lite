.PHONY: build build-backend build-frontend prepare

prepare:
	git submodule update --init --recursive
	cd frontend && bun install
	cd backend/rust-backend && cargo fetch

build-frontend:
	cd frontend && bun run build

build-backend: build-frontend
	mkdir -p build
	cd backend/rust-backend && cargo build --release
	cp backend/rust-backend/target/release/open-webui-rust build/open-webui-lite

build: build-backend

run:
	cd backend/rust-backend && cargo run
