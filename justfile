# Build configuration
cargo := "cargo"
binary := "ev-reth"
target_dir := "target"

# Docker configuration
git_tag := `git describe --tags --abbrev=0 2>/dev/null || echo "latest"`
git_commit := `git rev-parse --short HEAD 2>/dev/null || echo "unknown"`
docker_tag := env("DOCKER_TAG", git_commit)
bin_dir := "dist/bin"
docker_image := env("DOCKER_IMAGE_NAME", `echo "ghcr.io/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\(.*\)\.git/\1/' | cut -d'/' -f1)/ev-reth"`)
profile := env("PROFILE", "release")
features := env("FEATURES", "jemalloc")

# Default: list available recipes
default:
    @just --list

# Building ─────────────────────────────────────────────

# Build the ev-reth binary in release mode
build:
    {{cargo}} build --release --bin {{binary}}

# Build the ev-reth binary in debug mode
build-dev:
    {{cargo}} build --bin {{binary}}

# Build ev-reth with the most aggressive optimizations
build-maxperf:
    RUSTFLAGS="-C target-cpu=native" {{cargo}} build --profile maxperf --features jemalloc,asm-keccak --bin {{binary}}

# Build all workspace members
build-all:
    {{cargo}} build --workspace --release

# Testing ──────────────────────────────────────────────

# Run all tests
test:
    {{cargo}} test --workspace

# Run all tests with verbose output
test-verbose:
    {{cargo}} test --workspace -- --nocapture

# Run unit tests only
test-unit:
    {{cargo}} test --lib

# Run integration tests only
test-integration:
    {{cargo}} test -p ev-tests

# Test only the node crate
test-node:
    {{cargo}} test -p ev-node

# Test only the evolve crate
test-evolve:
    {{cargo}} test -p evolve-ev-reth

# Test only the common crate
test-common:
    {{cargo}} test -p ev-common

# Development ──────────────────────────────────────────

# Run the ev-reth node with default settings
run: build-dev
    ./{{target_dir}}/debug/{{binary}} node

# Run with debug logs enabled
run-dev: build-dev
    RUST_LOG=debug ./{{target_dir}}/debug/{{binary}} node

# Format code using rustfmt (nightly)
fmt:
    {{cargo}} +nightly fmt --all

# Check if code is formatted correctly (nightly)
fmt-check:
    {{cargo}} +nightly fmt --all --check

# Run clippy linter
lint:
    {{cargo}} clippy --all-targets --all-features -- -D warnings

# Run cargo check
check:
    {{cargo}} check --workspace

# Run all checks (fmt, lint, test)
check-all: fmt-check lint test

# Maintenance ──────────────────────────────────────────

# Clean build artifacts
clean:
    {{cargo}} clean

# Update dependencies
update:
    {{cargo}} update

# Audit dependencies for security vulnerabilities
audit:
    {{cargo}} audit

# Documentation ────────────────────────────────────────

# Build documentation
doc:
    {{cargo}} doc --no-deps --open

# Build documentation including dependencies
doc-all:
    {{cargo}} doc --open

# Docker ───────────────────────────────────────────────

# Build Docker image (tagged with git commit hash by default)
docker-build:
    @echo "Building Docker image: {{docker_image}}:{{docker_tag}}"
    docker build -t {{docker_image}}:{{docker_tag}} .

# Build and push a cross-arch Docker image
docker-build-push: _build-x86_64 _build-aarch64
    mkdir -p {{bin_dir}}/linux/amd64
    cp {{target_dir}}/x86_64-unknown-linux-gnu/{{profile}}/{{binary}} {{bin_dir}}/linux/amd64/{{binary}}
    mkdir -p {{bin_dir}}/linux/arm64
    cp {{target_dir}}/aarch64-unknown-linux-gnu/{{profile}}/{{binary}} {{bin_dir}}/linux/arm64/{{binary}}
    docker buildx build --file ./Dockerfile.cross . \
        --platform linux/amd64,linux/arm64 \
        --tag {{docker_image}}:{{docker_tag}} \
        --tag {{docker_image}}:{{docker_tag}} \
        --provenance=false \
        --sbom=false \
        --push

# Build and push a cross-arch Docker image tagged with latest
docker-build-push-latest: _build-x86_64 _build-aarch64
    mkdir -p {{bin_dir}}/linux/amd64
    cp {{target_dir}}/x86_64-unknown-linux-gnu/{{profile}}/{{binary}} {{bin_dir}}/linux/amd64/{{binary}}
    mkdir -p {{bin_dir}}/linux/arm64
    cp {{target_dir}}/aarch64-unknown-linux-gnu/{{profile}}/{{binary}} {{bin_dir}}/linux/arm64/{{binary}}
    docker buildx build --file ./Dockerfile.cross . \
        --platform linux/amd64,linux/arm64 \
        --tag {{docker_image}}:{{git_tag}} \
        --tag {{docker_image}}:latest \
        --provenance=false \
        --sbom=false \
        --push

# Cross-compile for x86_64
[private]
_build-x86_64:
    cross build --bin {{binary}} --target x86_64-unknown-linux-gnu --features "{{features}}" --profile "{{profile}}"

# Cross-compile for aarch64
[private]
_build-aarch64:
    JEMALLOC_SYS_WITH_LG_PAGE=16 cross build --bin {{binary}} --target aarch64-unknown-linux-gnu --features "{{features}}" --profile "{{profile}}"
