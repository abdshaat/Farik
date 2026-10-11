# Catervas roadmap

From docs/plans/project-plan.md (rev 41), docs/decisions/, CLAUDE.md "Current state" and git history. No release yet (README.md "Status"; project-plan phase 13).

## Now

- **Finishing the role kits.** Phase 7 ends at step 10g (product safety and eBay sources, landing-reviewed 2026-10-09); as of 2026-10-09, steps 04, 04b, 07, 07b (executed 2026-10-02) still owe landing reviews before the phase merges (ADR 0049; CLAUDE.md; each step plan's Status line).
- **The founder's live runs of the kits.** Live runs of steps 06, 07, 07b, 07c, 08, 08b, 08d, 10, 10b/10b2, 10c, 10d, 10f, 10g are open and join phase 8 step 07 (ADR 0049; project-plan phase 8).
- **Changing the project from the browser.** Merged in commit de0f2b8a (PR #32): Change project from the web app, team carried fresh, project name shown beside "Change" (ADR 0053; docs/plans/standalone/change-project.md Status: done; commit b89e0428).
- **The new name.** Farik renamed Catervas everywhere but docs/decisions/ and docs/plans/ (ADR 0050); task ids are CTV-<n> (ADR 0052).

## Next

- **Catervas Cloud.** Phase 8: free sign-in service (GitHub single click, Google Ads), website, privacy policy, Catervas account; hosting planned separately in the private ops repository (ADR 0044, ADR 0047, ADR 0048; project-plan phase 8).
- **Ask or auto.** Phase 9 step 01: owner chooses `ask` or `auto` for outward acts within limits; purchase orders, spending limits and human gates unchanged (ADR 0041; docs/plans/phase-9-milestones/step-01-ask-or-auto.md).
- **The kit check.** Phase 9 step 02: one real task per role but DevOps in the web app on Claude, recorded in docs/milestones/role-kits.md and signed off; not yet planned (project-plan phase 9).
- **The first real-user tests.** Phase 9 step 03: recorded team sprint in the browser, then Milestone 1's 30-minute test with 5 users, ≥3 non-technical (docs/SPEC.md §11; project-plan phase 9).

## Later

- **Any AI account and agent program.** Phase 10: providers beyond Anthropic and other engines (open-source included) under the same governor; every kit re-checked on each (ADR 0023; project-plan phase 10).
- **The ecosystem.** Phase 11: memory history and decisions view, audit viewer, browser notifications, premium hooks, then the optional DevOps Engineer that deploys, watches production and restores failed deploys (ADR 0027; project-plan phase 11).
- **The benchmark.** Phase 12: pre-registered, blind, published benchmark vs one fully equipped agent; launch gate (ADR 0025; project-plan phase 12).
- **The web launch.** Phase 13: `v0.1.0`, open source, web app alone, binaries for macOS, Windows, Linux (ADR 0018; project-plan phase 13).
- **Business workspaces.** Phase 14: any business, software or not; teams without a Developer (ADR 0040; project-plan phase 14).
- **Desktop.** Phase 15: desktop app with the pixel office, only after the web release check with real users (ADR 0018; project-plan phase 15).
- **Phone apps.** Phase 16: native iOS and Android after desktop; how a phone reaches a running Catervas is open (docs/SPEC.md §12 q8; project-plan phase 16).
- **Premium.** Phase 17: paid cloud-hosted workspaces and agents, and the Slack integration (ADR 0040; project-plan phase 17).
