# The image a task's container runs (`docs/SPEC.md` 8.3), tagged
# `catervas/sandbox:<crate version>-<digest>`, the digest naming this file's contents, so a changed
# image is a new tag. It ships with Catervas and is built by the computer check in Setup (since 0.23),
# or by hand with `docker build -t catervas/sandbox:<crate version>-<digest> crates/runtime/sandbox`;
# CI does not build it.
#
# The container runs as the user's own uid, which has no passwd entry here, so HOME is /tmp and
# the Rust toolchain lives under /usr/local, world-writable, so that cargo, npm and pnpm can write
# their caches whoever runs them. The image's /tmp must be empty: root's npm cache, left there by
# the build, would be the user's uid's to write, and it could not.
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

# pnpm pinned, like everything else the repository depends on (docs/standards/code.md).
RUN npm install -g pnpm@10.34.5 \
    && chmod -R a+rwX /usr/local/lib/node_modules \
    && find /tmp -mindepth 1 -delete

WORKDIR /workspace
