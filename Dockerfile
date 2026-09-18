# syntax=docker/dockerfile:1.7
ARG RUST_VERSION=1.91

# Toolchain for tests, fmt and clippy (git + a C compiler for tree-sitter grammars included).
FROM rust:${RUST_VERSION}-bookworm AS dev
RUN rustup component add rustfmt clippy
WORKDIR /workspace

FROM dev AS build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked -p autodoc-cli -p autodoc-mcp \
 && cp target/release/autodoc target/release/autodoc-mcp /usr/local/bin/

# Minimal runtime image: `docker run --rm -v "$PWD:/repo" autodoc-engine generate /repo`
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/bin/autodoc /usr/local/bin/autodoc-mcp /usr/local/bin/
RUN git config --system --add safe.directory '*'
ENTRYPOINT ["autodoc"]

# Browser journeys against HTML produced by the release binary.
FROM mcr.microsoft.com/playwright:v1.63.0-noble AS e2e
RUN apt-get update \
 && apt-get install -y --no-install-recommends git \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace/tests/e2e
COPY tests/e2e/package.json tests/e2e/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY --from=build /usr/local/bin/autodoc /usr/local/bin/autodoc
ENV AUTODOC_BIN=/usr/local/bin/autodoc CI=1
