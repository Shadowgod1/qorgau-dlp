# Multi-stage build for ultra-lightweight Qorgau DLP Rust Server
FROM rust:slim-bookworm AS builder

WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p qorgau-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    tesseract-ocr \
    tesseract-ocr-rus \
    tesseract-ocr-kaz \
    tesseract-ocr-eng \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 -s /usr/sbin/nologin qorgau \
    && mkdir -p /app/screenshots /app/shadow /app/audio /app/webcam_photos \
    && chown -R qorgau:qorgau /app

WORKDIR /app
COPY --from=builder /src/target/release/qorgau-server /usr/local/bin/qorgau-server

USER qorgau
EXPOSE 8000

CMD ["qorgau-server"]
