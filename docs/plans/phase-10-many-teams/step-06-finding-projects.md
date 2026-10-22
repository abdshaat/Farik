# Phase 10, step 06: Finding projects, behind the hub

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 3, 4.1, 4.4, 6.7, 8.4, 8.6; ADR 0059
Depends on: steps 02 to 05 (committed): the machine index and its scan (`crates/cli/src/discover.rs`, `crates/store/src/{machine,claude_history,jobs,claude_services,claude_export}.rs`), and the hub (`crates/runtime/src/daemon/hub.rs`, `/hub/rpc`)
Mockups: `ManyFindOffer`, `ManyScanning`, `ManyFound`, `ManyWebOnly` (frames 2, 4 and 5), `ManyStates`, approved with step 01. The screens are step 06c's; this step is what they call.
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

This plan's own row: the project plan's row 06 ("Finding projects in the web app") is split in three at the seam between the server and the screens:

- **06**, this plan: finding behind the hub.
- **06c**: the screens.
- **06d**: GitHub for an owner who uses Claude on the web, with "Copy to this computer".

Row 06b (linking and the proposal) is unchanged and comes after 06c.

## Goal

Everything the journey's finding screens need is answered by the hub. All of it lives in `machine.db`, never in a team's log:

- **The scan itself:** whether there is history to look through; a scan that runs in the background with live counts, stops and resumes; and a daily rescan while serve runs.
- **The found list:** "Here's what you work on", with a suggested Team, Watch or Ignore for each project, the owner's choice kept, and "Already a team".
- **Corrections:** "These are different" for two folders folded as one project, and Locate for one that moved.
- **Other sources:** claude.ai's export dropped onto the page; the services the owner says they use with Claude; projects and routines added by hand.
- **Settings:** the offer to keep Claude Code's history longer; Pause finding; the ignored list; Forget everything.

`catervas discover` keeps working, and calls the same scan.

Out of scope: every screen (06c); GitHub (06d); links, proofs, names and creating teams (06b); reminders (07).

## Decisions

- **The hub reaches the scan through a host trait.** The scan lives in the command-line crate. This is the pattern `SetupHost` uses (daemon/setup.rs:35): the trait `FindingHost` is in `crates/runtime/src/daemon/finding.rs`, and `CliFinder` implements it in `crates/cli/src/finding.rs`. `CliFinder` reuses `discover.rs`'s steps, `scan_history`, `find_jobs`, `read_service_settings` and `find_copies`, which move into a `pub(crate) fn scan(&ScanPlaces, &dyn Fn(ScanProgress)) -> Result<ScanTotals, String>`. `discover()` then calls `scan`, so the printout is unchanged. Rejected: moving the scan into runtime, a larger move with no gain.
- **One scan at a time**, on a blocking thread. Its state is kept in the hub:

  `FindingState { phase: Idle | Running | Stopped | Done, projects, jobs, sessions, reading: Option<String> /* "sessions from August" */, unreadable: u64, error: Option<String>, started_at, ended_at }`

  - `finding.start` while running is a no-op answer.
  - `finding.stop` sets a flag the reader checks between transcript files. A stopped scan keeps what it read, since the index is incremental.
  - The counts are published at most every 250 ms, and each publication bumps a hub event so the page refreshes.
  - "About a minute left" is `remaining = (files_total − files_read) × mean seconds per file so far`, shown only after 20 files. `ScanProgress` carries `files_total` and `files_read`; the reader already lists files before reading.
- **The daily rescan.** While serve runs, finding is not paused, and the index exists (a first scan was made with consent), serve rescans at start and then every 24 hours. Pause finding stops it. Rejected: a scan with no consent, since the first one needs the owner's "Look for my projects".
- **Whether there is history** is `finding.offer`, with three possible answers: `{ history: "claude_code" | "none", sessions_dir_exists }`. `claude_code` means `<claude_dir>/projects/` holds at least one `.jsonl` file. It is counted without reading any of them, so the first run's check can offer the scan.
- **The suggestion is pure, in core** (`catervas_core::finding::suggest`):

  | A project that is… | is suggested |
  |---|---|
  | the owner's own (`yours`), last worked in the last 30 days | Team |
  | the owner's own, older | Watch |
  | explored (`explored`) | Ignore |
  | formed from connector work, last used in the last 30 days | Team |
  | formed from connector work, older | Watch |
  | a claude.ai Project | Watch |
  | a job on this computer with no project | Watch |
  | gone (folder missing) | none: not offered, only Locate |

- **Choices are kept by the project's key** (step 04's: the normalised remote, else the real path, the connector project's id, or the claude.ai uuid). Migration 7 of `machine.db` adds:
  - `choices(key TEXT PRIMARY KEY, choice TEXT NOT NULL CHECK (choice IN ('team','watch','ignore')), at TEXT NOT NULL) STRICT`;
  - `separations(a TEXT NOT NULL, b TEXT NOT NULL, PRIMARY KEY (a, b)) STRICT`, holding two real paths, `a < b`, that the owner said are different;
  - `declared_services(service_id TEXT PRIMARY KEY, name TEXT NOT NULL, at TEXT NOT NULL) STRICT`;
  - `hand_routines(id INTEGER PRIMARY KEY, name TEXT NOT NULL, project_key TEXT, schedule TEXT NOT NULL, at TEXT NOT NULL) STRICT`.

  A project with no choice row shows its suggestion; a row overrides it. The choice is never acted on in this step: 06b turns "Team" into a team.
- **"These are different"** records a separation. The fold by remote (step 04's) then keeps the two folders as two projects. `catervas_core::claude_history::fold_by_remote` gains a `separated: &[(String, String)]` argument, and the store passes `separations`.
- **Locate** (`finding.locate { key, path }`) applies only to a gone folder. It checks the new path the way `repo.locate` does (inside home, a git repository's top, and the same remote when the old one had one: `not_the_same_repository`), then records `moved_from` so the sessions' old path folds into the new one. Migration 7 also adds `moved(old TEXT PRIMARY KEY, new TEXT NOT NULL) STRICT`, applied in placement.
- **The found list** is `finding.list`. It answers the rows of `machine.found()`, `found_services()` and `found_jobs()`, in sections:
  - `main`: newest first;
  - `explored`;
  - `gone`;
  - `home`: the home folder's session count and where its connector sessions went;
  - `computer_jobs`.

  Each row is `{ key, name, kind: "folder"|"connector"|"claude_ai"|"hand", folders: [string], last_at, jobs: [{ name, schedule }], services: [string], suggested, choice, already_team: slug|null, gone: bool }`. `already_team` is the hub's team whose root is one of the row's folders.
- **The export drop** is `POST /hub/export`, with the zip as the request body. It is behind the same session cookie and `Origin` check as `/hub/rpc`.
  - The body is capped at `ExportLimits`' size; past it the answer is 413 `export_too_large`.
  - It is written to a temporary file in the state folder (mode 0600), read by `import_claude_export`, then deleted, whatever the result.
  - The answer is the `ExportSummary` in JSON, or `{ error: "not_an_export" | "unsupported" | … }` with step 04's error words.
  - A dismissed export card is `finding.export_dismissed {}`, kept in `state.json` as `export_card: "dismissed"`. A drop clears it.
- **Declared services** (`finding.declare_services { ids: [catalog id], other?: string }`) replace the declared set. A declared bookkeeping service with no connector project yet forms one, "Bookkeeping (<service>)", sourced `declared`. `other` is kept as a name with no catalog id and forms nothing.
- **By hand.**
  - `finding.add_project { name, folder?, repository? }` is step 04's `add_project_by_hand`.
  - `finding.add_routine { name, project_key?, schedule }` adds a `hand_routines` row. `schedule` is free text of at most 100 characters, such as "every Monday at 9".
  - Routines show under their project's jobs, or under `computer_jobs` with no project.
- **Keep the history longer.** `finding.retention` answers `{ cleanup_period_days: n, default: true|false }`, read from `<claude_dir>/settings.json`'s `cleanupPeriodDays` (30 when absent).
  - `finding.keep_longer { days }` writes that key into the file, keeping every other key and the file's mode. It only runs on the owner's press, and `days` must be one of 90, 365 or 3650.
  - This is the only write Catervas ever makes into Claude Code's settings, and the spec says so.
- **Pause finding** (`finding.pause { paused }`) is kept in `state.json` as `finding_paused`. While paused, no scan starts, the daily rescan stops, and `finding.start` is refused `finding_paused`.
- **The ignored list** is `finding.list`'s rows whose choice is `ignore`, also offered alone as `finding.ignored`. Choosing Team or Watch brings one back.
- **Forget everything** (`finding.forget { confirmed: true }`) deletes `machine.db` (`forget_machine_index`) and clears `state.json`'s `finding_paused` and `export_card`. It answers `{ forgotten: true }`. A scan running is stopped first.
- **Nothing here records an event in a team's log.** All of it is the computer's.

## File map

```
crates/core/src/finding.rs                       creates: Choice, suggest, Section, section_of
crates/core/src/claude_history.rs                modifies: fold_by_remote's separations
crates/store/src/migrations/machine/0007_finding.sql  creates
crates/store/src/machine.rs                      modifies: choices, separations, moved, declared services, routines; found() honours them
crates/runtime/src/daemon/finding.rs             creates: FindingHost, FindingState, the finding.* methods
crates/runtime/src/daemon/hub.rs                 modifies: Hub.finding; POST /hub/export
crates/cli/src/finding.rs                        creates: CliFinder
crates/cli/src/discover.rs                       modifies: scan() shared with CliFinder
crates/cli/src/state.rs                          modifies: finding_paused, export_card
crates/cli/src/serve.rs                          modifies: the hub's finder; the daily rescan
docs/schemas/rpc.schema.json, crates/protocol, packages/protocol-client   modifies: the finding.* methods
docs/SPEC.md                                     modifies: revision 0.146
```

## Tasks

### Task 1: The suggestion and the sections

Tests (`crates/core/src/finding.rs`):
- `suggests_a_choice_for_each_project`: asserts each row of the suggestion table at a fixed `now`, with a project 29 days old and one 31 days old on each side of the 30 days.

- [x] `feat(core): suggest a team, watch or ignore for each found project`

### Task 2: Choices, separations, moves, declared services and routines in the index

Files: migration 7; `machine.rs`; `claude_history.rs` (`fold_by_remote`).

Tests (`machine.rs`):
- `keeps_the_owner_s_answers`: asserts these.
  - A choice survives a reopen of the index.
  - A separation of two folders with one remote makes `found()` list two projects.
  - A move folds the old path's sessions into the new folder's project.
  - Declaring Zoho Books forms "Bookkeeping (Zoho Books)" sourced `declared`.
  - A routine by hand shows under its project's jobs.
  - The migrations list ends at 7.

- [x] `feat(store): keep the owner's choices and corrections in the machine index`

### Task 3: The scan, shared and reporting its progress

Files: `discover.rs` (extract `scan`), `crates/cli/src/finding.rs` (`CliFinder`).

Tests:
- `scans_with_progress_and_stops_between_files` (`crates/cli/src/finding.rs`): asserts these.
  - Over a fixture history of 5 transcript files, the progress callback sees `files_read` rise from 0 to 5, and the totals match `catervas discover`'s.
  - With stop set after file 2, the scan ends `Stopped`, and the index holds the first two files' sessions.
- The existing `crates/cli/tests/discover.rs` tests keep passing.

- [ ] `refactor(cli): one scan for discover and the hub, with progress`

### Task 4: The hub's finding methods

Files: `crates/runtime/src/daemon/finding.rs`, `hub.rs`, `crates/cli/src/serve.rs`, `state.rs`, the schema, the protocol and the client.

The methods are `finding.offer`, `finding.start`, `finding.stop`, `finding.status`, `finding.list`, `finding.ignored`, `finding.choose { key, choice }`, `finding.separate { key, folder }`, `finding.locate { key, path }`, `finding.declare_services`, `finding.add_project`, `finding.add_routine`, `finding.retention`, `finding.keep_longer`, `finding.pause`, `finding.export_dismissed` and `finding.forget`. Their refusals, each one sentence:

| Refusal | Sentence |
|---|---|
| `finding_paused` | finding is paused: turn it on in This computer |
| `unknown_project` | Catervas doesn't know that project |
| `not_gone` | that project's folder is still there |
| `not_the_same_repository` | that folder holds another repository |
| `retention_not_offered` | Catervas keeps Claude Code's history for 90 days, a year or ten years |
| `forget_needs_confirming` | say yes to forget everything Catervas found |

Tests:
- `finds_projects_from_the_hub` (`crates/runtime/src/daemon/finding.rs`, with a fake `FindingHost`): asserts these.
  - `finding.start` then `finding.status` reports `Running` and then `Done` with the fake's counts.
  - `finding.list` rows carry `suggested`, and `choice` after `finding.choose`.
  - `already_team` names the hub's team for a root it drives.
  - `finding.pause { paused: true }` makes `finding.start` refused `finding_paused`.
  - `finding.forget` without `confirmed` is refused.
- `finds_projects_through_serve` (`crates/cli/tests/serving.rs`, ignored): asserts these.
  - With a fixture `~/.claude` under the test's home, `/hub/rpc` `finding.offer` is `claude_code`.
  - `finding.start` runs to `Done` and `finding.list` holds the fixture's project with suggestion `team`.
  - `finding.keep_longer { days: 365 }` writes `"cleanupPeriodDays": 365` into the fixture's `settings.json`, and its other keys are unchanged.

- [ ] `feat(runtime): find projects from the hub`

### Task 5: The export dropped onto the page, and the daily rescan

Files: `hub.rs` (`POST /hub/export`), `serve.rs` (the rescan timer).

Tests:
- `imports_an_export_posted_to_the_hub` (`crates/cli/tests/serving.rs`, ignored): asserts these.
  - Posting step 04's fixture zip with the session's cookie answers its summary, and `finding.list` then holds its claude.ai Project. No temporary file is left in the state folder.
  - Posting it with no cookie is 401.
  - Posting `not a zip` answers `not_an_export`.
- `rescans_each_day_while_finding_is_on` (`crates/runtime/src/daemon/finding.rs`, with tokio's paused time and the fake host): asserts these.
  - With an index present, a scan starts at once and again after 24 hours.
  - With finding paused, none starts.
  - With no index, none starts until `finding.start`.

- [ ] `feat(runtime): take claude.ai's export on the page, and look again each day`

### Task 6: Spec

`docs/SPEC.md` revision 0.146 (2026-10-22), "records phase 10 step 06, finding projects behind the hub". Insert it above the newest revision. It covers:
- 3 and 8.4: migration 7's tables;
- 4.1 and 4.4: the finding methods and the export drop;
- 6.7: the services declared;
- 8.6: the single write into Claude Code's settings, on the owner's press, and the temporary export file.

Then the project plan: row 06 becomes three rows, 06, 06c and 06d, as the start of this plan says. Row 06 records "planned and executed 2026-10-22 (`step-06-finding-projects.md`)".

- [ ] `docs(spec): finding projects behind the hub, revision 0.146`

## Verification

```
python3 reaper.py cargo xtask check
python3 reaper.py cargo test -p catervas --test serving -- --include-ignored finds_projects imports_an_export
# expected: xtask check: ok; both pass
```
