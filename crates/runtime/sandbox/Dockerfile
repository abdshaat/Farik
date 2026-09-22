# The image a task's container runs (`docs/SPEC.md` 8.3), tagged `farik/sandbox:<crate version>`.
# It ships with Farik and is built by `farik run` (phase 3 step 13), not by CI.
#
# The container runs as the user's own uid, which has no passwd entry here, so HOME is /tmp and
# the Rust toolchain lives under /usr/local, world-writable, so that cargo, npm and pnpm can write
# their caches whoever runs them.
FROM node:24-bookworm-slim

ENV HOME=/tmp \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates coreutils curl git python3 build-essential \
    && rm -rf /var/lib/apt/lists/*

# The toolchain pinned in rust-toolchain.toml.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.98.1 --component rustfmt,clippy \
    && chmod -R a+rwX /usr/local/cargo /usr/local/rustup

RUN npm install -g pnpm && chmod -R a+rwX /usr/local/lib/node_modules

WORKDIR /workspace
