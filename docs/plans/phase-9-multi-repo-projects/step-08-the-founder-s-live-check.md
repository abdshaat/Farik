# Phase 9, step 08: The founder's live check

Status: ready (readiness review 2026-10-12, three rounds, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 11; the flows it exercises are 4.1, 4.2, 4.4, 4.5, 5.3, 5.6, 5.14, 5.16 and 5.17
Depends on: steps 01 to 07b of this phase. A start gate applies: Task 1 does not begin until 07b has landed and the full check passes on the phase branch's head, whose sha is recorded.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, round 1: not ready on seven blockers (the scan page, the repositories' checks, nothing to refuse in E, D's count and sprint, F's open choices, the record, the founder's own presses); folded below. Round 2: not ready on the checks' page (only setup's "Show advanced" shows them), a test reading outside its repository, and F5's paths after `env.sh`; folded. Round 3: ready, its three notes folded.

Signatures, not bodies; around 300 lines at most (ADR 0008). This step writes no product code. It is a runbook, like phase 8's step 08, and keeps its two roles:
- **[A]** is the controller session preparing, fixing and recording.
- **[F]** is the founder. [A] stops, hands over, and waits.

[A] never acts in the founder's name. It does not file a request, answer, approve, accept, press Start, add, remove or locate a repository, or edit a document. It does not set, read, copy or move the AI account's key.

## Goal

The founder runs phase 9 once, for real, in the web app, on Claude (ADR 0016), and signs it off:
- a product repository and its infrastructure repository are set up as one project
- the project is documented by its Catervafication, each repository in its own folders, with the Architect's system map drawn in Files
- a feature is carried across both: planned provider first, built in each repository, added to each
- a third repository is added and documented on its own, its work waits while it is moved, and it is located
- a repository with work is refused removal, then removed
- a one-repo project gains a second repository, and its old files are merged away

The record is `docs/milestones/multi-repo.md`, with the founder's sign-off. Then the phase's final check passes and pull request #45 is marked ready.

Out of scope: Windows, where no repository is a read root in this phase (step 03); cloning from GitHub (after Catervas Cloud).

## Decisions

- **The run folder** is `~/catervas-p9-check/`, as phase 8's `~/catervas-p8-check/`: its own `bin/catervas`, an `env.sh` that makes it the home folder, and the repositories. An existing one is moved aside whole, never deleted.
- **The repositories are [A]'s proposal** (the founder, 2026-10-12: "use the proposa[l]"), all in the run folder, each with no `origin`, so nothing is pushed anywhere:
  - **`OpenMontage`**, a copy of the founder's own (`git clone --no-hardlinks ~/OpenMontage`), as phase 8 used. Before cloning, [A] checks that `git -C ~/OpenMontage ls-files .catervas` prints nothing (a committed `team.yaml` would be refused as `has_team`), and records OpenMontage's entry point (its server if it has one, else its main command), which the other two name.
  - **`montage-infra`**, written by [A] in Task 1, `git init -b main`, one commit: `compose.yaml` running OpenMontage's entry point with a storage volume and a `MONTAGE_BUCKET` setting; `infra/storage.tf`, a Terraform bucket whose name `compose.yaml` passes in; `.github/workflows/deploy.yml` building OpenMontage's image; and a Node check, since the scan finds Node checks by a lockfile and `scripts.test` and Python only by `poetry.lock` or `uv.lock` (`crates/store/src/scan.rs`): `package.json` with `"test": "node --test"`, a committed `package-lock.json` with no dependencies, and `test/compose.test.mjs`, which reads `compose.yaml`, `storage.tf` and `deploy.yml` as text (the safe box has no YAML library) and checks that every `${VAR}` in `compose.yaml` is defined in `storage.tf` or has a default, and that `deploy.yml` names the Compose file's service. Checks run in the safe box with only the task's own repository at `/workspace`, so no test reads a path outside its own repository; Task 1 confirms it.
  - **`montage-web`**, written by [A] in Task 1 the same way: `index.html` and `app.js` calling OpenMontage's entry point at `MONTAGE_URL`, and the same Node check shape over `app.js` (`package.json`, `package-lock.json`, `test/app.test.mjs`).
  - **`solo/OpenMontage`**, a second clean copy of OpenMontage, and **`solo/montage-infra`**, a second copy of `montage-infra` (`git clone --no-hardlinks` of [A]'s, `origin` removed), for section F only.
- **OpenMontage has no scanned checks** (phase 8's finding) and the safe box has no pytest. A task that fails only because a check needs a package the safe box lacks is a finding; the founder sends it back with "Test it with Python's standard library only." (phase 8's sentence).
- **`feature.txt`**, written by [A] in Task 1, holds two requests:
  1. one across `montage-infra` and OpenMontage: a new setting `montage-infra` provides (for example, the bucket's region, `MONTAGE_REGION`, in `storage.tf` and `compose.yaml`) and OpenMontage reads (its entry point reading it, with a default), each part checkable with Node 24 or Python's standard library alone
  2. one in `montage-web` alone: a small change to `app.js` with its test, for sections D and E
- **The team:** the six the setup suggests, the UI/UX Designer left out as in phase 8 (its rows need its preview), and a second Developer added with "Add someone", named Noor. Commands "Yes", sending to the online repository "No".
- **The AI account** is the founder's own Claude subscription key, pasted on the setup's account screen and kept in the keychain; [A] never sees it. The founder makes it in a separate terminal without `env.sh`, or pastes one they have; "Catervas already has your key" is also a pass.
- **Costs:** a daily limit of $25 on the setup's spending screen (phase 8's). Reaching it pauses everyone and Today says so: not a failure. The founder raises it on the Costs page or waits for the next day; the record lists each raise.
- **The integration policy** stays the default, which in these copies merges locally in each repository, since none has a remote.
- **"What we found" for several repositories is not built.** The setup's scan page shows the project folder's own facts (a README, no checks). Each repository's scanned checks are seen only on setup's last screen, behind "Show advanced settings", under "Checks for every piece of work" (`SetupAdvanced.tsx`), as "Found in your project. Runs: npm run test" (the scan's `{toolchain} run {script}`). B1 checks that. Three findings are recorded for a later step, not failures: the scan page's words, a check's row not naming its repository, and no page after setup showing the checks (so `montage-web`'s is never seen).
- **Agent judgement is a finding, not a failure:** the plan's order and compatibility note, a Developer reading the other repository, and the system documents' content are the agents' work; a miss there is a finding.
- **Each step's result** is pass, fail, finding or skipped, as phase 8's step 08 defines them. A failure is fixed on the phase branch before sign-off as phase 8's Decisions say (a test that fails for it, its own `fix(<scope>): …` commit, a fresh landing review, `bin/catervas` rebuilt and its sha256 recorded, the step run again). A failure that needs a product decision goes to the founder first.
- **The screens' words** are the approved mockups' (`SetupRepos`, `SettingsRepositories`, `TodayRepos`, `BoardRepos`, `FilesRepos`, `AddSecondRepository` in `docs/design/mockups/`) and steps 07 and 07b's "Words". Where the built words differ only in wording that keeps the meaning, that is a pass and a note.
- **The order is A, B, C, D, E, then F.** Between sittings and after a fix: Ctrl-C, then the two lines; it reopens the last project with a new link.

## File map

```
docs/milestones/multi-repo.md                             creates: the record and the sign-off (Task 2)
docs/milestones/multi-repo/                               creates: screenshots the founder chooses to keep, if any (Task 2)
<the files a failure's fix touches, with their tests>     modifies: one fix commit each (Task 2)
docs/plans/phase-9-multi-repo-projects/step-08-the-founder-s-live-check.md   modifies: status and ticks (Tasks 2, 3)
docs/plans/project-plan.md                                modifies: row 08 done, the phase done (Task 3)
```

## Interfaces

Consumes: the phase branch's web app and `catervas serve`, as built at the start gate or after a fix. Produces: nothing.

## Runbook

### A. Preparation [A]

Task 1's commands. Then [A] hands the founder this runbook from B on, `~/catervas-p9-check/feature.txt`, and the two lines to type in a new terminal:

    source /home/<user>/catervas-p9-check/env.sh      # the absolute path, so it also works once env.sh has made the run folder home
    catervas serve        # opens the web app; use the link it prints

### B. A project of several repositories [F]

1. **Set up.** The AI account; the computer check (Claude Code, git, Docker and the safe box Ready; press "Prepare it" once); "Yes, one or more repositories", "Add a repository" for `OpenMontage`, then for `montage-infra`. Pass if: from the second, "Name the project" and the notes folder appear, the page recommends repositories that work together, and the names default to `openmontage` and `montage-infra`. Name the project `montage`, keep both names and `~/Catervas/montage`, Continue; the team as in Decisions; commands Yes, sending No; a daily limit of 25. On the last screen, before "Start the team", press "Show advanced settings". Pass if: "Checks for every piece of work" holds a row "Found in your project. Runs: npm run test" (from `montage-infra`; OpenMontage has none), and setup ends on "Your team is ready".
2. **The repositories.** Settings › Repositories. Pass if: both rows read "Found", with their folders and "its usual branch".
3. **The Catervafication.** On "Your team is ready", **Start**. Pass if: the Board shows "Catervafy the project" in a sprint, then Mira's "Write the product plan from the project", and for each repository Ada's "Document <name>'s architecture" and a Developer's "Document how <name> is built", each tagged "notes".
4. **The product plan.** Mira's task comes to "Waiting on you": open it, **Accept**. Pass if: Files › Product shows Product description and Roadmap.
5. **Marketing.** Kai's marketing plan comes to Today after the product plan is added: **Approve** it. Pass if: it did not start before.
6. **The system map.** Once both architecture tasks are accepted, the Board shows Ada's "Map how the repositories connect". Accept each task as Today asks. Pass if: Files' picker reads "Project", "openmontage", "montage-infra"; each repository's Architecture and Engineering folders hold its documents; and Project › Architecture › System map draws both repositories and at least one link, each link naming the file it was read from (its content is agent judgement).
7. **The sprint ends.** Accept "Catervafy the project" when Today asks. Pass if: the sprint ends.

### C. A feature across the repositories [F]

1. **Ask.** Paste `feature.txt`'s first request into Today's request box. Answer Mira's questions; approve the plan when Today asks. Pass if: it waits in the Backlog.
2. **Start a sprint** from Today or the Board, its limit empty. Pass if: it starts.
3. **The plan.** Files › Project › Architecture › "Plan: <the feature's title>". Pass if: each task names its repository. Provider first and a compatibility note are agent judgement (a miss is a finding).
4. **The tasks.** Pass if: each task on the Board carries its repository's tag; `montage-infra`'s task moves before OpenMontage's starts, if the plan made one depend on the other.
5. **Accept** each as Today asks. Pass if: What moved says "… was added to montage-infra" and "… was added to openmontage", each in its own repository.
6. **The sprint review.** Pass if: What moved shows Ada's refresh of the architecture notes, or the system documents say "Changed by Ada" (agent judgement otherwise).

### D. A third repository [F]

1. **Add it.** Settings › Repositories › Add a repository, `montage-web`, name kept, branch empty. Pass if: it joins the table as "Found", and Today offers "montage-web joined your project" with Ada's and a Developer's lines.
2. **Document it.** **Start** on that card; if the Board shows "Catervafy montage-web" in the Backlog, start a sprint from Today or the Board (its limit empty). Pass if: the sprint holds "Document montage-web's architecture", "Document how montage-web is built" and, after the first, "Bring the system map up to date with montage-web". Accept each as Today asks.
3. **Work in it.** Paste `feature.txt`'s second request; approve its plan; start a sprint. Pass if: its task is tagged `montage-web`. Do not accept this task until E2.
4. **Move it.** While that task is on the Board and not yet added, [A] gives the line and [F] runs it in a second terminal, without `env.sh`: `mv ~/catervas-p9-check/montage-web ~/catervas-p9-check/montage-web-moved`. Pass if: Today says "Can't find montage-web. It was at ~/montage-web. …" and Settings shows "Can't find it"; the task does not move while it is missing.
5. **Locate it.** Locate… (on Today or Settings), `montage-web-moved`. Pass if: both notices go, and the task carries on.

### E. Removing a repository [F]

1. **With work.** While D3's task is not yet added, Settings › Repositories › Remove `montage-web`. Pass if: refused, "1 of your team's tasks still work in montage-web: CTV-<n>. Finish or cancel them first."
2. **Without.** Cancel that task (its page's Cancel), then Remove. Pass if: it leaves the table and Files' picker; its folder `montage-web-moved` is untouched.

### F. A one-repo project gains a second repository [F]

1. **A one-repo project.** Change beside the project's name, Change project…, "Yes, one or more repositories", Add `solo/OpenMontage` alone, Continue. Pass if: no name field shows, the notes line says `docs/catervas`, and it lands on Today, which shows "Your team hasn’t documented this project yet" with Start.
2. **Waiting.** **Start** on that card. While its first task is in progress, Settings › Repositories › Add a repository. Pass if: the dialog's alert names "the Catervafication" and Continue is disabled. Close it, and let the Catervafication finish, accepting what Today asks.
3. **Add the second.** Add a repository again; keep `~/Catervas/openmontage` and the first name; Continue; `solo/montage-infra`. Pass if: the page says the notes are moving and lands on Settings › Repositories with both; the Board keeps every task with its id; Files shows the notes under Project and `openmontage`.
4. **The old files.** Pass if: Today says "openmontage still holds Catervas's old files. Merge the branch catervas/moved-out in openmontage to remove them."
5. **The old repository opens the project folder.** Ctrl-C, then:

       source /home/<user>/catervas-p9-check/env.sh
       cd ~/solo/OpenMontage && catervas serve      # env.sh made the run folder home

   Pass if: it opens the project folder (Settings › Repositories lists both). Ctrl-C, then, in the same terminal, `git -C ~/solo/OpenMontage merge catervas/moved-out`, then the two lines of A. Pass if: Today's reminder is gone.

## Tasks

### Task 1: Prepare [A]

At the phase branch head, in a clean checkout, with Docker running (`docker info`):

```
cargo xtask check --integration      # the start gate (or /tmp/claude-0/fullcheck.sh in the cloud container); expected: xtask check: ok
git rev-parse HEAD                   # the start-gate sha, recorded
[ -e ~/catervas-p9-check ] && mv ~/catervas-p9-check ~/catervas-p9-check-$(date +%F)
mkdir -p ~/catervas-p9-check/bin ~/catervas-p9-check/solo docs/milestones
pnpm install --frozen-lockfile && pnpm -r --if-present generate
pnpm -C apps/web build && cargo build --release -p catervas
cp target/release/catervas ~/catervas-p9-check/bin/ && sha256sum ~/catervas-p9-check/bin/catervas
git -C ~/OpenMontage status --short && git -C ~/OpenMontage ls-files .catervas   # recorded; the second prints nothing
git clone --no-hardlinks ~/OpenMontage ~/catervas-p9-check/OpenMontage
git clone --no-hardlinks ~/OpenMontage ~/catervas-p9-check/solo/OpenMontage
# montage-infra and montage-web written as Decisions say, each git init -b main and one commit,
# their npm test run once with node 24 and passing, also from a copy of each alone in an empty folder
# (so no test reads outside its own repository); then:
git clone --no-hardlinks ~/catervas-p9-check/montage-infra ~/catervas-p9-check/solo/montage-infra
for r in OpenMontage solo/OpenMontage solo/montage-infra; do git -C ~/catervas-p9-check/$r remote remove origin; done
for r in OpenMontage montage-infra montage-web solo/OpenMontage solo/montage-infra; do git -C ~/catervas-p9-check/$r remote -v; git -C ~/catervas-p9-check/$r rev-parse HEAD; done   # no remote; each commit recorded
git -C ~/OpenMontage status --short                          # unchanged from before the clone
claude --version                     # at least MIN_CLAUDE_VERSION
```

`env.sh` is phase 8's with `p8` read `p9`. [A] writes `feature.txt` as Decisions say. [A] may run `catervas serve --no-open` once to see the wizard answer, stopping it before the founder starts, then removes what it wrote (`.config`, `.local`, state), keeping `bin/`, `env.sh`, `feature.txt` and the repositories.

- [ ] Task 1 [A]: nothing to commit; the start-gate sha, the binary's sha256, OpenMontage's entry point and each repository's commit go into Task 2's record

### Task 2: The founder's run, recorded [F] [A]

[F] runs B to F. [A] writes each result as the founder gives it, fixing each failure as Decisions say before the founder goes on or signs. Once the run is over, [A] writes `docs/milestones/multi-repo.md`:

- the date, the start-gate sha and every later binary's sha with the fix it carries, each repository's commit, OpenMontage's entry point, the team, the daily limit and each raise
- one table per section, B to F, each row `| Step | Result | Note |`: the step's number and name, its result (pass, fail, finding, skipped), and the founder's words or what was seen
- the failures, each with its fix commit, its landing review, and its result when run again
- the findings, as a list of fixes for later, none made here (B1's three among them)
- the founder's sign-off, in their own words: `Signed off by the founder, <date>: "<words>"`

Sign-off needs every step to be pass or finding.

- [ ] `fix(<scope>): …` per failure, each with its test and its landing review
- [ ] `docs(docs): record the founder's live check of multi-repo projects`

### Task 3: The phase's final verification [A]

On the final commit of the phase branch, after the record:

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; expected: xtask check: ok
git push                             # the record and every fix on the remote
gh pr checks 45                      # every check passes (in the cloud container: the GitHub tools' check runs for #45)
gh pr ready 45                       # #45 marked ready (in the cloud container: the GitHub tools' update, draft false)
```

`docs/plans/project-plan.md`'s row 08 and phase 9 are marked done, and this plan's status `done`.

- [ ] `docs(plan): record step 08 and phase 9 as done`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
gh pr view 45 --json isDraft,statusCheckRollup
# expected: "isDraft": false, every check SUCCESS
```

and `docs/milestones/multi-repo.md` holds the founder's sign-off.
