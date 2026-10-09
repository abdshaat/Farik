# 0049. Phase 7 ends at step 10g; ask or auto, the kit check and the milestones are phase 9

Date: 2026-10-09
Status: accepted (the founder, 2026-10-09, in conversation: "Lets stop this phase after completing step 10g. We have to start the next phase"; asked where phase 7's unfinished steps go, the founder chose "DevOps later, rest after Cloud": a short phase after Farik Cloud takes step 10h, the kit check and the milestone runs, and the DevOps Engineer moves to the Ecosystem phase, so the milestones test the team without it). Numbered 0049 because 0045 and 0046 stay reserved by the DevOps Engineer's step plans (step 11's and step 12d's, ADR 0047), which move with it. Amends ADR 0048 (the phases after Farik Cloud move up by one more), ADR 0029 (the milestone runs are phase 9's), ADR 0041 (ask or auto is phase 9 step 01), ADR 0027 (the DevOps Engineer is built in the Ecosystem phase), and every ADR that names a phase after phase 8 by its number.

## Context

On 2026-10-09 phase 7, Role kits, on `phase/7-role-kits` (pull request #22), stood as follows:
- **Landed and landing-reviewed:** steps 01 to 10f, except 04, 04b, 07 and 07b.
- **Executed on 2026-10-02 and still owing their landing reviews:** 04, 04b, 07 and 07b.
- **Executed and in its landing review:** 10g.
- **Not built:**
  - 10h, ask or auto (ADR 0041), ready;
  - the DevOps Engineer (ADR 0027): 11 and 11b ready, 11c to 11f and 12 to 12e drafts;
  - 13, the kit check, not yet planned;
  - 14, the Milestone 0 and 1 runs (ADR 0029), a draft to be re-planned last.
- **Owed by the founder, live:** the runs of steps 06, 07, 07b, 08, 08b, 08d, 10, 10b and 10b2 (one run), 10d, 10f and 10g. The checks that wait on Farik Cloud (03b's, and 08e to 08g's) were already phase 8 step 07's (ADR 0048).

Since ADR 0048 the order after phase 7 was: 8 Farik Cloud, 9 Engines and providers, 10 Ecosystem, 11 Proof of concept, 12 Web launch, 13 Business workspaces, 14 Desktop, 15 Native mobile, 16 Premium. The founder asked to stop phase 7 after step 10g and start the next phase.

The options were:
- **A new phase 9 after Farik Cloud for all of them.** 10h, the DevOps Engineer, the kit check and the milestone runs; the milestones still test the fully equipped team.
- **Into phase 8, after Farik Cloud's steps.** No renumbering, but one long phase and pull request about two subjects.
- **The DevOps Engineer later, the rest after Farik Cloud.** A short phase 9 takes 10h, the kit check and the milestone runs; the DevOps Engineer moves to the Ecosystem phase. This was the founder's choice.

## Decision

**Phase 7 ends at step 10g.** Its pull request is marked ready once three things hold:
- step 10g has landed;
- the four steps that still owe their landing reviews (04, 04b, 07 and 07b) are reviewed and their fixes committed;
- the full check passes on the final commit.

**Phase 7 merges with its founder's live runs outstanding**, as phase 6 merged without its milestone runs (ADR 0029).
- The outstanding runs join phase 8 step 07, "Phase 7's live checks", which already holds those that wait on Farik Cloud.
- A run that needs no Farik Cloud may be made earlier. Its record, and any fix it needs, go on the phase branch of the day.
- A step whose plan says it "is not done until the run passes" (07, 07b, 10g among them) stays not done in that sense until then. It has landed by the workflow's measure, landing review included.

**A new phase 9, Ask or auto and the milestones, comes right after phase 8.** Its steps:
- 01, ask or auto: phase 7 step 10h until now, its plan unchanged but for its pointers.
- 02, the kit check: phase 7 step 13 until now, not yet planned. It checks every role's kit except the DevOps Engineer's. Its Google Ads part, which ADR 0048 gave phase 8 step 07, is its own again, since it now runs after Farik Cloud.
- 03, the Milestone 0 and 1 runs: phase 7 step 14 until now, re-planned last, as ADR 0029 says.

**The DevOps Engineer moves to the Ecosystem phase.** Steps 11 to 12e, the role, its deploy tasks, watching production, incidents, its kit and its platforms, follow the Ecosystem phase's own steps, in their order. ADR 0045 and 0046 stay reserved for them. Steps 11 and 11b were found ready against phase 7's code; they take a readiness review again when their phase starts. Its kit is checked there, on each engine and provider that Engines and providers has added by then.

**The phases after phase 9 move up by one:**
- 10 Engines and providers;
- 11 Ecosystem, with the DevOps Engineer;
- 12 Proof of concept;
- 13 Web launch;
- 14 Business workspaces;
- 15 Desktop, the Finance Specialist's receipts intake still at its step 02;
- 16 Native mobile;
- 17 Premium.

The ADRs, and the spec's revision notes, keep the numbers of their day; each ADR that names a moved phase or step by number carries a dated line pointing here. The renumber changes no behaviour, so `docs/SPEC.md` takes no revision for it, as ADR 0048 says.

## Consequences

Easier:
- **Phase 7 merges** without waiting on the eight steps of the DevOps Engineer.
- **Farik Cloud, phase 8, starts next.**
- **The kit check and the milestone runs come after Farik Cloud.** So they run with GitHub signed in through Farik's GitHub App and with Google Ads, instead of splitting their Google Ads part off.

Harder:
- **Until phase 9 the team works in `ask` alone.** Every outward act waits for the founder, which was already the default (ADR 0041). The spec's and the step plans' "under `auto` (step 10h)" now point to phase 9 step 01.
- **The milestone runs and the kit check do not test a DevOps Engineer.** The Engines and providers phase re-checks every kit except the DevOps Engineer's, which arrives after it in Ecosystem. Ecosystem must check that kit on each engine and provider itself. The proof of concept (phase 12) comes after Ecosystem, so its brainstorm may still include the role.
- **Phase 7's pull request merges with founder's live runs open**, so it lands with steps whose own plans call them not done until those runs pass.
- **Phase 8's steps 01 to 04 are planned in `farik-ops`** (ADR 0047). On 2026-10-09 that repository was not reachable from this repository's sessions, so phase 8 can begin here only once it exists and is shared.
- **Every phase after phase 8 is renumbered again**, and so are phase 7's steps 10h, 11 to 12e, 13 and 14. The project plan, the spec's forward pointers, the design documents, the step plans, `CLAUDE.md` and a few code comments change with them.
