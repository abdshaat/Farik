# Many teams: several teams on one computer, found from the user's Claude Code history

Status: approved by the founder on 2026-10-12, in a brainstorm in a Claude Code session (ADR 0059). It is the design input to phase 10, Many teams, right after phase 9, Multi-repo projects. Its screens are mocked up and approved before they are built (ADR 0032). The parts marked **Proposed** are the brainstorm's own answers to questions the founder did not rule on; they are open to the founder's change in review.

## Why

A Catervas user who already works with Claude Code has many things going on at once: products, side projects, scheduled scrapers, cron jobs, deployments, bookkeeping. Today Catervas runs one team on one project, and the user has to tell it which. The founder wants Catervas to find every project and recurring job from the user's Claude history, work out which belong together, and set up one team for each group, so that Catervas runs several teams on one computer rather than one. Then Catervas reminds the user each day of the schedules, crons and deployments that need attention and of the sessions they may have forgotten, and shows analytics on workflows, usage per team and blockers.

## The founder's decisions (2026-10-12, in conversation)

- **Placement:** "Right after 9". This is phase 10; Catervas Cloud and every later phase move down by one (project plan revision 45).
- **Related projects:** one team with several repositories. Phase 9's project folder (ADR 0057) is that team's home, so a group of related repositories becomes one multi-repo project.
- **Detection:** heuristics plus AI labels. Grouping and scoring are deterministic code; the user's own Claude only names and describes the groups, with its cost shown first; the owner approves on a review board.
- **Reminder:** on Today, plus a browser notification.
- From the request: Catervas reads the user's history on the computer and never sends it to Catervas Cloud; nothing is created without the owner's approval.

## What Catervas can read, and what it may not

Checked against Claude Code's documentation on 2026-10-12 and against a real `~/.claude`.

| Source | What it gives | Standing |
|---|---|---|
| `~/.claude/projects/<encoded folder>/<session>.jsonl` | every session: `cwd`, `gitBranch`, `timestamp`, `sessionId`, the model and token usage of each reply, tool calls, an AI-written title (`ai-title`) | location and 30-day retention documented (`code.claude.com/docs/en/sessions`); the line format is internal and "changes between versions" |
| `<project>/.claude/scheduled_tasks.json` and `CronCreate` calls in transcripts | session crons, which expire after 7 days | documented (`scheduled-tasks`) |
| `~/.claude/jobs/*/state.json`, `~/.claude.json`, `~/.claude/stats-cache.json` | background jobs and their state; per-project last-session cost; daily activity | undocumented; read only if present, never relied on |
| the found repositories | GitHub Actions `schedule:`, `vercel.json` `crons`, Kubernetes `CronJob`, `render.yaml` cron services, Terraform `schedule_expression`; remotes and commit authors | stable |
| crontab, systemd timers, launchd agents, Windows Task Scheduler | the computer's own schedules | stable |
| GitHub, through phase 7's GitHub connector | the last run of each scheduled workflow, deployment statuses (Vercel, Netlify and Render report theirs to GitHub), pull requests | stable |

Not available, by design or by rule:
- **Cloud routines and claude.ai chats.** No public API lists them, and the Consumer Terms forbid scraping or automating claude.ai. A routine is entered by hand. Importing claude.ai's data export is later, not this phase.
- **The Claude sign-in.** Catervas never reads `~/.claude/.credentials.json` or the keychain entry of Claude Code, and never calls the API with them (`legal-and-compliance`). The reader refuses those paths and a test holds it.
- **Org analytics APIs** need an organisation's admin key and do not cover individual Pro or Max accounts. Not used.

Two consequences shape the design. The transcript format can change with any Claude Code release, so the reader takes only the few fields above, skips any line it does not understand, and has a fixture test per Claude Code version it was checked against; a broken read shows "Couldn't read N sessions from this Claude Code version" instead of failing. Transcripts are deleted after 30 days by default (`cleanupPeriodDays`), so Catervas keeps its own index, updated while `catervas serve` runs, and offers to raise the setting.

A probe on the founder's computer (2026-10-12) folded 36 transcript folders into five projects by git root and worktree, found three scheduled GitHub Actions and Vercel crons in one of them, found no crontab, and showed two traps: a deleted worktree has no git link left, so it is matched by its path under a known project; and Catervas's own agents' sessions (`.catervas/local/worktrees/CTV-*`, the live tests' `/tmp/farik-git-live-*`) appear in the same store.

## Words

- **Found project:** a folder the user worked in with Claude Code, after folding worktrees and subfolders into it.
- **Found job:** a schedule, cron, routine, session cron or background job, tied to a found project when its path or repository says so.
- **Proposal:** found projects grouped as teams, before the owner approves them.
- **Watched:** a found project with no team, whose jobs and sessions still get reminders and analytics. Nothing runs and nothing is spent.

## 1. Several teams on one computer

The teams share only the user, the computer, the AI account and an optional spending cap. Everything else is each team's own, as it is today: its project (one repository, or a phase 9 project folder), its `.catervas/` and event log (every event already carries `team_id` and `project_id`), its run lock, budget, ask-or-auto setting, channel, chats, memory and connector keys (already kept per project).

- **One `catervas serve` drives several projects.** `state.json` gains `projects`, the list of every team's project on this computer, beside `last_project`. Serve opens a drive for each project that is not paused, each holding its own `run.lock`, with its own orchestrator and idle tick. ADR 0053's switch becomes adding and removing teams; "Change project" stays for moving one team.
- **The AI account is shared fairly.** A machine-wide limit, "Agents working at once", caps concurrent agent sessions across all teams; a waiting session is started round-robin by team. It is off by default, so one team works exactly as before; the owner sets it when teams compete for the account. When the provider's limit is hit, the existing sleep until it resets applies to every team on that account.
- **An optional spending cap for the computer**, beside each team's own budget (ADR 0015 keeps limits optional). At the cap every team pauses as a team does at its own.
- **The web app** gains a team switcher at the top of the rail and an **All teams** page: a card per team (running agents, waiting on you, spend today and this month, next scheduled job), the watched projects, and the day's reminders. Each team's pages are as today, under `/t/<team-slug>/`; an old address goes to the last team viewed. "Waiting on you" on All teams gathers every team's.
- **Unrelated teams never share** memory, tasks, documents or the channel. Related work belongs in one team (a multi-repo project), not in teams that coordinate. A read-only "related to" note between two teams is the most this phase adds.

## 2. Reading the history

- **Consent first.** "Find my projects" (on All teams, and offered at the first run when `~/.claude/projects` exists) opens a screen that lists what is read (the transcript store, background jobs, the found repositories' schedule files, the computer's schedulers), what is kept (folders, times, token counts, titles, job schedules; never the conversation's text), and that nothing leaves the computer unless the owner asks for AI labels. Settings › This computer has "Pause finding", an ignore list, and "Forget everything", which deletes the index.
- **The index** is a machine-level SQLite file, `<state>/machine.db` (`~/.config/catervas/`), apart from every project's log. It keeps, per session: id, found project, `cwd`, first and last time, tokens by model, title, branch, how the session started (`entrypoint`), whether a Catervas agent ran it, and the signals the reminder needs. Per transcript file it keeps the byte offset read, so a rescan reads only what was appended.
- **When it runs:** at consent, then daily and on "Scan now" while serve runs.
- **Folding sessions into found projects**, in order: the git common directory's worktree root (so every worktree folds into its repository); else the longest known project root that contains the folder (deleted worktrees); else a folder that no longer exists is kept under "No longer on this computer", hidden by default; else a folder with no git is its own found project, its subfolders folded in. The home folder itself is not a project.
- **Catervas's own sessions** are recognised by the session ids in each team's log and by paths under `.catervas/local/`, and are counted as that team's agent activity, never as a found project.
- **Yours or explored.** A repository with a commit by the user's git email is the user's; one without (a clone of someone else's project) is "explored" and starts ignored in the proposal.
- **Longer history.** If `cleanupPeriodDays` is unset or under 90, a notice offers to set it to 365 in `~/.claude/settings.json`, showing the change and keeping a backup. Catervas never changes it without the click.

## 3. Finding the jobs

- **In each found repository:** GitHub Actions workflows with `on.schedule`, `vercel.json` `crons`, Kubernetes `CronJob` manifests, `render.yaml` services of type cron, Terraform `schedule_expression`. Each job keeps its name, schedule, file and repository.
- **On the computer:** `crontab -l`, systemd timers (user and system), launchd agents in `~/Library/LaunchAgents`, Windows Task Scheduler (`schtasks /query`). A job is tied to a found project when its command names a path inside it; otherwise it is listed under "This computer".
- **From Claude Code:** session crons from `CronCreate` calls and `scheduled_tasks.json`, each with its 7-day expiry; background jobs and their state; routines entered by hand (name, schedule, project).
- **Their status**, where the team has GitHub connected (phase 7 step 07c): the last run of each scheduled workflow, the latest deployment status on the default branch, and whether GitHub has disabled the workflow (GitHub stops scheduled workflows in a repository with no activity for 60 days). Without GitHub, a job shows its schedule and "Connect GitHub to see its runs".

## 4. Relationships and the proposal

- **Signals between two found projects**, scored by pure functions in `catervas-core`:
  - strong: one session worked in both; one references the other (a path dependency in `package.json`, `Cargo.toml` or `go.mod`, a compose `build` context, a submodule, the other's remote in its files);
  - medium: same remote owner and active on the same days;
  - weak: similar names; activity at the same times.
- **Grouping** is connected components over the pairs above a threshold. It is deterministic and tested on fixtures, the founder's probe among them.
- **AI labels**, optional: one headless run of the user's own `claude` (the adapter Catervas already uses) per proposal, given for each group only folder names, the first 40 lines of each README, the titles of its last 20 sessions and its jobs' names, after a secret scrub in `catervas-core` (key patterns, `.env` lines, tokens in URLs). It returns a name, one line of purpose, a suggested team (a built-in template, ADR 0055) and why the projects belong together. The estimated cost is shown before the run; "Skip" keeps folder names.
- **The review board:** each proposed team is a card listing its projects, jobs and the reason they are grouped. The owner merges, splits, renames, moves a project between cards, marks a project watched or ignored, picks the team template, then creates. Nothing is created before "Create teams".
- **Creating.** A card with one repository becomes a one-repo project as today (`catervas init` and the chosen template). A card with several repositories becomes a phase 9 project folder with them as members. A team starts paused, its Catervafication offered, not started, so no money is spent until the owner starts it.
- **Proposed: a found project with no git** (a bookkeeping folder, say) is watched by default. The owner may make it a team's member after agreeing to `git init` it, which Catervas does with nothing else written. Business workspaces, phase 17, are not pulled forward: work without git needs their step 02. (The brainstorm first offered pulling phase 17 step 01 forward; once phase 9's project folder covered several repositories, only folders without git were left, and this is the smaller answer.)

## 5. The daily reminder

- **When and where:** once a day at a time the owner picks (default 9:00 local), as a list on All teams and on each team's Today, and as a browser notification. The rules are pure functions in `catervas-core` over the index.
- **Forgotten sessions** (last activity between 1 and 14 days ago): its todo list still has open items; its folder has uncommitted changes or unpushed commits; its branch has a draft pull request with no activity for 3 days; its background job waits for input.
- **Session crons** that expire within a day, or expired in the last week while recurring.
- **Schedules and deployments:** a scheduled run failed; a schedule has had no run for twice its interval; GitHub disabled a scheduled workflow; the latest deployment failed.
- **Catervas teams:** what waits on the owner and escalations that have aged, as already counted.
- **Today's runs**, collapsed: jobs due today.
- **On each item:** Snooze (a day, a week), Dismiss (until it changes), Copy resume command (`cd <folder> && claude --resume <id>`), Give to the team (files a request in the project's team, prefilled from the session's title and state, for its Product Manager to triage), Open on GitHub.
- **Browser notifications** are phase 14 step 03's first slice, pulled forward: permission asked on All teams, one notification for the digest. They show only while a Catervas tab is open; otherwise the digest waits on All teams. Phase 14 step 03 adds the other notifications and quiet hours on this. **Open:** a notification from serve itself through the operating system when no tab is open.

## 6. Analytics

- **Usage per team:** tokens and estimated cost by day and model, split between the owner's own Claude Code sessions in the team's projects and the team's Catervas agents (already recorded in its log). Costs use the price table in `catervas-core` (`pricing.rs`); on a subscription they are labelled "what this would cost on the API".
- **Workflow:** sessions per day, active hours (replies less than five minutes apart), commits by the owner and by the team, pull requests, tools and skills used most, and, for Catervas work, task cycle time and throughput.
- **Blockers:** aged escalations, waiting on you, failing or silent schedules, failed deployments, forgotten sessions, permission denials, tasks blocked longer than three days.
- On All teams for every team side by side, and in each team for itself. Phase 14 step 02's audit viewer stays the deep, per-team view of the log.

## Out of scope

Reading claude.ai chats or cloud routines automatically; importing claude.ai's data export; Claude Code hooks or an OpenTelemetry receiver (the daily rescan covers this phase; either may replace it later for live updates); coordination between teams beyond a read-only note; sharing memory between teams; status of Vercel crons or deployments that do not report to GitHub; work in a folder without git; running teams on more than one computer.

## Steps

| Step | Name | Delivers |
|---|---|---|
| 01 | Several teams in one serve | `state.json`'s `projects`; serve driving several projects, each with its lock and orchestrator; adding and removing a team; "Agents working at once" and the round-robin start; the shared limit sleep; the computer's spending cap; ADR 0053's switch kept for moving one team |
| 01b | Several teams in the web app | the team switcher, `/t/<team-slug>/` addresses, All teams with its cards and gathered "Waiting on you"; mocked up first |
| 02 | Reading Claude Code history | consent screen, `machine.db`, the tolerant reader with its version fixtures and refused paths, incremental daily scan, folding into found projects, Catervas's own sessions, yours or explored, the retention offer |
| 03 | Finding the jobs | repository schedule files, the computer's schedulers, session crons, background jobs, routines by hand; scheduled runs, deployments and disabled workflows through the GitHub connector |
| 04 | Relationships and the proposal | the signals and grouping in `catervas-core`, the secret scrub, AI labels with their cost, the review board, creating one-repo projects and project folders, watched and ignored; mocked up first |
| 05 | The daily reminder | the rules in `catervas-core`, the digest on All teams and Today, the browser notification, snooze, dismiss, resume and give to the team; mocked up first |
| 06 | Analytics | usage per team, workflow, blockers, on All teams and per team; mocked up first |
| 07 | The founder's live check | on the founder's computer: find the projects, approve the teams, run two unrelated teams at once, receive and act on a reminder; `docs/milestones/many-teams.md`, signed off |

## Testing

Rust: the reader on recorded transcript fixtures of each checked Claude Code version, unknown lines skipped, refused paths; folding (worktree, deleted worktree, subfolder, no git, home, `/tmp`, Catervas's own); the job parsers on sample files and scheduler output; the grouping on fixtures with known answers; the scrub; the reminder rules with an injected clock; serve driving two projects with separate locks and logs, the round-robin start and the shared sleep; the computer's cap pausing every team. Web: the switcher and addresses; All teams; the consent screen; the review board's merge, split, move, watch, ignore and create; the reminder's actions; the analytics pages, each against a fake daemon. Live: step 07.

Docs: `docs/SPEC.md` 1 (the non-goal "One human, one team" becomes one human, several teams), 3, 4, 5.5, 8.4 and 8.5 change with the steps that build each part; ADR 0059.
