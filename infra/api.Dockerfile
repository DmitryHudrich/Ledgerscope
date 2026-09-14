# syntax=docker/dockerfile:1

# Build context is the repo root. cargo-chef splits the build so that the
# dependency compile lands in its own layer, keyed only on the manifests —
# editing crate sources then rebuilds just the workspace, not all of crates.io.

ARG RUST_VERSION=1.94
ARG DEBIAN_RELEASE=trixie

FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS chef
WORKDIR /app
RUN cargo install cargo-chef --locked --version '^0.1'

# The recipe is a distilled manifest: same content => same dependency layer,
# even when the sources that produced it changed.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# The expensive layer — cached until a dependency actually changes.
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo build --release --bin web-api

FROM debian:${DEBIAN_RELEASE}-slim AS runtime
# reqwest talks rustls and verifies against the system store, so the CA bundle
# is the one hard runtime requirement; curl backs the healthcheck.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --uid 10001 --user-group ledgerscope

COPY --from=builder /app/target/release/web-api /usr/local/bin/web-api

USER ledgerscope
ENV BIND_ADDR=0.0.0.0:3000 \
    RUST_LOG=info
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/web-api"]
