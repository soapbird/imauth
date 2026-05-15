# Multi-stage build for imauth

# Stage 1: Build
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY proto/ ./proto/

# Build release binaries
RUN cargo build --release -p imauth-server -p imauth-cli

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create imauth user
RUN useradd -m -s /bin/sh imauth

# Copy binaries
COPY --from=builder /build/target/release/imauth-server /usr/local/bin/imauth-server
COPY --from=builder /build/target/release/imauth /usr/local/bin/imauth

# Data directory
RUN mkdir -p /data/.imauth && chown -R imauth:imauth /data && chmod 700 /data
ENV HOME=/data

USER imauth

EXPOSE 50051

ENTRYPOINT ["/usr/local/bin/imauth-server"]
CMD ["serve"]
