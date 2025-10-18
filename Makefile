.PHONY: prepare

prepare:
	git submodule update --init --recursive
	cd frontend && bun install
	cd backend/rust-backend && cargo build --release
