# Scheduled and background jobs

Every job found in the code, the continuous integration and the configuration, read at commit
`de0f2b8a`. No cron file, systemd unit, scheduled GitHub workflow (`on: schedule`) or job queue was
found. The project's timed work runs inside the `catervas serve` or `catervas run` process: in the
orchestrator's tick, or in tasks spawned beside it. None of it runs while no Catervas process is
running.

## Continuous integration

| Job | When | What runs | Defined in |
|---|---|---|---|
| `check` | Every pull request; every push to `main` | On `ubuntu-latest`: Rust 1.98.1 with rustfmt and clippy, pnpm and Node, `docker pull alpine:3.22`, the Playwright image the UI/UX Designer's kit pins, Playwright's Chromium, then `cargo xtask check --integration` | `.github/workflows/check.yml` |

`cargo xtask check --integration` runs, in order (`xtask/src/main.rs` `check`, `xtask/src/check.rs`):
`pnpm -r --if-present generate` and `pnpm --filter @catervas/web build`; `cargo fmt --all --check`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -- --include-ignored`;
the bare-TODO check; the `catervas-core` no-I/O check; `pnpm install --frozen-lockfile` and
`pnpm check`; then the integration steps (clippy of `catervas-runtime` with feature `e2e`, the
end-to-end server's lint, tests and build, and the browser journey).

## Git hooks (local, triggered, not scheduled)

| Hook | When | What runs | Defined in |
|---|---|---|---|
| `pre-commit` | Each local commit, once `cargo xtask install-hooks` has been run | `cargo xtask pre-commit`: `cargo fmt --all --check` and the bare-TODO check | `xtask/src/main.rs` `install_hooks`, `pre_commit` |
| `commit-msg` | Each local commit, as above | `cargo xtask commit-msg <file>`: the message's type (`feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `build`, `ci`, `perf`) and a subject of at most 72 characters | `xtask/src/main.rs`, `xtask/src/commit_message.rs` |

## In the running process: every tick

`catervas serve` ticks again when a tick acts, when a command or a browser request wakes it, or at
the latest 60 seconds after an idle tick (`crates/runtime/src/orchestrator.rs` `RECHECK`, `wait_until`;
`crates/cli/src/run.rs` `OnIdle::Wait`). `catervas run` returns at the first idle tick with no
sleeping agent to wait for (`OnIdle::Return`). The first four rows run before the pause is read, so
they run while the team is paused (`crates/runtime/src/orchestrator.rs` `tick_within`).

| Job | When | What it does | Defined in |
|---|---|---|---|
| End marketing plans | Every tick not scoped to one task | Records the end of each approved plan a newer one replaced, and each whose last day has passed | `crates/runtime/src/orchestrator/rules.rs` `end_marketing_plans` |
| Hand posts to Buffer | Every tick not scoped to one task | Hands a scheduled post to Buffer from one hour before its time; not within five minutes of it or after | `crates/runtime/src/orchestrator/hand_over.rs` (`HAND_OVER_BEFORE`, `TOO_LATE`) |
| Close orders, flag renewals | Every tick not scoped to one task; the renewals check once a UTC day | Closes purchase orders nobody decided or placed in 30 days; flags renewals the vendor register says are coming up | `crates/runtime/src/orchestrator/rules.rs` `close_orders_and_flag_renewals` |
| Check the procurement mailbox | Due every 15 minutes, checked each tick; spawned, at most one at a time | Reads replies to the procurement address over IMAP | `crates/runtime/src/orchestrator/rules.rs` `check_mail`, `crates/runtime/src/procurement/replies.rs` (`EVERY`) |
| Sprint ends by itself | Each tick under `All` or `Planning` | Ends the open sprint once it holds a task and every task in it is accepted or cancelled | `rules.rs` `finished_sprint` |
| Sprint review and retro | After a sprint ends, under `All` | One ceremony session for the review, then one for the retro | `rules.rs` `review_and_retro` |
| Sprint planning | While the open sprint holds no task and its planning has not run | One ceremony session of the assigner | `rules.rs` `sprint_planning` |
| Standup | Once a UTC day, when a task in the sprint moved | One ceremony session in the `standup` thread | `rules.rs` `standup` |
| Escalate a blocked task | Each tick, for a task blocked at least the team's `blocked_limit_hours` | Asks the governor to escalate it | `rules.rs` rule 4 `blocked`; `crates/core/src/governor/escalation.rs` (`DEFAULT_BLOCKED_LIMIT`, 24 hours) |
| Age an escalation | Each tick, for the oldest open escalation past `escalation_age_hours` (default 24) | Records `escalation.aged` once and says so in the channel | `rules.rs` `aged`; `docs/schemas/team.schema.json` |
| Escalate a spent task | Each tick | Escalates a task whose dollars or sessions are spent | `rules.rs` `budget` |
| Wake a sleeping agent | When the time its model provider's limit resets comes | The tick waits until then (capped at 60 seconds) and starts its sessions again | `crates/runtime/src/sleep.rs`, `orchestrator.rs` `wait_until` |

## In the running process: beside the tick

| Job | When | What it does | Defined in |
|---|---|---|---|
| Watch Google Ads spend | Wakes every minute; reads a plan's spend when its last try is 15 minutes old; idle until the first campaign exists | Pauses campaigns that reached a budget and those an ended plan left running; runs while the team is paused | `crates/runtime/src/orchestrator/spend.rs` (`TRY_EVERY`, `watch_marketing_spend`), spawned in `crates/cli/src/start.rs` |
| Browser subscriptions | Every 500 ms while a browser is subscribed | Reads the log for new events and sends them over `/rpc` | `crates/runtime/src/daemon/web.rs` (`POLL`) |
| Recovery at start | Once, before the first tick of `run` or `serve` | Ends sessions a killed run left open, removes worktrees and sandboxes of accepted or cancelled tasks | `crates/runtime/src/orchestrator/recover.rs`, called in `crates/cli/src/start.rs` |

Sign-in tokens, the eBay grant and the preview's state are refreshed when a call needs them, not on a
schedule (`crates/runtime/src/daemon/signed_in.rs`, `crates/runtime/src/ebay.rs`,
`crates/runtime/src/preview.rs` `AVAILABLE_FOR`).
