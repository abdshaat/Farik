# 0031. Connector calls wait for the human

Date: 2026-10-01
Status: accepted

## Context

Phase 7 step 01 tags each connector tool `network`, `external_effect` or `denied`, and refuses every `external_effect` call with a sentence (`external_effect_refused`). A GitHub issue, a sent email or a payment are exactly the calls the role kits need, so step 02 lets the human allow them one at a time. Four questions bind step 05's allowances, which build on this.

How a call waits. The options were:
- **Hold the hook open until the human answers.** The hook has a 10-second limit, and a session must not sit for hours holding a model.
- **An escalation with reason `permission`.** It moves the task to `escalated`, and asking is not something going wrong.
- **Wait like a question (spec 5.7).** The session stops, the task keeps its status and waits on the human, and the agent's next session is told the answer.

What a grant allows. One call, or every call of the tool for a while; any agent, or the one that asked.

How large an input the human is asked about. A grant is for what the human saw, so the whole input must be shown.

Whether `preauthorized_external_tools` reaches connector tools. It lets a built-in `external_effect` tool run without asking. A team file is committed, so a commit could pre-authorise a connector's payment tool.

## Decision

**A call waits like a question.** With no matching grant, the hook denies an `external_effect` call `approval_needed` and records `tool_approval.requested { server, tool, input, input_sha256 }`. Its seq is the approval's id, as a question's is its `question.asked`'s. The session ends `aborted` with the detail `approval_needed: approval <seq>`; it does not raise the task's `iteration` and raises no escalation. While an approval is open the task waits on the human (`open_approvals`, beside `open_questions`), and the orchestrator starts no session for it. The human answers with `tool_approve` or `tool_refuse`, which only the daemon token or the browser's session cookie can send, never a Farik tool.

**A grant allows exactly one later call, by the agent that asked.** It is bound by `ApprovalKey`: the agent, the task, the server, the tool, and `input_sha256`, the sha256 of the input's canonical JSON (compact, object keys sorted at every depth, as ADR 0030's hash). The call that matches carries `approval: <seq>` on its `tool.called`, which uses the grant up; the check and the use happen under the daemon's one sessions lock, so two identical calls cannot both run. The grant is for the asking agent's next session about the task: that session is told the answer as the human's message, and when it ends without the call, the grant lapses and a later call asks again. Another agent's session about the task is not told and cannot use it.

**An input over 64 KiB of compact JSON is refused without asking** (`tool_input_too_large`), grant or not, because the human could not be shown all of it. Only `tool.denied` is recorded.

**`preauthorized_external_tools` does not reach connector tools.** A connector's `external_effect` call always asks, one grant per call, until step 05's allowances, which are the one pre-authorisation for connector tools.

**The Designer's plan gate comes first.** Before the Product Manager approves a Designer's plan, the Designer's `external_effect` call is refused as a write is, and not asked about. For a connector confined to the preview, every `url` in the input is checked before any ask or grant, so a grant never takes a call outside the preview.

**The human sees the input wherever they decide, and the agent is given it back.** The browser shows it in full; the terminal shows it in `farik waiting`'s row and in the line `farik tool approve` or `farik tool refuse` prints, with control, bidirectional and zero-width characters escaped. The next session starts fresh and could not write a long input again word for word, so the grant's message quotes the canonical input whole, in an `untrusted` block. A decision is recorded under one lock with the check that none exists, so two decisions at once let one through.

In `farik-core`, `evaluate_connector_call` takes the seq of the open grant the caller found by `ApprovalKey` and returns it beside the tag; it refuses `ApprovalNeeded` without one and `InputTooLarge` over the limit. A `network` call ignores grants.

## Consequences

The human sees and allows each call by its exact input. An agent that needs ten issues filed asks ten times, across ten sessions, until step 05's allowances; that is slow, and it is the safe side for a first release.

A grant that is never used costs nothing: it lapses with the asking agent's next session. A changed field, a reordered retry with a different value, or another agent asks again.

The event names are `tool_approval.*`, not `approval.*`, because `approval` already names a contract's approval (escalation reason `approval`, `human_accepts_contracts`).

Without the sandbox, `daemon.json`'s token lets any program running as the user approve a call, which spec 8.3's no-sandbox warning names.
