# Phase 10, step 06e: The proposal, and creating teams

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 3, 4.1, 4.4, 6.7, 8.4, 8.6, F1; ADR 0059, ADR 0055, ADR 0057
Depends on: steps 05, 06, 06b, 06c and 06d (committed)
Mockups: `ManyYourTeams` frames 1 and 2; `ManyAllTeams` frame 1 (after Create); `ManyFixMistake` frame 1 (This doesn't belong here); approved with step 01
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

## Goal

After "Do these work together?", the owner sees **Your teams**, one card per team to be made:

- a group of linked projects becomes one multi-repo team (a phase 9 project folder);
- a project marked Team alone becomes one team;
- a connector-work project becomes a team in a new folder.

Each card shows its projects, its jobs, why its projects are together, a name, and a team template. The owner can rename, change the template, move a project out, and optionally ask Claude for better names, with the cost shown first. **Create N teams** makes them all paused.

All teams then shows each new team with **Start**, **Document it first** and **Undo**. Undo stays until the team first starts.

A link with a team that already exists adds the project to it as a repository. In a team, **This doesn't belong here** takes a repository out, records the link as rejected, and watches it or makes it its own paused team. A team made from a project that uses services through Claude suggests connecting them where its kit can.

## Decisions

- **The proposal** is `proposal.get` (hub). It answers a card for:
  - each `linking.groups` group with at least one Team-chosen project and no driven team;
  - each Team-chosen project in no group;
  - each group or project linked to a driven team, which becomes an "Add to <team>" card instead of a new team.

  Cards are ordered by the newest last-worked project. Each is `{ id, kind: "new_team"|"add_to_team", projects: [{ key, name, folder?, services }], jobs: [name], why: [sentence], name, purpose?, template, folder? }`, where:
  - `id` is a hash of the sorted project keys, stable across calls;
  - `name` is the first project's folder name, or "Bookkeeping" for a bookkeeping connector project;
  - `template` is suggested as: a connector project with a bookkeeping service gets `financial-analysis`; a project with a `package.json` or `Cargo.toml` gets `software-product`; else `software-product`;
  - `folder` is shown for a connector project: `~/Catervas/<slug>`.
- **The owner's edits** are kept in `machine.db` (migration 10: `proposal_edits(card_id TEXT PRIMARY KEY, name TEXT, template TEXT, purpose TEXT) STRICT` and `moved_out(card_id TEXT NOT NULL, project_key TEXT NOT NULL, PRIMARY KEY (card_id, project_key)) STRICT`) through `proposal.edit { id, name?, template?, purpose? }` and `proposal.move_out { id, key }`.
  - Moving a project out records `links` No between it and each other project of the card. It becomes its own card if it is Team-chosen.
  - A template outside the built-in list (ADR 0055's three, and the owner's saved templates from `templates.list`) is `template_unknown`.
- **Names from Claude.**
  - `proposal.names_estimate` answers `{ usd, input_tokens }`, from the prompt's length (characters / 4) and the price table's Haiku input and output rates for at most 400 output tokens.
  - `proposal.names { confirmed: true }` runs the owner's `claude` once, through `ClaudeAccount` (`crates/runtime/src/account.rs`, from pull request #92, merged by step 05), with ADR 0084's lean flags but its own prompt and `--model haiku`.
  - For each card the prompt gives only the folder names, the first 40 lines of each README at `HEAD`, the titles of its last 20 sessions and its jobs' names. All of it goes first through `catervas_core::scrub::scrub`, which removes `sk-…`, `ghp_…`, `github_pat_…`, `AKIA…`, `xox…-` keys, any `KEY=value` line whose name names a key (spec 6.7's rule for `arg_holds_secret`), and userinfo and query strings in URLs.
  - Claude is asked for JSON: `[{ "id", "name", "purpose", "template" }]`. A name is at most 40 characters and a purpose at most 120; a template not in the list is ignored.
  - The result is stored as edits marked `from_claude`, and Keep the folder name reverts one.
  - The cost reported by `total_cost_usd` is shown as "Names from Claude · cost $x".
  - A failure is `names_failed` with Claude Code's words, and the cards keep their names.
  - Claude never makes or changes a link: its answer's keys other than these four are ignored.
- **Creating** is `proposal.create { confirmed: true }`. It runs for each `new_team` card, in order, each one alone (a failure stops only that card and is reported):
  1. Check every link's proof again (step 06b). A card whose proof no longer holds is split back into its projects, and the answer says which.
  2. Make the project:
     - One folder project: `CliHost`'s `open` path, which runs `init` and pauses the team with the setup-pending marker. Then `template.apply` with the card's template, then the marker removed.
     - Several projects: phase 9's `create_folder` with the members, a project folder at `~/Catervas/<slug>`, then the same template and marker steps.
     - A connector project: `~/Catervas/<slug>` made, `git init`, a README "# <name>\n\nBookkeeping kept by your Catervas team.\n", one commit, then `init`, the template and the marker.
  3. Rename the team to the card's name (`team.yaml`'s `name`).
  4. Add the team to the hub (step 05). It starts its drive paused, and serve remembers it.
  5. Record the card's projects' choices as `team` with the team's slug, in a new `made_teams(slug TEXT PRIMARY KEY, root TEXT NOT NULL, made TEXT NOT NULL /* JSON of what was created */, at TEXT NOT NULL) STRICT` (migration 10). `made` lists what this step created: `.catervas/`, the project folder, or the new folder.

  For each `add_to_team` card, the projects are added to that team as members through `repo.add` on the team's daemon. A one-repo team is first converted to a project folder by phase 9 step 07b's conversion, which asks the owner as that step does. The answer lists each card as `made`, `added` or `failed` with its reason.
- **Undo** is `teams.undo { slug }` on the hub, offered while the team's log has no `session.started`.
  - It stops the drive, then removes the team from the hub and `state.json`.
  - It deletes what `made_teams` lists: the `.catervas/` folder Catervas made in an owner's folder, or the whole project folder or new folder Catervas made, but only when its git log is still Catervas's own commits alone. Otherwise it removes the team and says "Catervas left <folder>: it has changes of yours."
  - The choices go back to Team, so the project shows on the proposal again.
  - After the first session, `teams.undo` is refused `team_has_started`.
- **Document it first** is phase 8's Catervafication offer (`catervafication_start`) on the team's socket. **Start** is `team.resume`.
- **This doesn't belong here**, in a team's Settings › Repositories:
  - phase 9's `repo.remove` takes the member out;
  - `links` No is recorded between it and each remaining member;
  - the owner chooses **Watch it** (the default), which sets its choice to `watch`, or **Make it its own team**, which runs creating for that one project.

  The page is `pages/settings/Repositories.tsx`. Phase 9's step 07 drew it but it was not built (no route exists, 2026-10-22). This step builds the list from `repos.list` with this one action per row.
- **Suggested connections.** At creation, a card's services are written to the new team's `.catervas/local/found-services.json` as `[{ service_id, name }]`. The team's suggested connections on Today (revision 0.105) also list a found service when an active agent's kit offers a connector whose catalog id matches, as "You use Zoho Books with Claude. Connect it for Finn?". Hiding one is the existing `connector_suggestion.hide`.

### The screens

- `/find/teams`, from `ManyYourTeams` frame 1:
  - stepper step 3; "Your teams"; "Check each one. Change anything.";
  - the "Better names?" box with its ⓘ and **Suggest names with Claude (about $x)**, its estimate from `proposal.names_estimate`;
  - one card per proposal card: the name with **Rename**, repository tags, the why line with its proof ⓘ, "Jobs: …", the Team select (the templates), **Move a project out**, the connector folder line;
  - "Watched, no team: …" from `finding.list`'s `watch` rows;
  - the footer **Back**, "They start paused. Nothing is spent until you start one.", and **Create N teams**.
- Frame 2: after names, "Names from Claude · cost $0.02", and each card's name and purpose with **Keep the folder name** and **Rename**.
- After Create, All teams (step 05b) shows the line "N teams created. They're paused until you start them." Each new team's card gets **Start**, **Document it first** (with its ⓘ), and **Undo**, while `teams.list` marks it `undoable: true`. `teams.list` gains `undoable`. Below the cards is the "Watched" section from `finding.list`'s `watch` rows, each with **Not mine** (Ignore) and **Make a team**, which marks Team and opens `/find/teams`.

## File map

```
crates/core/src/scrub.rs                          creates: scrub
crates/core/src/proposal.rs                        creates: cards from groups and choices; template suggestion; card id
crates/store/src/migrations/machine/0010_proposal.sql  creates
crates/store/src/machine.rs                        modifies: edits, moved_out, made_teams
crates/runtime/src/daemon/finding.rs               modifies: proposal.*; teams.undo; teams.list's undoable
crates/cli/src/finding.rs                          modifies: creating (open, create_folder, new folder, template, rename); undo's deletion; the names run
crates/runtime/src/daemon/team.rs                  modifies: suggested connections from found-services.json
docs/schemas/rpc.schema.json, crates/protocol, packages/protocol-client   modifies
apps/web/src/pages/find/YourTeams.tsx               creates
apps/web/src/pages/AllTeams.tsx                     modifies: created line, Start, Document it first, Undo, Watched
apps/web/src/pages/settings/Repositories.tsx        creates; App.tsx and SettingsLayout.tsx route it
apps/web/e2e/finding.spec.ts                        modifies: through to Create
docs/SPEC.md                                       modifies: revision 0.150
```

## Tasks

### Task 1: The scrub and the cards, in core

Tests:
- `scrubs_keys_before_claude_sees_them` (`scrub.rs`): asserts these.
  - `OPENAI_API_KEY=abc123`, `token: ghp_` followed by 36 characters, `https://user:pw@host/x?key=1` and `sk-ant-api03-` followed by 30 characters become `[removed]` (the URL becomes `https://host/x`).
  - `MAX_TOKENS=4096` and `--sort-key` stay.
- `makes_a_card_per_group_and_lone_team` (`proposal.rs`): asserts these.
  - Groups `{a,b}`, lone `c`, and `d` linked to driven team `shop` give two `new_team` cards and one `add_to_team` card.
  - The ids are stable across input order.
  - A bookkeeping connector card's template is `financial-analysis` and its folder is `~/Catervas/bookkeeping`.

- [x] `feat(core): propose teams from links and scrub what claude would see`

### Task 2: Edits and made teams in the index

Tests (`machine.rs`):
- `keeps_the_owner_s_edits_and_what_was_made`: asserts these.
  - An edit's name and template survive a reopen.
  - Moving b out of `{a,b}` records `links` No for a–b.
  - `made_teams` round-trips.
  - The migrations list ends at 10.

- [x] `feat(store): keep the proposal's edits and what catervas made`

### Task 3: Creating, adding and undoing

Tests (`crates/cli/tests/serving.rs`, ignored, with fixture repositories under the test's home and fake proofs set through the index):
- `creates_paused_teams_from_the_proposal`: asserts these.
  - A proposal of a two-repository group, a lone repository and a declared Zoho Books project is created by `proposal.create { confirmed: true }`. It gives three teams in `teams.list`, all `paused`.
  - The group's project folder lists both members, and the lone one has `.catervas/team.yaml` with the template's roles.
  - `~/Catervas/bookkeeping` is a git repository with Catervas's commit, and its team is the Financial analysis template's.
  - No `session.started` is in any log.
- `undoes_a_team_until_it_starts`: asserts these.
  - `teams.undo` on the lone team removes `.catervas/` from the owner's folder and leaves their files.
  - On the bookkeeping team it removes `~/Catervas/bookkeeping`.
  - A team whose log has a `session.started` is refused `team_has_started`.
  - A new folder with a commit of the owner's is left, with the sentence.
- `adds_a_linked_project_to_the_team_it_works_with`: asserts that a Yes between a driven one-repo team's project and another project gives an `add_to_team` card, and creating adds it as a member.

- [x] `feat(cli): create the proposed teams paused, add to a team, and undo`

### Task 4: Names from Claude

Tests (`crates/cli/src/finding.rs`, with a fake `claude` that records its standard input and prints a JSON answer):
- `asks_claude_for_names_with_nothing_secret`: asserts these.
  - `proposal.names_estimate` is above 0 and below $0.10 for three cards.
  - `proposal.names { confirmed: true }` runs the fake once with `--model haiku`. Its input holds the folder names and a README line, and not a fixture README's `API_KEY=sk-test-123`.
  - The cards' names and purposes come from its answer, and a link-like key in the answer changes nothing.
  - The cost shown is the answer's `total_cost_usd`.

- [x] `feat(cli): suggest names with claude, scrubbed and priced first`

### Task 5: The screens, Repositories and suggested connections

Tests:
- `checks_each_team_before_creating` (`pages/find/YourTeams.test.tsx`): asserts these.
  - The cards show their names, repositories, why and jobs.
  - Changing the Team select sends `proposal.edit { template }`.
  - **Move a project out** sends `proposal.move_out`.
  - **Suggest names with Claude (about $0.02)** shows its estimate, and pressing it sends `proposal.names { confirmed: true }`.
  - **Create 3 teams** sends `proposal.create { confirmed: true }` and goes to All teams with "3 teams created".
- `offers_undo_until_a_team_starts` (`pages/AllTeams.test.tsx`): asserts these.
  - An `undoable` card has **Start**, **Document it first** and **Undo**, and Undo confirms and sends `teams.undo`.
  - The Watched section's **Make a team** sends `finding.choose { choice: "team" }`.
- `takes_a_repository_out_of_a_team` (`pages/settings/Repositories.test.tsx`): asserts these.
  - **This doesn't belong here** opens "Take tidepool-infra out of Tidepool?" with Watch it chosen.
  - **Take it out** sends `repo.remove` and then `finding.choose { choice: "watch" }`.
- `suggests_connecting_a_service_found` (`crates/runtime/src/daemon/team.rs`): asserts that a `found-services.json` with Zoho Books and an agent whose kit offers it gives the suggestion. Without the kit offering it, there is none.

- [x] `feat(web): your teams, undo, and a repository taken out`

### Task 6: The journey and the spec

- `e2e/finding.spec.ts` continues the 06c journey through **Continue** on "Do these work together?" (the fixture has no proof) to **Your teams**. It presses **Create 2 teams** and sees both cards on All teams, paused, with Undo.
- `docs/SPEC.md` revision 0.150 (2026-10-22), "records phase 10 step 06e, the proposal and creating teams". It covers:
  - F1: creating from the proposal, undo, and add to a team;
  - 4.4: the screens and Repositories;
  - 6.7: suggested connections for services found;
  - 8.4: migration 10;
  - 8.6: the scrub, the one names run and its cost shown first.
- Then the project plan's row 06e: "planned and executed 2026-10-22".

- [x] `docs(spec): the proposal and creating teams, revision 0.150`

## Verification

```
python3 reaper.py cargo xtask check
python3 reaper.py cargo test -p catervas --test serving -- --include-ignored creates_paused_teams undoes_a_team adds_a_linked_project
pnpm -C apps/web exec playwright test e2e/finding.spec.ts
# expected: all pass
```
