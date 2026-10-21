# Autopilot, plan 5: the record

Status: ready
Branch: `feat/autopilot`, after plans 1 to 3, with plan 4 merged in
Spec and decisions: ADR 0082 decisions 1 to 12

No test in this plan: the spec and the plans are prose (hard rule 8 is the check). Before writing, read the top of `docs/SPEC.md` again: if `main` has taken 0.138 since this plan was written, use the next free number and say so in the commit's body.

## Task 21: the spec says it

Files: `docs/SPEC.md`
Writes revision 0.138 (2026-10-20, ADR 0082; standalone plan `autopilot`) at the end of the revision paragraph, after 0.137, and changes, each in the section that holds the rule now:
- 5.2: the seven rows the Product Manager takes under gate `Autopilot` (the human's rows but `ready -> refining`, which it already holds), and that the gate opens only while autopilot is on, for a `blocked` or `escalated` task, when the human's own gate for that move opens; the table is 40 rows. The Assignment gate: under autopilot an agent with no session running may be assigned a `ready` task of its role past `wip_limit_per_agent` (not past a limit of zero).
- 5.7: under autopilot no question and no escalation reaches the owner while the team has a Product Manager or is solo: `catervas_ask_team` and its `team.question_asked` / `team.question_answered`, answered in a `decide` session of the Product Manager (`product`) or the Architect (`technical`); every escalation, and a block older than an hour, is decided with `catervas_decide` among the owner's choices for its reason (the list of plan 1, `carry_on` included; how the app opens is flagged, being a Settings change), at most three `more_tries` or `more_room` and three `reopen_integration` per task; `flag` and `task.flagged` (and Catervas's own flag of a task whose agent kept calling a paid tool, 8.6), a flagged task getting no session while autopilot is on; three decide sessions without a decision flag the wait as Catervas; rule 4 and the aged rule leave the team's waits alone; turning autopilot off puts every wait back on the owner, and the owner may still answer a team question.
- 5.4 and 5.16: under autopilot the Product Manager approves an epic's or a high-risk contract (`approve_plan`) and accepts a result that waits on the owner, a `human` criterion's included (`accept_result`), each recorded as `human.accepted` with `accepted_by` its id and its reason as the message.
- 5.12: the switch, held in this computer's log (`autopilot.changed`, `autopilot.set`), never in `team.yaml`; refused `daily_limit_required` without a daily limit, and a save that removes the limit refused while it is on; the two presses. Among `policy`'s keys (the paragraph on `wip_limit_per_agent` and `escalation_age_hours`): `paid_calls_per_hour`, 1 to 1000, 30 when left out, set on Settings → Rules as "Paid tool calls per agent per hour" (decision 12).
- 5.14: under autopilot Catervas merges as with `merge_when_checks_pass` and the merge permission (ADR 0073), without either; a failed check is never merged over and is an integration escalation the Product Manager decides (`reopen_integration`), a pending one is waited for, and a repository with no checks merges once Catervas's Definition of Done has passed; a folder change whose integration failed is retried by the Product Manager (`catervas_decide { folder_change, choice: retry_folder_change }`, at most three times) or flagged.
- 5.18: a stalled goal is flagged on Today under autopilot, not asked; nothing else about goals changes, and the team never files work for an idle agent.
- 6.5: under autopilot a marketing plan that spends, and one that raises the budget, is approved when proposed (`by: autopilot`); the budget's hard stop holds; a post outside the plan already goes out (ADR 0061).
- 6.10: under autopilot the Product Manager decides a paid, unknown-cost or data-out data pipeline (`catervas_decide_data_pipeline`, `by: product_manager`); seller messages drafted under autopilot are sent by Catervas, at most 50 a UTC day (`sent_by: autopilot`), an order's email waiting with its order and a failed one with the owner; purchase orders stay the owner's.
- 8.5: the six new event kinds, `tool.approved` among them, `team.decided`'s optional `folder_change`, `human.accepted` written by the Product Manager under autopilot, the `decide` purpose, the `autopilot` gate, and the optional `by` and `sent_by` fields.
- 8.6: under autopilot a `paid` or `ask` connector call runs with no grant or allowance and is recorded `tool.approved { by: autopilot }` with its whole input; `denied` and plan-marked tools are unchanged; credentials and Catervas's own state are unchanged (decision 9). Such calls are rate limited per agent, counted from `tool.approved` in the log over the past hour (decision 12): refused `rate_limited` at `policy.paid_calls_per_hour` calls of any tool, at a third of it (at least 1) of one tool, and at 3 calls of one tool with the same input (its `input_sha256`), the refusal saying which and when the hour frees; the third such refusal in one session ends the session and flags its task by Catervas (`task.flagged { flagged_by: catervas }`, needs "<agent> kept calling <tool>"), and a flagged task gets no session while autopilot is on. The daily limit stays the hard stop above it.
- 4.2 (where "What waits on you" and Today are described, near line 176): Today's "Flagged by the team", before "Waiting on you", and "Done on its own", the last seven days' autopilot acts with each agent and reason.

- [x] `docs(spec): autopilot, revision 0.138 (ADR 0082)`

## Task 22: phase 11 step 01 is dropped

Files: `docs/plans/project-plan.md`, `docs/plans/phase-11-milestones/step-01-ask-or-auto.md`
- `project-plan.md`: the status line's revision becomes 45, and a revision note is appended after revision 44's: "Revision 45 (2026-10-20; ADR 0082) drops phase 11 step 01, ask or auto: autopilot, built by the standalone plan `autopilot`, is the one switch, and its "Done on its own" reuses the step's approved mockups." The phase 11 heading's name stays; row `01` (near line 660) is kept and its text starts "Dropped by revision 45 (ADR 0082): autopilot replaces it." The phase's "Ends with" line (it opens "the owner chooses, per team, to approve every outward act or to let the team act on its own") loses that clause and starts at "every role's kit but the DevOps Engineer's".
- `step-01-ask-or-auto.md`: its `Status:` line becomes "dropped (ADR 0082, 2026-10-20): autopilot replaces it; its mockups' Done on its own is reused by `docs/plans/standalone/autopilot/04-web.md`". Nothing else in it changes.

- [ ] `docs(plans): drop phase 11 step 01 for autopilot (ADR 0082)`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```
