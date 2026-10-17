# Catervas

Catervas runs its own small team of AI agents (Product Manager, Scrum Master, Architect, Developer, Marketing Specialist and other roles) against one git repository for one human; they plan, build, review and ship together (README.md; docs/SPEC.md opening). Problem: multi-agent failures are mostly organisational, not model capability: task never well specified, author verifies own work, nobody can reconstruct why a file changed or what it cost (README.md "Why Catervas"; docs/PRODUCT_ANALYSIS.md "The problem"). Fix: a deterministic governor in code, contracts before work, reviewer never the author, bounded sessions, append-only log (README.md "How it works"). "The governance layer is the product" (docs/SPEC.md opening).

## Who it is for

- Non-technical founder or creator, first: sets up a team, approves contracts, accepts work from plain-language summaries alone; never asked for a glob, regex or file format outside an advanced view (docs/SPEC.md §2; ADR 0016).
- Solo builder: reads diffs, wants progress while at day job, price sensitive, own key, cares agents do not wreck the repo unsupervised (docs/SPEC.md §2).
- Small-team lead: 2–5 person company, wants always-on "extra squad" for backlog grooming, routine implementation, release notes; would pay for hosted execution if it saves an afternoon a week and the audit trail is trustworthy (docs/SPEC.md §2).
- Small-business owner, later: non-software business (flipping cars, online resale); team researches, buys via purchase orders the owner approves, keeps books, markets; served from phase 14, after the launch (docs/SPEC.md §2; ADR 0040).
- Enterprise platform teams deliberately deferred (docs/SPEC.md §2).

## What it must do

1. Team of 2–7 agents, each named, with an avatar and a role; at least one Product Manager and one Software Developer (docs/SPEC.md §1 goal 1).
2. Point the team at an existing git repository or start a new project from a one-paragraph brief (docs/SPEC.md §1 goal 2).
3. Every task has a written contract with exit criteria before work starts; nobody accepts their own work; reviewer never the assignee (docs/SPEC.md §1 goal 3; README.md "How it works").
4. Governor in code checks every permission, budget, iteration limit and path allowlist; agents propose, the governor decides (README.md "How it works").
5. Every agent action in an append-only event log; every expense (tokens, wall clock, tool calls) attributable to a task and an agent (docs/SPEC.md §1 goal 4).
6. User talks to any agent directly and reads and joins the team channel (docs/SPEC.md §1 goal 5).
7. Per-agent MCP servers (connectors) and skills (docs/SPEC.md §1 goal 6).
8. Local first, on the user's own key; code never leaves the machine (docs/SPEC.md §1 goal 7; README.md "Local first").
9. Non-technical user sets up and runs a team in the web app with no terminal; first run is a one-question-per-screen wizard; every human gate explains itself in plain language with the diff available (docs/SPEC.md §1 goal 8).
10. Every session has token, time, tool-call and iteration limits; spending limits optional; hitting a limit escalates to the human (README.md "Bounded by default"; ADR 0015).

## Limits

- Hard cap of seven agents per team (docs/SPEC.md §1 non-goals).
- One human, one team; no multi-user (docs/SPEC.md §1 non-goals).
- A human must be reachable for escalations, even if hours pass (docs/SPEC.md §1 non-goals).
- Governor evaluates a tool call in under 5 ms (docs/SPEC.md §10).
- UI responsive with 10,000 events in a project (docs/SPEC.md §10).
- WCAG 2.2 AA on every screen; web app responsive from 360 px (docs/SPEC.md §10).
- English only in the first release (docs/SPEC.md §10).
- Works offline except model calls and network MCP servers (docs/SPEC.md §10).
- First day of the suggested team of six on a full sprint estimated at about $25; an estimate of the defaults, not a cap (docs/SPEC.md §10).

## Not now

- Desktop and phone apps: first launch is web alone; desktop after the web release is tested with real users, native iOS/Android after desktop (docs/SPEC.md §1 non-goals; ADR 0018).
- Pixel-art office: the desktop app's (docs/SPEC.md opening; docs/plans/project-plan.md phase 15).
- Non-software businesses at the first launch; phase 14, after it (docs/SPEC.md §1 non-goals; ADR 0040).
- Providers and engines other than Claude before the Milestone 1 test; phase 10 (docs/SPEC.md §1 non-goals; ADR 0023).
- Teams over seven, multi-user teams, fully autonomous operation (docs/SPEC.md §1 non-goals).
- Enterprise asks: SSO, SOC 2, private model endpoints (docs/SPEC.md §2).
- Paid cloud-hosted workspaces and Slack: phase 17, the last (docs/plans/project-plan.md phase 17).

## How we'll know it works

- Milestone 0 exit: on Catervas's own repo, PM contracts three real issues, one Developer implements, the other verifies, PM accepts, human reviews diffs and event log and agrees each was done as contracted (docs/SPEC.md §11).
- Milestone 1 exit: new user with no help goes from nothing to an accepted task on their own repo within 30 minutes; 5 test users, ≥3 non-technical; run in the browser (docs/SPEC.md §11; docs/plans/project-plan.md phase 9).
- Kit check: each role's kit but the DevOps Engineer's does one real task in the web app, on Claude, recorded in docs/milestones/role-kits.md and signed off (docs/plans/project-plan.md phase 9 step 02).
- Benchmark gate: pre-registered, blind benchmark vs one fully equipped agent; at least as good on quality, clearly better on failure rates and cost control; speed does not block; failure changes the product (ADR 0025; docs/plans/project-plan.md phase 12).
- Metrics: first-pass acceptance rate; escalations per accepted task (should fall); cost per accepted task and the share on PM and review sessions; share of exit criteria verified by command or test; weekly returning projects at four weeks (docs/PRODUCT_ANALYSIS.md "What to measure").
- Desktop gate: founder and ≥3 non-technical testers run the released web app on their own repos; founder signs off docs/milestones/web-release.md (docs/SPEC.md §11 Milestone 3; docs/plans/project-plan.md phase 15 step 01).

## Notes on the sources

README.md "Meet the team" still says five roles with Finance Specialist planned; docs/SPEC.md §1 goal 1 and §6 list seven launch roles (Finance Specialist, UI/UX Designer added) plus the Procurement Specialist. This document follows docs/SPEC.md. docs/PRODUCT_ANALYSIS.md (v0.1) puts solo builders first; ADR 0016 and docs/SPEC.md §2 (rev 0.18) put the non-technical founder first; this document follows the later decision.
