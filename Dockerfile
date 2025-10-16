FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"

RUN apt-get update && \
    apt-get -y upgrade && \
    apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    clang \
    libclang-dev \
    libc6-dev \
    && rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Copy workspace Cargo files for better caching
COPY Cargo.toml Cargo.lock ./
COPY bin/ev-reth/Cargo.toml bin/ev-reth/
COPY crates/common/Cargo.toml crates/common/
COPY crates/node/Cargo.toml crates/node/
COPY crates/evolve/Cargo.toml crates/evolve/
COPY crates/tests/Cargo.toml crates/tests/

ARG BUILD_PROFILE=docker
ENV BUILD_PROFILE=$BUILD_PROFILE

# Set memory-efficient build flags
ARG RUSTFLAGS="-C codegen-units=1"
ENV RUSTFLAGS="$RUSTFLAGS"
ENV CARGO_BUILD_JOBS=2
ENV CARGO_INCREMENTAL=0

# Cook dependencies first (better layer caching)
RUN cargo chef cook --profile $BUILD_PROFILE --recipe-path recipe.json --manifest-path bin/ev-reth/Cargo.toml

# Copy all source code
COPY . .

# Build the binary with memory-efficient settings
RUN cargo build --profile $BUILD_PROFILE --bin ev-reth --manifest-path bin/ev-reth/Cargo.toml -j 2

# Copy binary from correct location
RUN ls -la /app/target/$BUILD_PROFILE/ev-reth
RUN cp /app/target/$BUILD_PROFILE/ev-reth /ev-reth

FROM ubuntu:22.04 AS runtime

RUN apt-get update && \
    apt-get install -y ca-certificates curl jq libssl-dev pkg-config strace && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /ev-reth /usr/local/bin/
RUN chmod +x /usr/local/bin/ev-reth
COPY LICENSE-* ./

# Expose ports: P2P, Discovery, Metrics, JSON-RPC, WebSocket, GraphQL, Engine API
EXPOSE 30303 30303/udp 9001 8545 8546 7545 8551

# Add health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=40s --retries=3 \
    CMD /usr/local/bin/ev-reth --version || exit 1

ENTRYPOINT ["/usr/local/bin/ev-reth"]
