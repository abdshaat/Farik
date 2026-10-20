# Standalone: autopilot, the team runs the product without the owner

Status: ready
Branch: `feat/autopilot` (its own pull request to `main`); the web plan may run on `feat/autopilot-web` in its own worktree, after plan 1's Task 1 (the wire) is committed, and is merged into this branch before the record tasks
Decision: ADR 0082 (decision 8 as amended by the founder on 2026-10-20: no invented work for an idle agent; decision 11, the owner's approvals and remaining choices)
Spec: `docs/SPEC.md` 5.2, 5.7, 5.14, 5.18, 6.5, 6.10, 8.6 (revision 0.138)
Depends on: `main` at `0686738` (pull request #87 merged)
Readiness confirmed by: the planner (Opus 5.5), 2026-10-20, against the three readiness rules (ADR 0065)

## Goal

The owner can turn on one switch, "Autopilot: let the team run the product", in Settings → Allowed actions, once a daily spending limit is set and after a second press. While it is on, no question and no decision reaches the owner: agents ask the team (`catervas_ask_team`), the Product Manager or the Architect answers in a `decide` session, and the Product Manager takes every escalation, every block older than an hour, every plan or result that waits for the owner's approval or acceptance, and every folder change whose integration failed, with `catervas_decide`, choosing among the owner's own choices, or flags what only the owner can give. Paid and asking connector calls run, marketing plans and raised budgets are approved, data pipelines are decided by the Product Manager, seller messages go out (50 a day), and Catervas merges pull requests whose checks pass. An agent with no session running takes a `ready` task of its own role even at its WIP limit; with nothing of its role ready it stays idle. Today shows "Flagged by the team" and "Done on its own" (the last seven days). The daily limit and the marketing budget stay hard stops; the team never places a purchase order. Out of scope: what stays the owner's (placing a purchase order, credentials, a hash to confirm, a Settings change such as how the app opens: each is flagged), a command-line switch, and any change to how a goal is planned (ADR 0075 stands).

## Plans, in order

| File | Tasks | Where |
|---|---|---|
| `01-core.md` | 1 to 6 | this worktree |
| `02-runtime-team.md` | 7 to 12 | this worktree, after 01 |
| `03-runtime-acts.md` | 13 to 18 | this worktree, after 02 |
| `04-web.md` | 19 to 20 | worktree `feat/autopilot-web`, after Task 1, in parallel with the rest; it needs the daemon of Tasks 7 and 12 only in the founder's live look |
| `05-record.md` | 21 to 22 | this worktree, after 03 and after 04 is merged in |

## Decisions shared by the plans

- Every product decision is ADR 0082's; "decision n" is its number. Mechanical details the ADR leaves open are decided in each plan's Decisions section and say so.
- Wire names, all final, every addition optional on the wire (no older log changes meaning):
  - Event kinds (`docs/schemas/event.schema.json` `$defs.eventKind`, a body `$def` each, a `$ref` in `eventBodyWire.oneOf`; `crates/protocol/src/event.rs` wires each as `merge_permission.granted` is):
    - `autopilot.changed`, `autopilotChangedBody { on: boolean }`, required `on`. About no task; the envelope names no agent and no session.
    - `team.question_asked`, `teamQuestionAskedBody { question: string 1..=2000, asked_by: string, about: "product" | "technical" }`, all required. The event's seq is the question's id. The envelope names the task, the asking agent and its session.
    - `team.question_answered`, `teamQuestionAnsweredBody { question_seq: integer >= 1, answer: string 1..=4000, answered_by: string }`, all required. The envelope names the question's task, the answering agent and its session.
    - `team.decided`, `teamDecidedBody { choice, reason: string 1..=600, decided_by: string, answer?: string 1..=4000, needs?: string 1..=300, folder_change?: folderChangeNumber }`, `choice` one of `more_tries`, `more_room`, `back_to_plan`, `try_readiness_again`, `reopen_integration`, `answer_block`, `cancel`, `approve_plan`, `accept_result`, `carry_on`, `retry_folder_change`, `flag`. About the task; about no task when it names `folder_change`.
    - `task.flagged`, `taskFlaggedBody { needs: string 1..=300, flagged_by: string }`, both required; `flagged_by` is an agent id or `catervas`. About the task.
    - `tool.approved`, `toolApprovedBody { server, tool, input, input_sha256, by }`, all required, `server`, `tool`, `input`, `input_sha256` as `toolApprovalRequestedBody`'s, `by` enum `["autopilot"]`.
  - The Product Manager's approval of a plan and acceptance of a result are the owner's own event, `human.accepted { subject: contract | result, accepted_by: <the Product Manager's id>, message: <its reason> }` (`accepted_by` is already a string, and every reader asks only the subject), followed by `team.decided`: no new kind, so everything that reads an approval or an acceptance reads the team's.
  - Fields added to existing bodies: `marketingPlanApprovedBody.by?: "autopilot"`; `sellerMessageSentBody.sent_by?: "autopilot"`; `gateId` gains `autopilot`; `sessionStartedBody.purpose` gains `decide` (a decide session about a data pipeline names the request's task on its envelope and no `pipeline`, which stays an older log's).
  - RPC (`docs/schemas/rpc.schema.json`):
    - `autopilotSetRequest`: method `"autopilot.set"`, params `{ on: boolean, confirmed?: boolean }`, `required: ["on"]`, `additionalProperties: false`; answers `autopilotSetResult { autopilot: boolean }`.
    - `teamGetResult` gains `autopilot: boolean` (TypeScript `autopilot`).
    - `autopilotTodayQuery`: method `"autopilot.today"`, params `{}`; answers `autopilotTodayResult { on: boolean, flagged: flaggedRow[], done: doneRow[] }`, `flaggedRow { title: string, needs: string, at: date-time, task_id?: string, goal?: string, by?: string }`, `doneRow { at: date-time, line: string, agent_id?: string, task_id?: string, reason?: string }`. TypeScript `taskId`, `agentId`.
  - Refusals (`-32005`, `data.errors`, as `merging.set`'s): `autopilot_unconfirmed` at `/autopilot` ("Turning autopilot on needs its confirmation."); `daily_limit_required` at `/budgets/daily_usd` ("Autopilot needs a daily spending limit." from `autopilot.set`; "Autopilot is on, and it needs a daily spending limit: turn autopilot off first." from `team.save` and `agent.replace`).
  - Session purpose `decide` (`SessionPurpose::Decide`). Gate `Autopilot` (`GateId::Autopilot`).
  - Catervas tools, tier `read`, `#[serde(deny_unknown_fields)]`:
    - `catervas_ask_team { question: String, about: "product" | "technical" }` → `{ "question_seq", "next": "end your turn: the team's answer starts the next session" }`. Refusals `autopilot_off`, `no_task`, `decide_yourself` (the Product Manager or a solo agent).
    - `catervas_answer_question { question_seq: u64, answer: String }` → `{ "answered": question_seq }`. Refusals `not_a_team_question`, `already_answered`, `not_yours`.
    - `catervas_decide { task_id?: String, folder_change?: u64, choice: String, reason: String, answer?: String, needs?: String }`, exactly one of `task_id` and `folder_change` → `{ "decided": choice, "task_id" | "folder_change" }`. Refusals `autopilot_off`, `name_one`, `not_waiting_on_the_team`, `choice_not_offered` (naming the offered ones), `answer_required`, `needs_required`, `reason_required`, and the owner's own refusals of an approval or an acceptance (`not_awaiting_approval`, `not_waiting_for_the_human`, `already_accepted`, `criteria_not_run`, `criterion_failed`, as `human.rs` words them).
    - `catervas_decide_data_pipeline { pipeline: u64, approve: bool, reason: String }` → `{ "pipeline", "decided": "approved" | "declined" }`. Refusals as the owner's `pipeline_decide` and `autopilot_off`.
- Constants (`crates/core/src/autopilot.rs`): `TEAM_GRANTS_PER_TASK = 3` (decision 3), `REOPENS_PER_TASK = 3`, `DECIDE_SESSIONS_PER_WAIT = 3`, `BLOCK_WAIT_MINUTES = 60` (decision 3), `SELLER_MESSAGES_PER_DAY = 50` (decision 6), `DONE_DAYS = 7` (decision 10).
- Who decides: the team "decides" (`catervas_store::autopilot::team_decides`) while autopilot is on and the team has an active Product Manager or is solo. With neither, nothing changes for decisions and questions: they wait on the owner as now.
- Every cargo command in these plans runs with `CARGO_BUILD_BUILD_DIR=$PWD/target`.

## Verification of the whole change

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs the same under its workarounds (CLAUDE.md).

Then the founder's live look in the web app, on a throwaway project with a team of PM, Architect and Developer:
- Settings → Allowed actions: the switch is refused with "needs a daily spending limit" while none is set; with $5 set, the switch shows the list and turns on only at the dialog's Turn on. Clearing the daily limit in Spending is then refused.
- File a request whose contract leaves a technical question open: the Developer's question goes to the Architect (Today shows nothing to answer), and the task resumes with the answer.
- Let a task block on something the team can answer: within the hour the Product Manager answers it; on a sign-in it flags it, and Today shows it under "Flagged by the team".
- With two ready Developer tasks and WIP 1, the idle Developer takes the second when its first is under review.
- A pull request with green checks merges; Today's "Done on its own" lists the answer, the decision and the merge with their reasons.
- Turn autopilot off: the flagged task and any open team question show under "Waiting on you".
