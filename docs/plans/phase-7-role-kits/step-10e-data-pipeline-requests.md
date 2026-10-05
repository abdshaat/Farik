# Phase 7, step 10e: Data pipeline requests

Status: draft. Its readiness review runs once step 10d has landed (the founder answered O6 on 2026-10-05).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.7, 5.16, 6.1, 6.10, 8.5; F9
Depends on: steps 10b to 10d of this phase (the role, its folder and kit; 10c's human-only decision and Today rows, which this step follows); phase 6 step 11 (the Designer's plan, whose Product Manager decision session this step copies: `orchestrator/design.rs`, `tools/design.rs`); phase 6 (merged in #19)
Readiness confirmed by: not yet. A fresh-session Opus reviewer read the four plans on 2026-10-05 before their dependencies landed: 3 Blocking (step 10d: `fx` had no copy, `uv` and its first run were undecided; step 10c: `renewal.due` broke the event-naming rule), all folded with the Should items the same day. The readiness review proper runs when the step's dependencies have landed, as its Status says (ADR 0032: one round).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The founder's words of 2026-10-05: the Procurement Specialist "is supposed to research prices and different providers. He may request a data pipeline from the pm who can decide whether the data pipeline is necessary or whether this decision must be escalated to the owner." After this step, when a source of prices or provider data the agent lacks would change its recommendation, it asks; the Product Manager approves, declines, or escalates the request to the owner; a request that costs money, needs an account, or sends the project's data out always reaches the owner; and an approved one becomes an ordinary request for the team. Out of scope: building or connecting any candidate source (each is a later plan's, or the user's own connector); a pipeline request from any other role.

## Decisions

- **A request is a record and holds no task**, as a purchase order is (10c): `farik_request_data_pipeline` records `data_pipeline.requested` and returns its number; the agent's task goes on with public pages. Rejected: holding the task until decided, which would stall research on the slowest decision in the team.
- **Its input**, in this order: only a Procurement Specialist in a session about a procurement task (`pipeline_refused`); `name` 1 to 100 characters and `what` 20 to 600, no control characters (`pipeline_field_invalid` naming the field); `source_url` as `purchase_url_invalid`'s rule (10c), refused `pipeline_url_invalid`; `why` 20 to 600; `cost` `free`, `paid` or `unknown`; `needs_account` and `sends_project_data` booleans, both required; then at most one open request of the same `name` (case-insensitive) in the project (`pipeline_already_requested`), and at most 3 open requests in the project (`pipeline_limit_reached`), so a looping agent cannot flood the Product Manager.
- **The Product Manager's decision session copies the Designer's plan decision** (spec 6.8): with a request open and not escalated, the tick starts the active Product Manager's `verify` session about the requesting task, purpose `verify`, one Farik tool, `farik_decide_data_pipeline { pipeline, decision, reason }`, message from `decide_data_pipeline_message` holding every field of the request as untrusted text (spec 8.6) and the escalation rule in plain words. One request per session, oldest first. A session that ends without a decision is started again, at most three times a request, after which the request is escalated with the reason "The Product Manager did not decide". Its cost is the Product Manager's, under the day's budget (5.5), not charged to the procurement task's sessions allowance, so a slow decision never stops the task. A request whose task was `accepted` or `cancelled` meanwhile is still decided: approval files a request, which needs no open task. With no active Product Manager the request waits, as a design plan does. It runs beside the task's own sessions, not instead of them, under the sessions lock.
- **The rule that sends a request to the owner is the governor's, not the prompt's** (the founder's answer to O6, 2026-10-05: "the PM must escalate any process that cost money"). `decide_data_pipeline` refuses `approve` with `pipeline_needs_owner` when `cost` is `paid` or `unknown`; the refusal's sentence tells the Product Manager to decline or escalate. `needs_account` and `sends_project_data` do not force it: they are shown to the Product Manager, whose skill weighs them, and to the owner on an escalated request. `decline` and `escalate` are always allowed; `reason` 20 to 600 characters. Refused `pipeline_decision_refused` from any agent but the active Product Manager in that session, and `pipeline_decided` for one already decided or escalated. The rule is a pure function in `farik-core`, `pipeline_needs_owner(cost: PipelineCost) -> bool`.
- **The owner decides an escalated request** on Today, beside purchase orders: `data_pipeline_decide { pipeline, decision: approve | decline, note? }`, the human's alone (the daemon's token or the browser's cookie), `unknown_pipeline` and `pipeline_decided` as above, and `pipeline_not_escalated` for a request that is open and waits on the Product Manager. The agent's fields are untrusted; the row says which of the three reasons sent it ("it costs money", or "its cost is not known").
- **Events.** `data_pipeline.requested { pipeline, agent_id, task_id, name, what, source_url, why, cost, needs_account, sends_project_data }`; `data_pipeline.escalated { pipeline, reason }`; `data_pipeline.approved { pipeline, by: product_manager | human, reason?, request_seq }`; `data_pipeline.declined { pipeline, by, reason? }`. `pipeline` is the request's sequence number.
- **Approval files an ordinary request**, through the same path as a human's `request` command, its text "Set up <name> for the Procurement Specialist: <what>. Source: <source_url>. Asked because: <why>." and its sequence number written as `request_seq` on the approval. The team triages it (5.16); the Product Manager's triage of its own approval is ordinary. Connecting a service is still the human's (spec 6.7), and the new request says so when the source is a kit connector of the role ("Connect <title> on the Procurement Specialist's page"). Approval never connects, signs in, pays or builds.
- **`farik_read_data_pipelines {}`**, Procurement Specialist only: each request oldest first with `state` (`open`, `escalated`, `approved`, `declined`), `by`, and the reasons and notes quoted as untrusted text.
- **`waiting.list` gains `data_pipeline` rows** for escalated requests only (an open one waits on the Product Manager, not the human), `{ kind: data_pipeline, pipeline, agent, task, name, what, host, why, cost, needs_account, sends_project_data, product_manager_reason }`. The migration at the next free number (0014 if step 10c took 0013), `<n>_data_pipelines.sql`, adds `open_pipelines` to the task projection, read by the tick so it starts the decision session without scanning the log.
- **The gate trusts the agent's own `cost`**, which is acceptable only because approval merely files a request that the team triages and the human connects; nothing is connected, paid or sent on the agent's word. `cost` is `unknown` unless the source's own page says it is free, which `requesting-a-data-pipeline` requires the agent to cite in `why`.
- **Two skills.** The Procurement Specialist's kit gains `requesting-a-data-pipeline`: ask only when a source would change the recommendation; prefer a candidate from the design's list, else a public page; fill `cost`, `needs_account` and `sends_project_data` honestly, since they decide who decides and a false `free` is refused nowhere but read by the owner; carry on without it and say what it would have added. The Product Manager's kit gains `deciding-data-pipelines`: approve only what changes a decision this sprint or the next; prefer a free public source; escalate whatever costs, needs an account or sends data, saying why in one line for the owner; a decline names what to use instead. Both embedded as step 06 did.
- **Mockups first**: Today's escalated-pipeline row and its dialog, desktop and phone.
- **Command line.** `farik pipeline list [--json]`, `farik pipeline approve <n> [--note]`, `farik pipeline decline <n> [--note]`, escalated requests only, through `here_or_sent`.

## File map

```
docs/design/mockups/data-pipeline.*                          creates (Task 0)
docs/schemas/event.schema.json, command.schema.json, rpc.schema.json   modifies (Tasks 1, 3, 4)
crates/core/src/pipeline.rs, crates/core/src/lib.rs           creates/modifies: PipelineCost, pipeline_needs_owner (Task 1)
crates/protocol/src/event.rs                                  modifies: four kinds (Task 1)
crates/runtime/src/tools/pipeline.rs, tools.rs, orchestrator/session.rs   creates/modifies: the three tools and their offer (Tasks 1, 2)
crates/runtime/src/orchestrator/pipeline.rs, orchestrator/messages.rs, orchestrator/rules.rs   creates/modifies: the decision session (Task 2)
crates/store/src/migrations/<next>_data_pipelines.sql, projections.rs, waiting.rs   creates/modifies (Task 3)
crates/runtime/src/orchestrator/human.rs, daemon/gates.rs     modifies: data_pipeline_decide, waiting rows (Task 3)
crates/roles/roles/{procurement_specialist,product_manager}/skills/<name>/SKILL.md, kit.yaml, crates/roles/src/kit.rs   creates/modifies (Task 4)
crates/cli/src/pipeline.rs                                    creates (Task 5)
apps/web/src/pages/Today.tsx, dialogs/DataPipeline.tsx(+test)   modifies/creates (Task 6)
docs/SPEC.md, docs/plans/project-plan.md                      modifies (Task 7)
```

## Interfaces

Consumes: `Call`, `FarikTool`, `offered_tools`; `orchestrator/design.rs`'s decision-session pattern (`DECIDE_TOOL`, its `SessionAsk`); 10c's human-only command check, `waiting.list` rows and URL check; the request path of the `request` command; `embedded_skills`.

Produces:

```rust
pub enum PipelineCost { Free, Paid, Unknown }                      // farik_core::pipeline
pub fn pipeline_needs_owner(cost: PipelineCost) -> bool;
pub fn request_data_pipeline(call: &Call<'_>, input: RequestPipelineInput) -> Result<Value, ToolError>;   // tools::pipeline
pub fn read_data_pipelines(call: &Call<'_>) -> Result<Value, ToolError>;
pub fn decide_data_pipeline(call: &Call<'_>, input: DecidePipelineInput) -> Result<Value, ToolError>;
pub enum PipelineDecision { Approve, Decline, Escalate }
```

## Tasks

### Task 0: Mockups

- [ ] `docs(design): mock up a data pipeline the owner decides`

### Task 1: The ask

- `the_owner_decides_what_costs_money` (core): `pipeline_needs_owner` is false for `Free` and true for `Paid` and `Unknown`. RED.
- `records_a_pipeline_request`: valid input records `data_pipeline.requested` with every field. RED.
- `refuses_another_role_and_each_bad_field`: one case per refusal in Decisions, each writing nothing. RED.
- `refuses_a_fourth_open_request_and_a_repeated_name`. RED.

- [ ] `feat(runtime): let the Procurement Specialist ask for a data pipeline`

### Task 2: The Product Manager decides

- `an_open_request_starts_the_product_managers_decision`: the tick starts one `verify` session for the active Product Manager about the requesting task, with `farik_tools` exactly `["farik_decide_data_pipeline"]` and the request's fields inside the untrusted notice. RED.
- `approve_is_refused_for_what_costs_money`: `paid` and `unknown` each give `pipeline_needs_owner` and record nothing; a free request that needs an account can be approved. RED.
- `a_free_request_can_be_approved`: records `data_pipeline.approved { by: product_manager }` and files a request whose text names the source; `request_seq` is its number. RED.
- `escalate_and_decline_record_their_events`; `a_second_decision_is_refused`; `another_agent_cannot_decide`. RED each.
- `a_session_without_a_decision_starts_again`: up to three times, then the request is escalated with "The Product Manager did not decide"; the procurement task's session count is unchanged. RED.
- `a_request_outlives_its_task`: with the task `accepted`, approval still files a request. RED.
- `no_product_manager_means_it_waits`: no session starts and nothing is recorded. RED.

- [ ] `feat(runtime): let the Product Manager decide a data pipeline`

### Task 3: The owner decides an escalated one

- `an_escalated_request_waits_on_the_human`: `waiting.list` has its row with `host` and the Product Manager's reason; an open one has none. RED.
- `the_human_cannot_decide_an_open_request`: `pipeline_not_escalated`. RED.
- `only_the_human_approves_an_escalated_request`: `data_pipeline_decide` approve records `approved { by: human }` and files the request; an agent's recorded approval is ignored. RED.
- `farik_read_data_pipelines_gives_each_state`. RED.

- [ ] `feat(runtime): let the owner decide an escalated data pipeline`

### Task 4: The two skills

- `procurement_kit_carries_requesting_a_data_pipeline`: the kit's skills are step 10d's eight then `requesting-a-data-pipeline`. RED.
- `product_manager_kit_carries_deciding_data_pipelines`: step 06's five then `deciding-data-pipelines`. RED.

- [ ] `feat(roles): teach asking for and deciding data pipelines`

### Task 5: The command line

- `pipeline_list_prints_escalated_requests` (`--json` pure); `pipeline_approve_sends_the_number_and_note`. RED each.

- [ ] `feat(cli): decide escalated data pipelines`

### Task 6: Today

- `shows_why_the_owner_is_asked` (each of the three reasons); `approve_and_decline_send_the_decision`; `fields_render_as_text`. RED each.

- [ ] `feat(web): decide escalated data pipelines on Today`

### Task 7: Spec and plan

`docs/SPEC.md` 6.10 (as built), 6.1 (the Product Manager decides data pipelines and escalates to the owner), 5.7 (an escalated pipeline waits on the human), 8.5 (four kinds); the revision line. Project plan row 10e.

- [ ] `docs(spec): record data pipeline requests`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: the Procurement Specialist asks for Azure's retail price list (free, no account), and the Product Manager approves it, filing a request; it then asks for Firecrawl (paid), and the Product Manager's approval is refused, it escalates, and Today asks the founder, who declines.

## Execution notes

None yet.
