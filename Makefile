.PHONY: all build test clean fmt lint run help

# Build configuration
CARGO = cargo
BINARY_NAME = ev-reth
TARGET_DIR = target

# Default target
all: build

## help: Display this help message
help:
	@echo "Available targets:"
	@awk 'BEGIN {FS = ":.*##"; printf "\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Building

## build: Build the ev-reth binary in release mode
build:
	$(CARGO) build --release --bin $(BINARY_NAME)

## build-dev: Build the ev-reth binary in debug mode
build-dev:
	$(CARGO) build --bin $(BINARY_NAME)

## build-maxperf: Build ev-reth with the most aggressive optimizations
build-maxperf:
	RUSTFLAGS="-C target-cpu=native" $(CARGO) build --profile maxperf --features jemalloc,asm-keccak --bin $(BINARY_NAME)

##@ Testing

## test: Run all tests
test:
	$(CARGO) test --workspace

## test-verbose: Run all tests with verbose output
test-verbose:
	$(CARGO) test --workspace -- --nocapture

## test-unit: Run unit tests only
test-unit:
	$(CARGO) test --lib

## test-integration: Run integration tests only
test-integration:
	$(CARGO) test -p ev-tests

##@ Development

## run: Run the ev-reth node with default settings
run: build-dev
	./$(TARGET_DIR)/debug/$(BINARY_NAME) node

## run-dev: Run with debug logs enabled
run-dev: build-dev
	RUST_LOG=debug ./$(TARGET_DIR)/debug/$(BINARY_NAME) node

## build-ev-dev: Build the ev-dev binary in release mode
build-ev-dev:
	$(CARGO) build --release --bin ev-dev

## dev-chain: Build and run the local dev chain
dev-chain: build-ev-dev
	./$(TARGET_DIR)/release/ev-dev

## fmt: Format code using rustfmt (nightly)
fmt:
	$(CARGO) +nightly fmt --all

## fmt-check: Check if code is formatted correctly (nightly)
fmt-check:
	$(CARGO) +nightly fmt --all --check

## lint: Run clippy linter
lint:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

## check: Run cargo check
check:
	$(CARGO) check --workspace

##@ Maintenance

## clean: Clean build artifacts
clean:
	$(CARGO) clean

## update: Update dependencies
update:
	$(CARGO) update

## audit: Audit dependencies for security vulnerabilities
audit:
	$(CARGO) audit

##@ Documentation

## doc: Build documentation
doc:
	$(CARGO) doc --no-deps --open

## doc-all: Build documentation including dependencies
doc-all:
	$(CARGO) doc --open

##@ Workspace Management

## build-all: Build all workspace members
build-all:
	$(CARGO) build --workspace --release

## test-node: Test only the node crate
test-node:
	$(CARGO) test -p ev-node

## test-evolve: Test only the evolve crate
test-evolve:
	$(CARGO) test -p evolve-ev-reth

## test-common: Test only the common crate
test-common:
	$(CARGO) test -p ev-common

##@ Docker

# Docker configuration
GIT_TAG ?= $(shell git describe --tags --abbrev=0 || echo "latest")
GIT_COMMIT ?= $(shell git rev-parse --short HEAD || echo "unknown")
DOCKER_TAG ?= $(GIT_COMMIT)
BIN_DIR = dist/bin
DOCKER_IMAGE_NAME ?= ghcr.io/$(shell git config --get remote.origin.url | sed 's/.*github.com[:/]\(.*\)\.git/\1/' | cut -d'/' -f1)/ev-reth
PROFILE ?= release

# List of features to use when building
FEATURES ?= jemalloc

## docker-build: Build Docker image (tagged with git commit hash by default)
docker-build:
	@echo "Building Docker image: $(DOCKER_IMAGE_NAME):$(DOCKER_TAG)"
	docker build -t $(DOCKER_IMAGE_NAME):$(DOCKER_TAG) .

## docker-build-push: Build and push a cross-arch Docker image
docker-build-push:
	$(call docker_build_push,$(DOCKER_TAG),$(DOCKER_TAG))

## docker-build-push-latest: Build and push a cross-arch Docker image tagged with latest
docker-build-push-latest:
	$(call docker_build_push,$(GIT_TAG),latest)

# Cross-compilation targets
build-x86_64-unknown-linux-gnu:
	cross build --bin $(BINARY_NAME) --target x86_64-unknown-linux-gnu --features "$(FEATURES)" --profile "$(PROFILE)"

build-aarch64-unknown-linux-gnu:
	JEMALLOC_SYS_WITH_LG_PAGE=16 cross build --bin $(BINARY_NAME) --target aarch64-unknown-linux-gnu --features "$(FEATURES)" --profile "$(PROFILE)"

# Create a cross-arch Docker image with the given tags and push it
define docker_build_push
	$(MAKE) build-x86_64-unknown-linux-gnu
	mkdir -p $(BIN_DIR)/linux/amd64
	cp $(TARGET_DIR)/x86_64-unknown-linux-gnu/$(PROFILE)/$(BINARY_NAME) $(BIN_DIR)/linux/amd64/$(BINARY_NAME)

	$(MAKE) build-aarch64-unknown-linux-gnu
	mkdir -p $(BIN_DIR)/linux/arm64
	cp $(TARGET_DIR)/aarch64-unknown-linux-gnu/$(PROFILE)/$(BINARY_NAME) $(BIN_DIR)/linux/arm64/$(BINARY_NAME)

	docker buildx build --file ./Dockerfile.cross . \
		--platform linux/amd64,linux/arm64 \
		--tag $(DOCKER_IMAGE_NAME):$(1) \
		--tag $(DOCKER_IMAGE_NAME):$(2) \
		--provenance=false \
		--sbom=false \
		--push
endef

##@ CI Helpers

## check-all: Run all checks (fmt, lint, test)
check-all: fmt-check lint test
