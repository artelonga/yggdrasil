# yggdrasil-web — Fly.io image.
#
# IMPORTANT: build context is the PARENT directory of yggdrasil/ so we can
# pull in co/game-core (path dep `../co/game-core` in Cargo.toml). Until
# YG-17 switches to a git-rev pin, the deploy command must be:
#
#   cd /Users/artelonga/projects
#   flyctl deploy --config yggdrasil/fly.toml \
#                 --dockerfile yggdrasil/yggdrasil-web/Dockerfile
#
# Trixie matches the co-web image: glibc 2.40 avoids __isoc23_* link errors
# from native deps.

FROM rust:1.90-slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler pkg-config libssl-dev g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src

# --- game-core (path dep) ---
COPY co/game-core/Cargo.toml co/game-core/Cargo.toml
COPY co/game-core/build.rs co/game-core/build.rs
COPY co/game-core/1cafa6d8/ co/game-core/1cafa6d8/
COPY co/game-core/5e689e2b/ co/game-core/5e689e2b/
COPY co/game-core/src/ co/game-core/src/

# --- yggdrasil workspace manifests (dep-cache layer) ---
COPY yggdrasil/Cargo.toml yggdrasil/Cargo.lock yggdrasil/
COPY yggdrasil/yggdrasil-core/Cargo.toml yggdrasil/yggdrasil-core/Cargo.toml
COPY yggdrasil/yggdrasil-web/Cargo.toml yggdrasil/yggdrasil-web/Cargo.toml

# YG-56..58: universe-vim is a path-dep of yggdrasil-web; copy its manifest into the cache layer
COPY yggdrasil/universes/universe-vim/Cargo.toml yggdrasil/universes/universe-vim/Cargo.toml

RUN mkdir -p yggdrasil/yggdrasil-core/src yggdrasil/yggdrasil-web/src yggdrasil/universes/universe-vim/src && \
    echo "pub fn _dummy() {}" > yggdrasil/yggdrasil-core/src/lib.rs && \
    echo "fn main() {}" > yggdrasil/yggdrasil-web/src/main.rs && \
    echo "pub fn _dummy() {}" > yggdrasil/universes/universe-vim/src/lib.rs

WORKDIR /usr/src/yggdrasil
RUN cargo build --release -p yggdrasil-web 2>/dev/null || true

# --- real sources ---
COPY yggdrasil/yggdrasil-core/src yggdrasil-core/src
COPY yggdrasil/yggdrasil-web/src yggdrasil-web/src
COPY yggdrasil/yggdrasil-web/static yggdrasil-web/static
COPY yggdrasil/yggdrasil-web/build.rs yggdrasil-web/build.rs
COPY yggdrasil/universes/universe-vim/src universes/universe-vim/src

RUN rm -rf target/release/.fingerprint/yggdrasil* \
           target/release/deps/yggdrasil_* \
           target/release/yggdrasil-web 2>/dev/null; \
    cargo build --release -p yggdrasil-web

# --- runtime ---
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/yggdrasil/target/release/yggdrasil-web /usr/local/bin/yggdrasil-web
# Static is served via ServeDir::new("yggdrasil-web/static") (CWD-relative),
# so keep WORKDIR=/app and lay out the static tree under it.
COPY --from=builder /usr/src/yggdrasil/yggdrasil-web/static /app/yggdrasil-web/static

VOLUME /data

ENV YGGDRASIL_DB=/data/yggdrasil.db
ENV YGGDRASIL_SEMENTES_DB=/data/yggdrasil-sementes.db
ENV RUST_LOG=yggdrasil_web=info,tower_http=info

RUN mkdir -p /data && groupadd -r ygg && useradd -r -g ygg -d /data ygg && chown ygg:ygg /data
USER ygg

EXPOSE 3030

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3030/health || exit 1

CMD ["/usr/local/bin/yggdrasil-web"]
