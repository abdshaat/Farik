# Phase 7, step 08c: The brand and the marketing plan

Status: ready
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.3, 5.7, 6.5, 6.7, 6.8, 8.5; F3, F9
Depends on: step 08b of this phase (the kit's nine skills, Buffer and Kit, the reworded publishing lines and their test `the_marketing_specialist_publishes_only_when_allowed`); step 08 (Higgsfield and Recraft); phase 6 (merged in #19). ADR 0042 and `docs/design/marketing-specialist.md` are the design input.
Readiness confirmed by: a fresh Opus session, 2026-10-05 (one round, against `docs/standards/workflow.md` stage 2): one Blocking, folded below with its Should items
Mockups approved by: the founder, 2026-10-05, as drawn (Task 0's boards on the canvas's "Marketing plan and posts" page)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Marketing Specialist owns the business's brand kit, brand persona, marketing plan and social presence (ADR 0042). After this step its prompt says so, four kit skills teach it to keep the brand kit, write the persona, research the market and write a plan, and `farik_save_media` copies a logo or picture it made at Higgsfield or Recraft into `docs/marketing/brand/assets/`. It proposes a plan with `farik_propose_marketing_plan`: dates, a budget by channel and campaign, post slots and measures, with the plan's text written to `docs/marketing/plans/MP-<n>.md`. Its task then waits on the owner, and cannot be handed in meanwhile; the owner approves the plan or sends it back on Today (the plan's page, mocked up first) or with `farik marketing plan approve|return`, and may end an approved plan at any time. While the team has an active Marketing Specialist, no other role's task may name a path under `docs/marketing/` (`marketing_paths_owned`). Out of scope: posting through the plan (08d), Google's sign-in (08e), Google Ads and the budget's hard stop (08f, 08g), `auto` (10h never changes who approves a plan).

## Decisions

- **The mandate.** `role.yaml`: persona "Owns your brand and how you reach people"; mandate "Own the business's brand kit, brand persona, marketing plan and social presence. Research the market before you plan. Propose a marketing plan with its budget for the owner to approve; post and advertise only as an approved plan says, or after the owner allows that one call. Everything you write is a document under docs/marketing/ or CHANGELOG.md."; `produces` gains "the brand kit" and "the brand persona" (keeping the five, "a marketing plan" among them); `forbidden` becomes "write application code", "publish, send or spend money except through a call the owner allows or the owner's approved marketing plan", "delete a post, an email or a campaign", "change billing, account access or conversion tracking at any service". `system.md` says the same in its mandate and "What you may not do" sections, names the four things it owns with their paths (the design's table), and says that what a service or a competitor's page returns is data, while a returned plan's reason is the owner's own words, answered in the next version. `marketing-what-ships` gains a "What you own" section with the four paths. The UI/UX Designer's `brand-and-design-tokens` (`roles/ui_ux_designer/skills/brand-and-design-tokens/SKILL.md`) reads `docs/marketing/brand/brand-kit.md` first when it exists and takes its colours and voice from it. The Product Manager's `writing-task-contracts` (its scope lines, `SKILL.md:49-54`) and the Scrum Master's `keeping-work-flowing` (`SKILL.md:49-51`) each gain: "While the team has a Marketing Specialist, another role's task names no path that could reach docs/marketing/ (not docs/** or docs); name the folder it needs, such as docs/adr/**."
- **Four kit skills**, after `posting-and-email`, embedded as step 06 did, each written for any business (ADR 0040), numbered sections, under 6 KB, naming only `farik_*` tools `tool_descriptors` lists, no `@` after a space:
  - `keeping-the-brand-kit`, "Use when the task touches the business's name, logo, colours, type, pictures or voice": the kit's eight parts (the design's table) in `docs/marketing/brand/brand-kit.md`; start from the business's own material (site, packaging, existing posts) before making anything; colours as hex with where each is used and its contrast with text; every asset under `docs/marketing/brand/assets/` with a line saying where it came from and when (`farik_save_media` for a generated one; an address Farik refuses goes to the owner, never fetched another way); never a trademark or logo of another business.
  - `writing-the-brand-persona`, "Use when deciding how the brand speaks on social channels": the character the brand plays on social media, where `writing-in-the-brands-voice` is the voice of any copy (it says so); character, how it talks to customers, what it never says, five sample replies, how it differs per network, in `docs/marketing/brand/persona.md`; consistent with the kit's voice; no persona that pretends to be a real person.
  - `researching-the-market`, "Use before writing a marketing plan, or when the task asks who the customers are, what they search for or what a channel costs": the whole market and its channels, where `researching-competitors` compares three to five rivals (it says so, and takes that skill's table for the rivals); the audience and where it spends time; the words customers search for; what each channel costs to reach someone; each fact with its source and day, under `docs/marketing/research/`; a guess marked as one.
  - `writing-the-marketing-plan`, "Use when the task asks for a marketing plan": research first; goals and how each is measured; channels; the budget split by channel and campaign; the post calendar as slots (channel, day, topic); campaigns with dates inside the plan's; propose with `farik_propose_marketing_plan` and end the turn; what the owner sees (the summary first, in plain words); a returned plan's reason is the owner's, read and answered in the next version; never a budget the research does not support.
- **`farik_save_media { url, path }`**, tier `write_workspace`, a Marketing Specialist's `implement` session about a task only (`media_refused` otherwise). `path` is a file name matching `^[a-z0-9]+(-[a-z0-9]+)*\.(png|jpg|jpeg|webp|svg)$` (`media_path_invalid`), written to `docs/marketing/brand/assets/<path>` in the task's worktree after `Call::permit` with that path (the contract's `allowed_paths`, protected paths). `url` is `https` on port 443, or `http` on a loopback host as step 03's endpoints may be for the tests, with no userinfo and at most 2,000 characters, and its host is exactly one of the `media_hosts` of the session's connectors (`ToolContext::connectors`), each looked up by name in the agent's role kit (`(deps.kits)(role)`); any other host is refused, failing closed (`media_host_refused`); no shipped kit lists a loopback host. Farik fetches it itself: `reqwest`, no redirect, no proxy, no cookies, 30 seconds, at most 10 MiB read in chunks (`media_too_large`); a non-2xx is "<host> did not give the file" with no body. The bytes' type decides, by their first bytes: PNG, JPEG, WebP, or SVG (UTF-8 text whose first element is `<svg`); the extension must agree (`media_type_mismatch`). An SVG is refused `svg_not_safe` when, compared without case, it holds `<script`, `<foreignobject`, `<iframe`, `<embed`, `<object`, `<!entity`, `<!doctype`, `javascript:`, `@import`, `&#` or `\` (a reference or an escape could spell any of these), an attribute named `on` followed by letters, an `href`, `xlink:href` or `src` whose value does not start with `#`, or `url(` not followed by `#` (spaces and a quote skipped). Written beside and renamed over; no event of its own (the hook's `tool.called` and the task's commit record it). `tools::marketing` is a `pub(crate)` module, so 08d reuses `fetch_media`. Rejected: an XML parser (`quick-xml`), a new dependency for one check that a refusing scan does more strictly.
- **`media_hosts`** is a new optional field of an `http` kit connector: at most 8 exact host names (`^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$`), no wildcard; the loader refuses it on another transport (`kit_field_not_allowed`). It is not in the team entry or its hash: it names where results are fetched from, never what the agent may call. Only the `higgsfield` and `recraft` entries carry it: the exact hosts of the result addresses the founder reads before Task 10 (a Higgsfield image and video, a Recraft raster image and SVG), recorded with the date in the Execution notes; a host of a shared delivery network is pinned as that exact name only.
- **`farik_propose_marketing_plan`**, tier `write_workspace`, a Marketing Specialist's `implement` session about a task only (`marketing_plan_refused`). Its input is the design's, with two fields the design leaves open: `google_ads_account` (the ad account, `^[0-9]{3}-[0-9]{3}-[0-9]{4}$`, required exactly when `campaigns` is not empty, `google_ads_account_needed`; 08f uses it) and `replaces` (an approved plan's id this one supersedes, so a raised budget keeps its campaigns, 08g; `marketing_plan_unknown` otherwise). Every amount is a decimal string matching `^(0|[1-9][0-9]{0,7})(\.[0-9]{1,2})?$`, compared in hundredths, never floats. The checks, all reported at once, each with its field: `title` 3 to 100 characters, `summary` 20 to 600, `text` 200 to 16,000 (`marketing_plan_text`); `starts_on` no earlier than yesterday, so an owner west of UTC is not refused their own today, and `ends_on` not before it, at most 92 days inclusive (`marketing_plan_dates`); `currency` `^[A-Z]{3}$`; `budget.google_ads` at most `budget.total`, and the campaigns' budgets add up to at most `budget.google_ads` (`marketing_plan_budget`); 0 to 10 campaigns, each `channel: google_ads`, `name` 1 to 100, `goal` 1 to 300, a budget above 0, dates inside the plan's (`marketing_plan_campaign`); 0 to 200 posts, each `channel` one of the eleven (`instagram`, `x`, `facebook`, `linkedin`, `threads`, `bluesky`, `tiktok`, `pinterest`, `youtube`, `google_business`, `mastodon`), `on` inside the plan's dates, `topic` 1 to 200 (`marketing_plan_post`); keys `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 40, unique across campaigns and posts (`marketing_plan_key`); 1 to 10 measures of 3 to 200 characters; the task has no plan waiting already (`marketing_plan_waiting`). Today is the UTC date of the daemon's clock, as 5.5's day is.
- **Its number** is `MP-<n>`, `n` one more than the highest of the log's `marketing_plan.proposed` and of the `MP-<n>.md` files in `docs/marketing/plans/` of the project root and of the task's worktree, taken under one lock in the tool's module, so a fresh clone never reuses a committed number. The tool writes the text, with a heading of the id and title, to `docs/marketing/plans/MP-<n>.md` in the worktree (after `Call::permit` with that path; `marketing_plan_file_exists` if it is there), records `marketing_plan.proposed`, and answers `{ plan, next: "end your turn: the owner's decision starts the next session" }`, as `farik_ask_human` does.
- **The task waits, as for a question.** A proposed plan raises the task's new `open_plans` (migration 0013; `MIGRATIONS`, `migrations.rs:17`, becomes 13 long), which `waiting_on_human` counts beside `open_questions` and `open_approvals` (`projections.rs:443`); `approved` and `returned` lower it in `apply_waiting` (`projections.rs:745-777`). So rules 3 to 10 pass over the task (`waiting_on_nobody`, `rules.rs:856-865`; the spend check, `rules.rs:967-968`) and its card says "Waiting on you". While the plan is proposed, `farik_request_transition` (`tools/work.rs:156`) refuses its assignee's `in_progress → verifying` with `marketing_plan_waiting: MP-<n> waits for the owner; end your turn`. The task's next session is told the decision in its human message (`messages.rs:170`): "The owner approved your marketing plan MP-<n>.", then, with a note, "The owner adds: <note>"; or "The owner sent back your marketing plan MP-<n>:" and the reason, unwrapped, since the owner's words are the human's own (ADR 0011). Rejected: a plan that leaves the task free, whose returned reason would reach nobody once the task moved on.
- **Events**, in `event.schema.json` and `EVERY_KIND`: `marketing_plan.proposed { plan, title, summary, text, starts_on, ends_on, currency, budget, campaigns, posts, measures, google_ads_account?, replaces?, proposed_by }` (attributed by `proposed_by`); `marketing_plan.approved { plan, note? }` and `marketing_plan.returned { plan, reason }` (the proposing task on the envelope, no agent or session, no attribution, as `tool_approval.granted` is); `marketing_plan.ended { plan, why: replaced | by_owner | expired, replaced_by?, note? }`. `is_about_one_contract` (`event.rs:171`) is true for `proposed`, `approved` and `returned`, false for `ended`.
- **Only the owner decides**, under `ask` and `auto` alike (ADR 0041): commands `marketing_plan_decide { plan, decision: approve | return, note? }` and `marketing_plan_end { plan, note? }` in `command.schema.json`, arriving only on `POST /command` behind the daemon's token or the browser's RPC `command` behind its cookie, as `tool_approve` does (ADR 0031). Rejected: RPC methods of their own, since the command line reaches a running daemon only through `POST /command` (ADR 0014). `note` is the reason and required for `return`, 1 to 600 characters (`marketing_plan_reason_needed`), optional for the others. Refusals: `unknown_marketing_plan`; `marketing_plan_decided` (decided before); `marketing_plan_expired` (approving one whose `ends_on` is past); `marketing_plan_not_approved` (ending one not approved); `marketing_plan_ended`. Check and decision, and every end, are one step under one lock: `PLANS`, a static in `runtime/src/marketing.rs` (as `DECIDING`, `human.rs:454`), held by the decision, the owner's end and the dated ends alike; every end goes through `record_plan_end`, which takes the held lock, re-reads the plans and records nothing for one already ended. The store's fold ignores a decision event with an agent or a session on its envelope, as `open_grants` does (`waiting.rs:96-98`). A plan whose task was cancelled may still be approved: the plan does not depend on its task.
- **Which plan is active**, pure in `farik_core::marketing`: among approved plans not ended, the one approved last whose dates hold today. Approving a plan records, in the same step as its `approved`, `ended { replaced, replaced_by }` for every approved plan not ended whose `starts_on` is on or after the new one's; one that starts earlier ends on the new one's `starts_on`. A plan ended before its `starts_on` replaces nothing. A plan whose `ends_on` has passed ends `expired`. Those two dated ends are recorded by `end_marketing_plans`, a rule with no model that `tick_within` (`orchestrator.rs:462`) runs first, before its pause check, on every tick whose scope names no task: it starts no session, does not use up the tick, and so runs while the team is paused. Ending by the owner is immediate. Every end, whoever records it, goes through `record_plan_end` (`crates/runtime/src/marketing.rs`, new), which 08d extends to stop the plan's posts.
- **Queries** in `rpc.schema.json`: `marketing_plan.list {}` → `{ plans: [{ plan, title, state, starts_on, ends_on, currency, total, agent_id, task_id, proposed_at }] }`, newest first, `state` `proposed | returned | approved | active | ended`; `marketing_plan.get { plan }` → the proposal whole, its `state`, the decision with its note or reason and time, and the end with its `why` (`not_found` otherwise). `waiting.list` gains rows of kind `marketing_plan` with `plan`, `summary`, `total`, `currency`, `starts_on` and `ends_on`, which only that kind carries (`waitingListResult`, `rpc.schema.json:2257`), and the line "<agent name> proposes a marketing plan: <title>". Today skips a row whose kind it does not know (`WaitingRow`, `Today.tsx:325`).
- **The screens, mocked up first (Task 0).** Today's row "Marketing plan to approve": the agent's picture and name, the title, the summary, the total budget with its currency and the dates, and "Review", which opens `/marketing/plans/MP-<n>`. The plan's page: the summary first; the budget as a table by channel and campaign; the post calendar by week (each slot's day, channel and topic); the measures; the full text shown as text, never rendered as markup; then, while proposed, "Approve" and "Send back" (a required reason, up to 600 characters); while approved or active, "End the plan" with an optional note and a confirmation saying what ending does; once decided, who decided when and their words. The board's task card says "Waiting on you" through `waiting_on_human`, unchanged.
- **The command line.** `farik marketing plan show [<plan>] [--json]` (no plan: the list), `farik marketing plan approve <plan> [--note <text>]`, `farik marketing plan return <plan> --reason <text>`, `farik marketing plan end <plan> [--note <text>]`; the three decisions through `here_or_sent`, as `farik tool approve`; `show` reads the store, as `farik waiting` does. What a process prints when it ends (5.7) gains, after the connector calls, a line in the CLI's form `"{task_id} {what}: {command}"` (`cli/src/waiting.rs:31`): "FRK-1 waits: Kai proposes a marketing plan: <title>: farik marketing plan approve MP-1, or farik marketing plan return MP-1 --reason <text>".
- **`marketing_paths_owned`**, a structural readiness rule after `document_paths_only` (`readiness.rs:561`): a task (not an epic) whose `assignee_role` is not the Marketing Specialist, while `active_agents_by_role` counts at least one active Marketing Specialist, fails when an `allowed_paths` entry could match a path under `docs/marketing/`, by `reaches_the_marketing_directory` (`paths.rs`, beside `reaches_the_farik_directory` at `paths.rs:110`): after `.` segments are dropped and braces expanded, its first segment holds `**`, or matches `docs` regardless of case and either ends the glob as a literal, or its second segment holds `**`, or matches `marketing` and the glob goes on or is literal there. A segment that does not compile is taken to match, failing closed as `paths.rs:121-126` does: `docs[/]marketing/x` fails. So `docs/**`, `**/*.md` and `docs/marketing/x.md` fail, and `docs/adr/**`, `src/**` and `*.md` pass. Message: "allowed paths <list> could reach docs/marketing/, which the Marketing Specialist owns; name narrower paths or give the task to the Marketing Specialist". Plain words in `plain.rs` beside `DocumentPathsOnly`: "Only the Marketing Specialist changes the brand kit and the marketing plans, and this plan lets someone else." (`EVERY_RULE` and `listed`, `plain.rs:64-113`, gain the rule). Rejected: a Ready check on the literal prefix `docs/marketing/` with a Done check on the files changed. The Ready half misses `Docs/Marketing/x.md` and `docs/{marketing,adr}/**` without this rule's case folding and brace expansion; the Done half needs new plumbing in `DoneEvidence` and fails late, at acceptance, after `Call::permit` allowed the writes. The one accepted residual: a task made ready before the Marketing Specialist joined.

## File map

```
docs/design/mockups/{TodayMarketingPlan,MarketingPlan,PhoneMarketingPlan}.dc.html, canvas.json   creates (Task 0)
crates/roles/roles/marketing_specialist/{role.yaml,system.md}, skills/marketing-what-ships/SKILL.md   modifies (Task 1)
crates/roles/roles/ui_ux_designer/skills/brand-and-design-tokens/SKILL.md       modifies (Task 1)
crates/roles/roles/product_manager/skills/writing-task-contracts/SKILL.md       modifies (Task 1)
crates/roles/roles/scrum_master/skills/keeping-work-flowing/SKILL.md            modifies (Task 1)
crates/roles/roles/marketing_specialist/skills/<4 names>/SKILL.md, kit.yaml      creates, modifies (Tasks 2, 5, 10)
crates/roles/src/{lib.rs,kit.rs}, docs/schemas/kit.schema.json                   modifies (Tasks 1, 2, 10)
crates/core/src/governor/{paths.rs,readiness.rs,plain.rs}                        modifies (Task 3)
crates/core/src/marketing.rs, crates/core/src/lib.rs                             creates, modifies (Tasks 4, 6)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,event/fixtures.rs,lib.rs}   modifies (Task 5)
crates/store/src/migrations/0013_marketing_plans.sql, migrations.rs, projections.rs, marketing.rs, lib.rs   creates, modifies (Tasks 5, 6)
crates/store/src/waiting.rs                                                      modifies (Task 8)
crates/runtime/src/tools.rs, tools/marketing.rs, tools/git.rs, orchestrator/session.rs   modifies, creates (Tasks 5, 10)
crates/runtime/src/tools/work.rs                                                 modifies (Task 6)
crates/runtime/src/orchestrator/{messages.rs,human.rs,rules.rs}, orchestrator.rs modifies (Task 6)
crates/runtime/src/marketing.rs, crates/runtime/src/lib.rs                       creates, modifies (Task 6)
crates/runtime/src/daemon/gates.rs                                               modifies (Tasks 3, 8)
crates/runtime/src/daemon/team.rs                                                modifies (Tasks 1, 10)
docs/schemas/command.schema.json, crates/protocol/src/command.rs                 modifies (Task 6)
crates/cli/src/{lib.rs,marketing.rs}                                             modifies, creates (Task 7)
docs/schemas/rpc.schema.json, crates/cli/src/waiting.rs                          modifies (Task 8)
apps/web/src/pages/{Today.tsx,Today.test.tsx}                                    modifies (Tasks 8, 9)
apps/web/src/pages/MarketingPlan.tsx(+test), app/App.tsx, strings/en.ts          creates, modifies (Task 9)
docs/SPEC.md, docs/design/{role-kits.md,marketing-specialist.md}, docs/plans/project-plan.md   modifies (Task 11)
```

## Interfaces

Consumes: `load_role`, `load_kit`, `parse_kit`, `embedded_skills`, `KitConnector`, `check_skill` (`farik-roles`); `ReadinessRule`, `ReadinessContext`, `reaches_the_farik_directory`, `rules_evaluated`, `Role` (`farik-core`); `ToolDeps`, `ToolContext::connectors`, `KitSource`, `Call`, `Call::permit`, `tool`, `call_tool`, `paths_of`, `offered_tools`, `request_transition`, `human_message`, `handle`, `decide_tool_call`'s lock pattern, `waiting_on_nobody`, `OrchestratorDeps`, `CommandReport`, `CommandError`, `here_or_sent` (runtime, cli); `EventBody`, `EVERY_KIND`, `is_about_one_contract`, `a_body_wire`, `Command`, `command_from_value` (protocol); `apply_waiting`, `waiting`, `Waiting`, `EventQuery` (store).

Produces:

```rust
// farik_core::marketing (pure)
pub struct Amount(pub u64);                                   // hundredths of the currency
pub fn parse_amount(text: &str) -> Option<Amount>;
pub enum PostChannel { Instagram, X, Facebook, Linkedin, Threads, Bluesky, Tiktok, Pinterest, Youtube, GoogleBusiness, Mastodon }
pub struct PlanCampaign { pub key: String, pub name: String, pub goal: String, pub budget: Amount, pub starts_on: NaiveDate, pub ends_on: NaiveDate }
pub struct PostSlot { pub key: String, pub channel: PostChannel, pub on: NaiveDate, pub topic: String }
pub struct PlanProposal { pub title: String, pub summary: String, pub text: String, pub starts_on: NaiveDate, pub ends_on: NaiveDate,
    pub currency: String, pub total: Amount, pub google_ads: Amount, pub campaigns: Vec<PlanCampaign>, pub posts: Vec<PostSlot>,
    pub measures: Vec<String>, pub google_ads_account: Option<String>, pub replaces: Option<String> }
pub struct ProposalRefusal { pub code: &'static str, pub field: String, pub message: String }
pub fn check_proposal(proposal: &PlanProposal, today: NaiveDate) -> Result<(), Vec<ProposalRefusal>>;   // starts_on from today - 1
pub enum EndReason { Replaced, ByOwner, Expired }
pub struct PlanRecord { pub id: String, pub starts_on: NaiveDate, pub ends_on: NaiveDate, pub approved_seq: Option<u64>, pub returned: bool, pub ended: Option<EndReason> }
pub fn active_plan(plans: &[PlanRecord], today: NaiveDate) -> Option<&PlanRecord>;
pub fn plans_to_end(plans: &[PlanRecord], today: NaiveDate) -> Vec<(String, EndReason, Option<String>)>;  // id, why, replaced_by
// farik_core::governor::paths
pub fn reaches_the_marketing_directory(glob: &str) -> bool;
// ReadinessRule::MarketingPathsOwned
// farik_roles: KitConnector::Server gains `media_hosts: Vec<String>`
// farik_store::marketing
pub struct MarketingPlan { pub record: PlanRecord, pub proposal: PlanProposal, pub agent_id: String, pub task_id: TaskId,
    pub proposed_at: DateTime<Utc>, pub decided: Option<(bool, Option<String>, DateTime<Utc>)>, pub ended_at: Option<DateTime<Utc>> }
pub fn marketing_plans(log: &EventLog) -> Result<Vec<MarketingPlan>, StoreError>;   // oldest first
// farik_store::waiting: WaitingKind::MarketingPlan ("marketing_plan"); Waiting gains `pub plan: Option<PlanAsk>`
pub struct PlanAsk { pub plan: String, pub summary: String, pub total: String, pub currency: String, pub starts_on: NaiveDate, pub ends_on: NaiveDate }
// farik_runtime::tools::marketing (a pub(crate) module, for 08d)
pub struct ProposeMarketingPlanInput { /* the design's fields, amounts as String */ }
pub struct SaveMediaInput { pub url: String, pub path: String }
pub(crate) fn propose_plan(call: &Call<'_>, input: ProposeMarketingPlanInput) -> Result<Value, ToolError>;
pub(crate) async fn save_media(call: &Call<'_>, input: SaveMediaInput) -> Result<Value, ToolError>;
pub(crate) enum MediaKind { Png, Jpeg, Webp, Svg }
pub(crate) async fn fetch_media(url: &str, hosts: &[String]) -> Result<(MediaKind, Vec<u8>), ToolError>;   // the host rule, the limits, the type and SVG checks
// farik_runtime::marketing (new); `static PLANS: Mutex<()>`, as DECIDING (human.rs:454)
pub(crate) struct PlansHeld(MutexGuard<'static, ()>);
pub(crate) fn hold_plans() -> PlansHeld;
pub(crate) fn decide_plan(tools: &ToolDeps, plan: &str, approve: bool, note: Option<String>) -> Result<CommandReport, CommandError>;
pub(crate) fn end_plan(tools: &ToolDeps, plan: &str, note: Option<String>) -> Result<CommandReport, CommandError>;
pub(crate) fn record_plan_end(held: &PlansHeld, tools: &ToolDeps, plan: &str, why: EndReason, replaced_by: Option<&str>, note: Option<String>) -> Result<Vec<FarikEvent>, String>;
// farik_runtime::orchestrator::rules: today from the clock; each end through record_plan_end
pub(super) fn end_marketing_plans(deps: &OrchestratorDeps) -> Result<Vec<FarikEvent>, OrchestratorError>;
// Command::MarketingPlanDecide { plan: String, approve: bool, note: Option<String> }, Command::MarketingPlanEnd { plan: String, note: Option<String> }
```

## Tasks

### Task 0: Mockups

On the canvas the earlier steps used, desktop and phone, copied to `docs/design/mockups/`: Today's row; the plan's page while proposed (with the send-back dialog), while active (with the end confirmation), and ended. The founder's approval, with its date and the canvas version, goes into this plan's Execution notes in the same commit; Task 9 waits for it.

- [ ] `docs(design): mock up the marketing plan on Today and its page`

### Task 1: The mandate

Files: the Marketing Specialist's `role.yaml`, `system.md` and `marketing-what-ships`; the Designer's `brand-and-design-tokens`; the Product Manager's `writing-task-contracts`; the Scrum Master's `keeping-work-flowing`; `roles/src/lib.rs`; `runtime/src/daemon/team.rs`.

- `the_marketing_specialist_owns_the_brand_and_the_plan` (`lib.rs`, replacing `the_marketing_specialist_publishes_only_when_allowed` at `lib.rs:580`): `forbidden` is exactly the four lines of Decisions; the system prompt names `docs/marketing/brand/brand-kit.md`, `docs/marketing/brand/persona.md` and `docs/marketing/plans/`; no shipped Marketing skill says "never publish". RED.
- `the_designer_takes_the_brand_kit_first` (`lib.rs`): the Designer's `brand-and-design-tokens` names `docs/marketing/brand/brand-kit.md`. RED.
- `the_planners_keep_other_tasks_off_the_marketing_folder` (`lib.rs`): the Product Manager's `writing-task-contracts` and the Scrum Master's `keeping-work-flowing` each hold the sentence of Decisions word for word. RED.
- `ships_the_mockup_persona_per_role` (`lib.rs:398-415`) and `proposes_the_suggested_six` (`daemon/team.rs:2544`) expect the Marketing Specialist's "Owns your brand and how you reach people"; each fails until `role.yaml` changes. RED.

- [x] `feat(roles): make the Marketing Specialist the owner of the brand and the plan`

### Task 2: Four skills

- `marketing_kit_carries_the_brand_and_plan_skills` replaces `marketing_kit_carries_posting_and_email` (`kit.rs:1090`): the nine then `keeping-the-brand-kit`, `writing-the-brand-persona`, `researching-the-market`, `writing-the-marketing-plan`, each with its `SKILL.md`. RED.
- `kit_skills_name_only_tools_farik_lists` (`daemon/team.rs:4042`) holds. Guard. It refuses a tool that does not exist yet, so `writing-the-marketing-plan` names `farik_propose_marketing_plan` only from Task 5's commit and `keeping-the-brand-kit` names `farik_save_media` only from Task 10's; until then each says "the tool Farik gives you for it".

- [x] `feat(roles): teach the Marketing Specialist the brand kit, the persona, research and the plan`

### Task 3: `marketing_paths_owned`

Files: `paths.rs`, `readiness.rs` (the rule after `document_paths_only` in `CHECKS`, now 20 long), `plain.rs`, `daemon/gates.rs` (tests). Task 3 also changes, in `readiness.rs`, `refuses_allowed_paths_that_reach_under_the_farik_directory` (`:1113`, `:1119`), `keeps_a_ceiling_with_a_file_filter_exact` (`:1137`) and `refuses_a_wildcard_that_runs_past_the_directory_name` (`:1151`): each takes `a_ready_context()` with `Role::MarketingSpecialist` removed from `active_agents_by_role`, so each still tests its own rule. `a_ready_context()` keeps its Marketing Specialist. `checks_a_draft_without_saving` (`daemon/gates.rs:1294`) expects `total` 21, and `answers_tries_and_the_sprint` (`gates.rs:1826`) 23.

- `reaches_the_marketing_directory_as_the_rule_says` (`paths.rs`): true for `docs/**`, `**/*.md`, `Docs/Marketing/x.md`, `docs/{marketing,adr}/**`, `./docs/marketing`, `docs`, `docs[/]marketing/x`; false for `docs/adr/**`, `src/**`, `*.md`, `docs/*.md`. RED.
- `another_role_may_not_name_the_marketing_folder` (`readiness.rs`): a Product Manager's task with `docs/**` fails `MarketingPathsOwned` with the message's paths while one Marketing Specialist is active, and passes with none active; a Marketing Specialist's task with `docs/marketing/**` passes; an epic is not held. RED.
- `plain_readiness_covers_every_rule` (`plain.rs:116`) holds with the rule in `EVERY_RULE` and `listed`. Guard.

- [ ] `feat(core): keep the marketing folder the Marketing Specialist's`

### Task 4: The plan, checked

- `checks_a_proposal_field_by_field` (`core::marketing`): one case per refusal of Decisions, each naming its code and field, and a valid proposal passes; 93 days fails, 92 passes; a `starts_on` of yesterday passes, the day before fails `marketing_plan_dates`; `"10.999"`, `"1,000"` and `"-1"` are not amounts. RED.
- `campaign_budgets_fit_the_channel`: two campaigns of 600.00 against `google_ads` 1000.00 fail `marketing_plan_budget`; 500.00 each pass. RED.
- `one_plan_is_active`: an approved plan within its dates is active; a later approval starting today replaces it; one starting later leaves it active until that day, then `plans_to_end` names it `replaced` with the newer id; a newer plan ended before its `starts_on` replaces nothing, and the older stays active past that day; past `ends_on` it is `expired`; a returned plan is never active. RED.

- [ ] `feat(core): check a marketing plan and decide which one is active`

### Task 5: Proposing a plan

Files: the four `marketing_plan.*` kinds in `event.schema.json` and every exhaustive match (`event.rs`: `EventBody`, `kind`, `body_def_name`, `attribution`, `is_about_one_contract` as Decisions say, `EVERY_KIND`; `lib.rs` `KINDS`; `event/fixtures.rs` `a_body_wire`, a body for each; `projections.rs` `apply_to`), one commit so it compiles; the migration and `MIGRATIONS` (`migrations.rs:17`); `apply_waiting` (`projections.rs:745`) and `waiting_on_human` (`projections.rs:443`); `store::marketing` and `store/src/lib.rs`; `tools/marketing.rs` (`propose_plan`), its descriptor and `call_tool` arm (`tools.rs`); `tools/git.rs` (`worktree` becomes `pub(super)`); `offered_tools` (`session.rs:804`: `farik_propose_marketing_plan` only to a Marketing Specialist in `implement`); `writing-the-marketing-plan` (names the tool). `approved` and `returned` get `apply_to`'s no-op arm here and move to `apply_waiting` in Task 6, under its test.

- `proposes_a_plan_and_writes_its_text`: records `marketing_plan.proposed` as `MP-1` with every field, writes `docs/marketing/plans/MP-1.md` in the worktree, and the task waits on the human (`open_plans` 1). RED.
- `numbers_past_a_committed_plan`: with `docs/marketing/plans/MP-4.md` in the project root and an empty log, the next is `MP-5`. RED.
- `refuses_a_bad_proposal_with_every_reason`: one call with three faults answers all three codes, writes nothing. RED.
- `refuses_a_second_plan_on_the_task` (`marketing_plan_waiting`) and `refuses_another_role_or_session` (`marketing_plan_refused`). RED each.
- `the_store_folds_the_plans`: `marketing_plans` gives proposed, approved, returned and ended plans with their decisions, oldest first; a `marketing_plan.approved` with an agent or a session on its envelope decides nothing. RED.
- `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` and `writes_back_exactly_the_value_it_read_for_every_kind` (`event.rs:921`, `:1209`) hold with the four new bodies. Guard.

- [ ] `feat(runtime): let the Marketing Specialist propose a marketing plan`

### Task 6: The owner decides

Files: `command.schema.json`, `command.rs` (`Command`, `human_command`, `command_to_value`), `human.rs` (`handle` at `human.rs:54` calls `decide_plan` and `end_plan`), `runtime/src/marketing.rs` (`PLANS`, `hold_plans`, `decide_plan`, `end_plan`, `record_plan_end`) and `runtime/src/lib.rs`, `apply_waiting`, `end_marketing_plans` (`rules.rs`, called first from `tick_within`), `messages.rs` (`human_message` at `messages.rs:170`), `tools/work.rs` (`request_transition` at `work.rs:156`).

- `approving_lowers_the_wait_and_tells_the_task`: `marketing_plan_decide` approve records `approved`, `open_plans` falls to 0, and the task's next human message says "The owner approved your marketing plan MP-1."; another plan approved with the note `Start small` reads "The owner approved your marketing plan MP-2. The owner adds: Start small". RED.
- `returning_needs_a_reason_and_quotes_it`: without a note `marketing_plan_reason_needed`; with one, `returned`, and the next human message is "The owner sent back your marketing plan MP-1:" followed by the reason exactly as written, with no `untrusted` block. RED.
- `decided_once_and_never_expired`: a second decision `marketing_plan_decided`; an unknown id `unknown_marketing_plan`; approving one whose `ends_on` passed `marketing_plan_expired`; a plan whose task was cancelled is still approved. RED.
- `approving_a_newer_plan_replaces_one_not_yet_started`: with MP-1 approved to start next week, approving MP-2 starting tomorrow records `ended { replaced, replaced_by: "MP-2" }` for MP-1 in the same step. RED.
- `the_owner_ends_a_plan`: `marketing_plan_end` records `ended { by_owner }`, and `record_plan_end` for it afterwards records nothing; ending a proposed one is `marketing_plan_not_approved`, an ended one `marketing_plan_ended`. RED.
- `the_tick_ends_plans_by_their_dates`: a tick on the replaced plan's day records `ended { replaced, replaced_by }`, one past `ends_on` `ended { expired }`, once each, with no session, also while the team is paused. RED.
- `no_verifying_while_the_plan_waits` (`tools/work.rs`): while MP-1 is proposed, the assignee's `farik_request_transition` to `verifying` is refused `marketing_plan_waiting: MP-1 waits for the owner; end your turn` and nothing is recorded; once MP-1 is approved, the same call reaches the governor. RED.

- [ ] `feat(runtime): let the owner approve, return and end a marketing plan`

### Task 7: The command line

Files: `cli/src/lib.rs` (the subcommands), `cli/src/marketing.rs` (`show` from `marketing_plans`; the decisions through `here_or_sent`).

- `marketing_plan_approve_sends_the_decision` (and `end` its end) and `marketing_plan_return_needs_a_reason` (the parser refuses it without `--reason`); `marketing_plan_show_prints_the_list_and_one_plan` (`--json` stdout pure). RED each.

- [ ] `feat(cli): decide marketing plans`

### Task 8: The queries

Files: `rpc.schema.json` (`marketing_plan.list`, `marketing_plan.get`, `waitingListResult` at `:2257`), `gates.rs` (`query` at `gates.rs:132`, `waiting_row` at `gates.rs:114`), `store/src/waiting.rs` (`WaitingKind::MarketingPlan`, `PlanAsk`), and in the same commit, since they match `WaitingKind` or its kinds exhaustively, `cli/src/waiting.rs` (the end-of-run line, whose commands Task 7 made) and `Today.tsx` (`WaitingRow`, `Today.tsx:325`, skips a kind `KINDS` lacks).

- `lists_and_gets_plans`: `marketing_plan.list` newest first with each `state`; `marketing_plan.get` the whole proposal and its decision; `not_found` for `MP-9`. RED.
- `waiting_lists_a_plan_to_approve`: a row of kind `marketing_plan` with `plan`, `summary`, `total`, `currency`, `starts_on`, `ends_on` and the line; gone once decided. RED.
- `a_run_says_which_plan_waits` (`cli/src/waiting.rs`): for Kai's MP-1 titled "Spring launch" on FRK-1, the line is exactly "FRK-1 waits: Kai proposes a marketing plan: Spring launch: farik marketing plan approve MP-1, or farik marketing plan return MP-1 --reason <text>", and with `--json` it carries `plan`. RED.
- `today_skips_a_kind_it_does_not_know` (`Today.test.tsx`): a `waiting.list` with a question and a row of kind `not_a_kind` shows the question and nothing for the other, without an error. RED.

- [ ] `feat(runtime): list the marketing plans and the one waiting on the owner`

### Task 9: The screens

As the approved mockups. `MarketingPlan.test.tsx`, `Today.test.tsx`.

- `today_shows_a_plan_to_approve_with_its_summary_and_budget`; `review_opens_the_plan_page`; `the_page_shows_the_budget_calendar_and_text_as_text` (markup in the text is inert); `approve_sends_the_decision`; `send_back_needs_a_reason`; `end_asks_first_then_sends`. RED each.

- [ ] `feat(web): approve a marketing plan on Today`

### Task 10: `farik_save_media`

Founder's action first, with the live pin's sign-in: the result address of a Higgsfield image and a Higgsfield video, and of a Recraft raster image and a Recraft SVG, recorded with the date in the Execution notes; their exact hosts become `media_hosts` in `kit.yaml`. One commit: the `KitConnector::Server` patterns that name every field (`daemon/team.rs:666`; the `kit.rs` tests at `:1310`, `:1435` and `:2060`) take `..`, and the loader's construction (`kit.rs:644`) sets `media_hosts`; `allowances.rs:27` and `daemon/board.rs:523` already end in `..`. Files: `kit.schema.json`, `kit.rs` (`media_hosts`, its shape), `kit.yaml`, `tools.rs` (descriptor, `call_tool` arm), `tools/marketing.rs` (`save_media`, `fetch_media`, `MediaKind`), `session.rs` `offered_tools` (`farik_save_media` only to a Marketing Specialist in `implement`), `keeping-the-brand-kit` (names the tool, and says to give a refused address to the owner). Tests against a local `axum` fixture on loopback, through a fixture kit (`kits` swapped, ADR 0036) whose connector lists `127.0.0.1` in `media_hosts`.

- `media_hosts_only_on_a_web_service`: a `stdio` connector with `media_hosts` is `kit_field_not_allowed`; a host with `*` is a schema error. RED.
- `saves_an_image_from_a_kit_service`: a PNG from a listed host lands at `docs/marketing/brand/assets/logo.png` byte for byte. RED.
- `refuses_a_host_outside_the_session_s_services`: another host, a host listed only by a kit connector the session was not given, `http` to a host that is not loopback, `https` on a port other than 443, and userinfo are each `media_host_refused`, nothing fetched. RED.
- `no_shipped_kit_names_a_loopback_media_host` (`kit.rs`): no `media_hosts` of any shipped kit is `localhost`, `127.0.0.1` or `::1`. Guard.
- `only_the_creative_services_carry_media_hosts` (`kit.rs`): across every shipped kit, the connectors with `media_hosts` are exactly `higgsfield` and `recraft`. RED.
- `refuses_a_type_that_does_not_match`: a JPEG named `.png`, and an HTML page named `.svg`, are `media_type_mismatch`. RED.
- `refuses_an_unsafe_svg`: one case per item of the SVG list (`<SCRIPT`, `onload=`, `xlink:href="https://x"`, `url(https://x)`, `@&#105;mport`, `@\69mport`), each `svg_not_safe`; an SVG with `href="#a"` and `url(#g)` saves. RED.
- `refuses_a_large_or_redirected_answer`: 10 MiB + 1 byte is `media_too_large`; a 302 is not followed. RED.
- `only_the_marketing_specialist_in_a_task_saves_media`: a Developer, and a Marketing Specialist's chat, are `media_refused`. RED.

- [ ] `feat(runtime): let the Marketing Specialist keep the media it made in the brand kit`

### Task 11: Spec and plan

`docs/SPEC.md` 6.5 (what was built: the mandate, the skills, `farik_save_media`, the plan's checks, numbers, events and decisions, with any change made in execution), 5.3 (`marketing_paths_owned`), 5.7 (a plan waits on the owner as a question does, and its task is not handed in meanwhile), 6.7 (`media_hosts`), 6.8 (the Designer reads the brand kit), 8.5 (the four kinds); the revision line. `docs/design/role-kits.md` (the Marketing row). `docs/design/marketing-specialist.md`: a returned plan's reason reaches the next session as the owner's own words, not as untrusted text, and `starts_on` may be yesterday's UTC date. Project plan row 08c.

- [ ] `docs(spec): record the brand and the marketing plan`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: a marketing task that writes the brand kit with one Recraft logo and one Higgsfield picture saved by `farik_save_media`, then proposes a two-week plan with one post slot; Today shows it; send it back with a reason; the next session's plan is approved; `farik marketing plan show` lists both; a Product Manager's task naming `docs/**` fails readiness with the rule's message.

## Execution notes

- Task 1: `the_marketing_specialist_owns_the_brand_and_the_plan` keeps the old test's four `allows that call` counts (the four skills still say it) and the "never publish" scan, and adds that the prompt holds every `forbidden` line (as the Finance test does), the three paths of the design, the sentence about what a service or a competitor's page returns, and "a returned plan's reason is the owner's own words"; it also asserts `marketing-what-ships` names the four paths, `docs/marketing/research/` among them, and `produces` holds "the brand kit", "the brand persona" and "a marketing plan". RED, as named: `forbidden` was the old two lines; the Designer's skill lacked the path; the planners' skills lacked the sentence; the persona was the old line in `ships_the_mockup_persona_per_role` (`roles`) and, shown RED with `role.yaml`'s persona put back for one run, in `proposes_the_suggested_six` (`daemon/team.rs`, an ignored test, run with `--include-ignored`). In the Product Manager's skill the sentence is a bullet of its own after "Only the Developer and the UI/UX Designer change code", opening "Keep other roles off the marketing folder:"; in the Scrum Master's it follows the same paragraph.
- Task 2: `marketing_kit_carries_the_brand_and_plan_skills` is wider than the plan's line: for each new skill it also asserts the description opens with the plan's "Use when" text, `SKILL.md` has numbered sections (`## 1. `) and is under 6 KB, and that it names the skill or file it sits beside (`docs/marketing/brand/brand-kit.md`, `writing-in-the-brands-voice`, `researching-competitors`, `researching-the-market`). RED as named: the kit listed the nine only. The guard `kit_skills_name_only_tools_farik_lists` holds on the four skills (they name `farik_ask_human` and `farik_write_note`) and was proved with the test's own data, a `("synthetic", "call farik_nope here")` pushed onto its texts for one run (`product_manager/synthetic: farik_nope`), then removed; no `farik_*` name that Farik lacks was written into any shipped file. `writing-the-marketing-plan` says "the tool Farik gives you for it" until Task 5, and `keeping-the-brand-kit` says it of the media copy until Task 10.
