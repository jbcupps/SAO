# SAO - Secure Agent Orchestrator
# Multi-stage build for minimal production image

# Stage 1: Build frontend
FROM node:20-slim AS frontend-builder

WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm install
COPY frontend/ .
RUN npm run build

# Stage 2: Build Rust backend
FROM rust:1.77-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY .cargo/ .cargo/
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY migrations/ migrations/

# Build release binary
RUN cargo build --release --bin sao-server

# Stage 3: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sao-server /usr/local/bin/sao-server
COPY --from=frontend-builder /app/frontend/dist /srv/sao/static

ENV SAO_BIND_ADDR=0.0.0.0:3100
ENV SAO_DATA_DIR=/data/sao
ENV SAO_STATIC_DIR=/srv/sao/static
ENV RUST_LOG=info

EXPOSE 3100

VOLUME ["/data/sao"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=90s --retries=3 \
    CMD curl -f http://localhost:3100/api/health || exit 1

CMD ["sao-server"]
