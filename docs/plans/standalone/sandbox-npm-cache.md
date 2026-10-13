# Standalone: npm can write its cache in the sandbox image

Status: ready
Reviewed by: a fresh Opus 5.5 session, 2026-10-12 (one round: ready after fixes, its seven findings folded in below)
Branch: `fix/sandbox-npm-cache` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 8.3, the sandbox (revision 0.109)
Depends on: the sandbox image (phase 3 step 15) and the preview (0.33), both on main

## Goal

The founder, 2026-10-12, about an escalation the Developer and the UI/UX Designer both raised: "the preview's prepare failed: … npm error code EACCES … path /tmp/.npm/_cacache/tmp/… Your cache folder contains root-owned files … Please analyze the error and work on fixing it".

Cause, reproduced on `catervas/sandbox:0.0.0` (built 2026-10-10): the Dockerfile sets `HOME=/tmp` and then, as root, runs `npm install -g pnpm`, so npm leaves its cache at `/tmp/.npm` (11 MB) and Node its compile cache at `/tmp/node-compile-cache`, both owned by root and baked into the image. Every container runs as the user's own uid (1000 here), whose `HOME` is `/tmp`, so `npm install` in `prepare`, or in a task's sandbox, cannot open `/tmp/.npm/_cacache/tmp/…` and stops; the `TAR_ENTRY_ERROR ENOENT` warnings before it are npm tearing down its half-done extraction into `node_modules`, the same failure.

```
$ docker run --rm --user 1000:1000 catervas/sandbox:0.0.0 sh -c 'ls -ld /tmp/.npm/_cacache/tmp; touch /tmp/.npm/_cacache/tmp/x'
drwxr-xr-x 2 root root 4096 Oct 10 16:17 /tmp/.npm/_cacache/tmp
touch: cannot touch '/tmp/.npm/_cacache/tmp/x': Permission denied
```

A fixed Dockerfile alone would not reach this computer: the image is tagged `catervas/sandbox:<crate version>`, still `0.0.0`, the computer check counts the image ready whenever that tag exists, and nothing builds it again. So the tag also names the Dockerfile's contents: a changed Dockerfile is a new tag, the old image counts as missing, and the computer check in Setup (`/setup/computer`) offers "Prepare it", which builds the new one (`sandbox.build` answers under `serve` too). Settings → Computer has no such button; adding one is a separate change.

Out of scope: removing the old image after a new one is built (it stays until the user removes it); keeping npm's cache between containers (each `prepare` and each task container starts with an empty one, as before); putting the image's tag in the preview's prepare key (the task's tree and the `prepare` text, 8.3), so a `prepare` that succeeded under an old image is reused under a new one; it failed under this one, so nothing is reused here.

## Decisions

- The Dockerfile's last `RUN`, the one that runs npm, ends with `find /tmp -mindepth 1 -delete`, so the image's `/tmp` holds nothing; whatever runs later as the user's uid makes its caches there as their owner. `/tmp` itself stays root's with the sticky bit (`drwxrwxrwt`), as the base image has it.
- `SANDBOX_IMAGE` becomes `catervas/sandbox:<crate version>-<digest>`, `<digest>` the first 6 bytes of the sha256 of `crates/runtime/sandbox/Dockerfile` written as 12 lowercase hex digits (each byte `{:02x}`: sha2 0.11's digest has no `LowerHex`, so `format!("{:x}", …)` does not compile). `crates/runtime/build.rs` reads `sandbox/Dockerfile` (relative to the crate folder), computes it, panics with a message when the file cannot be read, and hands it over as `CATERVAS_SANDBOX_DIGEST`, with `cargo:rerun-if-changed=sandbox/Dockerfile` beside the web app's watch; `sha2` (already a workspace dependency) becomes a build dependency of `catervas-runtime`. `SANDBOX_IMAGE` stays a `&'static str` built with `concat!`, so no caller changes.
- A missing image now says how to get it: `SandboxError::ImageMissing` and the preview's "the sandbox image … is not on this machine" end with ". Open the computer check in Setup and choose Prepare it", as the Designer's browser already says "Open the computer check in Setup and choose Fetch it" (`preview/docker.rs:132`), since after this change an existing team meets a missing image the first time Catervas is upgraded.
- No ADR: the image's tag is not an architectural decision, and the spec records it.
- Nothing compares the tag's shape, so no other caller or test changes: `daemon/web.rs`'s tests use `crate::SANDBOX_IMAGE`, `host.rs`'s `"catervas/sandbox:0.0.0"` is sample input to `Display`, `setup.test.tsx` uses a fake reply, the end-to-end fixtures and `tests/docker_sandbox.rs` run `alpine:3.22`, and CI and xtask do not build the image.

## Tasks

### Task 1: the image leaves nothing in `/tmp`

- [x] `computer.rs`: `the_image_leaves_nothing_in_tmp` reads `DOCKERFILE`, joins its `\`-continued lines, takes the last `RUN` instruction, and asserts that it runs `npm` and ends with `find /tmp -mindepth 1 -delete`, and that no `RUN` follows it. Watched failing: the last `RUN` ends with `chmod -R a+rwX /usr/local/lib/node_modules`.
- [x] `crates/runtime/sandbox/Dockerfile`: append `&& find /tmp -mindepth 1 -delete` to the pnpm `RUN` (GNU find: bookworm's findutils is Essential), and say why in the header comment (root's caches under `HOME=/tmp` would be the user's uid's to write); the header's "neither `catervas run` nor CI builds it" becomes that the computer check in Setup builds it (since 0.23) and CI does not.
- [x] Live check, pasted in the pull request: `docker build -t catervas/sandbox:check crates/runtime/sandbox`, then, as `--user $(id -u):$(id -g)`, `find /tmp -mindepth 1` prints nothing and `npm install` of a small package in a mounted folder succeeds, where on `catervas/sandbox:0.0.0` it fails with `EACCES`.

### Task 2: the image's tag names its Dockerfile

- [ ] `sandbox.rs`: `the_image_tag_names_its_dockerfile` asserts that `SANDBOX_IMAGE` is `catervas/sandbox:<CARGO_PKG_VERSION>-<the first 6 bytes of sha256(include_str!("../sandbox/Dockerfile")) as {:02x} each>`, computing the hex in the test itself (not `crate::daemon::hex`, which is Unix-only), so a build script that hashed the wrong file fails it. Watched failing: the tag is `catervas/sandbox:0.0.0`.
- [ ] `crates/runtime/build.rs`: hash `sandbox/Dockerfile`, print `cargo:rustc-env=CATERVAS_SANDBOX_DIGEST=<12 hex>` and `cargo:rerun-if-changed=sandbox/Dockerfile`, keeping the web app's watch; its doc comment says both jobs. `Cargo.toml`: `[build-dependencies] sha2.workspace = true`.
- [ ] `sandbox.rs`: `SANDBOX_IMAGE` is `concat!("catervas/sandbox:", env!("CARGO_PKG_VERSION"), "-", env!("CATERVAS_SANDBOX_DIGEST"))`, its doc comment saying why; the Dockerfile's header comment names the new tag.
- [ ] The missing-image words of the Decisions: `displays_each_error_in_words` (`sandbox/host.rs`) first asserts that `ImageMissing`'s `Display` ends with "Open the computer check in Setup and choose Prepare it" (watched failing), then `SandboxError::ImageMissing`'s `Display` says it; `preview/docker.rs`'s own "the sandbox image … is not on this machine" says the same, built from that `Display` (`SandboxError::ImageMissing { image }.to_string()`) so the two cannot drift. `refuses_an_image_that_is_not_there` (`tests/docker_sandbox.rs`) asserts the variant and is unchanged.
- [ ] `docs/plans/project-plan.md`: the interface line naming `SANDBOX_IMAGE: &str = catervas/sandbox:<crate version>` (around line 282) names the new tag, with a note "(changed 2026-10-12, standalone plan `sandbox-npm-cache`)".
- [ ] Spec revision 0.109 (the next free number when the pull request merges): 8.3 says the image's tag carries the Dockerfile's digest, so a changed image counts as missing until it is prepared again, that the image's `/tmp` holds nothing, so npm, pnpm and cargo write their caches as the user's uid, and that a missing sandbox image, in a task's sandbox or a preview, says to open the computer check in Setup and choose Prepare it, as the browser's does; 4.1's "whether the sandbox image is built" adds "for this Catervas, its tag carrying the Dockerfile's digest"; the revision line at the top.

## Verification

- `cargo xtask check` (with `CARGO_BUILD_BUILD_DIR=$PWD/target`), output pasted.
- `cargo test -p catervas-runtime --test docker_sandbox -- --ignored` against the local Docker, output pasted.
- Task 1's live check, and on this computer: the computer check at `/setup/computer` shows the sandbox image missing, "Prepare it" builds `catervas/sandbox:0.0.0-<digest>`, and `npm install` in the live team's preview `prepare` gets past the cache.
