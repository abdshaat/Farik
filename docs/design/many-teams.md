# Many teams: several teams on one computer, found from the user's Claude history

Status: approved by the founder on 2026-10-12, in a brainstorm in a Claude Code session (ADR 0059), and extended the same day with the founder's second set of requirements (proof before a link, the user journey, people who use Claude only on the web, duplicates) and third (services the owner uses through Claude's connectors, notifications from the operating system, no Anthropic key read by Catervas). It is the design input to phase 10, Many teams, right after phase 9, Multi-repo projects, which merges into `main` first. Its screens are mocked up and approved before they are built (ADR 0032), all of them in step 01. The parts marked **Proposed** are the brainstorm's own answers to questions the founder did not rule on; they are open to the founder's change in review.

## Why

A Catervas user who already works with Claude has many things going on at once: products, side projects, scheduled scrapers, cron jobs, deployments, bookkeeping. Today Catervas runs one team on one project, and the user has to tell it which. The founder wants Catervas to find every project and recurring job from the user's Claude history, work out which belong together, and set up one team for each group, so that Catervas runs several teams on one computer rather than one. Then Catervas reminds the user each day of the schedules, crons and deployments that need attention and of the sessions they may have forgotten, and shows analytics on workflows, usage per team and blockers.

## The founder's decisions (2026-10-12, in conversation)

First set:
- **Placement:** "Right after 9". This is phase 10; Catervas Cloud and every later phase move down by one (project plan revision 45).
- **Related projects:** one team with several repositories. Phase 9's project folder (ADR 0057) is that team's home, so a group of related repositories becomes one multi-repo project.
- **Detection:** heuristics plus AI labels. Grouping and scoring are deterministic code; the user's own Claude only names and describes the groups, with its cost shown first; the owner approves on a review board.
- **Reminder:** on Today, plus a browser notification.

Second set:
- **"I do not want to mistakenly bundle 2 unrelated projects together."** Two different projects are linked only on proof Catervas can check and show, and only after the owner confirms (section 4).
- **"User experience and user journey is a huge concern."** The whole journey is designed, mocked up and clicked through before any code (step 01), every screen follows the principles in section 0, and the phase ends with a usability test by people other than the founder (step 09b).
- **People who use Claude only on the web** are served too (section 2b).
- **Duplicates** from more than one source are merged, never counted twice (section 2c).

Third set:
- **"Users may have claude connectors to other bookkeeping services such as Excel or zoho books. That must also be detected."** Catervas finds the services the owner reaches through Claude's connectors, where each is used, and what kind of work it is, so that bookkeeping done through a connector is found as a project like any folder (section 2d).
- **"Yes, serve may send notification through the operating system."** The daily reminder reaches the owner with no Catervas tab open (section 6).
- **"We are not storing or keeping any anthropic related keys with us. Anthropic keys will remain local to the user and will never be read by Catervas."** Nothing in this phase reads or keeps a key, Anthropic's or a connected service's (section 2d, and "A rule outside this phase").

From the request: Catervas reads the user's history on the computer and never sends it to Catervas Cloud; nothing is created without the owner's approval.

## What Catervas can read, and what it may not

Checked against Anthropic's documentation and terms, and against a real `~/.claude`, on 2026-10-12.

| Source | What it gives | Standing |
|---|---|---|
| `~/.claude/projects/<encoded folder>/<session>.jsonl` | every Claude Code session: `cwd`, `gitBranch`, `timestamp`, `sessionId`, each reply's model, token usage and message id, tool calls, an AI-written title (`ai-title`) | location and 30-day retention documented (`code.claude.com/docs/en/sessions`); the line format is internal and "changes between versions" |
| `<project>/.claude/scheduled_tasks.json` and `CronCreate` calls in transcripts | session crons, which expire after 7 days | documented (`scheduled-tasks`) |
| `~/.claude/jobs/*/state.json`, `~/.claude.json`, `~/.claude/stats-cache.json` | background jobs and their state; per-project last-session cost; daily activity | undocumented; read only if present, never relied on |
| claude.ai's data export, which the owner requests and hands to Catervas | the owner's claude.ai conversations and account data | documented as a manual export by email, link valid 24 hours (`support.claude.com` 9450526); its files and fields are not documented |
| GitHub, through phase 7's GitHub connector | the owner's repositories, branches and pull requests from Claude Code on the web, each scheduled workflow's last run, deployment statuses (Vercel, Netlify and Render report theirs to GitHub) | stable |
| the found repositories | GitHub Actions `schedule:`, `vercel.json` `crons`, Kubernetes `CronJob`, `render.yaml` cron services, Terraform `schedule_expression`; remotes, commit authors, manifests | stable |
| crontab, systemd timers, launchd agents, Windows Task Scheduler | the computer's own schedules | stable |
| Claude Code's MCP settings: `mcpServers` in `~/.claude.json` (the user's and each project's), a project's `.mcp.json`, enabled plugins' servers | the names of the services connected to Claude Code, and where | documented as settings (`code.claude.com/docs/en/mcp`); the file also holds each server's keys, which Catervas never reads |
| claude.ai's connectors as Claude Code sees them: `claudeAiMcpEverConnected` in `~/.claude.json`, and tool calls named `mcp__claude_ai_<Name>__<tool>` in transcripts | the claude.ai connectors the owner uses from Claude Code, and in which project | the field is undocumented; tool names in transcripts are what the sessions called |
| Claude Desktop's `claude_desktop_config.json` and its installed extensions | the services connected to the Claude app on this computer | a local settings file; keys in it are never read |

Not available, by design or by rule:
- **Reading claude.ai with the subscription's token.** The founder asked to query a web-only user's data "using the token available", the `CLAUDE_CODE_OAUTH_TOKEN` Catervas holds (ADR 0021). It cannot: that token "can only make model requests, so it can't establish Remote Control sessions or fetch claude.ai connectors" (`code.claude.com/docs/en/authentication`), and no documented API returns a consumer's conversations, Projects or memory (an open request for one, `anthropics/claude-code#100823`, has no answer). It may not either: Anthropic does not permit third-party developers "to route requests through Free, Pro, or Max plan credentials on behalf of their users", and they "may not collect, store, or intermediate Claude.ai credentials or session tokens" (`legal-and-compliance`); the Consumer Terms forbid accessing the Services "through automated or non-human means" except with an API key, and scraping. So Catervas never calls claude.ai, never drives a browser at it, and never uses the token for anything but running the unmodified `claude` program.
- **Cloud routines and Claude Code on the web's session list.** No public API lists them. A routine is entered by hand; a web session is reached through GitHub (its branch and pull request) or by the owner pulling it with `claude --teleport`, after which it is in the local store.
- **Claude Code's own credentials.** The reader refuses `~/.claude/.credentials.json` and Claude Code's keychain entry, and a test holds it.
- **Org analytics and the Compliance API** need an organisation's admin or Enterprise key and do not cover individual accounts. Not used.

Two consequences shape the design. The transcript format can change with any Claude Code release, so the reader takes only the few fields above, skips any line it does not understand, and has a fixture test per Claude Code version it was checked against; a broken read shows "Couldn't read N sessions from this Claude Code version" instead of failing. Transcripts are deleted after 30 days by default (`cleanupPeriodDays`), so Catervas keeps its own index, updated while `catervas serve` runs, and offers to raise the setting.

A probe on the founder's computer (2026-10-12) folded 36 transcript folders into five projects by git root and worktree, found three scheduled GitHub Actions and Vercel crons in one of them, found no crontab, and showed two traps: a deleted worktree has no git link left, so it is matched by its path under a known project; and Catervas's own agents' sessions (`.catervas/local/worktrees/CTV-*`, the live tests' `/tmp/farik-git-live-*`) appear in the same store.

## Words

- **Project** (on screen; "found project" in this document): a folder the user worked in with Claude Code after worktrees and subfolders are folded in, a GitHub repository, or a claude.ai Project from the export.
- **Job:** a schedule, cron, routine, session cron or background job, tied to a project when its path or repository says so.
- **Link:** the owner's confirmed "these two projects work together", with its proof.
- **Proposal:** projects grouped into teams by links, before the owner creates them.
- **Watched:** a project with no team, whose jobs and sessions still get reminders and analytics. Nothing runs and nothing is spent.

## 0. The user journey

The journey is the product here: a wrong group or a confusing screen costs more trust than a missing feature. Every screen of this phase follows these principles, and step 01 designs all of them before any code.

**Principles.**
1. **Show before asking.** The first thing after the scan is what Catervas found, not a form.
2. **One decision per screen**, with a safe answer already chosen: separate, watched, paused.
3. **Every claim says why**, in one plain line, with the proof behind an ⓘ.
4. **Nothing behind the owner's back.** Nothing is created, started, spent, cloned, or written to the owner's folders or Claude settings without a click that says so.
5. **Leave and come back.** The scan runs in the background, progress is kept, and the journey resumes where it stopped, across reloads and days (a claude.ai export arrives by email).
6. **Every step can be undone**, and the undo is where the owner would look for it.
7. **Plain words, few of them.** Short bullets, details behind ⓘ, no "transcript", "worktree" or "git" in the main text; the brand's muted colours, one colour per job.
8. **Fast first value.** Counts appear while the scan runs; the first list shows within a minute on a typical history.
9. **Quiet by default.** Explored, old and gone projects are tucked away; new finds later are a line on All teams, never a pop-up.

**Journey A: a new user with Claude Code history** (the main one).
1. The first run's computer check finds Claude Code history: "Catervas can look through your Claude Code history to find your projects. Nothing leaves this computer." **Look for my projects** · **Skip** · ⓘ what is read and kept.
2. Scanning: counts rise ("7 projects · 4 scheduled jobs · 312 sessions"). **Continue setup meanwhile**; "We'll show you when it's done."
3. **Here's what you work on.** One row per project, newest first, a bookkeeping project found through a connector among them: name, last worked, a jobs badge, the services it uses, and a choice: **Team**, **Watch** or **Ignore**. Chosen for the owner: Team for their own projects worked on in the last 30 days, Watch for older ones, Ignore for explored clones. Explored, gone and home-folder rows sit in collapsed sections.
4. **Do these work together?** Only for pairs with proof (section 4), one pair per screen: the two projects side by side, the proof in one sentence ("web uses api's code"), ⓘ for the files and lines, **Yes, one team** · **No, keep separate**. Pairs with only hints come last, collapsed, under "Maybe related", each answered No unless the owner opens it.
5. **Your teams.** A card per team: its projects, its jobs, a name, a team template (ADR 0055), why its projects are together. Rename, change template, move a project out. Optional: **Suggest names with Claude** (about $0.02, shown before it runs).
6. **Create 3 teams.** They are created paused. All teams opens with each new team's next step: **Start** or **Document it first** (the Catervafication, phase 8). An **Undo** stays on each new team until it starts.
7. The next morning: a notification from the computer, "3 things need you", opens the reminders (section 6), with no Catervas tab needed.

**Journey B: an existing Catervas user.** All teams shows "Find your other projects". The same steps follow; the project already driven shows as **Already a team**, and a link to it adds the other project to that team as a member (phase 9).

**Journey C: someone who uses Claude only on the web.** The computer check finds no history: "Where do you use Claude?" **Claude Code on this computer** · **claude.ai or the Claude app** · **I'm new to Claude**. For claude.ai, two optional cards, each one screen:
- **Bring your claude.ai history.** Numbered steps: open claude.ai, Settings, Privacy, Export data; wait for the email; download it within 24 hours; drop the file here. The card stays on All teams ("Waiting for your claude.ai export") until the file is dropped or the card dismissed.
- **Connect GitHub to find your repositories** (phase 7's connector; a single click once Catervas Cloud runs, phase 11).
- **Which of these do you use with Claude?** The catalog's services as tiles (Zoho Books, Excel, QuickBooks, Stripe…), each one tap; a bookkeeping service chosen becomes a project of its own.
Then steps 3 to 7 of journey A. A repository not on this computer offers **Watch** (its schedules and deployments still come through GitHub) or **Copy to this computer** (a clone the owner confirms, into a folder they choose), after which it can be a team. A claude.ai Project shows as a project of its own, watched; it joins a team only by a link with proof.

**Journey D: no history anywhere.** "I'm new to Claude", or Skip, leads to today's setup of one team, unchanged.

**Journey E: coming back.** A later scan's new projects are a line on All teams ("2 new projects found"); a new proof between two projects is asked once, the same way as step 4.

**Journey F: fixing a mistake.**
- In a team, Settings, Repositories, **This doesn't belong here** removes a member (phase 9's remove) and offers to make it its own team or watch it. The link is recorded as rejected.
- On a project, **Not mine** ignores it.
- **Forget everything** (Settings, This computer) deletes the index and every answer.

**Empty and error states, each designed in step 01:**
- History in a Claude Code version Catervas cannot read.
- An export that is not claude.ai's, or a link that expired.
- GitHub not connected.
- A project folder that moved.
- A scan stopped halfway.
- Nothing found.

**How the journey is judged**, in steps 09 and 09b:
- Time from **Look for my projects** to teams created.
- How many questions were asked.
- How often the owner says No to a proof (the aim is never).
- How often a step was undone.
- Whether each person can say, unprompted, why two projects ended up in one team.

## 1. Several teams on one computer

The teams share only the user, the computer, the AI account and an optional spending cap. Everything else is each team's own, as it is today: its project (one repository, or a phase 9 project folder), its `.catervas/` and event log (every event already carries `team_id` and `project_id`), its run lock, budget, ask-or-auto setting, channel, chats, memory and connector keys (already kept per project).

- **One `catervas serve` drives several projects.** `state.json` gains `projects`, the list of every team's project on this computer, beside `last_project`. Serve opens a drive for each project that is not paused, each holding its own `run.lock`, with its own orchestrator and idle tick. ADR 0053's switch becomes adding and removing teams; "Change project" stays for moving one team.
- **The AI account is shared fairly.** A machine-wide limit, "Agents working at once", caps concurrent agent sessions across all teams; a waiting session is started round-robin by team. It is off by default, so one team works exactly as before; the owner sets it when teams compete for the account. When the provider's limit is hit, the existing sleep until it resets applies to every team on that account.
- **An optional spending cap for the computer**, beside each team's own budget (ADR 0015 keeps limits optional). At the cap every team pauses as a team does at its own.
- **The web app** gains a team switcher at the top of the rail and an **All teams** page: a card per team (running agents, waiting on you, spend today and this month, next scheduled job), the watched projects, and the day's reminders. Each team's pages are as today, under `/t/<team-slug>/`; an old address goes to the last team viewed. "Waiting on you" on All teams gathers every team's.
- **Unrelated teams never share** memory, tasks, documents or the channel. Related work belongs in one team (a multi-repo project), not in teams that coordinate.

## 2. Reading the Claude Code history

- **Consent first** (journey A, step 1). The ⓘ lists what is read (the transcript store, background jobs, the found repositories' files, the computer's schedulers), what is kept (folders, times, token counts, titles, job schedules; never the conversation's text), and that nothing leaves the computer unless the owner asks for names from Claude. Settings, This computer has **Pause finding**, an ignore list, and **Forget everything**.
- **The index** is a machine-level SQLite file, `<state>/machine.db` (`~/.config/catervas/`), apart from every project's log. It keeps, per session: id, project, `cwd`, first and last time, tokens by model, title, branch, how it started (`entrypoint`), whether a Catervas agent ran it, its sources, and the signals the reminder needs. Per transcript file it keeps the byte offset read, so a rescan reads only what was appended.
- **When it runs:** at consent, then daily and on **Scan now** while serve runs.
- **Folding sessions into projects**, in order: the git common directory's worktree root (every worktree folds into its repository); else the longest known project root that contains the folder (deleted worktrees); else a folder that no longer exists goes under "No longer on this computer", hidden by default; else a folder with no git is its own project, its subfolders folded in. The home folder itself is not a project.
- **Catervas's own sessions** are recognised by the session ids in each team's log and by paths under `.catervas/local/`, and are counted as that team's agent activity, never as a project.
- **Yours or explored.** A repository with a commit by the owner's git email is theirs; one without is "explored" and starts ignored.
- **Longer history.** If `cleanupPeriodDays` is unset or under 90, a notice offers to set it to 365 in `~/.claude/settings.json`, showing the change and keeping a backup. Never without the click.
- **Settled in step 02's plan:**
  - The temporary folders (`/`, `/tmp`, `/var/tmp`, and `TMPDIR` when set) are not projects, as home is not, and a git root that holds one is ignored as if there were no git.
  - A folder with no git folds into the outermost session folder with no git that contains it, and only roots with git are known roots for a folder that still exists, so placement never depends on the order of earlier scans. A deleted folder may fall under any known root.
  - A subagent's replies count under its parent session.
  - A reply's counters are the largest seen, and the reply belongs to the session that started earliest.

## 2b. People who use Claude only on the web

Catervas still needs the `claude` program to run a team (the first run installs and signs it in, as today), but such a person has no history on the computer. Their routes, all started by them:
- **claude.ai's export.** Journey C guides the request; the owner drops the zip; Catervas reads it on the computer with the same tolerance as the transcripts (its files are undocumented, so a fixture is kept for each export shape seen, the founder's own first) and keeps the same metadata only: each conversation's title, dates and message count, and each claude.ai Project's name, description and file names when the export has them. A newer export updates the earlier one (section 2c).
- **GitHub.** The owner's repositories with recent pushes, their branches named `claude/…` and pull requests opened from Claude Code on the web, and their scheduled workflows and deployments. A repository not on the computer can be watched, or copied to the computer on the owner's click.
- **Teleported sessions.** A Claude Code on the web session the owner pulls with `claude --teleport` lands in the local store and is read like any other.
- **By hand.** A project or routine can be added with a name, a folder or repository, and a schedule.

What stays out: conversations' text, anything read from claude.ai by Catervas itself, and any use of the subscription's token beyond the `claude` program.

## 2c. One record per thing

The same session, project or job can reach Catervas more than once: from the local store and a teleported copy, from two exports, from a folder and its GitHub repository, and inside the local store itself, where a resumed or forked session's file repeats earlier messages. Each is kept once.

- **Keys:**

  | Thing | Key |
  |---|---|
  | Claude Code session | its session id |
  | Reply | its message id, else its API request id |
  | claude.ai conversation or Project | its uuid |
  | Repository | its normalised remote: host, owner and name, lower case, `.git` dropped, SSH and HTTPS alike |
  | Folder | its real path, links resolved |
  | Job | its repository, file and name, or its scheduler and unit |

- **One row per key**, with the set of sources it came from (local, teleported, export, GitHub, by hand) and when each saw it. Fields that change (a title, an update time) come from the newest source; fields read from content come from the richest (the local transcript over an export).
- **Tokens and costs are counted once per reply**, by its message id, so a resumed session, a fork or a copy never doubles usage.
- **A copy with a new id** (a teleported session may be one) is matched by a fingerprint of its replies (time to the second and a hash of the text), merged only when the match is one-to-one; otherwise both are kept and marked "maybe the same", and their replies are still counted once by fingerprint.
- **Re-imports** update by key. A thing missing from a newer export is marked "not in the latest export", not deleted.
- **A folder and a GitHub repository with the same remote** are one project. A claude.ai Project and a repository are never merged by name; they can only be linked, with proof (section 4).

## 2d. Services used through Claude

A connector is how the owner's Claude reaches another service: Zoho Books, Excel, QuickBooks, Stripe. Bookkeeping often lives there rather than in a folder, so a connector is found like a project.

- **Where they are found:**
  - Claude Code's MCP settings (the user's, each project's, `.mcp.json`, enabled plugins).
  - claude.ai's connectors as Claude Code knows them (`claudeAiMcpEverConnected`).
  - Claude Desktop's settings and extensions.
  - Above all, the tool calls in transcripts (`mcp__<server>__<tool>`), which say which service was used, in which folder, how often and when last.
  - For someone who uses Claude only on the web, claude.ai's connectors cannot be listed, so journey C asks "Which of these do you use with Claude?" from the catalog below.
- **Never a key.** From a settings file the reader takes only a server's name, its kind (local program or remote address), the host of its address and the name of its program; never `env`, `headers`, arguments or anything else. A fixture full of keys is read in a test, and none of them may reach `machine.db`.
- **Not services:** Catervas's own servers (`catervas`, and `farik` from before the rename), Claude Code's internal ones, and plugins' servers that are only listed in a marketplace, not enabled.
- **What kind of work.** A catalog in `catervas-core`, kept as data and updated with releases, matches a server's name or host to a service and its kind:
  - bookkeeping: Zoho Books, QuickBooks, Xero, FreshBooks, Wave, Sage;
  - spreadsheets: Microsoft 365 and Excel, Google Sheets;
  - payments: Stripe, PayPal;
  - selling: Shopify, Amazon Seller Central;
  - marketing: Buffer, Google Ads;
  - code: GitHub, Vercel, Sentry;
  - an unknown server is "Other", shown by its name.
- **Where it belongs.** A service is attached to every project whose sessions called it. Sessions with no project of their own (the home folder, a scratch folder) whose calls go mostly to one kind of service form a project named after the work and the service, such as "Bookkeeping (Zoho Books)". A folder with no git whose sessions read or write spreadsheets (`.xlsx`, `.csv`) also counts as bookkeeping.
- **Its team.** A bookkeeping project proposes the Financial analysis team (ADR 0055) with the Finance Specialist. Since it has no folder, **Make a team** creates a new one, `~/Catervas/<name>/`, as Catervas's first run already does when it starts a new project, so business workspaces (phase 17) are still not needed.
- **Catervas connects separately.** The team's agents never use the owner's Claude connectors or their keys. Where Catervas has a connector for the service, it appears in the team's suggested connections (Today, `suggested-connections`) for the owner to connect in Catervas, keys kept as Catervas keeps every connector key. Where it has none, the service shows "used with Claude; Catervas can't connect to it yet".
- **Never proof of a link.** Two projects that both use Excel are not related on that account, and a shared service is not even a hint. The tempting corpus (section 4) includes such pairs.
- **In the reminder and analytics:** a connector Claude Code has marked as needing to be signed in again (`mcp-needs-auth-cache.json`, undocumented, read only if present) is a reminder, "Zoho Books needs you to sign in again in Claude"; usage per team counts each service's calls.
- **Kept once.** A service is keyed by its catalog entry, else by its name and host, so the same Zoho Books from Claude Code's settings, a claude.ai connector, Claude Desktop and the web-only picker is one record with all four sources.

## 3. Finding the jobs

- **In each found repository:** GitHub Actions workflows with `on.schedule`, `vercel.json` `crons`, Kubernetes `CronJob` manifests, `render.yaml` services of type cron, Terraform `schedule_expression`. Each job keeps its name, schedule, file and repository.
- **On the computer:** `crontab -l`, systemd timers (user and system), launchd agents in `~/Library/LaunchAgents`, Windows Task Scheduler (`schtasks /query`). A job is tied to a project when its command names a path inside it; otherwise it is listed under "This computer".
- **From Claude Code:** session crons from `CronCreate` calls and `scheduled_tasks.json`, each with its 7-day expiry; background jobs and their state; routines entered by hand.
- **Their status**, where GitHub is connected: the last run of each scheduled workflow, the latest deployment status on the default branch, and whether GitHub has disabled the workflow (it stops scheduled workflows in a repository with no activity for 60 days). Without GitHub, a job shows its schedule and "Connect GitHub to see its runs".

## 4. Linking projects: proof before a link

Catervas asks two different questions and never mixes them.

**Is this the same project?** Decided by Catervas alone, on proof only: one git repository's worktrees; two folders, or a folder and a GitHub repository, with the same normalised remote. The project shows "2 folders" and ⓘ why, with **These are different** to separate them.

**Do these two projects work together?** Never decided by Catervas alone. The evidence has two grades.

- **Proof:** a reference from one project to the other that Catervas can check in committed files.
  - A path dependency or workspace member (`"api": "file:../api"`, a Cargo `path`, a `go.mod` `replace` to a local path).
  - A git submodule.
  - A package the other project declares as its own (its manifest's `name`, with its own remote as `repository`) appearing in this one's dependencies.
  - A compose, Kubernetes or Terraform file that builds or deploys the other (a build context path into it, an image its workflow publishes, a module `source` at its remote).
  - A workflow that checks out or dispatches to the other's repository.
  - A domain the other deploys to (its `vercel.json`, `CNAME` or `homepage`) named in this one's committed config.
  - A claude.ai Project's description or file names naming the repository's remote.

  Each proof is kept as both ends: the file, line and quoted text in each project. It is checked again when shown and when the team is created; a proof that no longer holds is dropped.
- **Hints:** behaviour, never enough on its own.
  - A session wrote files in both (reading both does not count).
  - The same remote owner.
  - Activity on the same days.
  - Similar names.
  - Claude's opinion.

  Hints are shown only as context under a proof or, with no proof, in the collapsed "Maybe related" list, answered No unless the owner opens it.

The rules:
- **Separate is the default.** Two different projects are in one team only after the owner answers **Yes** to that pair on a screen that shows its proof in a plain sentence, with the files and lines behind ⓘ.
- **Groups form only through confirmed pairs**, never through hints. Before creating, the whole group is shown ("web, api and infra become one team: web uses api's code; infra deploys both") and confirmed again.
- **Answers are kept.** No is recorded (`link.rejected`) and never asked again unless new proof appears; Yes is recorded (`link.confirmed`) with its proof. Both are in `machine.db`.
- **Claude never makes a link.** The optional names from Claude may explain a proven link in words. A link Claude claims counts only if it cites a file and line that Catervas then checks, and becomes a proposal like any other proof.
- **Mistakes are cheap** (journey F): a team made from linked projects can be split later, and a team created in this flow can be undone until it starts.
- **The bar.** A fixture corpus of related and unrelated pairs, the unrelated ones chosen to tempt:
  - the same owner;
  - the same days;
  - similar names;
  - a session that read both;
  - both depending on the same public package, which neither declares as its own;
  - both using the same service through Claude.

  The test holds that no unrelated pair produces a proof. The live check and the usability test record every proposal and the owner's answer; a wrong proof is a bug fixed before the phase merges.

## 5. The proposal and creating teams

- Teams are the groups of confirmed links, plus every project marked Team on its own. A group of several repositories becomes a phase 9 project folder with them as members; a single repository becomes a one-repo project as today (`catervas init` with the chosen template).
- **Names from Claude**, optional: one headless run of the owner's own `claude` (the adapter Catervas already uses), given for each group only folder names, the first 40 lines of each README, the titles of its last 20 sessions and its jobs' names, after a secret scrub in `catervas-core` (key patterns, `.env` lines, tokens in URLs). It returns a name, one line of purpose and a suggested template. The estimated cost is shown before the run; skipping keeps folder names.
- A team is created paused, its Catervafication offered, not started, so nothing is spent until the owner starts it.
- **Proposed: a project with no git** (a bookkeeping folder, say) is watched by default; the owner may make it a team's member after agreeing to `git init` it, which Catervas does with nothing else written. Business workspaces (phase 17) are not pulled forward, since work without git needs their step 02.

## 6. The daily reminder

- **When and where:** once a day at a time the owner picks (default 9:00 local), as a list on All teams and on each team's Today, and as a browser notification. The rules are pure functions in `catervas-core` over the index.
- **Forgotten sessions** (last activity between 1 and 14 days ago):
  - its todo list still has open items;
  - its folder has uncommitted changes or unpushed commits;
  - its branch has a draft pull request with no activity for 3 days;
  - its background job waits for input.
- **Session crons** that expire within a day, or expired in the last week while recurring.
- **Schedules and deployments:**
  - a scheduled run failed;
  - a schedule has had no run for twice its interval;
  - GitHub disabled a scheduled workflow;
  - the latest deployment failed.
- **Catervas teams:** what waits on the owner and escalations that have aged.
- **Services:** a connector Claude Code says needs signing in again (section 2d).
- **Today's runs**, collapsed.
- **On each item:**
  - Snooze (a day, a week).
  - Dismiss (until it changes).
  - Copy resume command (`cd <folder> && claude --resume <id>`), or for a web session, Open on claude.ai.
  - Give to the team: files a request in the project's team, prefilled from the session's title and state, for its Product Manager to triage.
  - Open on GitHub.
- **Notifications.** One notification per digest, never two:
  - **With a Catervas tab open,** it is a browser notification (permission asked on All teams). This is phase 14 step 03's first slice, pulled forward.
  - **With none open,** serve sends it through the operating system: Linux's notifications, macOS's Notification Center, Windows' notifications. Under WSL it goes to Windows' notifications, where the owner will see it.
  - Clicking it opens All teams where the system allows. Settings, This computer turns either kind off.
  - A machine with no way to notify keeps the digest on All teams.

## 7. Analytics

- **Usage per team:** tokens and estimated cost by day and model, split between the owner's own sessions in the team's projects and the team's Catervas agents, counted once per reply (section 2c). Costs use `catervas-core`'s price table (`pricing.rs`); on a subscription they are labelled "what this would cost on the API". claude.ai conversations count as activity (how many, when), with no tokens, since the export carries none.
- **Workflow:** sessions per day, active hours (replies less than five minutes apart), commits by the owner and by the team, pull requests, tools and skills used most, and, for Catervas work, task cycle time and throughput.
- **Blockers:** aged escalations, waiting on you, failing or silent schedules, failed deployments, forgotten sessions, permission denials, tasks blocked longer than three days.
- On All teams for every team side by side, and in each team for itself. Phase 14 step 02's audit viewer stays the deep, per-team view of the log.

## Out of scope

Reading claude.ai or Claude Code on the web through any token or by automating a browser; using the owner's Claude connectors, or their keys, for Catervas's agents; claude.ai's memory export; Claude Code hooks or an OpenTelemetry receiver (the daily rescan covers this phase; either may replace it later for live updates); coordination between teams beyond a link; sharing memory between teams; status of Vercel crons or deployments that do not report to GitHub; work in a folder without git; running teams on more than one computer.

## A rule outside this phase

The founder ruled on 2026-10-12: "We are not storing or keeping any anthropic related keys with us. Anthropic keys will remain local to the user and will never be read by Catervas." This phase follows it: it reads no key of any kind.

Catervas as built does not follow it yet. The first run takes the owner's subscription token or API key and `crates/runtime/src/credential.rs` keeps it in the OS keychain (service `catervas`), or in `credential.json` in Catervas's state folder on a computer with no keychain, then reads it to pass to `claude` as `CLAUDE_CODE_OAUTH_TOKEN` or `ANTHROPIC_API_KEY` (ADR 0021, ADR 0022). The ruling needs a change there, in its own standalone plan and ADR before this phase: the owner signs in through Claude Code's own sign-in, which keeps the credential in Claude Code's own store; Catervas starts `claude` without passing a key, and the first run's check asks `claude` whether it is signed in rather than holding the answer itself.

## Steps

| Step | Name | Delivers |
|---|---|---|
| 01 | The journey, mocked up | every screen of journeys A to F, the services found and the web-only picker, their empty and error states, the reminder and its notifications, and the analytics, as clickable mockups on the design canvas's page "Many teams", with the words in the house style; clicked through and approved by the founder before any screen is built |
| 02 | Reading the Claude Code history | `machine.db`, the tolerant reader with its version fixtures and refused paths, incremental scan, folding into projects, Catervas's own sessions, yours or explored; one record per session and per reply (section 2c); `catervas discover` prints what was found, `--forget` deletes it; no screen |
| 03 | Finding the jobs | repository schedule files, the computer's schedulers, session crons, background jobs, each tied to its project; printed by `catervas discover`; no screen |
| 03b | Services used through Claude | connectors from Claude Code's settings, claude.ai's connectors as Claude Code knows them, Claude Desktop, and transcripts' tool calls, with no key read; the service catalog; projects formed from connector work ("Bookkeeping (Zoho Books)"); printed by `catervas discover`; no screen |
| 04 | claude.ai's export, and one record across sources | the export read for metadata (`catervas discover --claude-export <file>`), its fixtures and re-import; the keys and merging across sources (section 2c); no screen |
| 05 | Several teams in one serve | after phase 9 is merged into `main`: `state.json`'s `projects`; serve driving several projects, each with its lock and orchestrator; adding and removing a team; "Agents working at once" and the round-robin start; the shared limit sleep; the computer's spending cap; ADR 0053's switch kept for moving one team |
| 05b | Several teams in the web app | the team switcher, `/t/<team-slug>/` addresses, All teams with its cards and gathered "Waiting on you" |
| 06 | Finding projects in the web app | the first run's offer and consent, the scan's progress, "Here's what you work on", the web-only journey (export drop, GitHub repositories and their `claude/…` branches and pull requests, Copy to this computer, the services picker), projects and routines by hand, the retention offer, Pause finding, the ignore list, Forget everything |
| 06b | Linking with proof, and the proposal | same-project folding by remote; proofs and hints in `catervas-core`; the "Do these work together?" screens; `link.confirmed` and `link.rejected`; the group confirmation; the tempting fixture corpus; names from Claude with the secret scrub and its cost; creating paused teams, one-repo, project folder, or a new folder for connector work; watched and ignored; undo; suggested connections |
| 07 | The daily reminder | the rules in `catervas-core`; scheduled runs, deployments and disabled workflows through GitHub; the connector sign-in reminder; the digest on All teams and Today; the browser notification with a tab open and the operating system's without one (WSL included); snooze, dismiss, resume, give to the team |
| 08 | Analytics | usage per team counted once per reply, workflow, blockers, on All teams and per team |
| 09 | The founder's live check | on the founder's computer: find the projects, answer every proof, create the teams, run two unrelated teams at once, act on a reminder; the journey's measures; `docs/milestones/many-teams.md`, signed off |
| 09b | The usability test | three people other than the founder, unassisted and thinking aloud: one with several Claude Code repositories, one who uses Claude only on the web, one with a single project; the journey's measures, every proposal and answer, and what confused them, recorded in `docs/milestones/many-teams.md`; what they found is fixed before the phase merges |

## Testing

Rust:
- **Reading:** the reader on recorded transcript fixtures of each checked Claude Code version, unknown lines skipped, refused paths; the export reader on its fixtures.
- **Duplicates:** a resumed session, a teleported copy, two overlapping exports, SSH and HTTPS remotes, a linked folder.
- **Folding:** worktree, deleted worktree, subfolder, no git, home, `/tmp`, Catervas's own sessions.
- **Jobs:** the job parsers on sample files and scheduler output.
- **Services:** settings files full of keys yielding names only; the catalog's matching; Catervas's own and internal servers left out; a connector-only bookkeeping project formed from tool calls; one service from four sources kept once.
- **Links:** each proof kind found and checked; the tempting corpus yielding no proof; groups formed only through confirmed pairs; a rejected pair not asked again.
- **The rest:** the scrub; the reminder rules with an injected clock; one notification per digest, by the browser or the operating system, behind a fake; serve driving two projects with separate locks and logs, the round-robin start and the shared sleep; the computer's cap pausing every team.

Web, each against a fake daemon:
- every screen of step 01's journeys, the empty and error states among them;
- the switcher and addresses;
- the review screens' answers, group confirmation, create and undo;
- the export drop;
- the reminder's actions;
- the analytics.

Live: steps 09 and 09b.

Docs: `docs/SPEC.md` 1 (the non-goal "One human, one team" becomes one human, several teams), 3, 4, 5.5, 8.4, 8.5 and 8.6 change with the steps that build each part; ADR 0059.
