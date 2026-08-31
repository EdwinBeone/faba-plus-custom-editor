FROM rust:1.97-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release \
    && cp /build/target/release/faba-cloud /tmp/faba-cloud

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --create-home faba
RUN install -d -o faba -g faba /data/audio
COPY --from=builder /tmp/faba-cloud /usr/local/bin/faba-cloud
USER faba
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl --fail --silent http://127.0.0.1:8787/health >/dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/faba-cloud"]
