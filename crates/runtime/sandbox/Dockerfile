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
#
# The libraries a headless Chrome needs (libnss3, libnspr4, ...) are installed, because a project's
# own browser binary (Remotion, Playwright, Puppeteer) fails to start without them and an agent
# cannot install system packages here. The browser itself is not: projects bring it through npm.
FROM node:24-bookworm-slim

ENV HOME=/tmp \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates coreutils curl git python3 build-essential \
        libnss3 libnspr4 libdbus-1-3 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 libgbm1 \
        libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libpango-1.0-0 libcairo2 \
        libasound2 fonts-liberation \
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
