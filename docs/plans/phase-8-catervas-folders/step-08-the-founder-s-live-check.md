# Phase 8, step 08: The founder's live check

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 11; the flows it exercises are 4.1 to 4.5, 5.3, 5.9, 5.14, 5.16 and 5.17
Depends on: steps 01 to 07b of this phase (01 to 06 landed and landing-reviewed; 06b, 07 and 07b not yet executed). A start gate applies: Task 1 does not begin until 07b has landed and `cargo xtask check --integration` passes on the phase branch head, whose sha is recorded.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): not ready on one Blocking (OpenMontage has no scanned checks and the safe box has no pytest), decided by the controller as the reviewer proposed; the Should items and nits folded

Signatures, not bodies; around 300 lines at most (ADR 0008). This step writes no product code. It is a runbook, like phase 4's step 08 and phase 10's step 03, and keeps their two roles:
- **[A]** is the controller session preparing, fixing and recording.
- **[F]** is the founder. [A] stops, hands over, and waits.

[A] never acts in the founder's name. It does not file a request, answer, approve, accept, send back, start or end a sprint, press Start or Later, edit a document in Files, or chat with an agent. It does not set, read, copy or move the AI account's key. Every one of those is recorded in the log as the human.

## Goal

The founder runs phase 8 once, for real, in the web app, on Claude (ADR 0016), and signs it off: an existing repository is set up, offered and given its Catervafication sprint, and its team's documents appear in Files, folder by folder; the founder edits a document and sees the team bring its own version up to date; a feature is planned in two lanes for two Developers; a sprint review proposes product plan changes and refreshes the architecture notes; and a new project goes from the founder's paragraph through Mira's interview to an approved product plan and its own, lighter Catervafication, with no second approval. The record is `docs/milestones/catervas-folders.md`, with the founder's sign-off. Then the phase's final check passes and pull request #34 is marked ready. Out of scope: the founder's live runs phase 7 left open (phase 9 step 07); the UI/UX Designer's Catervafication (below); a timed test with other users (phase 10 step 03).

## Decisions

- **[A] prepares, in a fresh run folder `~/catervas-p8-check/`**, as `~/farik-m1/` was: its own `bin/catervas`, `env.sh`, and the copy of the repository. `env.sh` makes the run folder the home folder, so the web app's folder picker shows only the run's folders and no earlier project is remembered. An existing `~/catervas-p8-check/` is moved aside whole (`mv ~/catervas-p8-check ~/catervas-p8-check-<date>`), never deleted. Rejected: the founder's own home folder, where `catervas serve` would reopen the last project served.
- **The existing repository is a copy of the founder's own OpenMontage** (the founder, 2026-10-10: "One of my own" → "OpenMontage"; 563 MB without dependencies, last changed 2026-08-22, 448 commits). [A] copies it with `git clone --no-hardlinks ~/OpenMontage ~/catervas-p8-check/OpenMontage`, so its history comes but nothing of the original changes, and removes `origin` in the copy, so nothing is pushed anywhere. It is Python with a Node part (`lib/`, `scripts/`, `remotion-composer/`, and more), so a feature can be planned in two lanes that do not touch. The scan finds no checks for OpenMontage ("No checks found"), and the safe box has no pytest. [A] writes `feature.txt` so that each lane can be checked with what the safe box holds: Python's standard library (`python3 -m unittest`, `py_compile`) and Node 24 (`node --test`). A task that fails only because a check needs a package the safe box lacks is a *finding*. When it comes to the founder, they send it back with "Test it with Python's standard library only." Rejected: a small public repository (the founder chose their own); the original folder (the team commits to it).
- **The new project** is made by the web app itself, under "Change project…" then "No, start a new one", in `~/catervas-p8-check/`, after every section on the existing repository is done, so the team moves with it, as a user's would. Rejected: a second `home/` and a second `catervas serve`, which no user does.
- **The team**, on the existing repository: the six the setup suggests with the UI/UX Designer left out and a second Developer added with "Add someone" (Product Manager, Scrum Master, Architect, two Developers, Marketing Specialist), names left as suggested, the second Developer named Noor. The Designer is left out because it gets a Catervafication task only once its app preview is set (step 07's `designer_ready`), which this run does not test; its row and task are covered by 07 and 07b's tests. Two Developers are for section E. Asked on the setup's permission screen: commands "Yes", sending to the online repository "No".
- **Costs: a daily spending limit of $25, set by the founder on the setup's "Do you want a daily spending limit?" screen** (the founder, 2026-10-10: "$25 a day"; Catervas has no limit for a whole run; a sprint's limit is left empty). When the team reaches it, everyone pauses and Today says so: that is not a failure. The founder raises the limit on the Costs page, or waits for the next day, and the record lists each raise with its time. The new project keeps the team's rules; the founder checks the Costs page shows the limit and sets it there if not.
- **The AI account** is the founder's own Claude subscription key, pasted by the founder on the setup's "Your Claude account" screen and kept in the computer's keychain (ADR 0021). [A] never sees it. The founder makes the key in a separate terminal, without `env.sh`, or pastes one they have; "Catervas already has your key" is also a pass.
- **The order is A, B, C, E, F, G, then D**: the existing repository's sections, then Change project to the new one. C's last step waits for E's sprint, which plans the re-derive task first, and F is E's sprint ending.
- **Each step's result is one of:** *pass*; *fail*: what the step says should happen does not (a screen errors or shows a refusal not listed, a button does nothing, words that change the meaning, work lost, an agent writing outside its folder, the Marketing Specialist reading code, a second approval asked for the new project's Catervafication); *finding*: it works as built but the founder wants it different, or an agent's writing is thin or wrong about the project (a document that misreads the code, a plan in one lane, a review that proposes nothing), which goes to a list of fixes for later and does not block sign-off unless the founder says it does; *skipped*: an optional step not run.
- **A failure is fixed on the phase branch before sign-off**, under the workflow's rules: [A] finds the cause (systematic debugging), writes a test that fails for it, fixes it in its own `fix(<scope>): …` commit, changes `docs/SPEC.md` in the same pull request when behaviour changes, and has a fresh session give it a landing review with the mutation bar. [A] then rebuilds `~/catervas-p8-check/bin/catervas` (Task 1's build and copy, its new sha256 recorded), and the founder runs the failed step again, and any later step it could have changed. After a fix, and between sittings: Ctrl-C, then the same two lines; it reopens the last project with a new link. A failure that needs a product decision goes to the founder first. The record shows each failure, its fix commit and the step's result when run again.
- **The screens' words** quoted below are those of steps 04b, 06b and 07b's plans and the approved mockups in `docs/design/mockups/`. Where the built app's words differ only in wording that keeps the meaning, that is a pass and a note.

## File map

```
docs/milestones/catervas-folders.md                       creates: the record and the sign-off (Task 2)
docs/milestones/catervas-folders/                          creates: screenshots the founder chooses to keep, if any (Task 2)
<the files a failure's fix touches, with their tests>     modifies: one fix commit each (Task 2)
docs/plans/phase-8-catervas-folders/step-08-the-founder-s-live-check.md   modifies: status and ticks (Tasks 2, 3)
docs/plans/project-plan.md                                 modifies: row 08 done, the phase done (Task 3)
```

## Interfaces

Consumes: the phase branch's web app and `catervas serve`, as built at the start gate or after a fix. Produces: nothing.

## Runbook

### A. Preparation [A]

Task 1's commands. Then [A] hands the founder this runbook from B on, `~/catervas-p8-check/feature.txt`, and the two lines to type in a new terminal:

    source ~/catervas-p8-check/env.sh
    catervas serve        # opens the web app; use the link it prints

These are the founder's only terminal actions, besides making the AI account's key (Decisions); after a fix, and between sittings, Ctrl-C, then the same two lines, which reopen the last project with a new link. Everything else is in the browser. To look at a phone screen, the founder makes the browser window narrow (about a third of the screen); the app switches to its phone layout below 1024 pixels.

### B. An existing repository, Catervafied [F]

1. **Set up.** The AI account (make the key in a separate terminal, without `env.sh`, or paste one you have; "Catervas already has your key" is also a pass); the computer check (Claude Code, git, Docker and the safe box show Ready (press "Prepare it" once); the Designer's browser row may stay as it is); "Yes, I have a project folder", choose `OpenMontage`; "Here is what Catervas found", "That's right"; the team as in Decisions; commands Yes, sending No; a daily limit of 25. Pass if: setup ends on "Your team is ready".
2. **The offer** (`CatervaficationOffer`). "Your team is ready", "<names> have joined <team>.", "Recommended", "Start with a Catervafication sprint", one row each for Mira, Ada, the first Developer and Kai with their lines, no Scrum Master row, "Your team only adds its notes; nothing else in your project changes.", Start and Later. Press **Later**. Pass if: the page reads as drawn and Later goes to Today.
3. **The quiet card** (`CatervaficationToday`). Today shows "Your team hasn’t documented this project yet" under the request box, outside "Waiting on you" and its count; the Files page shows the same card at its top. Press **Start** on Today's card. Pass if: both cards show, and Start asks nothing more. (Later then Start sees both screens; both Starts send the same order.)
4. **The sprint.** The Board shows a sprint holding "Catervafy the repository", and within minutes its tasks: "Write the product plan from the project" (Mira), "Document how the project is put together" (Ada), "Document how the project is built and worked on" (a Developer). Pass if: those three appear, and nothing else is planned into the sprint.
5. **The folders fill** (`Files`). As each task is accepted and added, Files shows its folder with its owner's face and colour: Product, Architecture, Engineering; an empty folder reads "Nothing here yet". Pass if: each document sits in its owner's folder and nowhere else.
6. **The product plan to accept.** Mira's task comes to "Waiting on you" for you to accept, not an agent. Open it, read the product description and the roadmap, and **Accept**. Pass if: you are the one asked, and once added, Files → Product shows Product description and Roadmap, each with "For you" and "For the team".
7. **Marketing follows.** Only after step 6's plan is added, Kai's task "Propose the first marketing plan from the product plan" appears, then a marketing plan to approve on Today (`MarketingPlanApproval`). Approve it (suggested; sending it back is also fine), since no posting or ads account is connected, so nothing goes out. Pass if: Kai's task did not start before the plan was added.
8. **The architecture.** Files → Architecture → Overview. Pass if: at least one diagram is drawn as a picture, not code, and the line under the title says "Changed by Ada …, in CTV-<n>", the task a link.
9. **The engineering notes.** Files → Engineering → Workflow. Pass if: it says near its top whether it records the project's own way of working or adopts Catervas's recommended workflow.
10. **The sprint ends.** Accept "Catervafy the repository" when Today asks; the sprint ends after. After its look back, Files → Delivery shows "Sprint rhythm", by Sol. Pass if: it does.

### C. The Files page [F]

1. **Edit** (`FilesEdit`). Files → Product → Roadmap, "For you", **Edit**. You see "You are changing the version for you.", Write and Preview, the hint, and "Saving keeps your change at once. Mira will update the team’s version." Add a line such as "- Gift cards", look at Preview, **Save**. Pass if: the page reads so and Save closes the editor.
2. **It goes out.** "Saved. Catervas adds it to your project in a moment.", then the line goes and "Changed by you just now" shows. Pass if: so.
3. **The team's version** (`FilesStale`). Press "For the team". Pass if: Mira's face and "Mira is bringing this up to date with your change." show, and the Board shows her task to bring `roadmap.agent.md` up to date, waiting for the next sprint.
4. *Optional:* **changed while editing** (`FilesChanged`). Open the roadmap in two tabs; Edit in both; Save in the first; save in the second tab after the first shows "Changed by you just now". Pass if: the second says "This file changed while you were editing. Your text is still here; …" with Reload, and your text is still in the box.
5. *Optional:* **busy file.** At your word [A] adds a line to the roadmap in the project folder, outside Catervas; you Edit and Save. Pass if: "This file was also changed on your computer, outside Catervas. …" shows and your text is kept. [A] then undoes its line.
6. **On a phone** (`PhoneFiles`). Narrow the window; Team → Files; the "File" picker; read the roadmap; "Back". Pass if: every folder and document can be reached and read.
7. **Later, the team's copy** (during E's sprint). Pass if: "For the team" no longer says Mira is bringing it up to date, and its line says "Changed by Mira …, in CTV-<n>".

### E. A feature in two lanes [F]

1. **Ask.** Paste `feature.txt` into Today's request box (reword it if you like). Answer Mira's questions; approve the plan when Today asks. Pass if: it becomes one piece of work waiting in the Backlog.
2. **Start a sprint** from Today or the Board, the sprint's limit left empty. Pass if: the sprint starts.
3. **The Architect's plan.** Files → Architecture → "Plan: <the feature's title>". Pass if: it lists the tasks in two lanes (*finding* if one lane; you may file a second request [A] prepared once).
4. **The contracts and the lanes.** On the Board, Mira's tasks appear under the feature. Pass if: every task of lane 1 goes to one Developer and every task of lane 2 to the other, and the two work at the same time.
5. **Accept** what Today asks of you as the work finishes. Pass if: the sprint ends once every task is accepted.

### F. The sprint review [F]

1. **Product plan changes** (`DocumentApproval`). When E's sprint ends, Today shows "Product plan changes to approve" from Mira: her summary, each document's changes line by line, "Open in Files", a notes box, Approve and Send back; the team band says "Waiting on you: product plan changes". Narrow the window once (`PhoneDocumentApproval`): the buttons span the width. **Approve.** Pass if: so, and Files → Product shows the change, "Changed by Mira". (*Finding* if Mira proposes nothing for a feature that changed what the product does.)
2. **The architecture refresh.** Pass if: "What moved since yesterday" says "Ada brought the architecture notes up to date with sprint <n>", and Files → Architecture → Features or Structure says "Changed by Ada".
3. **The sprint report.** Pass if: Files → Product shows the sprint's report.

### G. The Marketing Specialist's read limit [F] (optional)

1. Chats → Kai: "Please read <a code file [A] names> and tell me what its first function does." Pass if: Kai says he cannot read it, and his reply holds nothing of the file.

### D. A new project [F]

1. **Change project.** "Change" beside the project's name, "Change project", "No, start a new one", the paragraph: "An ordering site for my bakery, Little Oak Cakes, where customers order cakes online and pick them up in the morning." (or your own), named Little Oak Cakes, its folder in the home folder. Change project goes straight to Today; check the Costs page shows your daily limit. Pass if: it lands on Today, with no Catervafication offer.
2. **Waiting for the plan.** Pass if: Today's band says "Little Oak Cakes: your team waits for your product plan" and the agents' lines say "Nothing to do until your plan is approved" (Sol: "Nothing to plan …").
3. **The interview** (`ProductInterview`). Chats → Mira. Pass if: the chat says "Before your team builds anything, Mira asks what you want to make. …", and Mira greets you and asks her first question herself. Answer each question in your own words, as many rounds as she asks.
4. **What I heard.** Pass if: her summary shows "What I heard" with "Your customers", "The problem", "It must", "Not now", "It works when", and "Draft the product plan" under it. Press it. Pass if: "Drafting it as CTV-<n>. It comes to you on Today to approve."
5. **The plan card** (`ProductPlanApproval`). Pass if: Today shows "Your product plan to approve", "From Mira, written from your chat …", both documents in full, "Nothing else is planned until you approve. …", a notes box, Approve and Send back.
6. **Send back once** with a note, such as "Add a way for customers to choose a pickup time." Pass if: the card goes, and comes back later with your note taken in.
7. **Approve.** Pass if: once the plan is added, the Board shows "Catervafy the new project" in a sprint with Ada's "Write the first architecture from the product spec", a Developer's "Set the workflow and the stack's conventions" and Kai's marketing plan task; Today does not ask you to approve the Catervafication itself (Kai's marketing plan and, at the end, accepting the finished work still come to you); and its page's history says "Catervas filed it to document the project."
8. **Its documents.** Pass if: as its tasks are accepted, Files shows Product's Product description and Roadmap, Architecture's Overview with a drawn diagram, and Engineering's Workflow and Conventions.

At the end [F] stops `catervas serve` with Ctrl-C. Between sittings and after a fix: Ctrl-C, then the same two lines; it reopens the last project with a new link.

## Tasks

### Task 1: Prepare [A]

At the phase branch head, in a clean checkout, with Docker running (`docker info`):

```
cargo xtask check --integration      # the start gate (or /tmp/claude-0/fullcheck.sh in the cloud container); expected: xtask check: ok
git rev-parse HEAD                   # the start-gate sha, recorded
[ -e ~/catervas-p8-check ] && mv ~/catervas-p8-check ~/catervas-p8-check-$(date +%F)
mkdir -p ~/catervas-p8-check/bin
pnpm install --frozen-lockfile && pnpm -r --if-present generate
pnpm -C apps/web build && cargo build --release -p catervas
cp target/release/catervas ~/catervas-p8-check/bin/ && sha256sum ~/catervas-p8-check/bin/catervas
git -C ~/OpenMontage status --short                          # recorded, before the clone
git clone --no-hardlinks ~/OpenMontage ~/catervas-p8-check/OpenMontage
git -C ~/catervas-p8-check/OpenMontage remote remove origin
git -C ~/catervas-p8-check/OpenMontage remote -v             # prints nothing
git -C ~/catervas-p8-check/OpenMontage rev-parse HEAD        # recorded
git -C ~/OpenMontage status --short                          # unchanged from before the clone
claude --version                     # at least 2.1.272 (MIN_CLAUDE_VERSION)
```

`~/catervas-p8-check/env.sh`:

```
# Source before catervas serve for phase 8's live check.
export HOME=/home/ashaat/catervas-p8-check
export PATH=/home/ashaat/catervas-p8-check/bin:$PATH:/home/ashaat/.local/bin
cd /home/ashaat/catervas-p8-check
```

[A] writes `~/catervas-p8-check/feature.txt`, one plain request for OpenMontage with two parts in two separate areas, each checkable with Python's standard library or Node 24 alone (Decisions), and a second one for E3's retry, and names a code file for G. [A] may run `catervas serve --no-open` once to see the wizard answer, stopping it with Ctrl-C before the founder starts, and then removes what it wrote in `~/catervas-p8-check/` (its `.config`, `.local` and state), keeping `bin/`, `env.sh`, `feature.txt` and `OpenMontage/`.

- [ ] Task 1 [A]: nothing to commit; the start-gate sha, the binary's sha256 and the copy's commit go into Task 2's record

### Task 2: The founder's run, recorded [F] [A]

[F] runs B, C, E, F, G and D. [A] writes each result as the founder gives it, fixing each failure as Decisions says before the founder goes on or signs. Once the run is over, [A] writes `docs/milestones/catervas-folders.md`:

- the date, the start-gate sha and every later binary's sha with the fix it carries, the copy's commit, the team, the daily limit and each raise;
- one table per section, B, C, E, F, G and D, each row `| Step | Result | Note |`, the step's number and name, its result (pass, fail, finding, skipped), and the founder's words or what was seen;
- the failures, each with its fix commit and its landing review, and its result when run again;
- the findings, as a list of fixes for later, none made here;
- the founder's sign-off, in the founder's own words: `Signed off by the founder, <date>: "<words>"`.

Sign-off needs every non-optional step to be pass or finding.

- [ ] `fix(<scope>): …` per failure, each with its test and its landing review
- [ ] `docs(docs): record the founder's live check of Catervas folders`

### Task 3: The phase's final verification [A]

On the final commit of the phase branch, after the record:

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; expected: xtask check: ok
git push                             # the record and every fix on the remote
gh pr checks 34                      # every check passes
gh pr ready 34                       # the phase's pull request, marked ready
```

`docs/plans/project-plan.md`'s row 08 and phase 8 are marked done, and this plan's status `done`.

- [ ] `docs(plans): record step 08 and phase 8 as done`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
gh pr view 34 --json isDraft,statusCheckRollup
# expected: "isDraft": false, every check SUCCESS
```

and `docs/milestones/catervas-folders.md` holds the founder's sign-off.

## The founder's answers

Both questions of the first draft were answered on 2026-10-10 and are in Decisions: the repository ("One of my own" → "OpenMontage") and the daily limit ("$25 a day").
