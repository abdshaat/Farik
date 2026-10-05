# 0041. The user chooses to approve every outward act or run on auto

Date: 2026-10-05
Status: accepted (the founder, 2026-10-05, answering ADR 0040's G6: "no the user can configure whether he needs to approve everything or set it on auto mode"). Amends spec 5.6 and 6.7 (an `external_effect` call, a send and a post always asking) and ADRs 0031, 0037 and 0039 where they say a call or a message waits for the human.
Amended 2026-10-05 by ADR 0042: an approved marketing plan approves the posts and Google Ads changes in it under `ask` and `auto` alike; approving a marketing plan stays the owner's under `auto`, as a purchase order does; Google Ads writes outside the plan are refused, never asked or run on auto; and the marketing budget is a limit no mode passes.

## Context

Since spec 0.3, an `external_effect` call, anything that changes state outside the sandbox, asks the human unless pre-authorized (spec 5.6). Phase 7 built it for connectors: a call waits on Today (ADR 0031), an allowance pre-approves a number of credit-spending calls (ADR 0037), and a tool that publishes, sends, posts or pays has no allowance and always asks (spec 6.7). ADR 0039 added messages to sellers that only the founder sends, and data pipelines the Product Manager must escalate to the owner when they cost money.

For a business that is not software, these acts are daily work: writing to suppliers, posting listings, asking for quotes. The founder decided that the user chooses: approve everything, as today, or run on auto.

Three facts constrained the design:
- Content a connector or a seller returns is untrusted (spec 8.6) and can try to steer an agent; under auto, nothing stands between a steered agent and an outward act but the limits.
- Spending limits (spec 5.5, ADR 0015) and allowances are the user's own numbers; auto removes the asking, not the numbers.
- ADR 0039's O1, "the final decision is the founder['s]", is about buying.

The options:
- **Auto per connector or per tool.** Fine-grained, but it puts a long list of switches in front of a non-technical user.
- **One switch, with fixed limits that hold in both positions.** "Ask me first" or "Run on its own", per team, with the spending limits, the allowances and the daily send cap holding either way. This is the chosen way.

## Decision

A team policy, `approvals`, is `ask` (the default, and every team's until the user changes it) or `auto`, set in the team rules ("Ask me before anything leaves Farik" or "Let the team act on its own") and recorded as `approval_mode.changed`.

Under `auto`:
- An `external_effect` connector call runs without asking, recorded as every call is (`tool.called`), with `approved_by: auto`.
- A message to a seller (ADR 0039, phase 7 step 10f) is sent when the agent drafts it, within the daily cap.
- A data pipeline the Product Manager escalates (step 10e) is approved for the owner, with `by: auto`; it still only files a request.
- An allowance becomes a limit: a call within it runs, a call beyond it is refused with "<n> of <n> used; raise it to let the team do more", never asked.

These never change with the mode:
- A purchase order waits for the founder's decision (ADR 0039, O1).
- No tool pays; nothing in Farik holds a card.
- Spending limits, allowances, the 50-a-day send cap, session limits and the human's gates on contracts and acceptance (spec 5.4, 5.16) hold as they are.
- `denied` tools are never offered.

Turning `auto` on shows, once, what it lets the team do and that it can be turned off at any time; Today gains "Done on its own", the outward acts taken under `auto` since the person last looked, each with its input. Turning it back to `ask` applies from the next call.

It is phase 7 step 10h, after the procurement steps, so it covers every kind of outward act phase 7 builds; the DevOps Engineer's steps (11 and 12) honour it from their plans, its sprint-approved deploys excepted, which are unchanged.

## Consequences

Easier:
- A business that writes to suppliers or posts every day is not stopped at every act.
- One switch, plainly worded, and the limits the user already sets keep their meaning.

Harder:
- Under `auto`, a prompt-injected agent can act outward within the limits; the limits and the "Done on its own" list are the only guard, which the switch's wording must say.
- Every place that asks now has two paths to test.
- The proof-of-concept benchmark (phase 10) runs in `ask`, or reports both.
