# SAO - Secure Agent Orchestrator
# Multi-stage build for minimal production image

FROM rust:1.77-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY Cargo.toml rust-toolchain.toml ./
COPY crates/ crates/

# Build release binary with PostgreSQL support
RUN cargo build --release --bin sao-server --features postgres

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sao-server /usr/local/bin/sao-server

ENV SAO_BIND_ADDR=0.0.0.0:3100
ENV SAO_DATA_DIR=/data/sao
ENV RUST_LOG=info

EXPOSE 3100

VOLUME ["/data/sao"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3100/api/health || exit 1

CMD ["sao-server"]
