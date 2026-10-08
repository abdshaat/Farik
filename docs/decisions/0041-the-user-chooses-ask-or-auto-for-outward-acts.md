# 0041. The user chooses to approve every outward act or run on auto

Date: 2026-10-05
Status: accepted (the founder, 2026-10-05, answering ADR 0040's G6: "no the user can configure whether he needs to approve everything or set it on auto mode"). Amends spec 5.6 and 6.7 (an `external_effect` call, a send and a post always asking) and ADRs 0031, 0037 and 0039 where they say a call or a message waits for the human.
Amended 2026-10-05 by ADR 0042: an approved marketing plan approves the posts and Google Ads changes in it under `ask` and `auto` alike; approving a marketing plan stays the owner's under `auto`, as a purchase order does; Google Ads writes outside the plan are refused, never asked or run on auto; and the marketing budget is a limit no mode passes.
Amended 2026-10-07 by ADR 0039's amendment of that day: a site the Procurement Specialist asks to read (phase 7 step 10b2) waits for the owner under `auto` as under `ask`; the approved sites are a limit that holds in both positions, as the spending limits do.
Amended 2026-10-07 by the founder's answers to phase 7 step 10e's readiness review, in conversation (ADR 0039's "Amendment of 2026-10-07: data pipelines"): under `auto`, the Product Manager's approval is enough for a data pipeline that costs money or whose cost is unknown ("Auto may approve"), recorded `by: auto`, and it still only files a request; one that sends the project's data out waits for the owner in both positions ("Yes, always to me"); and a pipeline the Product Manager escalates, or does not decide, waits for the owner under `auto` too. This replaces the Decision's "A data pipeline the Product Manager escalates (step 10e) is approved for the owner, with `by: auto`".
Amended 2026-10-07 by the founder's answers to phase 7 step 10f's readiness review, in conversation (ADR 0039's "Amendment of 2026-10-07: contacting sellers"): under `auto` a drafted message to a seller is sent without the owner's press ("Auto may send"), to any address the agent wrote, the approved sites not limiting it ("Any address"), within the daily cap; a message that sends a purchase order (step 10f's "Approve and send to <seller>") waits for the owner's press in both positions. This narrows the Decision's "A message to a seller ... is sent when the agent drafts it" to every message but an order's.
Amended 2026-10-07 by phase 7 step 10h's readiness review and the founder's answers to it, in conversation: the mode is kept in this computer's log alone, set only by the owner's command, never in `team.yaml`; under `auto`, a post outside the marketing plan is scheduled as a plan's post is, with its Stop; a message the team sends on its own carries no AI line ("No AI line"); every tool that spends credits gets a limit, which still asks under `ask` ("Yes, give each a limit"); and Today names a limit the team reached, with "Raise it" ("Yes, with 'Raise it'"). The "never change" list no longer names allowances, which the Decision makes limits. See "Amendment of 2026-10-07: step 10h" at the end, which carries every outward act in both positions.
Amended 2026-10-07 by phase 7 step 11b's readiness review and the founder's answers to it, in conversation: a deploy task's first deploy while it is in the running sprint is the sprint's in both positions, one that joined the sprint after it started included ("Yes, the sprint covers it"); any other `farik_deploy` asks under `ask` and runs under `auto`, recorded `approved_by: auto` and listed under "Done on its own" as "{name} put {sha} live for {task}" (`"Lena put <version> live for <task>"`); a deploy that cannot run is refused in both positions, never asked. The amendment table's deploy rows replace its row for the DevOps Engineer's deploys; its incident steps stay step 11d's.
Amended 2026-10-08 by ADR 0048 (the founder: "Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8"): a phase, Farik Cloud, follows the role kits as phase 8, so the phases after phase 7 moved up by one (Engines and providers 9, Ecosystem 10, Proof of concept 11, Web launch 12, Business workspaces 13, Desktop 14, Native mobile 15, Premium 16); the numbers below are the old ones.

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
- Spending limits, the 50-a-day send cap, session limits and the human's gates on contracts and acceptance (spec 5.4, 5.16) hold as they are; an allowance holds as a limit, as above (corrected 2026-10-07: this line named allowances among what never changes).
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

## Amendment of 2026-10-07: step 10h

Step 10h's readiness review found that the draft left open where the switch lives, where `auto` sits among the hook's checks, what an auto call records, how an auto send and an auto post are recorded, what happens to what already waits, the dialog's words, the words that say every call asks, and the line ending a message the team sends on its own; and it listed every outward act in both positions. The founder answered its three questions in conversation. The line ending a seller message the team sends on its own: "No AI line". Credit-spending tools with no allowance, most of Higgsfield's and two of Recraft's, which would run without limit on `auto`: "Yes, give each a limit". When the team uses up an allowance on `auto`, should Today say so: "Yes, with 'Raise it'".

Decision:
- **The switch lives in this computer's log, never in `team.yaml`.** The team file travels with the repository (spec 8.6), the web app sends the whole team on every save, and people edit it by hand, so a pull, a stale page's save or a hand edit could turn `auto` on with no dialog and no event; the approved sites are kept out of it for the same reason (ADR 0039's amendment of 2026-10-07). Only the owner's command, `approval_mode_set`, from the daemon's token or the browser's cookie, changes it, recording `approval_mode.changed`; the team file's `policy` refuses an `approvals` key, and templates carry none. This replaces the Decision's "A team policy, `approvals`". It is shown and changed on the team rules page, and turning `auto` on shows the dialog first.
- **`auto` replaces only the ask.** Every refusal before the ask still applies (the approved sites, the session's connector, the tag, `denied`, the preview, the Designer's plan gate, the 64 KiB limit, and a plan-marked tool with no active plan); a grant and an allowance come first; a session about no task is refused in both positions. A change applies from the next call; what already waits on Today keeps waiting for the owner.
- **Every tool that spends credits has a limit.** The ones a call count cannot bound under `ask` (a voice-over, a batch, a preset, an ad set, a 3D model, a video edit priced by its length, a change to the account's library) still ask every time under `ask`; under `auto` each runs up to its own number, which the owner changes on the agent page, then stops. Step 10h lists the 24 and their numbers.
- **A message the team sends on its own carries no AI line.** It ends with the owner's signature, or the owner's name; a message the founder sends with a press keeps step 10f's line in both positions.
- **Today shows the mode, a line with "Raise it" for each limit reached, and "Done on its own"**, each act with what went out, an agent's words shown as text.

Every outward act, in both positions (step 10h's plan holds the same table and is its single source):

| Act | `ask` | `auto` |
|---|---|---|
| A connector's `external_effect` call with no allowance (a GitHub issue or comment; Kit's series, pages and blocks) | asks each time | runs, recorded `approved_by: auto`, its input whole |
| A call within an allowance | runs | runs |
| A call past an allowance | asks | refused `allowance_reached`; Today's line with "Raise it" |
| A credit-spending tool a call count cannot bound | asks each time | runs up to its own number, then refused |
| A Google Ads write inside the active marketing plan | runs | runs |
| A Google Ads write with no active plan, or one the plan does not cover | refused | refused, never run |
| Approving, returning or ending a marketing plan; raising its budget | owner | owner |
| A post in the plan | goes out, with Stop | same |
| A post outside the plan | waits for the owner's "Post it" | scheduled as a plan's post is, three hours ahead at least, with Stop |
| Removing Google Ads | owner | owner |
| A site the Procurement Specialist asks to read | owner | owner |
| A purchase order: approve, place, receive, close | owner | owner |
| An order's email ("Approve and send to <seller>") | owner | owner |
| A quote request, question or follow-up to a seller | the founder's Send | sent when drafted, with no AI line, 50 a day at most |
| A data pipeline that is free | the Product Manager | the Product Manager |
| A data pipeline that is paid or of unknown cost | owner | the Product Manager, recorded `by: auto` |
| A data pipeline that sends the project's data out, or one escalated or undecided | owner | owner |
| A connector's call from a session about no task | refused | refused |
| A connector's call whose input is over 64 KiB | refused | refused |
| An act already waiting on Today when the mode changes | waits for the owner | waits for the owner |
| Contracts, epics and acceptance; adding a skill or a connector | owner, per the team's rules | same |
| An integration push or pull request; an agent's push | team policy, tier | same |
| Spending limits, the marketing budget, session limits, the 50-a-day cap | hold | hold |
| A deploy task's first deploy while it is in the running sprint, however it joined it | runs, recorded `approved_by: sprint` | same |
| Any other deploy: a retry, one after a send-back, one outside the running sprint, one with no sprint running | asks each time | runs, recorded `approved_by: auto`; "Done on its own" reads "Lena put a1b2c3d live for FRK-40" |
| A deploy outside a deploy task's session, with no production settings, on a platform Farik cannot drive, while one runs, or before its work is in | refused, never asked | refused, never asked |
| The DevOps Engineer's incident steps | step 11d | step 11d |

Consequences:
- Easier: no file, pull or stale page can turn `auto` on; the owner sees in one table what stops asking; credits cannot run away on `auto`; Today says when the team stopped at a limit.
- Harder: the mode is the project folder's own on this computer, in its log under `.farik/local/` (spec 8.4), so another clone of the repository, on this computer or another, starts at `ask`; a seller cannot tell from a message the team sent on its own that an AI wrote it, by the founder's choice, which spec 8.6 records; the kit carries a second meaning for an allowance (`asks_always`), which the allowance editor says in words.
