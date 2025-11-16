.PHONY: prepare-git prepare-frontend prepare-frontend-svelte prepare-backend prepare-desktop build-frontend build-frontend-svelte build-backend build-backend-slim run-backend-slim run-backend run-desktop build-desktop

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
build-backend: build-frontend-svelte
	cd backend/rust-backend && cargo build --release
	mkdir -p bin
	cp backend/rust-backend/target/release/open-webui-rust bin/open-webui-lite-${BUILD_HOST}

# Without static frontend
build-backend-slim:
	cd backend/rust-backend && cargo build --release --no-default-features
	mkdir -p bin
	cp backend/rust-backend/target/release/open-webui-rust bin/open-webui-lite-slim-${BUILD_HOST}

# Without static frontend
run-backend-slim:
	cd backend/rust-backend && cargo run --no-default-features

run-backend:
	cd backend/rust-backend && cargo run

run-desktop:
	cargo tauri dev

build-desktop:
	cargo tauri build

update-version:
	@echo "Current version: $$(grep '^version = ' src-tauri/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')"
	@read -p "Enter new version (default: $$(grep '^version = ' src-tauri/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')): " NEW_VERSION; \
	NEW_VERSION=$${NEW_VERSION:-$$(grep '^version = ' src-tauri/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')}; \
	echo "Updating to version: $$NEW_VERSION"; \
	sed -i.bak "s/^version = \".*\"/version = \"$$NEW_VERSION\"/" src-tauri/Cargo.toml && rm src-tauri/Cargo.toml.bak; \
	sed -i.bak "s/\"version\": \".*\"/\"version\": \"$$NEW_VERSION\"/" src-tauri/tauri.conf.json && rm src-tauri/tauri.conf.json.bak; \
	echo sed -i.bak "s/\"version\": \".*\"/\"version\": \"0.6.32-$$NEW_VERSION\"/" backend/package.json && echo rm backend/package.json.bak; \
	echo sed -i.bak "s/\"version\": \".*\"/\"version\": \"0.6.32-$$NEW_VERSION\"/" backend/svelte-frontend/package.json && echo rm backend/svelte-frontend/package.json.bak; \
	sed -i.bak "s/^version = \".*\"/version = \"$$NEW_VERSION\"/" backend/rust-backend/Cargo.toml && rm backend/rust-backend/Cargo.toml.bak; \
	echo "Version updated successfully to $$NEW_VERSION"