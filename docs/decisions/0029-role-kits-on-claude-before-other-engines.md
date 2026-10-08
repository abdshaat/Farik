# 0029. Build the role kits on Claude before other engines

Date: 2026-10-01
Status: accepted (the founder, in conversation, 2026-10-01)
Amended 2026-10-05 by ADR 0040: a phase, Business workspaces, follows the web launch as phase 12, so the phases after it moved up by one (Desktop 13, Native mobile 14, Premium 15); the numbers below are the old ones.
Amended 2026-10-08 by ADR 0048 (the founder: "Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8"): a phase, Farik Cloud, follows the role kits as phase 8, so the phases after phase 7 moved up by one (Engines and providers 9, Ecosystem 10, Proof of concept 11, Web launch 12, Business workspaces 13, Desktop 14, Native mobile 15, Premium 16); the numbers below are the old ones.

## Context

Phase 6, the web UI, has landed fifteen of its sixteen steps. Its step 16 is the Milestone 0 and 1 runs in the browser: the recorded team sprint and the thirty-minute test with five users. The order after phase 6 was, since ADRs 0020, 0023 and 0025: 7 Engines and providers; 8 Ecosystem, which held the plumbing the kits need (MCP per agent with credentials in the OS keychain, skills per agent) and the Finance Specialist; 9 Role kits; 10 Proof of concept; 11 Web launch; 12 Desktop; 13 Native mobile; 14 Premium.

On 2026-10-01 the founder said, in order:
- "Before testing, complete the role kit phase."
- "Make it work with claude api first then do the role kit. And then we can expand to every other LLM provider."
- "Merge [phase 6] now."

So the milestone runs should test the team the user will get, every role equipped, and they should run once, not before the kits and again after. And the kits come before other engines, built on Claude, the one engine Farik runs on today.

Three facts constrained the order:
- A kit needs its plumbing first: a connector is an MCP server configured per agent, its credential lives in the keychain, its tools are tagged and pinned, and a skill loads into a session at the role level. That plumbing was in the ecosystem phase, after engines.
- The Finance Specialist's kit needs the Finance Specialist, which was also in the ecosystem phase.
- ADR 0025 requires every kit to work on every supported engine and provider. Built before other engines exist, a kit can only be checked on Claude at first.

The options were these:
- **Keep the order; run the milestones in phase 6.** Phase 6 would wait on the founder's runs, and the runs would test a team without kits, then need repeating once the kits land.
- **Kits after engines, milestones after kits.** The runs test the equipped team, but on the old order the kits wait for a whole engines phase and the ecosystem, and phase 6 cannot close until then.
- **Kits next, on Claude, with only the plumbing they need pulled forward; the milestone runs as the kits' last step; engines after.** Phase 6 merges now, the runs test the equipped team once, and the engines phase re-checks every kit. This is the chosen order.

## Decision

Phase 6 merges now, without its milestone runs. Its step 16, the runbook, moves to the end of the role kits phase.

The next phase is Role kits, on Claude. It pulls forward from the ecosystem only what the kits need:
- MCP servers per agent, with credentials in the OS keychain, every tool tagged and its list pinned against drift (step 01);
- skills loaded into sessions (step 02);
- the Finance Specialist role (step 07), because its kit needs the role. The founder named the plumbing; the role comes with it so that the Finance kit has something to equip.

Then it builds every role's kit: the Product Manager's, the Scrum Master's, the Architect's, the Developer's, the Marketing Specialist's, the Finance Specialist's, the UI/UX Designer's, whose built-in Playwright connector moves into the kit format with the loader (step 03), and the DevOps Engineer's with its role (ADR 0027). Then the kit check, on Claude. Then the Milestone 0 and 1 runs, in the browser, with the fully equipped team, on Claude.

The phases after phase 6 are:
- 7 Role kits;
- 8 Engines and providers;
- 9 Ecosystem: the rest, that is, the memory history and decisions view, the audit viewer, notifications, and the premium hooks;
- 10 Proof of concept;
- 11 Web launch;
- 12 Desktop;
- 13 Native mobile;
- 14 Premium.

ADR 0025's rule that every kit works on every supported engine and provider stands. Phase 7 builds and checks the kits on Claude; phase 8 checks every kit again on each engine and provider it adds, running phase 7's kit check on each, and fixes a kit that fails there. ADR 0023's aim, that no connector or skill is built twice, holds because the kits use open standards: a connector is an MCP server and a skill is in the Agent Skills format.

## Consequences

Easier:
- Phase 6 merges now, and its pull request stops waiting on the founder's runs.
- The milestone runs test the team a user will get, every role equipped, and they run once.
- The kits are built on the engine Farik already runs, with the governor's hook already in place, so nothing waits on a second engine.

Harder:
- The runs move further away. Milestone 0 and 1 stay open until the kits are built and checked, so a fault in the governance loop that only a live run shows is found later.
- A kit is built against Claude Code's behaviour first. A connector or a skill that leans on something only Claude Code does will fail phase 8's re-check and has to be changed there; ADR 0023's aim of building nothing twice now rests on the kits keeping to MCP and the Agent Skills format.
- The Finance Specialist comes forward with the plumbing, which is more than the founder named. Phase 7 grows to twelve steps, past what one review comfortably holds.
- The runbook was planned and readiness-reviewed for phase 6's team. It moves as it is and is re-planned for the equipped team when phase 7 is planned, so its readiness review is repeated.

ADRs 0020, 0023 and 0025 carry a one-line amendment pointing here and keep their text. ADR 0027's step references are updated in place. The renumber changes no behaviour, so `docs/SPEC.md` takes no revision for it; its forward pointers to phase numbers are updated, as revision 21's renumber did.
