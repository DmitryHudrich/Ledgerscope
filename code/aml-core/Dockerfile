# aml-core (Rust graph engine) — cargo-chef layered build.
#
# cargo-chef splits the build in two: `prepare` distils the workspace into a
# recipe.json holding only the dependency graph, `cook` compiles those
# dependencies. Because the recipe changes only when Cargo.toml/Cargo.lock
# change, editing application source re-runs just the final `cargo build`
# instead of rebuilding ~600 crates (alloy, aws-lc, jemalloc) every time.
#
# Build context: code/aml-core

ARG RUST_IMAGE=lukemathwalker/cargo-chef:latest-rust-1-slim-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS chef
WORKDIR /build

# Build-time egress proxy. Set these when crates.io / apt are only reachable
# through a proxy; they are confined to the build stages and never end up in
# the runtime image. Runtime egress uses the SOCKS proxy from config instead
# (see `proxy.socks_url` / PROXY__SOCKS_URL).
ARG HTTP_PROXY=""
ARG HTTPS_PROXY=""
ARG NO_PROXY="localhost,127.0.0.1"
ENV http_proxy=${HTTP_PROXY} \
    https_proxy=${HTTPS_PROXY} \
    no_proxy=${NO_PROXY} \
    HTTP_PROXY=${HTTP_PROXY} \
    HTTPS_PROXY=${HTTPS_PROXY} \
    NO_PROXY=${NO_PROXY}

# build-essential + make: tikv-jemalloc-sys builds jemalloc via autotools.
# cmake + perl: aws-lc-sys (jsonwebtoken's aws_lc_rs backend) builds AWS-LC.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        perl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Dependency layer — invalidated only by Cargo.toml / Cargo.lock changes.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
# Application layer.
COPY . .
RUN cargo build --release --bin api \
    && strip target/release/api

FROM ${RUNTIME_IMAGE} AS runtime
# ca-certificates: TLS to etherscan/alchemy/moralis/trongrid through the SOCKS
# proxy. curl: container healthcheck.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/app app

WORKDIR /app
COPY --from=builder /build/target/release/api /usr/local/bin/paytraces-api
# Baseline config so the image runs standalone; compose bind-mounts the repo
# copy over it and overrides host-specific keys via PROXY__/DATABASE__/... env.
COPY config.yaml /app/config.yaml
# `labels.bootstrap_file` is the relative path "api/examples/wallet_labels.jsonc";
# keep that layout so the default resolves without an override.
COPY api/examples/wallet_labels.jsonc /app/api/examples/wallet_labels.jsonc

# Provider disk caches live here; mount a volume to persist them.
RUN mkdir -p /data && chown -R app:app /data /app
USER app

ENV SERVER__HOST=0.0.0.0 \
    SERVER__PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=40s --retries=5 \
    CMD curl -fsS http://127.0.0.1:8080/scalar >/dev/null || exit 1

ENTRYPOINT ["/usr/local/bin/paytraces-api"]
CMD ["--config", "/app/config.yaml"]
