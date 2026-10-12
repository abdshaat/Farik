# Phase 9, step 08: The founder's live check

Status: draft (one decision open: the repositories, below)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 11; the flows it exercises are 4.1, 4.2, 4.4, 4.5, 5.3, 5.6, 5.14, 5.16 and 5.17
Depends on: steps 01 to 07b of this phase. A start gate applies: Task 1 does not begin until 07b has landed and `cargo xtask check --integration` passes on the phase branch's head, whose sha is recorded.
Readiness confirmed by: (pending; one round, once the open decision is made)

Signatures, not bodies; around 300 lines at most (ADR 0008). This step writes no product code. It is a runbook, like phase 8's step 08, and keeps its two roles:
- **[A]** is the controller session preparing, fixing and recording.
- **[F]** is the founder. [A] stops, hands over, and waits.

[A] never acts in the founder's name. It does not file a request, answer, approve, accept, press Start, add, remove or locate a repository, or edit a document. It does not set, read, copy or move the AI account's key.

## Goal

The founder runs phase 9 once, for real, in the web app, on Claude (ADR 0016), and signs it off:
- a product repository and its infrastructure repository are set up as one project
- the project is documented by its Catervafication, each repository in its own folders, with the Architect's system map drawn in Files
- a feature is carried across both: planned provider first, built in each repository, integrated into each, with the Developer reading the other repository
- a third repository is added, documented on its own, then moved and located
- a repository with work is refused removal, then removed
- a one-repo project gains a second repository, and its old files are merged away

The record is `docs/milestones/multi-repo.md`, with the founder's sign-off. Then the phase's final check passes and pull request #45 is marked ready.

Out of scope: Windows, where no repository is a read root in this phase (step 03); cloning from GitHub (after Catervas Cloud).

## Decisions

- **[A] prepares a fresh run folder `~/catervas-p9-check/`**, as phase 8's `~/catervas-p8-check/`. It has its own `bin/catervas` and an `env.sh` that makes the run folder the home folder. An existing one is moved aside whole, never deleted.
- **Open: the repositories.** The founder names a product repository and the repository that runs it (infrastructure as code, a deploy setup, or a service it calls), their own or public. [A] copies each with `git clone --no-hardlinks` into `~/catervas-p9-check/` and removes `origin` in each copy, so nothing is pushed anywhere.
  - [A]'s proposal, if the founder has no pair: a copy of OpenMontage as the product, and a small `montage-infra` repository [A] writes in Task 1. It would hold a Compose file running OpenMontage's server, a Terraform module for its storage bucket, and a CI deploy job, each naming OpenMontage's paths, so that the system map has real links to find.
  - A third repository for section D is a further small public repository, or one [A] writes, `montage-web`.
- **The team:** the six the setup suggests, the UI/UX Designer left out as in phase 8, and a second Developer added for the cross-repository feature. Commands "Yes", sending to the online repository "No", and a daily spending limit of $25 (phase 8's).
- **The integration policy** stays the default, `auto_merge`, which in these copies merges locally in each repository, since no copy has a remote.
- **Each step's result** is pass, fail, finding or skipped, as phase 8's step 08 defines them. A failure is fixed on the phase branch before sign-off under the workflow's rules, with a landing review, a rebuilt `bin/catervas`, and the step run again.
- **The screens' words** are the approved mockups' (`docs/design/mockups/*Repos*.dc.html`, `SetupRepos`, `SettingsRepositories`, `AddSecondRepository`) and steps 07 and 07b's plans. Where the built words differ only in wording that keeps the meaning, that is a pass and a note.

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

Task 1's commands. Then [A] hands the founder this runbook from B on, and the two lines to type in a new terminal:

    source ~/catervas-p9-check/env.sh
    catervas serve        # opens the web app; use the link it prints

### B. A project of several repositories [F]

1. **Set up.** The account, the computer check, then "Yes, one or more repositories": add both repositories, name the project `montage`, keep both names and `~/Catervas/montage`. Then the team as in Decisions. Pass if: setup ends on "Your team is ready", and Settings › Repositories lists both repositories as "Found".
2. **What Catervas found.** The scan page shows each repository's facts and checks. Pass if: each repository's checks are named, such as `<repo>-the-tests-pass`.
3. **The Catervafication.** Press Start. Pass if:
   - the board shows the Product Manager's task, then each repository's Architect and Developer tasks tagged "notes", and later the system map's task
   - each lands in Files under its repository in the picker
   - the system map draws both repositories and at least one link, each link naming the file it was read from

### C. A feature across the repositories [F]

4. **Ask for it.** File a request that needs both repositories (for example, a new setting the product reads and the infrastructure provides). Approve the epic.
5. **The plan.** Pass if: the Architect's plan names each task's repository, puts the providing repository's task first, and says how the two stay compatible in between; and each task on the board carries its repository's tag.
6. **The work.** Pass if:
   - each task's work is in its own repository's history, merged there by Catervas
   - the consuming task started only after the providing one was integrated
   - the Developer's notes or review show it read the other repository
7. **The sprint review.** Pass if: the Architect's refresh updates the system documents when the feature changed a link, or says it did not.

### D. A third repository [F]

8. **Add it.** Settings › Repositories › Add a repository, `montage-web`. Pass if: Today offers "montage-web joined your project", and Start files its three tasks and the system map's update.
9. **Move and locate it.** In a terminal, [F] moves `montage-web`'s folder. Pass if: Today and Settings say "Can't find montage-web"; Locate… with its new folder clears both; its tasks waited meanwhile.

### E. Removing a repository [F]

10. **With work.** While a task works in `montage-web`, Remove it. Pass if: refused, naming the task.
11. **Without.** Cancel or finish the task, then Remove. Pass if: it leaves the table; its notes leave Files; its folder is untouched.

### F. A one-repo project gains a second repository [F]

12. **A one-repo project.** Settings › Computer › Change project…, "Yes, one or more repositories" with one repository, a fresh copy of the product repository. Pass if: the page recommends repositories that work together, and Continue opens it as a one-repo project, its notes in its own `docs/catervas`.
13. **Add a second.** With a task in progress, the dialog says so and waits. Once nothing is in progress, add the infrastructure copy. Pass if: the page says the notes are moving and lands on Settings with both repositories; the board keeps every task with its id; Files shows the moved notes.
14. **The old files.** Pass if: Today reminds you to merge `catervas/moved-out`; once [F] merges it (`git merge catervas/moved-out` in the product copy), the reminder goes; `catervas serve` run in the product copy opens the project folder.

## Tasks

### Task 1: Prepare the run [A]

The run folder, `bin/catervas` built at the start gate's sha (its sha256 recorded), `env.sh`, the copies or the written repositories (each with `origin` removed), and `feature.txt` suggesting section C's request. The founder's answer to the open decision is recorded first.

- [ ] `chore(milestones): prepare the multi-repo live check`

### Task 2: The founder's run and its record [F, A]

Sections B to F, each step's result, each failure's fix commit and its re-run, the findings, the daily limit's raises, and the founder's sign-off in `docs/milestones/multi-repo.md`.

- [ ] `docs(milestones): record the multi-repo live check`

### Task 3: The phase's end [A]

`cargo xtask check --integration` on the final head, its output in the pull request; row 08 and the phase marked done in the project plan; #45 marked ready.

- [ ] `docs(plan): phase 9 is done`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
