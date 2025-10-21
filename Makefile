.PHONY: build build-backend build-frontend prepare

prepare:
	git submodule update --init --recursive
	cd frontend && bun install
	cd backend/rust-backend && cargo fetch

build-frontend:
	cd frontend && bun run build

build-backend:
	cd backend/rust-backend && cargo build --release

build: build-frontend build-backend

run-backend:
	cd backend/rust-backend && cargo run