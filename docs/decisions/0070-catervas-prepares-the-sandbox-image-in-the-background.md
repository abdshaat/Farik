# 0070. Catervas prepares the sandbox image in the background

Date: 2026-10-18
Status: accepted (the founder, 2026-10-18: "Add a loading bar next to 'The safe box itself, which Catervas prepares in Docker' so that when it prepares the docker it can immediately refresh and allow the user to move forward in the process."; the mechanism decided by the planner the same day)

Amends:
- **SPEC 4.1** (setup's computer check, added in 0.23): "it builds the sandbox image when Docker is ready and the image is not" stays; how the build runs and is watched changes.

## Context

Setup's computer check builds `SANDBOX_IMAGE` through the wizard's `sandbox.build` call, which runs `docker build` inside the request (`build_sandbox_image`, `crates/runtime/src/computer.rs`) and answers only when the build ends, a few minutes on a first run. Until then the page shows a busy button and nothing else: no sign of progress, and the child is `kill_on_drop`, tied to the request, so a reload or a dropped socket ends the build half done. Setup mode has no event log, so nothing tells the page the image is ready but the answer to that one call.

## Decision

1. **`sandbox.build` starts a job and answers at once.** The job is held by the daemon (`DaemonState`), one at a time: a call while one runs joins it and starts no second `docker build`; a call after one failed starts afresh. The answer's shape is unchanged (`{ "image": SANDBOX_IMAGE }`, the image being prepared); a missing Docker is still refused at once. The child is the job's, so closing the page does not end it; stopping Catervas still does (`kill_on_drop` on the job's child).
2. **The computer check reports the job.** `computer.check`'s `sandbox_image` item gains the state `preparing`, with `done` and `of` when the build has said which step it is on, and `failed`, Docker's last words, on a `missing` item whose last build failed. A running job wins over what Docker says of the image, so the row reads Ready only once the job ended well.
3. **Progress is Docker's own step count.** The build runs with `BUILDKIT_PROGRESS=plain` in its environment (an environment variable, not `--progress`, which the legacy builder refuses), and each output line is read: BuildKit's `#<n> [<stage> ]<k>/<m>]` and the legacy builder's `Step <k>/<m> :` give step `k` of `m`; `done` is the highest `k` seen. No line matched: no `done` and `of`, and the page shows a bar that moves without a measure.
4. **The page asks again while it prepares**: once a second while the item is `preparing`, then stops; Ready enables Continue with no press.

Rejected:
- Events for the build: setup mode has no project and no log to record them in.
- A streamed answer to `sandbox.build`: the protocol answers each call once, and a reload would still lose it.
- Showing Docker's step text ("RUN apt-get …"): it means nothing to the owner; the step count and a plain sentence do.

## Consequences

- A reload during the build shows it still preparing, at the step it reached.
- The `computerItem` schema gains one state and three optional fields; every other row of the check is unchanged.
