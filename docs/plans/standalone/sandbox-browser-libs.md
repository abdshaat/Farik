# Standalone: the sandbox image carries the libraries a headless browser needs

Status: executed
Branch: `fix/sandbox-browser-libs` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 8.3, the sandbox (revision 0.135)
Depends on: the sandbox image (phase 3 step 15), its tag carrying the Dockerfile's digest (0.110)

## Goal

Found live 2026-10-20: a team whose tests render in a headless browser (Remotion, Playwright, Puppeteer) fails in the sandbox with "chrome-headless-shell: error while loading shared libraries: libnspr4.so" (also libnss3, libnssutil3). `node:24-bookworm-slim` has none of Chrome's system libraries, and agents cannot install system packages in the sandbox.

Because the image's tag names the Dockerfile's digest, the changed Dockerfile is a new tag and the computer check in Setup offers "Prepare it".

## Decisions

- Install, in the existing `apt-get install --no-install-recommends` line: libnss3 libnspr4 libdbus-1-3 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 libgbm1 libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libpango-1.0-0 libcairo2 libasound2 fonts-liberation.
- Rejected: installing Chromium itself. Projects bring their own browser binary through npm, at the version they test with.

## Tasks

### Task 1: the image names Chrome's libraries

- [x] `computer.rs`: `the_image_carries_the_libraries_a_headless_browser_needs` reads `DOCKERFILE`, finds the `apt-get install` line and asserts `--no-install-recommends` and each package above. Watched failing: "libnss3 is missing".
- [x] `crates/runtime/sandbox/Dockerfile`: add the packages and say why in the header comment.
- [x] `docs/SPEC.md` 8.3 and revision 0.135.

## Verification

`cargo xtask check` (output in the pull request). No test builds the image (CI does not build it); by hand: `docker build -t catervas/sandbox:check crates/runtime/sandbox`, then `docker run --rm catervas/sandbox:check sh -c 'ldconfig -p | grep -E "libnss3|libnspr4"'`.
