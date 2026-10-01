# Phase 7, step 02: Approving a connector's calls

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 5.6, 5.7, 8.3, 8.5, 8.6; F9
Depends on: step 01 of this phase (connectors per agent, not yet committed), whose `ConnectorRefusal::ExternalEffectRefused` this step replaces; phase 6 (merged in #19), whose `question.asked` waiting projection and Today page this step extends
Readiness: fresh-session reviewer, 2026-10-01, round 1 not ready → findings folded in; round 2 ready with findings, folded

## Goal

After step 01, an `external_effect` connector tool is refused with a sentence. When this step is done, such a call stops the session and asks the human, who sees the whole input and allows that one call or refuses it, from Today's list in the web app or with `farik tool approve` and `farik tool refuse`. The agent's next session about the task is told the answer, and an allowed call runs once, with exactly the input the human saw. Out of scope: allowances, which pre-approve a number of calls of a tool that spends credits (step 05), and OAuth (step 03). This step was split from step 01 on the founder's word of 2026-10-01, after step 01's readiness review.

## Decisions

The ask:
- **An `external_effect` call waits like a question (5.7).** With no matching grant, the hook denies the call with `approval_needed: <server> <tool> waits for the human (approval <seq>)` and records `tool_approval.requested { server, tool, input, input_sha256 }`.
  - The approval's id is the event's own seq, as a question's is the seq of its `question.asked`, so the event needs no field it cannot know before it is appended (finding B5).
  - `input` is the full compact JSON, never cut, so the human always sees all of what they allow, and a later call can be matched to the grant from the log.
  - An input over 64 KiB is not asked about. It is denied `tool_input_too_large: <tool>'s input is over 64 KiB, too long to show you, so it is refused`, and nothing is recorded but its `tool.denied`.
- **The session ends.** The hook's denial stops the session, as a pause does, so it ends `aborted` with the detail `approval_needed: approval <seq>`; `tool_approval.requested` is the record of why, so `session.ended` gains no reason (finding R2-S8). It does not raise the task's `iteration`, counts towards `max_sessions` as any session does, and raises no escalation (finding S7).
- **The task waits on the human.** It keeps its status, is listed in `waiting.list` while an approval is open, and the orchestrator starts no session for it until the approval is decided. An open approval counts as an open question does: `task_projections` gains `open_approvals`, raised by `tool_approval.requested` and lowered by `.granted` or `.refused`, and `waiting_on_human` becomes `open_questions > 0 OR open_approvals > 0`, which the orchestrator's rules already skip (finding R2-S7).
- Rejected: holding the hook open until the human answers, because the hook has a 10-second limit and a session must not sit for hours. Rejected: an escalation with reason `permission`, because it moves the task to `escalated`, and asking is not something going wrong.

The answer:
- **Commands `tool_approve { approval, note? }` and `tool_refuse { approval, note? }`**, named as `command.schema.json` names commands (`question_answer`, `human_accept`), with no dots (finding S1). They record `tool_approval.granted { approval, note? }` and `tool_approval.refused { approval, note? }`, with `note` absent when none was given. The browser sends them through the RPC `command`, as it sends `question_answer`.
- **Refused commands.** An unknown seq, or a seq that is not a `tool_approval.requested`, is `unknown_approval`. A second decision on one approval, either way, is `approval_decided`.
- **Who can decide.** No Farik tool records a decision. `tool_approve` arrives only on `POST /command` (the daemon token) or the browser's RPC (the session cookie), so an agent in the sandbox cannot approve its own call. In no-sandbox mode the token in `daemon.json` already lets a command approve, which 8.3's warning names.
- **Event names** (Note N2): `tool_approval.*`, not `approval.*`, because `approval` already means a contract's approval (escalation reason `approval`, `human_accepts_contracts`), and one word with two meanings would confuse the board and the log.

The grant:
- **A grant allows exactly one later call** by the same agent, on the same task, to the same server and tool, with the same `input_sha256`: the sha256 of step 01's `canonical_json` of the input, object keys sorted at every depth. That call's `tool.called` carries `approval: <seq>`, which uses the grant up.
- **It is for the asking agent's next session about the task** (Note N3). The decision is given to that session as the human's message: "You may call `<tool>` once with the input you asked for", or "The human did not allow `<tool>`", with the note. A session about the task by another agent (a reviewer's) is not told and cannot use it. When that next session ends without using the grant, the grant lapses, and a later call asks again.
- **Check and use are atomic.** The hook holds the daemon's one sessions lock across `judge` and `record_decision` (`decide_pre_tool_use`), so two identical calls after one grant cannot both run.
- **`preauthorized_external_tools` does not apply to connector tools** (finding B3, ruled 2026-10-01). The hook never consults it for an `mcp__<server>__*` call: a connector tool tagged `external_effect` always asks, one grant per call, until step 05's allowances, which are the one pre-authorisation and which remove or map that field. `ApprovedCall`, `ToolCallContext.approved_calls` and `RequiresHumanApproval` stay unused by connector calls; `ApprovalKey` is the connector path's record, because it binds agent and task, which `ApprovedCall` does not.
- **The Designer's plan gate** holds an `external_effect` connector call as it holds a write: before the Product Manager approves the plan, it is refused as a write would be, and not asked about. `check_design_plan` gains `ExternalEffect`, and the hook passes it for an `external_effect` connector call, before any grant lookup or ask (finding R2-S9).

What a non-technical user sees:
- **Today's list** of what waits on the user gets the row "<agent name> wants to use <server>". It opens `ToolApproval`, a dialog, which shows the tool's plain name, the whole input as the agent wrote it in an `untrusted` frame, an optional note, "Allow once" and "Don't allow".
- **On the command line**, `farik tool approve <approval> [--note <text>]` and `farik tool refuse <approval> [--note <text>]`, through `here_or_sent`. Each prints one plain line: "Allowed <tool> once for <agent> (approval <seq>)." or "Not allowed: <tool> for <agent> (approval <seq>)."

ADR 0031 records approvals that wait like questions, the grant's scope and lapse, the size limit, and that `preauthorized_external_tools` does not reach connector tools. It is written in Task 2's commit, because step 05's allowances build on it.

Open, for the founder:
- **O3, the mockups.** The founder approves Task 1's boards when they are drawn; Task 5 does not start until then.

## File map

```
docs/design/mockups/{ToolApproval,TodayBacklog}.dc.html, canvas.json   Task 1
docs/decisions/0031-connector-calls-wait-for-the-human.md       creates: the ADR (Task 2)
crates/core/src/governor/permissions.rs                         modifies: ApprovalKey, input_sha256, ApprovalNeeded, check_design_plan (Task 2)
crates/runtime/src/daemon/hooks.rs                              modifies: call site (Task 2); ask, grant lookup and use (Task 3)
crates/runtime/src/orchestrator/session.rs                      modifies: the aborted detail approval_needed (Task 3)
crates/runtime/src/orchestrator/messages.rs                     modifies: the decision as the human's message (Task 3)
crates/runtime/src/daemon/team.rs                               modifies: tool_approve, tool_refuse (Task 3)
docs/schemas/{event,command}.schema.json                        modifies: three events, approval on tool.called, two commands (Task 3)
crates/protocol/src/event.rs, command.rs                        modifies: hand-written EventKind, EventBody, Command (Task 3)
crates/store/src/waiting.rs, projections.rs                     modifies: open approvals wait on the human; open grants (Task 3)
crates/store/src/migrations/0011_tool_approvals.sql             creates: open_approvals (Task 3)
docs/schemas/rpc.schema.json                                    modifies: waiting row kind tool_approval (Task 3)
crates/cli/src/human.rs, lib.rs                                 modifies: farik tool approve|refuse (Task 4)
packages/protocol-client/src/mapping.ts                         modifies: the waiting row's camelCase mapping (Task 5)
apps/web/src/pages/dialogs/ToolApproval.tsx, test               creates (Task 5)
apps/web/src/pages/Today.tsx, Today.test.tsx                    modifies: the row (Task 5)
apps/web/src/strings/en.ts                                      modifies: the copy (Task 5)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies (Task 6)
```

## Interfaces

Consumes: `canonical_json`, `SessionConnector`, `ConnectorRefusal::ExternalEffectRefused` (`farik-core`, step 01); `decide_pre_tool_use`, `record_decision`, `waiting.list`'s projection, `here_or_sent` (main).

Produces:

```rust
// farik-core
pub fn input_sha256(input: &serde_json::Value) -> String;  // sha256 hex of canonical_json(input)
pub struct ApprovalKey { pub agent_id: String, pub task_id: TaskId, pub server: String,
    pub tool: String, pub input_sha256: String }
pub const MAX_APPROVAL_INPUT: usize = 64 * 1024;
pub enum ConnectorRefusal { /* step 01's, less ExternalEffectRefused */, ApprovalNeeded, InputTooLarge }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>,
    granted: Option<u64>) -> Result<(ConnectorTag, Option<u64>), ConnectorRefusal>;
// farik-store
pub struct OpenGrant { pub approval: u64, pub key: ApprovalKey }
pub fn open_grants(events: &[Event]) -> Vec<OpenGrant>;     // granted, not used, not lapsed
```

Wire (`snake_case`): events `tool_approval.requested { server, tool, input, input_sha256 }` (its seq is the approval id), `tool_approval.granted { approval, note? }`, `tool_approval.refused { approval, note? }`; `tool.called` gains `approval?`; commands `tool_approve { approval, note? }`, `tool_refuse { approval, note? }`; `waiting.list` gains rows of kind `tool_approval`.

## Tasks

### Task 1: The approval, mocked up

Files: on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf), copied into `docs/design/mockups/`: `ToolApproval`, new, with a long input and a short one; Today's row, on `TodayBacklog`, the repository's one Today board. Each at desktop and phone width.

Gate (O3): the founder approves the boards, and the approval is written into this plan's Decisions with its date. Task 5 does not start until then; Tasks 2 to 4 do not depend on the boards.

- [ ] `docs(design): mock up the tool approval`

### Task 2: The governor's check, with grants

Files: `permissions.rs`, ADR 0031, and, as call sites only, `daemon/hooks.rs` (`judge_connector`, passing `None` until Task 3). Produces `input_sha256`, `ApprovalKey`, `ApprovalNeeded`, `InputTooLarge`, the new `evaluate_connector_call`.

- `external_effect_without_a_grant_needs_approval`: `ApprovalNeeded`.
- `external_effect_with_a_grant_runs_and_names_it`: `Ok((ExternalEffect, Some(7)))`.
- `denied_is_refused_even_with_a_grant`: `ToolDenied`.
- `a_large_input_is_refused_not_asked`: an input of 64 KiB and one byte gives `InputTooLarge`, with or without a grant.
- `the_approval_key_ignores_key_order`: `{a:1,b:2}` and `{b:2,a:1}` give one `input_sha256`.
- `a_network_tool_ignores_grants`: `Ok((Network, None))` whatever `granted` holds.
- `a_designers_external_effect_waits_for_the_plan`: `check_design_plan(UiUxDesigner, ExternalEffect, false)` is `DesignPlanNotApproved`.

- [ ] `feat(core): let an external_effect connector call run once the human allows it`

### Task 3: The hook, and approvals that wait

Files: `daemon/hooks.rs`, `orchestrator/session.rs`, `orchestrator/messages.rs`, `daemon/team.rs`, the event, command and RPC schemas, `protocol/src/event.rs`, `command.rs`, `store/src/waiting.rs`, `projections.rs`, migration `0011_tool_approvals.sql`.

The hook reads only the task's events for `open_grants`, through the log's query by task, so the sessions lock stays short. `append` returns the seq it wrote, so the hook appends `tool_approval.requested` first and names its seq in the denial; a `tool.denied` that then fails to record leaves a request with no denial, which is harmless, since the call was denied either way.

- `an_external_effect_call_asks_and_stops`: the call is denied `approval_needed`, `tool_approval.requested` is recorded with the full input and its `input_sha256`, and the session ends `aborted` with the detail `approval_needed: approval <seq>`.
- `a_large_input_is_refused_not_asked`: the hook denies `tool_input_too_large` and records no `tool_approval.requested`.
- `an_approval_stop_is_not_a_failed_try`: after it, the task's `iteration` is unchanged, `max_sessions` counts the session, and no escalation is open (checked against `orchestrator/human.rs`'s special handling of `aborted`).
- `a_designers_call_before_the_plan_is_refused_not_asked`: a Designer's `external_effect` call before plan approval is refused, and no `tool_approval.requested` is recorded.
- `an_open_approval_waits_on_the_human`: the task is listed in `waiting.list` and the orchestrator starts no session for it.
- `approve_then_the_same_call_runs_once`: after `tool_approve`, the matching call in the asking agent's next session is allowed with `approval` on `tool.called`, and a second one asks again.
- `a_grant_is_used_once_under_concurrency`: two identical calls through `decide_pre_tool_use` from two threads after one grant; exactly one is allowed.
- `a_different_input_is_not_approved`: one changed field asks again.
- `a_grant_is_for_the_asking_agent_only`: the same call by another agent on the same task asks again, and that agent's session is not told.
- `a_grant_lapses_with_the_next_session`: that session ends without the call; a later session's same call asks again.
- `a_preauthorized_external_tool_still_asks`: an agent listing `mcp__github__create_issue` in `preauthorized_external_tools` is denied `approval_needed`.
- `approve_refuses_an_unknown_or_decided_approval`: an unknown seq and a seq that is not `tool_approval.requested` are `unknown_approval`; a second approve, and an approve after a refuse, are `approval_decided`.
- `refuse_tells_the_next_session`: the asking agent's next session's human message carries the refusal and the note.

- [ ] `feat(runtime): ask the human before a connector changes anything`

### Task 4: The command line

Files: `cli/src/human.rs`, `cli/src/lib.rs`.

- `farik_tool_approve_sends_the_command`: `farik tool approve 12` with a daemon running sends `tool_approve { approval: 12 }` and prints "Allowed create_issue once for theo (approval 12)."
- `farik_tool_refuse_carries_the_note`: `farik tool refuse 12 --note "not this repo"` sends `tool_refuse { approval: 12, note: "not this repo" }` and prints "Not allowed: create_issue for theo (approval 12)."
- `farik_tool_approve_writes_here_when_nothing_drives`: with no daemon, `tool_approval.granted` is appended under the run lock.

- [ ] `feat(cli): approve or refuse a connector's call`

### Task 5: The screens

Files: `pages/dialogs/ToolApproval.tsx` and its test, `Today.tsx`, `Today.test.tsx`, `strings/en.ts`, `mapping.ts`. Built from Task 1's approved boards.

- `today_lists_an_approval_and_opens_the_dialog`: the row reads "Theo wants to use github".
- `the_dialog_sends_approve_or_refuse_with_the_note`: "Allow once" sends `tool_approve`, "Don't allow" `tool_refuse`, each with the note when one is typed.
- `the_input_is_shown_as_untrusted_text`: markup in the input renders as text, in the `untrusted` frame, whole.

- [ ] `feat(web): approve a connector's call from Today`

### Task 6: Spec and plan

`docs/SPEC.md`: 5.6 (an `external_effect` connector call asks; `preauthorized_external_tools` does not reach connector tools), 5.7 (an approval waits like a question), 8.3 (who can decide), 8.5 (the three events, `approval` on `tool.called`). `docs/plans/project-plan.md`: phase 7's row 02 and the approvals decision bullet, and `docs/design/role-kits.md`'s steps table, both written by the readiness commit, corrected if execution changed them.

- [ ] `docs(spec): approving a connector's calls`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```
