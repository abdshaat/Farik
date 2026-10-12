# 0059. Several teams on one computer, found from the user's Claude Code history

Date: 2026-10-12
Status: accepted (the founder's decisions of 2026-10-12 in a brainstorm in a Claude Code session, recorded in `docs/design/many-teams.md`). Numbered 0059 because 0057 (multi-repo projects, pull request #44) and 0058 (HTML documents, pull request #47) are taken on open branches.

Amends:
- `docs/SPEC.md` 1's non-goal "One human, one team, for now": one human, several teams, each on its own project.
- ADR 0021 decision 2 and ADR 0053: one `catervas serve` drives every team's project on the computer at once, each with its own run lock and log. ADR 0053's switch stays, for moving one team to another project.
- ADR 0057, and every ADR that names a phase from 10 on by its number: each of those phases moves down by one (Catervas Cloud is phase 11).

Merged step plans and earlier design documents keep the numbers they were written with. In a document written before this ADR, read a phase number from 10 on as one higher.

## Context

Catervas runs one team on one project, chosen by the user. A user who already works with Claude Code has many projects and recurring jobs at once (products, scheduled scrapers, cron jobs, deployments, bookkeeping), some related and some not. The founder asked for Catervas to find them from the user's Claude history, work out which belong together, and set up one or more teams accordingly, so that it runs several teams rather than one; then to remind the user daily of schedules, crons, deployments and forgotten sessions, and to show analytics on workflows, usage per team and blockers.

What can be read was checked against Claude Code's documentation and a real `~/.claude` on 2026-10-12. Claude Code keeps every session as a JSONL transcript under `~/.claude/projects/`, at a documented location, for 30 days by default (`cleanupPeriodDays`), in a line format the documentation calls internal and liable to change between versions. Session crons are documented. No public API lists cloud routines or claude.ai chats; the Consumer Terms forbid scraping claude.ai; the legal page forbids third-party apps from using a Claude sign-in; the organisation analytics APIs need an admin key and do not cover individual accounts. The schedules that matter mostly live in the repositories themselves (GitHub Actions, Vercel crons) and the computer's schedulers.

The options for detection were heuristics alone (cheap, predictable, poor names), an AI model reading the transcripts (costly, unpredictable, most text exposed), or heuristics with AI labels. For related projects, the options were one team per repository with links between teams, or one team over several repositories, which phase 9's project folder already provides.

## Decision

This is phase 10, Many teams, right after phase 9 ("Right after 9"). Catervas Cloud and every later phase move down by one (project plan revision 45).

- **Several teams.** `state.json` lists every team's project on the computer; one `catervas serve` drives each project that is not paused, each with its own `run.lock`, orchestrator, log, budget, ask-or-auto setting, channel, memory and keys. Teams share only the user, the computer, the AI account and an optional spending cap for the computer. A machine-wide "Agents working at once", off by default so one team works as before, caps concurrent agent sessions, started round-robin by team; the provider's limit sleep applies to every team on that account. The web app gains a team switcher, `/t/<team-slug>/` addresses and an All teams page.
- **Related projects are one team**, over several repositories as a phase 9 project folder. Unrelated teams share no memory, tasks, documents or channel; a read-only "related to" note is the most that links them.
- **The history is read on the computer only.** After the owner's consent, Catervas indexes Claude Code's transcripts into a machine-level `machine.db`, keeping metadata and titles, never the conversation's text, reading only the fields it needs and skipping lines it does not understand. It refuses Claude Code's credentials. It counts its own agents' sessions as their team's activity. It offers, never forces, a longer `cleanupPeriodDays`. Routines are entered by hand.
- **Jobs** are found in the repositories' schedule files, the computer's schedulers and Claude Code's session crons and background jobs, with their runs and deployments read through the GitHub connector.
- **Heuristics with AI labels.** Grouping is deterministic scoring in `catervas-core`; the user's own `claude` may name and describe each group from folder names, READMEs, session titles and job names after a secret scrub, its cost shown first; the owner approves every team on a review board. A team starts paused.
- **A daily reminder** on All teams and Today, with a browser notification (phase 14 step 03's first slice, pulled forward), covers forgotten sessions, expiring session crons, failed or silent schedules, failed deployments and what waits on the owner.
- **Analytics** across teams: usage by team, split between the owner's own sessions and the team's agents; workflow; blockers.
- **Proposed, open to the founder's change:** a found folder with no git is watched (reminders and analytics, no team) unless the owner agrees to `git init` it as a member. Business workspaces (phase 17) are not pulled forward.

## Consequences

- Catervas becomes useful to an existing Claude Code user from the first minute: it shows what they already have before they build anything. A user with no Claude Code history sees no change.
- The transcript reader depends on a format Anthropic does not promise to keep. Each Claude Code version it is checked against gets a fixture; a version it cannot read degrades to fewer sessions with a notice, never a failure.
- History older than the user's `cleanupPeriodDays` is gone before Catervas first runs; Catervas's index keeps what it has seen since.
- A machine-level database now exists beside each project's log. "Forget everything" deletes it; no project's log is touched.
- Several teams compete for one AI account's limits; the cap on agents working at once and the round-robin keep one busy team from starving the others.
- Browser notifications show only while a Catervas tab is open until the desktop app (phase 18) or a notification from serve itself.
- Every phase from 10 on moves down by one, and the plan folders `phase-11-milestones` and `phase-13-ecosystem` become `phase-12-milestones` and `phase-14-ecosystem`.
- `docs/SPEC.md` (1, 3, 4, 5.5, 8.4, 8.5) changes with the steps that build each part.
