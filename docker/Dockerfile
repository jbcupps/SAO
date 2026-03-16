# SAO - Secure Agent Orchestrator
# Multi-stage production image for Azure Container Apps and local Docker.

FROM node:20-bookworm-slim AS frontend-builder

WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --ignore-scripts
COPY frontend/ ./
RUN npm run build

FROM rust:1.77-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY .cargo/ .cargo/
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY migrations/ migrations/
RUN cargo build --locked --release --bin sao-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libpq5 \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 sao \
    && useradd --system --uid 10001 --gid sao --home /srv/sao --shell /usr/sbin/nologin sao \
    && mkdir -p /srv/sao/static /data/sao \
    && chown -R sao:sao /srv/sao /data/sao

COPY --from=builder --chown=sao:sao /build/target/release/sao-server /usr/local/bin/sao-server
COPY --from=frontend-builder --chown=sao:sao /build/frontend/dist /srv/sao/static

ENV SAO_BIND_ADDR=0.0.0.0:3100 \
    SAO_DATA_DIR=/data/sao \
    SAO_STATIC_DIR=/srv/sao/static \
    RUST_LOG=info

USER sao:sao
EXPOSE 3100
VOLUME ["/data/sao"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=90s --retries=3 \
    CMD curl --fail --silent http://localhost:3100/api/health >/dev/null || exit 1

CMD ["sao-server"]
