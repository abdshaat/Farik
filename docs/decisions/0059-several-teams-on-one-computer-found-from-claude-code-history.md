# 0059. Several teams on one computer, found from the user's Claude history

Date: 2026-10-12
Status: accepted (the founder's decisions of 2026-10-12 in a brainstorm in a Claude Code session, and the founder's second set of requirements the same day, recorded in `docs/design/many-teams.md`). Numbered 0059 because 0057 (multi-repo projects, pull request #44) and 0058 (HTML documents, pull request #47) are taken on open branches.

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
- **Related projects are one team**, over several repositories as a phase 9 project folder. Unrelated teams share no memory, tasks, documents or channel.
- **Proof before a link** ("I do not want to mistakenly bundle 2 unrelated projects together"). Catervas decides alone only that two folders are the same project (one repository's worktrees, or the same normalised remote). Two different projects are linked only when Catervas finds a proof it can check in committed files (a path dependency, a submodule, a package one declares and the other uses, a deploy file that builds the other, a workflow that checks out the other, a domain one deploys to named in the other's config) and the owner answers Yes to that pair on a screen that shows the proof. Behavioural hints (a session that wrote in both, the same owner, the same days, similar names, Claude's opinion) never link on their own. Groups form only through confirmed pairs and are confirmed again as a whole; a No is kept and not asked again without new proof. A fixture corpus of tempting unrelated pairs must yield no proof.
- **The journey first** ("User experience and user journey is a huge concern"). The phase's first step designs every screen of its journeys, their empty and error states included, as clickable mockups the founder approves before any code, under principles written in the design (show before asking, one decision per screen with a safe answer chosen, a plain why on every claim, nothing behind the owner's back, leave and come back, undo everywhere). The phase ends with a usability test by three people other than the founder.
- **The history is read on the computer only.** After the owner's consent, Catervas indexes Claude Code's transcripts into a machine-level `machine.db`, keeping metadata and titles, never the conversation's text, reading only the fields it needs and skipping lines it does not understand. It refuses Claude Code's credentials. It counts its own agents' sessions as their team's activity. It offers, never forces, a longer `cleanupPeriodDays`. Routines are entered by hand.
- **People who use Claude only on the web** bring their history themselves: claude.ai's data export, which they request and drop into Catervas, read on the computer for metadata only; their GitHub repositories, including Claude Code on the web's `claude/…` branches and pull requests; sessions they pull with `claude --teleport`; and projects added by hand. A repository not on the computer can be watched or copied to it on the owner's click.
- **One record per thing.** Sessions, replies, conversations, repositories, folders and jobs are each keyed (session id, message id, uuid, normalised remote, real path, file and name), kept once with the sources they came from, and usage is counted once per reply, so a resumed session, a fork, a teleported copy, an overlapping export or a folder and its GitHub repository never count twice.
- **Rejected: reading claude.ai with the subscription's token**, as the founder first asked for web-only users. The `CLAUDE_CODE_OAUTH_TOKEN` "can only make model requests" (Claude Code's authentication page), no documented API returns a consumer's conversations, Projects or memory, Anthropic does not permit third-party developers "to route requests through Free, Pro, or Max plan credentials on behalf of their users" (Claude Code's legal and compliance page), and the Consumer Terms forbid automated access except with an API key. Catervas uses the token only to run the unmodified `claude` program.
- **Jobs** are found in the repositories' schedule files, the computer's schedulers and Claude Code's session crons and background jobs, with their runs and deployments read through the GitHub connector.
- **Heuristics with AI labels.** Proofs and hints are found by deterministic code in `catervas-core`; the user's own `claude` may name and describe each group from folder names, READMEs, session titles and job names after a secret scrub, its cost shown first, and never makes a link. A team starts paused and can be undone until it starts.
- **A daily reminder** on All teams and Today, with a browser notification (phase 14 step 03's first slice, pulled forward), covers forgotten sessions, expiring session crons, failed or silent schedules, failed deployments and what waits on the owner.
- **Analytics** across teams: usage by team, split between the owner's own sessions and the team's agents; workflow; blockers.
- **Proposed, open to the founder's change:** a found folder with no git is watched (reminders and analytics, no team) unless the owner agrees to `git init` it as a member. Business workspaces (phase 17) are not pulled forward.

## Consequences

- Catervas becomes useful to an existing Claude user from the first minute: it shows what they already have before they build anything. A user with no history sees no change.
- Proof before a link means some projects that do work together (two services that only talk over a URL nobody committed) are not proposed; the owner links them by hand. Missing a link is cheap; a wrong one is not.
- A web-only user's history is only as fresh as their last export, which they must request again by hand; claude.ai conversations count as activity with no tokens.
- The legal page's "may not collect, store, or intermediate Claude.ai credentials or session tokens" may also cover Catervas keeping the subscription's token in the keychain (ADR 0021). It is outside this phase and needs its own legal read and decision.
- The transcript reader depends on a format Anthropic does not promise to keep. Each Claude Code version it is checked against gets a fixture; a version it cannot read degrades to fewer sessions with a notice, never a failure.
- History older than the user's `cleanupPeriodDays` is gone before Catervas first runs; Catervas's index keeps what it has seen since.
- A machine-level database now exists beside each project's log. "Forget everything" deletes it; no project's log is touched.
- Several teams compete for one AI account's limits; the cap on agents working at once and the round-robin keep one busy team from starving the others.
- Browser notifications show only while a Catervas tab is open until the desktop app (phase 18) or a notification from serve itself.
- Every phase from 10 on moves down by one, and the plan folders `phase-11-milestones` and `phase-13-ecosystem` become `phase-12-milestones` and `phase-14-ecosystem`.
- `docs/SPEC.md` (1, 3, 4, 5.5, 8.4, 8.5, 8.6) changes with the steps that build each part.
