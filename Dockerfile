# syntax=docker/dockerfile:1.7

FROM oven/bun:1.3-debian AS webui
WORKDIR /src/plugins/webui
COPY plugins/webui/package.json plugins/webui/bun.lock ./
RUN bun install --frozen-lockfile
COPY plugins/webui/ ./
RUN bun run build

FROM rust:1.88-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY --from=webui /src/plugins/webui/dist plugins/webui/dist
RUN cargo build --locked --release -p rustfrp-daemon

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --create-home rustfrp
COPY --from=builder /src/target/release/rustfrp-daemon /usr/local/bin/rustfrp-daemon

USER rustfrp
VOLUME ["/data"]
EXPOSE 7900

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD curl --fail --silent http://127.0.0.1:7900/api/v1/health >/dev/null || exit 1

ENTRYPOINT ["rustfrp-daemon"]
CMD ["--db-path", "/data/config.db", "--config-dir", "/data/runtime", "--api-listen", "0.0.0.0:7900"]
