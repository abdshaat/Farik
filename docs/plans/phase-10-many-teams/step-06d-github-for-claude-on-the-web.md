# Phase 10, step 06d: GitHub, for an owner who uses Claude on the web

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 4.4, 6.7, 8.4, 8.6; ADR 0059, ADR 0044
Depends on: steps 06 and 06c (committed)
Mockups: `ManyWebOnly` frames 3 and 5 (GitHub, and a repository not on this computer), and `ManyStates` "GitHub not connected", approved with step 01
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

The owner connects GitHub once for the computer. Catervas then lists their repositories with recent pushes, each with:

- whether it is on this computer;
- its branches named `claude/…`, made by Claude Code on the web;
- its open pull requests from those branches.

A repository not on this computer shows on "Here's what you work on" with **Watch** or **Copy to this computer**. The copy is a clone into a folder the owner picks, after which it is a project like any other.

Until Catervas Cloud's single click (phase 11, ADR 0044), GitHub is connected with a fine-grained token the owner pastes, read-only, as phase 7 step 07c's kits take one. Step 07 reads the same connection for scheduled runs and deployments.

Out of scope:
- the single click (phase 11);
- writing anything to GitHub;
- scheduled runs and deployments (step 07).

## Decisions

- **One GitHub connection for the computer.** Its token is kept by the connector key store (`io.connector_secrets`, the OS keychain else `connectors.json`) under the account `machine:github`, never in `machine.db` or a team's files. It is read only by the hub's GitHub client, to send to `api.github.com`, and by the clone, below. It is never shown, logged or given to an agent. This is not an Anthropic key, so the founder's ruling (ADR 0059) does not forbid it, and phase 7 already keeps such keys.
- **Connecting.**
  - `github.connect { token }` checks the token with `GET /user`. A 401 is `github_token_refused`. It then keeps the token and answers `{ login }`.
  - The page asks for a fine-grained token with Contents read, Metadata read, Pull requests read, Actions read and Deployments read on the owner's repositories, and says so in the words of 07c's setup copy.
  - `github.status` answers `{ connected: bool, login? }`.
  - `github.disconnect` deletes the key.
  - `github.connect` joins `SECRET_METHODS`.
- **The client** (`crates/runtime/src/github.rs`) uses `reqwest` with `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28`, a 15-second timeout and no redirect off `api.github.com`. Its base address is `https://api.github.com`, replaced only in tests by `GithubClient::new_at(base)`, as `google_ads` is tested against a fake server. Its calls:
  - `repositories()` is `GET /user/repos?sort=pushed&per_page=100&affiliation=owner,collaborator`, the first page only, keeping those pushed in the last 90 days.
  - `claude_branches(repo)` is `GET /repos/{full}/branches?per_page=100`, keeping names starting `claude/`.
  - `claude_pulls(repo)` is `GET /repos/{full}/pulls?state=open&per_page=100`, keeping heads starting `claude/`, with title, number, draft and `updated_at`.

  A 403 with `x-ratelimit-remaining: 0` is `github_rate_limited`, which shows the reset time. Every response body is parsed for these fields alone.
- **Into the index.** Migration 8 of `machine.db` adds:
  - `github_repos(full_name TEXT PRIMARY KEY, remote_key TEXT NOT NULL, pushed_at TEXT NOT NULL, private INTEGER NOT NULL, default_branch TEXT NOT NULL, seen_at TEXT NOT NULL) STRICT`;
  - `github_claude_work(full_name TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('branch','pull')), name TEXT NOT NULL, number INTEGER, draft INTEGER, updated_at TEXT, PRIMARY KEY (full_name, kind, name)) STRICT`.

  `remote_key` is step 04's normalised remote, so a repository whose remote a found folder already has is the same project, sourced `github` as well (section 2c). One that is not on the computer is a `github` project in `found()`, suggested Watch.
- **When it is read.** After `github.connect`, as part of every scan (step 06's `scan` gains a GitHub phase when connected), and at the daily rescan. Each read replaces the two tables' rows for the repositories it saw. A repository missing from a newer read is kept, marked "not seen lately", not deleted, as an export is.
- **`finding.list` rows** gain `kind: "github"` for one not on the computer, and every row gains `claude_work: [{ kind, name, number?, draft? }]`. The page shows, for example, "2 branches from Claude on the web · 1 pull request (draft)".
- **Copy to this computer** is `github.copy { full_name, parent, name? }`.
  - It checks `parent` and the folder name as `project.create` does (inside home, a new name).
  - It clones with `git clone --` of `https://github.com/<full>.git`, the token given to git through the environment and never on the command line: `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=http.https://github.com/.extraheader`, `GIT_CONFIG_VALUE_0=AUTHORIZATION: bearer <token>`. It also sets `GIT_TERMINAL_PROMPT=0`.
  - It answers `{ folder }` and then rescans that folder, so the row becomes a folder project with its sessions and jobs.
  - A failed clone is `copy_failed` with git's last line, the token cut out of it if present.
- **The pages.**
  - Frame 3, "Connect GitHub", sits at `/find/github` between the export and the services, as "2 of 3 · optional". It has the three bullets, a token field with the link "Make a token on GitHub" (`https://github.com/settings/personal-access-tokens/new`), **Connect GitHub** and **Skip**.
  - On "Here's what you work on", a `github` row offers **Watch** (the default) and **Copy to this computer**. Copy opens the folder browser for the parent, then shows "Copied to ~/…".
  - `ManyStates`' "GitHub not connected" belongs to a job card, step 07's; this step adds the `github.status` it reads.
  - This computer gains "GitHub: connected as <login>" with **Disconnect**, or **Connect GitHub**.

## File map

```
crates/runtime/src/github.rs                       creates: GithubClient
crates/store/src/migrations/machine/0008_github.sql  creates
crates/store/src/machine.rs                        modifies: github rows into found()
crates/runtime/src/daemon/finding.rs               modifies: github.connect/status/disconnect/copy
crates/cli/src/finding.rs                          modifies: the GitHub phase of the scan; the clone
docs/schemas/rpc.schema.json, crates/protocol, packages/protocol-client   modifies
apps/web/src/pages/find/ConnectGithub.tsx           creates
apps/web/src/pages/find/Found.tsx, ExportDrop.tsx, pages/ThisComputer.tsx   modifies
docs/SPEC.md                                       modifies: revision 0.148
```

## Tasks

### Task 1: The GitHub client

Tests (`github.rs`, against a fake server on localhost that answers recorded GitHub-shaped JSON, written by hand with invented repositories):
- `reads_repositories_and_claude_s_work_from_github`: asserts these.
  - `repositories()` keeps the two pushed within 90 days of the three.
  - `claude_branches` keeps `claude/fix-login` and not `main`.
  - `claude_pulls` keeps the one from `claude/fix-login` with `draft: true`.
  - Each request carried `Authorization: Bearer <token>` and the API version header.
  - A 403 with no remaining rate is `RateLimited` with its reset.
  - A 401 is `Refused`.

- [x] `feat(runtime): read the owner's repositories and claude's work from github`

### Task 2: GitHub in the index

Tests (`machine.rs`):
- `keeps_github_repositories_once_beside_folders`: asserts these.
  - A GitHub repository whose remote key equals a found folder's adds `github` to that project's sources and makes no second project.
  - One with no folder is a `github` project with its Claude work.
  - A later read without it keeps it, marked not seen lately.
  - The migrations list ends at 8.

- [x] `feat(store): keep github repositories and claude's work in the machine index`

### Task 3: Connect, read and copy from the hub

Tests:
- `connects_github_and_lists_its_repositories` (`crates/runtime/src/daemon/finding.rs`, the fake server and a memory key store): asserts these.
  - `github.connect` with a token the fake accepts keeps it under `machine:github` and answers the login.
  - `finding.list` then has the `github` rows with `claude_work`.
  - A refused token is `github_token_refused` and keeps nothing.
  - The refusal of `github.connect` holds no part of the token.
- `copies_a_repository_to_this_computer` (`crates/cli/tests/serving.rs`, ignored): asserts these.
  - The test serves a bare repository over a local `file://` path through a test-only base. `github.copy` clones it into the chosen parent and answers the folder.
  - The folder is a git repository whose `origin` is the given address.
  - The token was not in any process's arguments: the clone is spawned through a seam that records its arguments.

- [ ] `feat(runtime): connect github, list its work and copy a repository`

### Task 4: The pages

Tests (`pages/find/Github.test.tsx`):
- `connects_github_and_offers_a_copy`: asserts these.
  - On `/find/github`, pasting a token and pressing **Connect GitHub** sends `github.connect`, and the login shows.
  - **Skip** goes to `/find/services`.
  - A `github` row on Found offers Watch and **Copy to this computer**, which sends `github.copy` after the folder pick and shows "Copied to".
  - This computer shows "GitHub: connected as octo" with **Disconnect**.

- [ ] `feat(web): connect github and copy a repository to this computer`

### Task 5: Spec

`docs/SPEC.md` revision 0.148 (2026-10-22), "records phase 10 step 06d, GitHub for an owner who uses Claude on the web". It covers:
- 4.4: the pages;
- 6.7 and 8.6: the computer's GitHub key, read only for GitHub's API and the clone, never on a command line;
- 8.4: migration 8.

Then the project plan's row 06d: "planned and executed 2026-10-22".

- [ ] `docs(spec): github for claude on the web, revision 0.148`

## Verification

```
python3 reaper.py cargo xtask check
python3 reaper.py cargo test -p catervas --test serving -- --include-ignored copies_a_repository
# expected: xtask check: ok; the copy test passes
```
