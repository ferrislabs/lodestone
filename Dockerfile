FROM rust:1.95.0-bookworm AS chef

WORKDIR /usr/local/src/oxid

RUN cargo install cargo-chef --version 0.1.77 --locked && \
    cargo install sqlx-cli --version 0.8.6 --no-default-features --features postgres --locked

# --- Plan: extract a recipe of the workspace dependency graph ----------
# Only Cargo.toml / Cargo.lock changes invalidate this layer.
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Cook: build every dependency from the recipe ----------------------
# This layer is cached as long as recipe.json (i.e. the dep graph) is
# stable. Source changes do not invalidate it.
FROM chef AS builder

COPY --from=planner /usr/local/src/oxid/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# --- Build the actual workspace binaries -------------------------------
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

RUN \
    apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates=20230311+deb12u1 \
    libssl3=3.0.17-1~deb12u2 && \
    rm -rf /var/lib/apt/lists/* && \
    addgroup \
    --system \
    --gid 1000 \
    oxid && \
    adduser \
    --system \
    --no-create-home \
    --disabled-login \
    --uid 1000 \
    --gid 1000 \
    oxid

USER oxid

# The migration CLI is a full DDL tool; it has no business in the image that
# serves traffic. It gets its own, run as a job before a rollout.
FROM runtime AS migrator

COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/
COPY --from=builder /usr/local/src/oxid/migrations /usr/local/oxid/migrations

WORKDIR /usr/local/oxid
ENTRYPOINT [ "sqlx", "migrate", "run" ]


FROM runtime AS api

COPY --from=builder /usr/local/src/oxid/target/release/api /usr/local/bin/api
COPY --from=builder /usr/local/src/oxid/migrations /usr/local/oxid/migrations

# The API listens on 3456 and its health/metrics router on 3457. EXPOSE 80 was
# wrong, and the unprivileged user this image runs as could not bind it anyway.
EXPOSE 3456 3457

# No HEALTHCHECK: the runtime image carries no HTTP client, and `api --help`
# would pass whatever the server is doing. Probe /health on the internal port
# from the orchestrator instead.

ENTRYPOINT [ "api" ]


FROM node:24.13.0-alpine AS webapp-deps

WORKDIR /usr/local/src/oxid

ENV PNPM_HOME="/pnpm"
ENV PATH="$PNPM_HOME:$PATH"

RUN \
    corepack enable && \
    corepack prepare pnpm@9.15.0 --activate && \
    apk --no-cache add dumb-init=1.2.5-r3

COPY apps/webapp/package.json apps/webapp/pnpm-lock.yaml ./

RUN pnpm install --frozen-lockfile

FROM webapp-deps AS webapp-build

COPY apps/webapp/ ./

RUN pnpm build

FROM nginxinc/nginx-unprivileged:1.28.0-alpine3.21 AS webapp

# Assets are placed at build time and owned by the runtime user. The entrypoint
# used to wipe and repopulate this directory on every start, which a non-root
# process cannot do - and which was needless work besides.
COPY --from=webapp-build --chown=101:101 /usr/local/src/oxid/dist/client /usr/share/nginx/html
COPY apps/webapp/nginx.conf /etc/nginx/conf.d/default.conf

# The pristine template stays out of the served tree so that every start
# re-substitutes from placeholders rather than from the previous start's values.
COPY --from=webapp-build /usr/local/src/oxid/dist/client/config.json /usr/local/share/oxid/config.json

# Mode set at copy time rather than by a later RUN: this image runs as a
# non-root user, which cannot chmod a file root owns.
COPY --chmod=0755 apps/webapp/docker-entrypoint.sh /docker-entrypoint.d/docker-entrypoint.sh

# The unprivileged nginx image listens on 8080: a non-root process cannot bind
# a port below 1024.
EXPOSE 8080
