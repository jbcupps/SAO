# SAO - Secure Agent Orchestrator
# Multi-stage build for minimal production image

FROM rust:1.77-slim AS builder

WORKDIR /app

# Copy workspace files
COPY Cargo.toml rust-toolchain.toml ./
COPY crates/ crates/

# Build release binary
RUN cargo build --release --bin sao-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sao-server /usr/local/bin/sao-server

ENV SAO_BIND_ADDR=0.0.0.0:3100
ENV SAO_DATA_DIR=/data/sao
ENV RUST_LOG=info

EXPOSE 3100

VOLUME ["/data/sao"]

CMD ["sao-server"]
