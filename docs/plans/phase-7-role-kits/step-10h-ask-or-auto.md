# Phase 7, step 10h: Ask or auto

Status: draft until its second round and its mockups (executes after steps 10c to 10f land)
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.4, 5.6, 5.7, 5.12, 6.1, 6.5, 6.7, 6.10, 8.5, 8.6; F3, F9, F15
Depends on: the commits of steps 08g (the budget's hard stop), 10c (`purchase_order_decide`, `ORDERS`), 10d (the Procurement Specialist's kit, SerpApi's allowance), 10e (`pipeline_needs_owner`, `farik_decide_data_pipeline`, its decision session, `data_pipelines`, `decide_data_pipeline_message`, `deciding-data-pipelines`) and 10f (`send_message`, `compose`, `MAIL`, `seller_mail`, `draft_seller_message`, `sellerLead`, `mailboxDisclose`), each ready and not yet executed: execution starts only after their commits exist. Step 10b2 (`site.requested`, the approved sites); step 08f (`plan_approved`, `plan_tools`, `NoActivePlan`); step 08d (`farik_schedule_post`, `record`, `fits_the_plan`, `social_posts`, `hand_over_posts`, `PostGoingOut`); step 08c (`marketing_plan_decide`); step 07c (the Product Manager's GitHub entry, `using-product-sources`); step 05b (allowances, `calls_made`, `AllowanceCounts`, `allowances.list`, `ConnectorAllowance`, `KitConnect`'s how-many step); step 02 (`judge_connector`, `grant_for`, `ask`, `APPROVAL_NEEDED`); phase 6 step 15 (`TeamRules.tsx`, a rule's effect shown first); phase 6 (merged in #19). File:line citations are at 7aedef8; the names are what count.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-07 (one round, ADR 0032): not ready, 9 Blocking and the Should items, all folded below with the founder's answers; a confirming second round follows
Mockups approved by: pending
Decided by the founder, 2026-10-07, in conversation (this plan's readiness review): the line ending a seller message the team sends on its own, "No AI line" (under `auto` such a message ends with the owner's name and carries no line saying an AI assistant wrote it; under `ask`, step 10f's approved line stays); credit-spending tools with no allowance (most of Higgsfield's, two of Recraft's), "Yes, give each a limit" (each gets a number the owner can change on the agent page, and on `auto` stops at it); when the team uses up an allowance on `auto`, "Yes, with 'Raise it'" (one line on Today names the limit reached, with "Raise it", which opens the allowance editor). ADR 0041 is amended the same day.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The founder's decision of 2026-10-05 (ADR 0041): "the user can configure whether he needs to approve everything or set it on auto mode". After this step the team rules page holds one choice, "Ask me before anything leaves Farik" (the default) or "Let the team act on its own", kept on this computer and never in the project's files. On auto, a connector's outward call runs without asking, a message to a seller is sent when drafted (without the AI line), a post outside the marketing plan is scheduled as a plan's post is, with its Stop, and the Product Manager may approve a data source that costs money; each is listed under "Done on its own". Every allowance becomes a limit that stops rather than asks, and every tool that spends credits has one. What the table below marks "owner" stays the owner's in both positions. Out of scope: a switch per connector or per tool; the DevOps Engineer's deploys and incidents (steps 11 to 12 honour the mode from their plans); paying, which no tool can.

## Decisions

- **Every outward act, in both positions** (this table is the single source; ADR 0041's amendment of 2026-10-07 carries it too):

| Act | `ask` | `auto` | Source |
|---|---|---|---|
| A connector's `external_effect` call with no allowance (a GitHub issue or comment; Kit's series, pages and blocks) | asks each time | runs, `approved_by: auto`, input whole | ADR 0041, 5.6 |
| A call within an allowance | runs, `allowance: <n>` | same | 05b |
| A call past an allowance | asks | refused `allowance_reached`; Today's limit line | ADR 0041, answer 3 |
| A credit-spending tool marked `asks_always` (below) | asks each time | runs up to its number, `approved_by: auto`, then `allowance_reached` | answer 2 |
| A Google Ads write inside the active plan | runs, `marketing_plan` | same | 0042, 08f |
| A Google Ads write with no active plan, or one the plan does not cover | refused (`no_active_marketing_plan`, `not_in_marketing_plan`) | same, never run | 08f, 5.6 |
| Approving, returning or ending a marketing plan; raising its budget | owner | owner | 0042, 08c, 08g |
| A post in the plan | goes out, with Stop | same | 08d |
| A post outside the plan | waits for "Post it" | scheduled `approved_by: auto`, 3 hours ahead at least, with Stop | 0042, 08d |
| Removing Google Ads | owner | owner | 08g |
| A site the agent asks to read | owner | owner | 10b2 |
| A purchase order: approve, place, receive, close | owner | owner | 0039, 10c |
| An order's email ("Approve and send to <seller>") | owner | owner | 10f |
| A quote request, question or follow-up to a seller | the founder's Send | sent when drafted, no AI line, 50 a day at most | 10f, answer 1 |
| A data pipeline that is free | Product Manager | Product Manager (`by: product_manager`) | 10e |
| A data pipeline that is paid or of unknown cost | owner | Product Manager, `by: auto` | 10e, 0039 |
| A data pipeline that sends the project's data out, or one escalated or undecided | owner | owner | 10e |
| A connector's call from a session about no task | refused `external_effect_refused` | same | 5.6 |
| A connector's call whose input is over 64 KiB | refused `tool_input_too_large` | same | 5.6 |
| An act already waiting on Today when the mode changes | waits for the owner | waits for the owner | below |
| Contracts, epics and acceptance; adding a skill or a connector | owner, per the team's rules | same | 5.4, 5.16, 6.7, 0034 |
| An integration push or pull request; an agent's push (`git_remote`) | team policy, tier | same | 5.14, 5.6 |
| Spending limits, the marketing budget, session limits, the 50-a-day cap | hold | hold | 5.5, 0042, 10f |
| The DevOps Engineer's deploys and incident steps | steps 11 to 12 | steps 11 to 12 | out of scope |

- **Where the mode lives** (Blocking 1). In this computer's log alone, never in `team.yaml`: the file travels with the repository (8.6), the web app sends the whole team on every `team.save`, and people edit it by hand, so a pull, a stale page's save or a hand edit could turn auto on with no dialog and no event; 10b2 kept sites out of it for the same reason. It is set only by the command `approval_mode_set { mode: ask | auto }` (`command.schema.json`), on `POST /command` behind the daemon's token or the browser's RPC `command` behind its cookie, as `purchase_order_decide` is. Under `MODE`, a static lock in `orchestrator/human.rs` beside `DECIDING` (`human.rs:488`), it reads the mode and records `approval_mode.changed { from, to }` naming no agent and no session, or nothing when `mode` is the current one. `farik_store::approvals::approval_mode(log)` is the `to` of the latest such event, through the kind index, and `Ask` with none; one naming an agent or a session is ignored. `team.save` and templates carry no `approvals`: `$defs/policy` already has `additionalProperties: false`, so a team file naming `policy.approvals` is a schema error at `/policy/approvals`. The query `approvals.get {}` (`daemon/gates.rs`) answers `{ mode, since? }`, `since` the time of the change that set `auto`. Rejected: `policy.approvals` in `team.yaml`, the draft's. Step 11b consumes `approval_mode`, not `Team::acts_on_its_own` (its Depends on is corrected by this fold).
- **What `auto` replaces** (Blocking 2): only the ask. Every earlier refusal in 5.6's order still applies (sites, connector, tag, `denied`, the preview's `url`s, the Designer's plan gate, 64 KiB, `NoActivePlan` for a plan-marked tool); a grant and an allowance come first; a tool with an allowance is a limit; and a session about no task is still `external_effect_refused`, refused before the mode is read. `judge_connector` (`hooks.rs:561`) reads `approval_mode` only for an `external_effect` call that is not plan-marked and has no grant, so a change applies from the next call, unlike tiers and allowances, which apply from the next session (4.4); under `auto` it counts `calls_made` (`hooks.rs:653`) for every allowance, 0 included. `evaluate_connector_call` (`permissions.rs:270`) takes `mode: ApprovalMode` last. Under `Auto` a tool with no allowance passes `on_its_own`; one with an allowance passes inside it (`allowance`, or `on_its_own` for an `asks_always` tool) and past it is `AllowanceReached { used, of }`, a new code: `allowance_reached: <tool> of <server> has made <used> of the <of> the owner allows this <sprint|day>, and the team acts on its own, so it stops here; carry on without it and say so in your note`. Nothing waits on Today, and the session goes on.
- **One field per approval** (Should). An `external_effect` connector call's `tool.called` names exactly one of `approval` (a grant), `allowance` (it ran inside an allowance that runs it without asking under `ask` too), `marketing_plan` (the plan) or `approved_by` (it would have asked under `ask`). `approved_by` is `$defs/approvedBy` in `event.schema.json`, re-exported by `farik-protocol` as `ApprovedBy`, with the one value `auto` here; steps 11b and 11d add `sprint` and `incident`. Rejected: the draft's `ApprovedBy { Grant, Allowance, Auto }`, which said again what `approval` and `allowance` say and could disagree with them.
- **The whole input** (Blocking 3). An auto call's `tool.called` carries its input whole, as `tool_approval.requested` does (at most 64 KiB, the limit checked before it); every other call's input is still cut at 4 KiB (`RECORD_LIMIT_BYTES`, `hooks.rs:87`). `toolCalledBody`'s description says so.
- **A limit on every credit-spending tool** (answer 2). A kit allowance gains `asks_always: true`: under `ask` that tool asks every call whatever its number, as before (05b's "a batch tool … always asks" and step 08's landing review, "a voice-over always asks", stand); under `auto` its number is its limit and each call within it is `approved_by: auto`. `parse_kit` refuses `asks_always` on a tool with no allowance (`asks_always_without_allowance`). `SessionConnector` gains `asks_always`, filled at setup from the kit for an entry that `matches_kit`, as `plan_tools` is. An entry connected before this step lacks the new numbers (`matches_kit` takes allowances out), so an `asks_always` tool the entry gives no number is held at the kit's `calls`. The new allowances, each `asks_always`, `{ calls, what }` per sprint: Higgsfield `generate_audio` 5 "voice-overs", `generate_image_batch` 2 "image batches", `generate_video_batch` 1 "video batches", `generate_audio_batch` 1 "voice-over batches", `generate_3d` 2 "3D models", `upscale_video` 2 "video upscales", `reframe` 2 "reshaped videos", `motion_control` 2 "animated pictures", `dubbing` 1 "dubbed videos", `voice_change` 1 "voice changes", `ads_studio_generate` 1 "ad sets", `ads_studio_create_brand` 1 "brand studies", `ads_studio_add_product` 3 "products added for ads", `ads_studio_update_product` 3 "product changes for ads", `ai_influencer_generate` 1 "character sheets", `execute_preset` 2 "preset runs", `media_import_url` 5 "pictures brought in", `resolve_explainer_preset` 2 "explainer styles", `shorts_studio_create` 1 "shorts restyles", `shorts_studio_create_preset` 1 "saved shorts styles", `video_analysis_create` 2 "video analyses", `virality_predictor` 2 "reach predictions"; Recraft `creative_upscale` 5 "redrawn upscales", `create_style` 2 "brand styles". `ads_studio_cancel_run` gets none: it stops an ad set and spends nothing. Rejected: plain allowances for these (each would run without asking under `ask`, undoing step 08's landing review); 0 for every default (`auto` could use none until the owner raised each).
- **Seller messages** (Blocking 4, answer 1). `draft_seller_message` becomes `async`. Under `auto`, for a purpose other than `purchase_order`, after recording the draft and releasing `MAIL`, it calls `send_message(call.deps(), message, subject, body, None)` (`procurement.rs`), which takes `MAIL` and records `seller_message.sent { …, sent_by: auto }` on the task naming no agent and no session, as the founder's press does, so 10f's fold counts it and Today shows it sent. It answers `{ message, sent: true }`, or `{ message, sent: false, why }` when no mailbox is connected (`mailbox_not_connected`), 50 went today (`seller_send_limit`) or the server refused (10f's `seller_message.failed`); such a message stays a draft for the founder's Send and is never tried again by itself. A Send pressed in between finds it sent (`seller_message_sent`). `compose(settings, body, on_its_own: bool)`: an auto send is the body, a blank line, and the signature, or the mailbox's `name` when there is none, with no disclosure line whatever `disclose_ai` says; a send on the founder's press, in either position, is 10f's. `sent_by` is optional on `seller_message.sent`, `auto` alone. Rejected: a tick that sends drafts later, which could not tell the session whether its message went.
- **Posts outside the plan** (Blocking 5). An auto post keeps a plan post's rule: at least three hours ahead (`post_too_soon`, the check `fits_the_plan` makes, which `schedule_post` applies to a slotless post on `auto` before Buffer is asked), handed to Buffer an hour before its time, with Stop until then. `record` (`tools/posts.rs`) records `social_post.scheduled { approved_by: auto }` under the agent's session, with no `plan` and no `slot`, and answers as for a plan's post. `approved_by` gains `auto` in `event.schema.json`; the store's fold (`store/src/marketing.rs:576`) takes `approved_by` from the body instead of writing "plan". Under `ask` it is 08d's `social_post.requested`.
- **Data pipelines** (Should). `pipeline_needs_owner(cost, sends_project_data, mode)`: true when `sends_project_data`, and for `paid` and `unknown` under `Ask` alone. Under `auto` the Product Manager's `approve` of a `paid` or `unknown` request records `data_pipeline.approved { by: auto }`, filed in the Product Manager's name as its own approval is; a `free` one stays `by: product_manager`; the fold accepts `by: auto` only from that request's decision session. `decide_data_pipeline_message` gives the rule of the mode at the session's start (Task 9's words).
- **What already waits keeps waiting** (Blocking 6). Anything waiting on Today when the mode changes keeps waiting for the owner; the mode applies to acts decided after the change. No change of mode sends a draft, schedules a request, grants an approval or decides a pipeline.
- **"Done on its own"**. `auto_acts.list { since? }` (`daemon/gates.rs`), from `farik_store::auto_acts`, reads by kind through `EventQuery` the acts of the last 7 days: `connector_call` (`tool.called` with `approved_by: auto`), `seller_message` (`seller_message.sent` with `sent_by: auto`), `post` (`social_post.scheduled` with `approved_by: auto`), `data_pipeline` (`data_pipeline.approved` with `by: auto`); newest first, at most 200, with `seq` above `since`, which defaults to the last look. It answers `{ acts: [{ seq, at, kind, agent, task?, server?, tool?, input?, seller?, to?, subject?, body?, channel?, post?, text?, pipeline?, name?, reason?, request? }], seen_up_to }`, every field an agent wrote shown as untrusted text. The page records a look when its list loads: `auto_acts_seen { up_to }`, the human's alone, records `auto_acts.seen { up_to }` naming no agent and no session, or nothing when `up_to` is not above the last. Rejected: local storage, which a phone and a computer would not share.
- **No migration**: the acts and the mode are read from the log by kind (`EventQuery`, `crates/store/src/event_log.rs:59`). `EVERY_KIND` gains `approval_mode.changed` and `auto_acts.seen` (101 to 103 after 10f; the number the code holds when Task 1 starts is what counts).
- **The command line**: `farik rules approvals [ask|auto] [--yes]` (`RulesCommands::Approvals`, beside `RulesCommands::Show`, `cli/src/lib.rs:861`; `crates/cli/src/team.rs`) through `here_or_sent` (`cli/src/start.rs:109`): no mode prints the current one; `auto` without `--yes` prints the turn-on dialog's sentences, one a line, and changes nothing (exit 2); with it, and `ask` always, it sends `approval_mode_set`.

## The web app's words

As Task 0's boards, on `canvas.json`'s pages "Team and settings" (the card, the dialog) and "Ask or auto" (Today, "Done on its own"). What an agent wrote goes through `visibly`.

- **The rules card** (`ApprovalsCard`, `TeamRules.tsx`, after "Planning work"): "Approvals" (`rulesApprovals`); two choices, "Ask me before anything leaves Farik" (`approvalsAsk`) with "The team stops and waits for your yes before it sends, posts or changes anything outside Farik." (`approvalsAskNote`), and "Let the team act on its own" (`approvalsAuto`) with "The team goes ahead without asking, inside your limits, and Today lists what it did." (`approvalsAutoNote`); "Kept on this computer, not in your project’s files. It saves as soon as you choose, and applies from the team’s next action." (`approvalsHere`); on auto, "Your team has acted on its own since {day}." (`approvalsSince`). Choosing auto opens the dialog; choosing ask sends `approval_mode_set` at once and shows "Saved. The team asks you again from its next action." (`approvalsAskSaved`). Neither is part of "Save changes".
- **The turn-on dialog** (`dialogs/AutoMode.tsx`; Blocking 7): "Let the team act on its own?" (`autoTitle`); "Without asking you first, the team will:" (`autoDoes`) over "use the services you connected to change things outside Farik, such as filing a GitHub issue or changing a Kit email series" (`autoDoesCalls`), "post on your social channels outside your marketing plan, at least three hours ahead, each shown on Today with Stop until an hour before it goes" (`autoDoesPosts`), "send its messages to sellers as soon as they are written, up to 50 a day, without the line that says an AI assistant wrote them" (`autoDoesSellers`), "let the Product Manager approve a data source that costs money, or whose cost is unknown" (`autoDoesData`), "use your credits on services such as Higgsfield and Recraft up to the number on each agent’s page, then stop instead of asking for more" (`autoDoesCredits`); "These still wait for you:" (`autoStill`) over "purchase orders, and the email that sends an order" (`autoStillOrders`), "marketing plans and their budgets; Google Ads only ever runs inside a plan you approved" (`autoStillPlans`), "a site the Procurement Specialist asks to read" (`autoStillSites`), "a data source that sends your project’s data out, and one the Product Manager passes to you" (`autoStillData`), "requests, plans and finished work, as your team’s rules say, and the skills and connectors you add" (`autoStillWork`); "Your spending limits, the marketing budget and 50 messages a day still hold." (`autoLimits`); "If a web page, an email or a file misleads an agent, only these limits stop what it does next. Check what the team did under “Done on its own” on Today." (`autoGuard`); "You can turn this off at any time on this page. It applies from the team’s next action, and anything already waiting for you keeps waiting." (`autoOff`); "Turn on" (`autoTurnOn`), "Keep asking" (`autoKeepAsking`), which changes nothing.
- **Today on auto**: at the top, "Your team acts on its own: it goes ahead without asking you, inside your limits." (`todayAuto`) with "Change" (`todayAutoChange`, to the rules), and "{count} things done on its own since you last looked." / "1 thing done on its own since you last looked." / "Nothing new done on its own since you last looked." (`todayDone`, `todayDoneOne`, `todayDoneNone`) with "See what it did" (`todayDoneSee`); under ask, none of it. On auto alone, for each `allowances.list` row with `of` above 0 and `used` at or past it: "{name} has made {used} of {of} {what} {period}, the most you allow, so it stops there." (`todayLimit`, `{period}` 05b's `allowApprovalSprint` or `allowApprovalDay`) with "Raise it" (`todayRaiseIt`), which opens `ConnectorAllowance` for that agent and service. "Messages to sellers" leads with "Your team acts on its own, so Farik sends each message as soon as it is written. These could not go: send each one yourself, or discard it." (`sellerLeadAuto`) instead of `sellerLead`. "Going out" leads with "Your team acts on its own, so these posts go out without asking you. Stop any of them before its time." (`goingOutLeadAuto`), and an auto post's row says "Outside your plan, posted on its own" (`postOnItsOwn`) where a plan's says `postApprovedBefore`, then 08d's hand-over line and Stop.
- **"Done on its own"** (`pages/DoneOnItsOwn.tsx`, route `/done`): "Done on its own" (`autoActsTitle`) under "Back to Today" (`autoActsBack`); "What your team did without asking you in the last 7 days, newest first. What an agent wrote is shown as text, exactly as it went out." (`autoActsLead`); "New" (`autoActsNew`) on acts above the last look; rows "{name} used {service}: {label}" (`autoActCall`, the kit's label, else the tool's name), "{name} wrote to {seller}" (`autoActMessage`), "{name} posted on {network}, outside your plan" (`autoActPost`), "{name} approved a data source for {agent}: {source}" (`autoActData`), each with its day and time and "Show what went out" / "Hide" (`autoActShow`, `autoActHide`). The detail: a call's "What {name} sent to {service}" (`autoActInput`) over its input as indented JSON in an `untrusted` frame, and "Tool: {tool}" (`autoActTool`, code face); a message's To, Subject and `sellerBody` in an `untrusted` frame, with "Sent without the AI line, because your team acts on its own." (`autoActNoLine`); a post's text and pictures in an `untrusted` frame with 08d's state line, and Stop while it can be stopped; a data source's "{pm}’s reason" (`autoActReason`) in an `untrusted` frame and "Filed as {request}" (`autoActFiled`); "Nothing in the last 7 days." (`autoActsNone`).

## File map

```
docs/design/mockups/{SettingsApprovals,PhoneSettingsApprovals,AutoMode,PhoneAutoMode,TodayOnItsOwn,PhoneTodayOnItsOwn,DoneOnItsOwn,PhoneDoneOnItsOwn}.dc.html, canvas.json   creates, modifies (Task 0)
docs/schemas/{event,command,rpc}.schema.json, crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs,command.rs}   modifies: the kinds, approvedBy, sent_by, the commands, the queries (Tasks 1, 2, 4 to 7)
crates/core/src/governor/permissions.rs                          modifies: ApprovalMode, the auto path, AllowanceReached, asks_always (Tasks 2, 3)
crates/store/src/{approvals.rs,auto_acts.rs,lib.rs}              creates: approval_mode (Task 1), auto_acts (Task 7)
crates/runtime/src/orchestrator/human.rs, crates/runtime/src/daemon/gates.rs   modifies: approval_mode_set, approvals.get (Task 1); auto_acts_seen, auto_acts.list (Task 7)
crates/runtime/src/daemon/hooks.rs, crates/runtime/src/orchestrator/session.rs   modifies: the mode in judge_connector, the whole input (Task 2); asks_always at setup (Task 3)
crates/roles/src/kit.rs, crates/roles/roles/marketing_specialist/kit.yaml   modifies: asks_always and the 24 allowances (Task 3); the copy (Task 9)
crates/runtime/src/tools/seller.rs, crates/runtime/src/procurement.rs, crates/store/src/seller_mail.rs   modifies: the auto send, compose (Task 4)
crates/runtime/src/tools/posts.rs, crates/store/src/marketing.rs modifies: the auto post, its fold (Task 5)
crates/core/src/pipeline.rs, crates/runtime/src/tools/pipeline.rs, crates/store/src/pipelines.rs, crates/runtime/src/orchestrator/messages.rs   modifies (Task 6)
crates/cli/src/{lib.rs,team.rs}, crates/cli/tests/human.rs       modifies, tests: farik rules approvals (Task 8)
crates/roles/roles/{product_manager,marketing_specialist}/{kit.yaml,skills/*/SKILL.md}, crates/roles/src/lib.rs, crates/runtime/src/tools.rs   modifies: the copy (Task 9)
apps/web/src/pages/{TeamRules,Today,PostGoingOut,DoneOnItsOwn,KitConnect,ConnectorAdd,allowances}.tsx, dialogs/{AutoMode,ConnectorAllowance}.tsx, strings/en.ts, app/routes   modifies, creates (Task 10; en.ts's rewordings Task 9)
apps/web/src/pages/{team,Today,allowances,connectors,DoneOnItsOwn}.test.tsx, dialogs/AutoMode.test.tsx   tests (Tasks 9, 10)
docs/SPEC.md, docs/design/{role-kits,marketing-specialist,procurement-specialist}.md, docs/plans/project-plan.md   modifies (Task 11)
```

## Interfaces

Consumes: `judge_connector`, `grant_for`, `ask`, `calls_made`, `RECORD_LIMIT_BYTES` (`daemon/hooks.rs`); `evaluate_connector_call`, `SessionConnector`, `ConnectorPass` (`farik-core`); `Kit`, `KitAllowance`, `parse_kit` (`farik-roles`), `matches_kit` (`daemon/team.rs`); `send_message`, `compose`, `MAIL`, `draft_seller_message` (10f); `schedule_post`, `record`, `fits_the_plan`, `social_posts` (08d); `pipeline_needs_owner`, `farik_decide_data_pipeline`, `data_pipelines`, `decide_data_pipeline_message` (10e); `EventLog`, `EventQuery` (`farik-store`); `here_or_sent` (`farik-cli`); `TeamRules`, `ConnectorAllowance`, `PostGoingOut`, `visibly` (web).

Produces:

```rust
pub enum ApprovalMode { Ask, Auto }                                          // farik_core::governor::permissions
pub struct SessionConnector { /* …, */ pub asks_always: BTreeSet<String> }
pub struct ConnectorPass { pub tag: ConnectorTag, pub approval: Option<u64>, pub allowance: Option<u32>, pub plan_approved: bool, pub on_its_own: bool }
// ConnectorRefusal gains AllowanceReached { used: u32, of: u32 }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>, granted: Option<u64>,
    used: u32, active_plan: bool, mode: ApprovalMode) -> Result<ConnectorPass, ConnectorRefusal>;
pub struct KitAllowance { pub calls: u32, pub what: String, pub asks_always: bool }   // farik_roles
pub fn pipeline_needs_owner(cost: PipelineCost, sends_project_data: bool, mode: ApprovalMode) -> bool;   // farik_core::pipeline
pub fn approval_mode(log: &EventLog) -> Result<ApprovalMode, StoreError>;     // farik_store::approvals
pub enum AutoActKind { ConnectorCall, SellerMessage, Post, DataPipeline }    // farik_store::auto_acts
pub struct AutoAct { pub seq: u64, pub at: DateTime<Utc>, pub kind: AutoActKind, pub agent_id: String, pub task_id: Option<TaskId>, pub detail: Value }
pub fn auto_acts(log: &EventLog, from: DateTime<Utc>, since: Option<u64>) -> Result<(Vec<AutoAct>, u64), StoreError>;   // (acts, seen_up_to)
pub(crate) async fn draft_seller_message(call: &Call<'_>, input: &DraftSellerMessageInput) -> Result<Value, ToolError>;
pub(crate) fn compose(settings: &MailboxSettings, body: &str, on_its_own: bool) -> String;
// ApprovedBy ($defs/approvedBy): Auto, on ToolCalledBody.approved_by; SellerMessageSentBody.sent_by: Option<…> (auto);
// SocialPostScheduledBodyApprovedBy and data_pipeline.approved's `by` gain auto;
// Command gains ApprovalModeSet { mode } and AutoActsSeen { up_to }; kinds approval_mode.changed { from, to }, auto_acts.seen { up_to }
```

## Tasks

Each test is watched to fail for the reason given, before the code that satisfies it.

### Task 0: Mockups

The rules card, the turn-on dialog, Today on auto and "Done on its own", desktop and phone, approved by the founder.

- [ ] `docs(design): mock up ask or auto`

### Task 1: The mode, in this computer's log

- `a_project_without_a_change_asks` (store): `approval_mode` is `Ask` for a log with no `approval_mode.changed`. RED: no `farik_store::approvals`.
- `the_latest_owners_change_is_the_mode` (store): after changes to `auto` then `ask` it is `Ask`; a change to `auto` naming an agent or a session leaves it `Ask`. RED: the fold reads every envelope.
- `setting_the_mode_records_one_change` (runtime): `approval_mode_set { mode: auto }` records `approval_mode.changed { from: ask, to: auto }` with no agent and no session; a second one records nothing; `approvals.get` answers `auto` and `since`. RED: no such command.
- `a_team_file_cannot_hold_the_mode` (core): a team file with `policy.approvals: auto` is a schema error at `/policy/approvals`. A guard: watched to fail by adding `approvals` to `$defs/policy`, then reverted.
- `round_trips_the_new_kinds` (protocol): both kinds validate and read back equal; `EVERY_KIND` counts two more; `to: maybe` is refused. RED: no such kinds.

- [ ] `feat(runtime): keep ask or auto in this computer's log`

### Task 2: A connector's outward call on auto

- `auto_runs_an_external_effect_call_without_asking`: under `auto` a GitHub `issue_write` with no grant records no `tool_approval.requested`, is allowed, and its `tool.called` has `approved_by: auto`, no `approval`, `allowance` or `marketing_plan`, and its 10 KiB input whole. RED: it is `approval_needed`.
- `ask_still_waits`: the same call under `ask` is `approval_needed` and records `tool_approval.requested`. A guard against a default flip, watched to fail with `Auto` as the fold's default.
- `a_grant_is_used_before_auto`: under `auto` a call with an open grant records `approval: <seq>` and no `approved_by`. RED: `approved_by` set beside it.
- Under `auto`, each the refusal it names, nothing waiting: `the_designers_plan_gate_still_refuses_first` (`design_plan_not_approved`), `a_denied_tool_stays_denied_on_auto` (`tool_denied`), `an_input_over_64_kib_is_refused_on_auto` (`tool_input_too_large`, only `tool.denied` recorded), `a_session_about_no_task_is_refused_on_auto` (`external_effect_refused`), `a_plan_marked_write_without_a_plan_is_refused_on_auto` (`no_active_marketing_plan`). RED each: the mode read before that refusal.
- `switching_back_to_ask_applies_from_the_next_call`: in one running session, a call after `approval_mode_set { mode: ask }` is `approval_needed`. RED: the mode fixed at registration.

- [ ] `feat(runtime): run a connector's outward call on auto`

### Task 3: Allowances as limits, and a limit on every credit-spending tool

- `past_the_allowance_auto_stops_rather_than_asks`: under `auto` the 51st SerpApi `search` of a sprint is `allowance_reached` with the sentence above, records no `tool_approval.requested`, and does not stop the session. RED: it asks.
- `within_the_allowance_nothing_changes`: the 50th records `allowance: 50` in both positions. RED: `approved_by` set.
- `an_allowance_of_zero_stops_every_call_on_auto`: the first call is `allowance_reached` with "0 of the 0". RED: no count read at 0, so it asks.
- `every_credit_tool_has_a_limit` (roles): Higgsfield's and Recraft's `external_effect` tools all carry an allowance but `ads_studio_cancel_run`: the 24 above with exactly those numbers and `asks_always`, the 11 before without it. RED: 23 and 2 have none.
- `asks_always_needs_an_allowance` (roles): `parse_kit` refuses it on a tool with none (`asks_always_without_allowance`). RED: accepted.
- `a_voice_over_still_asks_under_ask`: `generate_audio` with 5 left is `approval_needed` under `ask` and `approved_by: auto` under `auto`; the sixth on auto is `allowance_reached`. RED: it runs inside the number under `ask`.
- `an_entry_connected_before_is_held_at_the_kits_number`: an entry with no `generate_audio` allowance stops on auto after 5. RED: unlimited.

- [ ] `feat(runtime): hold every allowance as a limit on auto`

### Task 4: Messages to sellers on auto

- `a_drafted_message_is_sent_on_auto`: the fixture's SMTP receives it ending with the signature and no disclosure line; `seller_message.sent` has `sent_by: auto` and no agent or session; `seller_messages.list` shows it `sent`; the tool answers `sent: true`. RED: it waits.
- `without_a_signature_it_ends_with_the_name`: the body, a blank line, then the mailbox's name. RED: it ends with the body.
- `a_press_still_carries_the_line`: the founder's Send of a draft that could not go, under `auto`, carries 10f's line. RED: the mode, not the press, decides.
- `what_cannot_go_stays_a_draft`: with no mailbox, at the 51st of the day, and on a refused login, the tool answers `sent: false` with the reason, and the draft waits for Send. RED: lost or retried.
- `an_orders_message_is_not_sent_on_auto`: a `purchase_order` draft records no `sent`. RED: sent.
- `a_draft_from_before_the_change_keeps_waiting`: a draft made under `ask` is not sent when the mode turns `auto`. RED: sent on the change.

- [ ] `feat(runtime): send a drafted message to a seller on auto`

### Task 5: Posts outside the plan on auto

- `a_post_outside_the_plan_is_scheduled_on_auto`: with no slot, four hours ahead, it records `social_post.scheduled { approved_by: auto }` under the agent's session and no `requested`; the tick an hour before hands it over; Stop before then takes it back. RED: `requested`.
- `an_auto_post_keeps_the_three_hours`: two hours ahead is `post_too_soon`, recording nothing. RED: scheduled.
- `the_fold_reads_who_approved_it` (store): the post's `approved_by` is `auto`, not `plan`. RED: `plan` (`marketing.rs:576`).
- `refuses_a_social_post_body_that_breaks_a_rule_of_its_kind` (protocol, `event.rs:2124`) loses its `auto` case, and `round_trips_an_auto_post` reads one back. RED: the schema refuses `auto`.
- `a_request_from_before_the_change_keeps_waiting`: a `requested` post stays on `waiting.list` after the change. RED: scheduled on the change.

- [ ] `feat(runtime): schedule a post outside the plan on auto`

### Task 6: Data pipelines on auto

- `the_owner_decides_what_costs_money_or_sends_data` (core) gains: under `Auto`, `(Paid, false)` and `(Unknown, false)` are false; `(Free, true)` and `(Paid, true)` true. RED: no mode.
- `the_product_manager_approves_a_paid_pipeline_on_auto`: its `approve` records `approved { by: auto }` and files the request; a `free` one records `by: product_manager`; one that sends the project's data out is `pipeline_needs_owner`. RED: refused.
- `only_the_decision_session_approves_on_auto` (store): `by: auto` from any other session changes nothing. RED: counted.
- `an_escalated_request_waits_on_auto`: escalated before or after the change, it stays on `waiting.list`. RED: decided.
- `the_message_says_the_rule_for_the_mode`: `decide_data_pipeline_message` holds Task 9's sentence under `auto`, 10e's under `ask`. RED: one text.

- [ ] `feat(runtime): let the Product Manager approve a paid data source on auto`

### Task 7: Done on its own

- `lists_each_kind_newest_first`: one act of each kind, beside a `tool.called` with `allowance` and one with `approval`, gives the four, newest first, with their fields, the input whole. RED: no query.
- `since_defaults_to_the_last_look`: after `auto_acts_seen { up_to: <the second's seq> }` the default answer holds the newer two and `seen_up_to`; `since: 0` holds all four; an act older than 7 days, none. RED: no `auto_acts.seen`.
- `a_look_never_goes_back`: an `up_to` below the last records nothing. RED: recorded.
- Guards under `auto`, each watched to fail by deleting that act's check: `a_site_request_still_waits`, `a_marketing_plan_still_waits`, `a_purchase_order_still_waits` (only `purchase_order_decide` records `approved`), and `none_of_these_is_listed` (none is in `auto_acts.list`).

- [ ] `feat(runtime): list what the team did on its own`

### Task 8: The command line

- `rules_approvals_auto_needs_yes`: without `--yes` it prints the dialog's sentences, exits 2 and records nothing; with it the log holds the change; `ask` needs none; with no mode it prints `ask` or `auto`; a running daemon receives the command. RED: no subcommand.

- [ ] `feat(cli): set ask or auto`

### Task 9: The words that said every call asks (Blocking 8)

Each sentence true in both positions, old → new; the tests that pin them change with them (`github_for_the_product_manager_files_issues_only_when_asked`, `buffer_posts_only_through_farik`, `the_marketing_skills_keep_the_allowance_lines_and_name_what_is_owned`, `allowances.test.tsx`, `connectors.test.tsx`), each watched to fail on the new words first.

- Product Manager kit, GitHub `why`: "… and file an issue or a comment when you say yes." → "… and file an issue or a comment: Farik asks you first, unless your team acts on its own."; `setup`: "Farik asks you before each issue or comment it posts." → "Farik asks you before each issue or comment it posts, unless your team acts on its own."
- `using-product-sources`: description "… and ask before you write to GitHub." → "… and write to GitHub only when asked."; "## 4. You ask before you write" → "## 4. Before you write"; "- Each call waits for the human, who sees it whole." → "- Each call goes out as you write it: the human reads it first, or, when the team acts on its own, nobody does."
- Marketing kit, Higgsfield and Recraft `why`: "… so Farik counts them and asks you before going past the number you allow." → "… so Farik counts them and stops at the number you allow: past it, Farik asks you first, or, if your team acts on its own, makes no more."; `setup`: "you choose how many it may make without asking." → "you choose how many it may make without asking, and the most it may make if your team acts on its own."; Buffer `why` and `setup`: "asks you about any other" → "asks you about any other, unless your team acts on its own"; Kit `setup`: "asks you before changing a page or an email series," → "asks you before changing a page or an email series unless your team acts on its own,".
- Skills: `planning-a-launch` (twice), `marketing-what-ships`, `keeping-a-content-calendar`, `making-images-and-video` §6: "any other post waits for the owner" → "any other post waits for the owner unless the team acts on its own". `running-social-channels` §1, after "they allow it": "When the team acts on its own, it goes out as a plan's post does, at least three hours ahead, with the owner's Stop." `making-images-and-video`: the intro's "and asks the user before you go past the number they allowed." → "and stops you at the number the user allowed: past it, the user is asked, or, when the team acts on its own, nothing more is made."; §5 "## 5. Some things always wait for the user" → "## 5. Some things wait for the user", "A voice-over always waits for the user" → "A voice-over waits for the user", and, at the paragraph's end, "When the team acts on its own, each runs without asking up to its own number, then stops." `posting-and-email`: "one beyond it waits for the user." → "one beyond it waits for the user, or, when the team acts on its own, is not made."; "## 4. Series and pages wait for the user" → "## 4. Series and pages"; "asks the user each time." → "asks the user each time, unless the team acts on its own." `deciding-data-pipelines` gains: "When the team acts on its own, Farik lets you approve a source that costs money or whose cost is unknown. Approve one only when its price is worth what it changes, and say the price in your reason. One that sends the project's data out still goes to the owner." `decide_data_pipeline_message` under `auto`: "The team acts on its own, so you may approve a source that costs money or whose cost is unknown. A source that sends the project's data out still needs the owner: decline it or escalate it."
- Tool descriptions (`tools.rs`): `farik_schedule_post`'s "… without a slot it waits for the owner's yes." → "… without a slot it waits for the owner's yes, or, when the team acts on its own, goes out as a plan's post does, at least three hours ahead."; `farik_draft_seller_message`'s gains "When the team acts on its own, Farik sends it at once, an order's message excepted, and tells you whether it went."
- Web (`en.ts`): `tagExternal` "Changes things, asks you" → "Changes things outside Farik"; `tagExternalNote` gains " If your team acts on its own, {name} goes ahead, and Today lists each use."; `kitAsks` → "{name} asks you first, unless your team acts on its own, before"; `allowQuestion` → "How many may {name} make each sprint?"; `allowSpends` → "Each one uses your {service} credits. Up to these numbers, {name} goes ahead without asking. Past them, {name} asks you first, or stops if your team acts on its own."; `allowRange` → "0 means {name} asks every time, or makes none if your team acts on its own. Up to 1,000 each."; `allowAlwaysAsk` → "Tools that publish or post ask you every time, unless your team acts on its own."; new `allowAsksAlways`, under an `asks_always` tool's field: "Asks you every time. The number counts only if your team acts on its own."; `allowBoardAsks` → "{name}: {used} of {of} {what}. {name} asks you before making more, or stops if your team acts on its own."; `marketingAllowsNothing` → "Nothing else. Another campaign or a bigger budget needs a new plan from you, and another post asks you first unless your team acts on its own."; `tierExternalNote` → "Sending mail, posting, deploying. You approve each one, unless you let the team act on its own."; `mayExternal` → "acts outside your computer, with your approval or, if you allow it, on its own"; under 10f's `mailboxDisclose`, new `mailboxDiscloseAuto`: "If your team acts on its own, Farik sends without this line, ending with your name."

- [ ] `feat(roles): word the kits for ask and auto`

### Task 10: The web app

- `the_rules_card_shows_the_mode` (`team.test.tsx`): `approvals.get` `ask` checks `approvalsAsk`; `auto` checks `approvalsAuto` and shows `approvalsSince`. RED: no card.
- `choosing_auto_shows_the_dialog_first`: choosing `approvalsAuto` sends nothing and opens `AutoMode` with every sentence above; "Turn on" sends `approval_mode_set { mode: "auto" }` once; "Keep asking" sends nothing and leaves `approvalsAsk` checked. RED: no dialog.
- `choosing_ask_saves_at_once`: sends `{ mode: "ask" }` with no dialog and shows `approvalsAskSaved`. RED: it waits for "Save changes".
- `today_shows_the_mode_and_whats_new_on_auto` (`Today.test.tsx`): `todayAuto` and "3 things done on its own since you last looked." on auto, neither on ask. RED: nothing shown.
- `today_names_a_limit_reached_with_raise_it`: a row at 20 of 20 on auto shows `todayLimit`, and "Raise it" opens `ConnectorAllowance` for that agent and service; on ask, and for a row at 0 of 0, no line. RED: no line.
- `the_sellers_and_posts_lead_for_the_mode`: on auto, `sellerLeadAuto` and `goingOutLeadAuto`, and an auto post's row says `postOnItsOwn` with Stop. RED: `sellerLead`.
- `done_on_its_own_shows_each_act_as_text` (`DoneOnItsOwn.test.tsx`): the four kinds' rows; "Show what went out" shows a call's input in an `untrusted` frame, a `<script>` in it as text; acts above `seen_up_to` say "New"; loading sends `auto_acts_seen { up_to: <the newest> }` once. RED: no page.
- `the_allowance_editor_marks_what_always_asks` (`allowances.test.tsx`): an `asks_always` field carries `allowAsksAlways`. RED: none.

- [ ] `feat(web): choose ask or auto, and see what was done on its own`

### Task 11: Spec and plan

`docs/SPEC.md`: 5.6 (the tier table's `external_effect` row; the order paragraph: `auto` replaces only the ask, after the allowance; a session about no task and 64 KiB refused in both; the four fields of `tool.called`); 5.12 (the mode sits beside the rules but in the log, not `team.yaml`); 4.4 (it applies from the next call); 5.7 (what waits keeps waiting; Today's lines); 6.1 and 6.10 (pipelines and messages on auto; an auto message carries no AI line, by the founder's choice); 6.5 (posts outside the plan on auto); 6.7 (allowances as limits; `asks_always` and the 24 numbers; the copy); 8.5 (`approval_mode.changed`, `auto_acts.seen`, `approved_by`, `sent_by`, `by: auto`); 8.6 (the mode's home and why; on auto a misled agent acts within the limits alone, and a message goes out with no human reading it, no AI line, to any address); F3, F15; the revision line. `docs/design/role-kits.md` (row 10h, the Marketing row's limits), `marketing-specialist.md` (posts on auto), `procurement-specialist.md` (sending on auto, no AI line). Project plan row 10h.

- [ ] `docs(spec): record ask or auto`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check, and the mail fixture's tests)
```

Then, in the web app, by the founder: turn auto on and read the dialog; have the Procurement Specialist draft a quote request and see it sent without the AI line and listed under "Done on its own"; have the Marketing Specialist post outside the plan and Stop it on Today; use up a small SerpApi allowance and see Today's line and "Raise it"; have it try a Google Ads change with no plan and see it refused; draft a purchase order and see it wait; leave a GitHub issue waiting, turn auto off, and see it still waiting and the next message wait for Send.

## Execution notes

None yet.
