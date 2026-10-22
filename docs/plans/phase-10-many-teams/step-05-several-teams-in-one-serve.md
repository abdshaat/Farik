# Phase 10, step 05: Several teams in one serve

Status: ready
Branch: `phase/10-many-teams` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 1, 4.1, 5.5, 8.1, 8.4; ADR 0059, ADR 0053
Depends on: steps 02 to 04 of this phase (committed); phase 9 (merged in #45); the standalone `claude-code-signs-in` (pull request #92), merged into this branch before Task 1. With it, sessions hold no key of Catervas's, so several drives share Claude Code's own sign-in and no credential is passed around.
Mockups: none for this step (no screen; 05b draws the switcher and All teams from `ManyAllTeams.dc.html`).
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008). Line numbers are at `a50731a9` and the code wins when they have moved.

## Goal

One `catervas serve` drives every team on this computer at once:

- `state.json` lists every team's project.
- Each team gets its own drive: its `run.lock`, `daemon.json`, orchestrator, idle tick and log, as today.
- The browser reaches every team through the one port serve holds: `/t/<team>/rpc` for a team, and `/hub/rpc` for what belongs to the computer.
- Adding a team through the wizard starts its drive beside the others.
- "Change project" (ADR 0053) now stops and removes one team.
- Three limits are shared by every team, each off by default so that one team works exactly as before:
  - "Agents working at once" caps agent sessions across teams, giving a freed place to the next team in turn;
  - when the AI account's limit is hit, every team waits for its reset;
  - a spending cap for the computer stops every team for the day once all teams together reach it.

Out of scope: the web app's switcher, All teams and `/t/` pages (05b), finding projects (06), linking (06b), reminders (07), analytics (08). Until 05b the web app keeps working as today, because `/rpc` still answers for one team: the current one.

## Decisions

### The hub and the teams' daemons

- **The held port serves the hub.** It is the one serve binds once (`hold`, serve.rs:64), and from this step it is never a team's own daemon. The hub is the setup-mode `DaemonState` that serve already builds (`DaemonState::setup`, daemon.rs:366, with `CliHost`). It gains a table of the driven teams:

  `Hub { teams: RwLock<BTreeMap<TeamSlug, Arc<DaemonState>>>, current: RwLock<Option<TeamSlug>> }`

  It lives in `crates/runtime/src/daemon/hub.rs` and is held as `DaemonState.hub: Option<Arc<Hub>>`, set only on the hub's own state.
- **Each team's daemon listens on a port the system picks**, as `catervas run`'s does (`StartOptions { listener: None, web: true }`), and writes its `daemon.json`. Its hooks, `/mcp`, `/command`, `/connector/*` and `catervas open`'s `/connect/code` are unchanged, since every one of them already finds the daemon through `daemon.json`. Rejected:
  - one daemon serving every team's hooks and MCP, which would need a team on every session lookup;
  - one held listener shared by several daemons, which would race (`serve_held`).
- **The browser's routes on the hub** (`router_serving` gains them only when `state.hub` is set; a team's daemon keeps its own unchanged):
  - `/t/{team}/rpc` and `/t/{team}/documents/raw` call `web::rpc` and `documents::raw` with that team's `Arc<DaemonState>`. An unknown team is 404.
  - `/hub/rpc` is `web::rpc` with the hub's own state: the setup methods and this step's `teams.*` and `computer.*` methods.
  - `/rpc` and `/documents/raw` are the current team's while there is one, else the hub's. Today's web app is therefore unchanged until 05b.
  - `/`, `/connect`, `/session`, `/disconnect` and the app's fallback stay the hub's.
- **One browser session for every team.** The hub's `BrowserSessions` and `ConnectCodes` are shared, as `Arc`, into every team's `WebState`, which today holds its own (`web.rs:41-75`; `BrowserSessions` becomes `Arc<BrowserSessions>` there). Each team's `WebState.port` is set to the hub's port, so `own_origin` and `own_host` accept the page when the hub hands it over. Rejected: a session per team, which would make the owner connect once per team.
- **The team's slug** is `ProjectIds::of(root).team_id` (project.rs:51), the slug of the team's name. When two teams share one, the later in `state.json`'s order gets `-2`, then `-3`, and so on. Slugs are recomputed at each start, and a team's address may change when another team is removed. Rejected: the `local_project_id`, 32 hex digits, unreadable in an address.

### Driving several projects

- **`state.json`** (state.rs) becomes:

  `{ last_project?, projects: [<root>...], agents_at_once?: 1..=16, daily_cap_usd?: number > 0 }`

  Read tolerantly: a file with only `last_project` reads as `projects: [last_project]`. `remember` adds a root to `projects` (no duplicates; canonical paths) and sets `last_project`; `forget(root)` removes one. Writes go through `write_private` as now.
- **Every team in `projects` is driven, paused or not.** A paused team still answers its pages and its Start, and its orchestrator idles as it does today. This departs from the design's "each project that is not paused", which would leave a paused team's pages unreachable.
  - A root whose `run.lock` is held elsewhere (`catervas start` in a terminal) is not driven. `teams.list` shows it as `elsewhere`, and serve tries it again every 60 seconds.
  - A root that is gone, or has no `.catervas/team.yaml`, stays in `projects` and shows as `missing`. Only a Remove takes it out.
- **The serve loop** (serve.rs:74-116) becomes a select over:
  1. a team chosen in the wizard (`chosen`, setup.rs:231), which starts its drive and remembers it;
  2. a drive ending;
  3. Ctrl-C, sent to every drive;
  4. the 60-second retry of `elsewhere` teams.

  Each drive runs `drive()`'s body (serve.rs:239-308) on its own tokio task, with its own Ctrl-C receiver fed by the loop. A drive needs from `CliIo` only cloneable parts, which are split into `DriveIo { env, clock, sleeper, session_ids, connector_secrets, out: SharedOut }`. `SharedOut` is `Arc<Mutex<Box<dyn Write + Send>>>`, so the drives' lines are never interleaved within a line.
- **A drive ending.** `Left(root)`, from `project.leave`, takes the team out of `projects` and the hub. When it was the last team, the hub's `leaving` is set, exactly as ADR 0053's switch does today. `Ended(code)` with an error marks the team `failed` with its message, shown in `teams.list` and kept until a Start or a restart. Other teams keep running either way.
- **Starting a team that is already a team** from the wizard is refused `team_already_here` ("that project is already one of your teams").
- **Ctrl-C:** the first press stops every orchestrator, the second the sessions of every drive. Serve ends with the worst code of its drives: 130 after Ctrl-C, else 1 when any failed, else 0.

### Shared limits

- **"Agents working at once"** is `catervas_runtime::slots::Slots`, one per serve, given to every drive in `OrchestratorDeps.slots: Option<Arc<Slots>>`. `catervas run` and `catervas start` pass `None` and keep today's behaviour.
  - `session::drive` (session.rs:1574) awaits `slots.take(team).await` before `adapter.start_session` and holds the guard until the session ends. A conversation (chat) session takes a place too.
  - `Slots` keeps a FIFO queue per team. A freed place goes to the next team after the last one served, in the order teams first queued, so that no team waits behind another's whole queue.
  - With no limit set, `take` returns at once.
  - Changing the limit (`computer.set`) applies to the next `take`; running sessions are never stopped.
  - While waiting, the task's session counts as not started: nothing is recorded until the place is given.
- **The AI account's sleep is shared.** `Slots` also holds `account_asleep_until: Mutex<Option<DateTime<Utc>>>`.
  - A session ending `ProviderLimit` (session.rs:346) sets it to the same `until` as the agent's sleep, keeping the later of the two times. Every drive's `take` waits until that time before giving a place.
  - Only the agent of the session that hit the limit records `agent.slept`, as today. The other teams' agents are not written to; they simply wait, and their pages show "Waiting for the AI account's limit to reset at <time>" through `teams.list`'s `account_asleep_until`.
  - It is in memory alone: a restart forgets it, and the next session that hits the limit sets it again.
- **The computer's daily cap.** `state.json`'s `daily_cap_usd` is read into `Slots.daily_cap_usd`, and the spend is the sum, over every driven team, of `cost::day_spent(log, today)` (cost.rs:195).
  - `rules::day_is_spent` (rules.rs:934) gains an `elsewhere: f64` argument, the other teams' spend today from `Slots`. The day is spent when the team's own limit is reached, or when `own + elsewhere >= daily_cap_usd`.
  - The idle reason then is `COMPUTER_DAY_SPENT`: "all your teams together reached this computer's daily limit; they go on tomorrow (UTC)".
  - `Slots` keeps each team's `day_spent` from the `session.ended` it sees (`record_spend(team, usd)`), reset at the UTC day. It never reads another team's log, and starts each drive from that drive's own `cost::day_spent` at start.
  - Crossing mid-session stops the other sessions of the team that crossed (as `stop_when_the_day_is_crossed` does today), and no new session starts anywhere until the next UTC day. Running sessions of other teams finish.

### The hub's methods (`docs/schemas/rpc.schema.json`)

| Method | Answer |
|---|---|
| `teams.list` (query) | `{ teams: [{ slug, name, root, status: "running"\|"paused"\|"elsewhere"\|"missing"\|"failed", failed?: string, working_agents: n, waiting: n, today_usd, month_usd }], current: slug\|null, account_asleep_until: string\|null }` |
| `teams.current { slug }` | sets which team `/rpc` answers: `{}`; an unknown slug is `team_unknown` |
| `teams.remove { slug }` | stops that team's drive and forgets its root: `{ removed: true }`. It deletes no file, and needs `confirmed: true` |
| `computer.get` (query) | `{ agents_at_once: n\|null, daily_cap_usd: number\|null, spent_today_usd }` |
| `computer.set { agents_at_once?, daily_cap_usd? }` | writes `state.json` and applies at once; `null` clears a field |

- `working_agents` counts the team's running sessions (its `DaemonState.sessions`); `waiting` is the length of its `waiting.list`; `today_usd` is `cost::day_spent`; `month_usd` is a new `cost::month_spent(log, now)`, the same sum from the first of the UTC month.
- `teams.list` is answered by the hub alone and is also offered on `/rpc` while there is no current team.
- Out of range values are refused: `agents_at_once` outside 1..=16 is `agents_at_once_out_of_range`; `daily_cap_usd` at most 0 or over 100000 is `daily_cap_out_of_range`.

## File map

```
crates/cli/src/state.rs                      modifies: ServeState, remember, forget, read tolerantly
crates/cli/src/serve.rs                      modifies: the hub on the held port; drives on tasks; the select loop
crates/cli/src/start.rs                      modifies: DriveIo; StartOptions for a team behind the hub; Slots into OrchestratorDeps
crates/cli/src/setup.rs                      modifies: choose adds a team; team_already_here
crates/runtime/src/daemon/hub.rs             creates: Hub, TeamSlug, slugs_for, the hub's routes and methods
crates/runtime/src/daemon.rs                 modifies: DaemonState.hub; router_serving's hub routes
crates/runtime/src/daemon/web.rs             modifies: shared BrowserSessions and ConnectCodes; /rpc to the current team
crates/runtime/src/slots.rs                  creates: Slots (places, round robin, the account's sleep, the computer's day)
crates/runtime/src/orchestrator.rs           modifies: OrchestratorDeps.slots
crates/runtime/src/orchestrator/session.rs   modifies: take a place; set the shared sleep; record spend
crates/runtime/src/orchestrator/rules.rs     modifies: day_is_spent's elsewhere, COMPUTER_DAY_SPENT
crates/runtime/src/cost.rs                   modifies: month_spent
docs/schemas/rpc.schema.json                 modifies: teams.list, teams.current, teams.remove, computer.get, computer.set
packages/protocol-client/src/{client.ts,generated/rpc.ts}  modifies, regenerated
crates/cli/tests/serving.rs                  tests: two teams in one serve
docs/SPEC.md                                 modifies: 1, 4.1, 5.5, 8.1, 8.4; revision 0.144
docs/decisions/0053-*.md                     modifies: a line, amended by ADR 0059: serve drives several projects
```

## Interfaces

Consumes: `DaemonState::setup` (daemon.rs:366), `serve_held` (daemon.rs:1038), `router_serving` (daemon.rs:1168), `web::rpc` (web.rs:445), `documents::raw`, `ProjectIds::of` (project.rs:51), `cost::day_spent` (cost.rs:195), `session::drive` (session.rs:1574), `start` (start.rs:436).

Produces:

```rust
// catervas_runtime::slots
pub struct Slots { /* limit, queues, rotation, account_asleep_until, daily_cap_usd, spent */ }
pub struct Place { /* returns the place on drop */ }
impl Slots {
    pub fn new(limit: Option<usize>, daily_cap_usd: Option<f64>) -> Arc<Slots>;
    pub async fn take(&self, team: &str) -> Place;
    pub fn set_limit(&self, limit: Option<usize>);
    pub fn set_daily_cap(&self, cap: Option<f64>);
    pub fn account_asleep_until(&self, until: DateTime<Utc>);   // keeps the later
    pub fn asleep_until(&self) -> Option<DateTime<Utc>>;
    pub fn record_spend(&self, team: &str, usd: f64, at: DateTime<Utc>);
    pub fn spent_elsewhere(&self, team: &str, today: NaiveDate) -> f64;
    pub fn day_capped(&self, today: NaiveDate) -> bool;
}
// catervas_runtime::daemon::hub
pub struct TeamSlug(String);
pub fn slugs_for(team_ids: &[String]) -> Vec<TeamSlug>;      // -2, -3 for repeats, in order
pub struct Hub { /* teams, current */ }
impl Hub {
    pub fn add(&self, slug: TeamSlug, state: Arc<DaemonState>);
    pub fn remove(&self, slug: &TeamSlug) -> Option<Arc<DaemonState>>;
    pub fn team(&self, slug: &str) -> Option<Arc<DaemonState>>;
    pub fn current(&self) -> Option<Arc<DaemonState>>;
}
// catervas_runtime::cost
pub fn month_spent(log: &EventLog, now: DateTime<Utc>) -> Result<f64, StoreError>;
// catervas (cli) state.rs
pub(crate) struct ServeState { pub last_project: Option<PathBuf>, pub projects: Vec<PathBuf>, pub agents_at_once: Option<usize>, pub daily_cap_usd: Option<f64> }
pub(crate) fn read_state(state_dir: &Path) -> ServeState;
pub(crate) fn remember(state_dir: &Path, root: &Path) -> io::Result<()>;   // adds to projects
pub(crate) fn forget(state_dir: &Path, root: &Path) -> io::Result<()>;
```

## Tasks

### Task 1: Merge the sign-in through Claude Code

`git merge origin/docs/claude-code-signs-in` (pull request #92) into this branch with a merge commit, resolving any conflict in `docs/SPEC.md`'s revision list by keeping every revision in number order. Then run `cargo xtask check`; it must pass before Task 2.

- [x] `merge: claude code keeps the ai account (#92) into phase/10-many-teams`

### Task 2: Places for agents across teams, and the shared sleep and day

Files: created `crates/runtime/src/slots.rs`; modified `crates/runtime/src/lib.rs`.

Tests (in `slots.rs`):

- `gives_places_in_turn_across_teams`: asserts this sequence, with the limit at 1.
  - Team a takes a place.
  - Then a, a and b queue, in that order.
  - Dropping a's place gives the next to b, not to a's second, and then to a.
  - With no limit, ten `take`s from one team all return at once.
- `waits_for_the_account_and_the_computer_s_day`: asserts these.
  - After `account_asleep_until(t + 1h)`, a `take` at t is not given until the clock passes t + 1h. Use tokio's paused time.
  - Setting a later time keeps the later; an earlier one is ignored.
  - With `daily_cap_usd` 10, `record_spend("a", 6.0)` and `record_spend("b", 4.0)` the same day, `day_capped` is true and `spent_elsewhere("a")` is 4.0.
  - The next UTC day is not capped.

- [x] `feat(runtime): share places, the account's sleep and the computer's day across teams`

### Task 3: `state.json` lists every team

Files: modified `crates/cli/src/state.rs`, and serve.rs's use of `remember` and `last_project`.

Tests (in `state.rs`):

- `remembers_every_team_and_reads_an_old_state`: asserts these.
  - A state of `{"last_project":"/x"}` reads as `projects: ["/x"]`.
  - `remember` of `/y` then `/x` gives `projects ["/x","/y"]` with no duplicate, and `last_project` `/x`.
  - `forget("/x")` leaves `["/y"]`.
  - `agents_at_once` and `daily_cap_usd` round-trip.
  - An unreadable file reads as empty.

The existing serve tests `remembers_the_project_it_serves` and `reopens_the_last_project` must still pass.

- [x] `feat(cli): remember every team's project in state.json`

### Task 4: The hub's routes

Files: created `crates/runtime/src/daemon/hub.rs`; modified `daemon.rs`, `web.rs`.

Tests (in `hub.rs`, with two test `DaemonState`s as `web.rs`'s tests build them):

- `routes_a_page_to_its_team`: asserts these.
  - With teams `bakery` and `shop` added and `current` set to `shop`, a WebSocket to `/t/bakery/rpc` asking `serve.status` gets bakery's `project_root`.
  - `/rpc` gets shop's, and `/t/nope/rpc` is 404.
  - A cookie made by the hub's `/connect` is accepted on both teams' sockets.
- `names_teams_once`: asserts that `slugs_for(["bakery","shop","bakery"])` is `bakery, shop, bakery-2`.

- [ ] `feat(runtime): a hub routes each page to its team`

### Task 5: Serve drives every team

Files: modified `crates/cli/src/serve.rs`, `start.rs`, `setup.rs`.

Tests (`crates/cli/tests/serving.rs`, ignored like their neighbours):

- `drives_two_teams_at_once`: asserts these.
  - With two projects in `state.json`, each with a team, both `run.lock`s are held and both `daemon.json`s exist.
  - `/t/<each>/rpc` `serve.status` answers each root.
  - `catervas stop` in one project stops that team alone: its lock frees, and the other's `/t/` still answers.
- `adds_a_team_from_the_wizard_beside_the_running_one`: asserts that with one team driven, `/hub/rpc` `project.open` of a second project starts its drive (its lock held), and `state.json` lists both.
  - A second `project.open` of the same root is refused `team_already_here`.
- `leaves_one_team_and_keeps_the_other`: asserts that `project.leave` on `/t/<a>/rpc` frees a's lock and removes a from `state.json`, while b's `/t/` still answers.
- `skips_a_team_driven_elsewhere`: asserts that with one project's lock held by the test (`hold_the_run_lock`), serve drives the other. Its `teams.list` shows the held one `elsewhere`, and once the lock is freed the team is driven within 70 seconds; drive the test's clock or use a shorter test retry if `start.rs` exposes one.

The existing tests `takes_on_the_chosen_project_on_the_same_port`, `leaves_the_project_for_the_wizard_on_the_same_port`, `stays_on_the_project_it_left` and `changes_project_from_the_browser` keep passing: one team behaves as today.

- [ ] `feat(cli): serve drives every team on this computer`

### Task 6: The shared limits in sessions

Files: modified `crates/runtime/src/orchestrator.rs`, `orchestrator/session.rs`, `orchestrator/rules.rs`, `cost.rs`, `crates/cli/src/start.rs`.

Tests (orchestrator tests, with the recorded adapter and two orchestrators sharing one `Slots`):

- `waits_for_a_place_held_by_another_team`: asserts that with limit 1 and team a's session running, team b's ready task gets no `session.started` until a's ends, and then does.
- `every_team_waits_for_the_account_s_reset`: asserts these.
  - Team a's session ends `ProviderLimit` with `resets_at` = t + 30m.
  - Team b's next session starts only after t + 30m.
  - b's log has no `agent.slept`.
- `stops_every_team_at_the_computer_s_cap`: asserts that with `daily_cap_usd` 1.0, a session of a costing 0.6 and one of b costing 0.5 leave both teams idle with `COMPUTER_DAY_SPENT`. The next UTC day starts them again.

- [ ] `feat(runtime): sessions share places, the account's reset and the computer's cap`

### Task 7: The hub's methods

Files: modified `docs/schemas/rpc.schema.json`, `crates/protocol/src/rpc.rs`, `crates/runtime/src/daemon/hub.rs`, `crates/cli/src/serve.rs` (writing `state.json` on `computer.set`), `packages/protocol-client`.

Tests:

- `lists_every_team_with_its_state` (hub.rs): asserts these.
  - `teams.list` answers both teams with `status`, `working_agents`, `waiting`, `today_usd` and `month_usd`. Their logs are seeded with `session.ended` costs, one of them last month.
  - `teams.current { slug: "shop" }` changes what `/rpc` answers.
  - `teams.remove` without `confirmed` is refused.
- `sets_the_computer_s_limits` (serving.rs): asserts these.
  - `computer.set { agents_at_once: 2, daily_cap_usd: 25 }` is in `state.json` and in `computer.get`.
  - `null` clears each.
  - 17 is `agents_at_once_out_of_range`, and 0 is `daily_cap_out_of_range`.
- The rpc schema test holds the five methods to the schema.

- [ ] `feat(runtime): list teams and set the computer's limits from the hub`

### Task 8: Spec and ADR

- `docs/SPEC.md` revision 0.144 (2026-10-22), "records phase 10 step 05, several teams in one serve (ADR 0059)". Insert it above 0.143. It changes these sections:
  - 1: several teams on one computer.
  - 4.1: the first run adds a team, and Change project removes one.
  - 5.5: places, the shared account sleep, and the computer's cap.
  - 8.1 and 8.4: the hub and its routes, a daemon per team, `state.json`.
  - F9: the five methods.
- ADR 0053 gains under its Decision: "Amended 2026-10-22 by ADR 0059 (phase 10 step 05): serve drives every team's project at once behind one port; Change project stops and removes one team."
- The project plan's row 05 gains "planned and executed 2026-10-22 (`step-05-several-teams-in-one-serve.md`)".

- [ ] `docs(spec): several teams in one serve, revision 0.144`

## Verification

```
python3 reaper.py cargo xtask check
python3 reaper.py cargo test -p catervas --test serving -- --include-ignored drives_two_teams adds_a_team leaves_one_team skips_a_team
# expected: xtask check: ok; the four serving tests pass
```
