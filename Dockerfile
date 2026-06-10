# syntax=docker/dockerfile:1

# --- Cross-compilation helper ---
FROM --platform=$BUILDPLATFORM tonistiigi/xx AS xx

# --- Stage 1: Build frontend (native, output is platform-independent) ---
FROM --platform=$BUILDPLATFORM node:26-alpine AS frontend-build
ARG REPRESENT_IMAGE_TAG
ENV VITE_REPRESENT_IMAGE_TAG=$REPRESENT_IMAGE_TAG
WORKDIR /app
COPY frontend/package.json frontend/yarn.lock frontend/.yarnrc.yml ./
COPY frontend/.yarn/releases ./.yarn/releases
# Yarn is vendored (.yarn/releases/*.cjs + yarnPath in .yarnrc.yml) and invoked
# via node — no corepack, so the build is independent of the node version
# (node 25+ dropped the bundled corepack; vendoring sidesteps that entirely).
RUN node .yarn/releases/yarn-*.cjs install --immutable --network-timeout 1000000
COPY frontend/ .
# adapter-static is configured to emit to ./dist (see svelte.config.js).
RUN node .yarn/releases/yarn-*.cjs build

# --- Stage 2: Build workspace dependencies (cross-compiled, cached) ---
FROM --platform=$BUILDPLATFORM rust:1-alpine AS workspace-deps
COPY --from=xx / /
RUN apk add --no-cache clang lld musl-dev curl
ARG TARGETPLATFORM
RUN xx-apk add --no-cache musl-dev gcc
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
COPY e2e/Cargo.toml e2e/Cargo.toml
# Stub sources for every workspace member — cargo must parse all member
# manifests to load the workspace. e2e is test-only (not shipped), so it just
# needs a stub lib to exist; we build only the backend so e2e's deps stay out
# of the image dep cache.
RUN mkdir -p backend/src e2e/src \
    && printf 'fn main() {}\n' > backend/src/main.rs \
    && : > backend/src/lib.rs \
    && : > e2e/src/lib.rs \
    && xx-cargo build --release -p represent-backend

# --- Stage 3: Build the backend ---
FROM workspace-deps AS backend-build
ARG TARGETPLATFORM
COPY backend/src ./backend/src
# touch so cargo notices the stub→real source swap.
RUN touch backend/src/main.rs backend/src/lib.rs \
    && xx-cargo build --release -p represent-backend

# --- Stage 4: Runtime (scratch) ---
FROM scratch AS runner
WORKDIR /app
LABEL org.opencontainers.image.description="represent — markdown demo scripts, viewed and patched at the table"
LABEL org.opencontainers.image.source="https://github.com/eetu/represent"

COPY --from=backend-build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=backend-build /app/target/*/release/represent-backend ./represent-backend
COPY --from=frontend-build /app/dist ./dist

ENV STATIC_DIR=./dist
ENV REPRESENT_BIND=0.0.0.0:3008
ENV REPRESENT_DB_PATH=/data/represent.db

USER 1000

EXPOSE 3008

CMD ["./represent-backend"]
