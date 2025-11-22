.PHONY: build-backend build-backend-slim build-desktop build-frontend prepare-backend prepare-desktop prepare-frontend run-backend run-backend-slim run-desktop

BUILD_HOST := $(shell rustc -Vv | grep host | cut -d' ' -f2)

prepare-frontend:
	cd frontend && bun install

prepare-backend:
	cd backend && cargo fetch

prepare-desktop:
	cd src-tauri && cargo fetch

build-frontend: prepare-frontend
	cd frontend && bun run build

# backend/src/static_files.rs
build-backend: build-frontend
	cd backend && cargo build --release
	mkdir -p bin
	cp backend/target/release/open-webui-rust bin/open-coreui-${BUILD_HOST}

# Without static frontend
build-backend-slim:
	cd backend && cargo build --release --no-default-features
	mkdir -p bin
	cp backend/target/release/open-webui-rust bin/open-coreui-slim-${BUILD_HOST}

# Without static frontend
run-backend-slim:
	cd backend && cargo run --no-default-features

run-backend:
	cd backend && cargo run

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
	echo sed -i.bak "s/\"version\": \".*\"/\"version\": \"0.6.32-$$NEW_VERSION\"/" frontend/package.json && echo rm frontend/package.json.bak; \
	sed -i.bak "s/^version = \".*\"/version = \"$$NEW_VERSION\"/" backend/Cargo.toml && rm backend/Cargo.toml.bak; \
	sed -i.bak "s/^pkgver=.*/pkgver=$$NEW_VERSION/" PKGBUILD && rm PKGBUILD.bak; \
	echo "Version updated successfully to $$NEW_VERSION"

build-arch-pkg: build-backend
	makepkg -f

update-aur:
	makepkg --printsrcinfo > .SRCINFO
