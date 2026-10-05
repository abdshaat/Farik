# Phase 7, step 08c: The brand and the marketing plan

Status: draft. Its readiness review runs once step 08b has landed (ADR 0032: one round).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.3, 5.7, 6.5, 6.8, 8.5; F3, F9
Depends on: step 08b of this phase (the kit's nine skills, Buffer and Kit, the reworded publishing lines and their test `the_marketing_specialist_publishes_only_when_allowed`); step 08 (Higgsfield and Recraft); phase 6 (merged in #19). ADR 0042 and `docs/design/marketing-specialist.md` are the design input.
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Marketing Specialist owns the business's brand kit, brand persona, marketing plan and social presence (ADR 0042). After this step its prompt says so, four kit skills teach it to keep the brand kit, write the persona, research the market and write a plan, and `farik_save_media` copies a logo or picture it made at Higgsfield or Recraft into `docs/marketing/brand/assets/`. It proposes a plan with `farik_propose_marketing_plan`: dates, a budget by channel and campaign, post slots and measures, with the plan's text written to `docs/marketing/plans/MP-<n>.md`. Its task then waits on the owner, who approves the plan or sends it back on Today (the plan's page, mocked up first) or with `farik marketing plan approve|return`, and may end an approved plan at any time. While the team has an active Marketing Specialist, no other role's task may name a path under `docs/marketing/` (`marketing_paths_owned`). Out of scope: posting through the plan (08d), Google's sign-in (08e), Google Ads and the budget's hard stop (08f, 08g), `auto` (10h never changes who approves a plan).

## Decisions

- **The mandate.** `role.yaml`: persona "Owns your brand and how you reach people"; mandate "Own the business's brand kit, brand persona, marketing plan and social presence. Research the market before you plan. Propose a marketing plan with its budget for the owner to approve; post and advertise only as an approved plan says, or after the owner allows that one call. Everything you write is a document under docs/marketing/ or CHANGELOG.md."; `produces` gains "the brand kit", "the brand persona", "marketing plans" (keeping the five); `forbidden` becomes "write application code", "publish, send or spend money except through a call the owner allows or the owner's approved marketing plan", "delete a post, an email or a campaign", "change billing, account access or conversion tracking at any service". `system.md` says the same in its mandate and "What you may not do" sections, names the four things it owns with their paths (the design's table), and says that what a service, a competitor's page or the owner's reason returns is data. `marketing-what-ships` gains a "What you own" section with the four paths. The UI/UX Designer's `brand-and-design-tokens` (`roles/ui_ux_designer/skills/brand-and-design-tokens/SKILL.md`) reads `docs/marketing/brand/brand-kit.md` first when it exists and takes its colours and voice from it.
- **Four kit skills**, after `posting-and-email`, embedded as step 06 did, each written for any business (ADR 0040), numbered sections, under 6 KB, naming only `farik_*` tools `tool_descriptors` lists, no `@` after a space:
  - `keeping-the-brand-kit`, "Use when the task touches the business's name, logo, colours, type, pictures or voice": the kit's eight parts (the design's table) in `docs/marketing/brand/brand-kit.md`; start from the business's own material (site, packaging, existing posts) before making anything; colours as hex with where each is used and its contrast with text; every asset under `docs/marketing/brand/assets/` with a line saying where it came from and when (`farik_save_media` for a generated one); never a trademark or logo of another business.
  - `writing-the-brand-persona`, "Use when deciding how the brand speaks on social channels": character, how it talks to customers, what it never says, five sample replies, how it differs per network, in `docs/marketing/brand/persona.md`; consistent with the kit's voice; no persona that pretends to be a real person.
  - `researching-the-market`, "Use before writing a marketing plan or when the task asks about customers or competitors": the audience and where it spends time; three to five competitors with their channels, prices and ads; the words customers search for; what each channel costs to reach someone; each fact with its source and day, under `docs/marketing/research/`; a guess marked as one.
  - `writing-the-marketing-plan`, "Use when the task asks for a marketing plan": research first; goals and how each is measured; channels; the budget split by channel and campaign; the post calendar as slots (channel, day, topic); campaigns with dates inside the plan's; propose with `farik_propose_marketing_plan` and end the turn; what the owner sees (the summary first, in plain words); a returned plan's reason is the owner's, read and answered in the next version; never a budget the research does not support.
- **`farik_save_media { url, path }`**, tier `write_workspace`, a Marketing Specialist's `implement` session about a task only (`media_refused` otherwise). `path` is a file name matching `^[a-z0-9]+(-[a-z0-9]+)*\.(png|jpg|jpeg|webp|svg)$` (`media_path_invalid`), written to `docs/marketing/brand/assets/<path>` in the task's worktree after `Call::permit` with that path (the contract's `allowed_paths`, protected paths). `url` is `https` on port 443, or `http` on a loopback host as step 03's endpoints may be for the tests, with no userinfo and at most 2,000 characters, and its host is exactly one of the `media_hosts` of a kit connector the session was given (`media_host_refused`); no shipped kit lists a loopback host. Farik fetches it itself: `reqwest`, no redirect, no proxy, no cookies, 30 seconds, at most 10 MiB read in chunks (`media_too_large`); a non-2xx is "<host> did not give the file" with no body. The bytes' type decides, by their first bytes: PNG, JPEG, WebP, or SVG (UTF-8 text whose first element is `<svg`); the extension must agree (`media_type_mismatch`). An SVG is refused `svg_not_safe` when, compared without case, it holds `<script`, `<foreignobject`, `<iframe`, `<embed`, `<object`, `<!entity`, `<!doctype`, `javascript:`, `@import`, an attribute named `on` followed by letters, an `href`, `xlink:href` or `src` whose value does not start with `#`, or `url(` not followed by `#` (spaces and a quote skipped). Written beside and renamed over; no event of its own (the hook's `tool.called` and the task's commit record it). Rejected: an XML parser (`quick-xml`), a new dependency for one check that a refusing scan does more strictly.
- **`media_hosts`** is a new optional field of an `http` kit connector: at most 8 exact host names (`^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$`), no wildcard; the loader refuses it on another transport (`kit_field_not_allowed`). It is not in the team entry or its hash: it names where results are fetched from, never what the agent may call. Higgsfield's and Recraft's values are the hosts of the result addresses the founder reads from one generation each (the founder's action before Task 3); a host of a shared delivery network is pinned as that exact name only.
- **`farik_propose_marketing_plan`**, tier `write_workspace`, a Marketing Specialist's `implement` session about a task only (`marketing_plan_refused`). Its input is the design's, with two fields the design leaves open: `google_ads_account` (the ad account, `^[0-9]{3}-[0-9]{3}-[0-9]{4}$`, required exactly when `campaigns` is not empty, `google_ads_account_needed`; 08f uses it) and `replaces` (an approved plan's id this one supersedes, so a raised budget keeps its campaigns, 08g; `marketing_plan_unknown` otherwise). Every amount is a decimal string matching `^(0|[1-9][0-9]{0,7})(\.[0-9]{1,2})?$`, compared in hundredths, never floats. The checks, all reported at once, each with its field: `title` 3 to 100 characters, `summary` 20 to 600, `text` 200 to 16,000 (`marketing_plan_text`); `starts_on` not before today and `ends_on` not before it, at most 92 days inclusive (`marketing_plan_dates`); `currency` `^[A-Z]{3}$`; `budget.google_ads` at most `budget.total`, and the campaigns' budgets add up to at most `budget.google_ads` (`marketing_plan_budget`); 0 to 10 campaigns, each `channel: google_ads`, `name` 1 to 100, `goal` 1 to 300, a budget above 0, dates inside the plan's (`marketing_plan_campaign`); 0 to 200 posts, each `channel` one of the eleven (`instagram`, `x`, `facebook`, `linkedin`, `threads`, `bluesky`, `tiktok`, `pinterest`, `youtube`, `google_business`, `mastodon`), `on` inside the plan's dates, `topic` 1 to 200 (`marketing_plan_post`); keys `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 40, unique across campaigns and posts (`marketing_plan_key`); 1 to 10 measures of 3 to 200 characters; the task has no plan waiting already (`marketing_plan_waiting`). Today is the UTC date of the daemon's clock, as 5.5's day is.
- **Its number** is `MP-<n>`, `n` one more than the highest of the log's `marketing_plan.proposed` and of the `MP-<n>.md` files in `docs/marketing/plans/` of the project root and of the task's worktree, taken under one lock in the tool's module, so a fresh clone never reuses a committed number. The tool writes the text, with a heading of the id and title, to `docs/marketing/plans/MP-<n>.md` in the worktree (after `Call::permit` with that path; `marketing_plan_file_exists` if it is there), records `marketing_plan.proposed`, and answers `{ plan, next: "end your turn: the owner's decision starts the next session" }`, as `farik_ask_human` does.
- **The task waits, as for a question.** A proposed plan raises the task's new `open_plans` (migration 0013), which `waiting_on_human` counts beside `open_questions` and `open_approvals` (`projections.rs:443`); the owner's decision lowers it. The task's next session is told the decision in its human message (`messages.rs:170`): "The owner approved your marketing plan MP-<n>." or "The owner sent back your marketing plan MP-<n>:" and the reason in an `untrusted` block (`source="owner_reason"`). Rejected: a plan that leaves the task free, whose returned reason would reach nobody once the task moved on.
- **Events**, in `event.schema.json` and `EVERY_KIND`: `marketing_plan.proposed { plan, title, summary, text, starts_on, ends_on, currency, budget, campaigns, posts, measures, google_ads_account?, replaces?, proposed_by }` (attributed by `proposed_by`; about the task); `marketing_plan.approved { plan, note? }` and `marketing_plan.returned { plan, reason }` (the proposing task on the envelope, no agent or session, no attribution, as `tool_approval.granted` is); `marketing_plan.ended { plan, why: replaced | by_owner | expired, replaced_by?, note? }` (about no contract).
- **Only the owner decides**, under `ask` and `auto` alike (ADR 0041): commands `marketing_plan_decide { plan, decision: approve | return, note? }` and `marketing_plan_end { plan, note? }` in `command.schema.json`, arriving only on `POST /command` behind the daemon's token or the browser's RPC `command` behind its cookie, as `tool_approve` does (ADR 0031). Rejected: RPC methods of their own, since the command line reaches a running daemon only through `POST /command` (ADR 0014). `note` is the reason and required for `return`, 1 to 600 characters (`marketing_plan_reason_needed`), optional for the others. Refusals: `unknown_marketing_plan`; `marketing_plan_decided` (decided before); `marketing_plan_expired` (approving one whose `ends_on` is past); `marketing_plan_not_approved` (ending one not approved); `marketing_plan_ended`. Decision and check are one step under one lock, as `decide_tool_call` (`human.rs:444`).
- **Which plan is active**, pure in `farik_core::marketing`: among approved plans not ended, the one approved last whose dates hold today. Approving a plan records, in the same step as its `approved`, `ended { replaced, replaced_by }` for every approved plan not ended whose `starts_on` is on or after the new one's; one that starts earlier ends on the new one's `starts_on`. A plan whose `ends_on` has passed ends `expired`. Those two dated ends are recorded by `end_marketing_plans`, a rule with no model that `tick_within` (`orchestrator.rs:462`) runs first, before its pause check, on every tick whose scope names no task: it starts no session, does not use up the tick, and so runs while the team is paused. Ending by the owner is immediate. Every end, whoever records it, goes through one function, `record_plan_end` (`crates/runtime/src/marketing.rs`, new), which 08d extends to stop the plan's posts.
- **Queries** in `rpc.schema.json`: `marketing_plan.list {}` → `{ plans: [{ plan, title, state, starts_on, ends_on, currency, total, agent_id, task_id, proposed_at }] }`, newest first, `state` `proposed | returned | approved | active | ended`; `marketing_plan.get { plan }` → the proposal whole, its `state`, the decision with its note or reason and time, and the end with its `why` (`not_found` otherwise). `waiting.list` gains rows of kind `marketing_plan` with `plan`, line "<agent name> proposes a marketing plan: <title>".
- **The screens, mocked up first (Task 0).** Today's row "Marketing plan to approve": the agent's picture and name, the title, the summary, the total budget with its currency and the dates, and "Review", which opens `/marketing/plans/MP-<n>`. The plan's page: the summary first; the budget as a table by channel and campaign; the post calendar by week (each slot's day, channel and topic); the measures; the full text shown as text, never rendered as markup; then, while proposed, "Approve" and "Send back" (a required reason, up to 600 characters); while approved or active, "End the plan" with an optional note and a confirmation saying what ending does; once decided, who decided when and their words. The board's task card says "Waiting on you" through `waiting_on_human`, unchanged.
- **The command line.** `farik marketing plan show [<plan>] [--json]` (no plan: the list), `farik marketing plan approve <plan> [--note <text>]`, `farik marketing plan return <plan> --reason <text>`, `farik marketing plan end <plan> [--note <text>]`; the three decisions through `here_or_sent`, as `farik tool approve`; `show` reads the store, as `farik waiting` does. What a process prints when it ends (5.7) gains, after the connector calls, "<plan> waits: farik marketing plan approve <plan>, or farik marketing plan return <plan> --reason <text>".
- **`marketing_paths_owned`**, a structural readiness rule after `document_paths_only` (`readiness.rs:561`): a task (not an epic) whose `assignee_role` is not the Marketing Specialist, while `active_agents_by_role` counts at least one active Marketing Specialist, fails when an `allowed_paths` entry could match a path under `docs/marketing/`, by `reaches_the_marketing_directory` (`paths.rs`, beside `reaches_the_farik_directory` at `paths.rs:110`): after `.` segments are dropped and braces expanded, its first segment holds `**`, or matches `docs` regardless of case and either ends the glob as a literal, or its second segment holds `**`, or matches `marketing` and the glob goes on or is literal there. So `docs/**`, `**/*.md` and `docs/marketing/x.md` fail, and `docs/adr/**`, `src/**` and `*.md` pass. Message: "allowed paths <list> could reach docs/marketing/, which the Marketing Specialist owns; name narrower paths or give the task to the Marketing Specialist". Plain words in `plain.rs` beside `DocumentPathsOnly`. Rejected: refusing only paths whose fixed prefix is `docs/marketing/`, which `docs/**` would pass.

## File map

```
docs/design/mockups/{TodayMarketingPlan,MarketingPlan,PhoneMarketingPlan}.dc.html, canvas.json   creates (Task 0)
crates/roles/roles/marketing_specialist/{role.yaml,system.md}, skills/marketing-what-ships/SKILL.md   modifies (Task 1)
crates/roles/roles/ui_ux_designer/skills/brand-and-design-tokens/SKILL.md       modifies (Task 1)
crates/roles/roles/marketing_specialist/skills/<4 names>/SKILL.md, kit.yaml      creates, modifies (Task 2)
crates/roles/src/{lib.rs,kit.rs}, docs/schemas/kit.schema.json                   modifies (Tasks 1 to 3)
crates/core/src/governor/{paths.rs,readiness.rs,plain.rs}                        modifies (Task 4)
crates/core/src/marketing.rs, crates/core/src/lib.rs                             creates, modifies (Tasks 5, 7)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs}            modifies (Tasks 6, 7)
crates/store/src/migrations/0013_marketing_plans.sql, projections.rs, marketing.rs, waiting.rs, lib.rs   creates, modifies (Tasks 6, 7)
crates/runtime/src/tools.rs, tools/marketing.rs, tools/git.rs, orchestrator/session.rs   modifies, creates (Tasks 3, 6)
crates/runtime/src/{allowances.rs,daemon/board.rs,daemon/team.rs}               modifies: KitConnector patterns (Task 3)
crates/runtime/src/orchestrator/{messages.rs,human.rs,rules.rs}, orchestrator.rs, daemon/gates.rs   modifies (Tasks 6, 7, 8)
crates/runtime/src/marketing.rs, crates/runtime/src/lib.rs                       creates, modifies: record_plan_end (Task 7)
docs/schemas/{command,rpc}.schema.json, crates/protocol/src/command.rs           modifies (Tasks 7, 8)
crates/cli/src/waiting.rs                                                        modifies (Task 8)
crates/cli/src/{lib.rs,marketing.rs}                                             modifies, creates (Task 9)
apps/web/src/pages/{Today.tsx,MarketingPlan.tsx(+test),Today.test.tsx}, app/App.tsx, strings/en.ts   modifies, creates (Task 10)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md               modifies (Task 11)
```

## Interfaces

Consumes: `load_role`, `load_kit`, `parse_kit`, `embedded_skills`, `KitConnector`, `check_skill` (`farik-roles`); `ReadinessRule`, `ReadinessContext`, `reaches_the_farik_directory`, `Role` (`farik-core`); `ToolDeps`, `Call`, `Call::permit`, `tool`, `call_tool`, `paths_of`, `offered_tools`, `human_message`, `handle`, `decide_tool_call`'s lock pattern, `here_or_sent` (runtime, cli); `EventBody`, `EVERY_KIND`, `Command`, `command_from_value` (protocol); `apply_waiting`, `waiting`, `EventQuery` (store).

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
pub fn check_proposal(proposal: &PlanProposal, today: NaiveDate) -> Result<(), Vec<ProposalRefusal>>;
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
// farik_runtime::tools::marketing
pub struct ProposeMarketingPlanInput { /* the design's fields, amounts as String */ }
pub struct SaveMediaInput { pub url: String, pub path: String }
pub(crate) fn propose_plan(call: &Call<'_>, input: ProposeMarketingPlanInput) -> Result<Value, ToolError>;
pub(crate) async fn save_media(call: &Call<'_>, input: SaveMediaInput) -> Result<Value, ToolError>;
pub(crate) enum MediaKind { Png, Jpeg, Webp, Svg }
pub(crate) async fn fetch_media(url: &str, hosts: &[String]) -> Result<(MediaKind, Vec<u8>), ToolError>;   // the host rule, the limits, the type and SVG checks
// farik_runtime::marketing
pub(crate) fn record_plan_end(tools: &ToolDeps, plan: &str, why: EndReason, replaced_by: Option<&str>, note: Option<String>) -> Result<Vec<FarikEvent>, String>;
// Command::MarketingPlanDecide { plan: String, approve: bool, note: Option<String> }, Command::MarketingPlanEnd { plan: String, note: Option<String> }
```

## Tasks

### Task 0: Mockups

On the canvas the earlier steps used, desktop and phone, copied to `docs/design/mockups/`: Today's row; the plan's page while proposed (with the send-back dialog), while active (with the end confirmation), and ended. The founder's approval, with its date and the canvas version, goes into this plan's Execution notes in the same commit; Task 10 waits for it.

- [ ] `docs(design): mock up the marketing plan on Today and its page`

### Task 1: The mandate

- `the_marketing_specialist_owns_the_brand_and_the_plan` (`lib.rs`, replacing `the_marketing_specialist_publishes_only_when_allowed` at `lib.rs:580`): `forbidden` is exactly the four lines of Decisions; the system prompt names `docs/marketing/brand/brand-kit.md`, `docs/marketing/brand/persona.md` and `docs/marketing/plans/`; no shipped Marketing skill says "never publish". RED.
- `the_designer_takes_the_brand_kit_first` (`lib.rs`): the Designer's `brand-and-design-tokens` names `docs/marketing/brand/brand-kit.md`. RED.

- [ ] `feat(roles): make the Marketing Specialist the owner of the brand and the plan`

### Task 2: Four skills

- `marketing_kit_carries_the_brand_and_plan_skills` replaces `marketing_kit_carries_posting_and_email` (`kit.rs:1090`): the nine then `keeping-the-brand-kit`, `writing-the-brand-persona`, `researching-the-market`, `writing-the-marketing-plan`, each with its `SKILL.md`. RED.
- `kit_skills_name_only_tools_farik_lists` (`daemon/team.rs:4034`) holds. Guard. It refuses a tool that does not exist yet, so `keeping-the-brand-kit` names `farik_save_media` only from Task 3's commit and `writing-the-marketing-plan` names `farik_propose_marketing_plan` only from Task 6's; until then each says "the tool Farik gives you for it".

- [ ] `feat(roles): teach the Marketing Specialist the brand kit, the persona, research and the plan`

### Task 3: `farik_save_media`

Founder's action first: the result address of one generated image from Higgsfield and one from Recraft (with the live pin's sign-in), whose hosts become `media_hosts` in `kit.yaml`. One commit: every `KitConnector::Server` pattern that names its fields (`kit.rs`, `runtime/src/allowances.rs:27`, `daemon/board.rs:523`, `daemon/team.rs:666`) takes `media_hosts` or `..`. Files: `kit.schema.json`, `kit.rs` (`media_hosts`, its shape), `tools.rs` (descriptor, `call_tool` arm), `tools/marketing.rs`, `tools/git.rs` (`worktree` becomes `pub(super)`), `session.rs` `offered_tools` (`session.rs:804`: `farik_save_media` and, from Task 6, `farik_propose_marketing_plan` only to a Marketing Specialist in `implement`), `keeping-the-brand-kit` (names the tool). Tests against a local `axum` fixture on loopback, through a fixture kit (`kits` swapped, ADR 0036) whose connector lists `127.0.0.1` in `media_hosts`.

- `media_hosts_only_on_a_web_service`: a `stdio` connector with `media_hosts` is `kit_field_not_allowed`; a host with `*` is a schema error. RED.
- `saves_an_image_from_a_kit_service`: a PNG from a listed host lands at `docs/marketing/brand/assets/logo.png` byte for byte. RED.
- `refuses_a_host_outside_the_session_s_services`: another host, `http` to a host that is not loopback, `https` on a port other than 443, and userinfo are each `media_host_refused`, nothing fetched. RED.
- `no_shipped_kit_names_a_loopback_media_host` (`kit.rs`): no `media_hosts` of any shipped kit is `localhost`, `127.0.0.1` or `::1`. Guard.
- `refuses_a_type_that_does_not_match`: a JPEG named `.png`, and an HTML page named `.svg`, are `media_type_mismatch`. RED.
- `refuses_an_unsafe_svg`: one case per item of the SVG list (`<SCRIPT`, `onload=`, `xlink:href="https://x"`, `url(https://x)`), each `svg_not_safe`; an SVG with `href="#a"` and `url(#g)` saves. RED.
- `refuses_a_large_or_redirected_answer`: 10 MiB + 1 byte is `media_too_large`; a 302 is not followed. RED.
- `only_the_marketing_specialist_in_a_task_saves_media`: a Developer, and a Marketing Specialist's chat, are `media_refused`. RED.

- [ ] `feat(runtime): let the Marketing Specialist keep the media it made in the brand kit`

### Task 4: `marketing_paths_owned`

- `reaches_the_marketing_directory_as_the_rule_says` (`paths.rs`): true for `docs/**`, `**/*.md`, `Docs/Marketing/x.md`, `docs/{marketing,adr}/**`, `./docs/marketing`, `docs`; false for `docs/adr/**`, `src/**`, `*.md`, `docs/*.md`. RED.
- `another_role_may_not_name_the_marketing_folder` (`readiness.rs`): a Product Manager's task with `docs/**` fails `MarketingPathsOwned` with the message's paths while one Marketing Specialist is active, and passes with none active; a Marketing Specialist's task with `docs/marketing/**` passes; an epic is not held. RED.

- [ ] `feat(core): keep the marketing folder the Marketing Specialist's`

### Task 5: The plan, checked

- `checks_a_proposal_field_by_field` (`core::marketing`): one case per refusal of Decisions, each naming its code and field, and a valid proposal passes; 93 days fails, 92 passes; `"10.999"`, `"1,000"` and `"-1"` are not amounts. RED.
- `campaign_budgets_fit_the_channel`: two campaigns of 600.00 against `google_ads` 1000.00 fail `marketing_plan_budget`; 500.00 each pass. RED.
- `one_plan_is_active`: an approved plan within its dates is active; a later approval starting today replaces it; one starting later leaves it active until that day, then `plans_to_end` names it `replaced` with the newer id; past `ends_on` it is `expired`; a returned plan is never active. RED.

- [ ] `feat(core): check a marketing plan and decide which one is active`

### Task 6: Proposing a plan

Files: the four `marketing_plan.*` kinds in `event.schema.json` and every exhaustive match (`event.rs`: `EventBody`, `kind`, `body_def_name`, `attribution`, `is_about_one_contract`, `EVERY_KIND`; `lib.rs` `KINDS`; `projections.rs` `apply_to`), one commit so it compiles; the migration; `apply_waiting` (`projections.rs:745`) and `waiting_on_human` (`projections.rs:443`); `store::marketing` and `store/src/lib.rs`; `tools/marketing.rs` `propose_plan`, its descriptor and `call_tool` arm; `offered_tools`; `writing-the-marketing-plan` (names the tool). `approved` and `returned` get `apply_to`'s no-op arm here and move to `apply_waiting` in Task 7, under its test.

- `proposes_a_plan_and_writes_its_text`: records `marketing_plan.proposed` as `MP-1` with every field, writes `docs/marketing/plans/MP-1.md` in the worktree, and the task waits on the human (`open_plans` 1). RED.
- `numbers_past_a_committed_plan`: with `docs/marketing/plans/MP-4.md` in the project root and an empty log, the next is `MP-5`. RED.
- `refuses_a_bad_proposal_with_every_reason`: one call with three faults answers all three codes, writes nothing. RED.
- `refuses_a_second_plan_on_the_task` (`marketing_plan_waiting`) and `refuses_another_role_or_session` (`marketing_plan_refused`). RED each.
- `the_store_folds_the_plans`: `marketing_plans` gives proposed, approved, returned and ended plans with their decisions, oldest first. RED.

- [ ] `feat(runtime): let the Marketing Specialist propose a marketing plan`

### Task 7: The owner decides

Files: `command.schema.json`, `command.rs` (`Command`, `human_command`, `command_to_value`), `human.rs` (`handle` at `human.rs:54`), `runtime/src/marketing.rs` (`record_plan_end`), `apply_waiting`, the end rule (`rules.rs`, called from `tick_within`), `messages.rs` (`human_message` at `messages.rs:170`).

- `approving_lowers_the_wait_and_tells_the_task`: `marketing_plan_decide` approve records `approved`, `open_plans` falls to 0, and the task's next human message says "The owner approved your marketing plan MP-1." RED.
- `returning_needs_a_reason_and_quotes_it`: without a note `marketing_plan_reason_needed`; with one, `returned`, and the next human message holds it inside `<untrusted source="owner_reason">`. RED.
- `decided_once_and_never_expired`: a second decision `marketing_plan_decided`; an unknown id `unknown_marketing_plan`; approving one whose `ends_on` passed `marketing_plan_expired`. RED.
- `approving_a_newer_plan_replaces_one_not_yet_started`: with MP-1 approved to start next week, approving MP-2 starting tomorrow records `ended { replaced, replaced_by: "MP-2" }` for MP-1 in the same step. RED.
- `the_owner_ends_a_plan`: `marketing_plan_end` records `ended { by_owner }`; ending a proposed one is `marketing_plan_not_approved`, an ended one `marketing_plan_ended`. RED.
- `the_tick_ends_plans_by_their_dates`: a tick on the replaced plan's day records `ended { replaced, replaced_by }`, one past `ends_on` `ended { expired }`, once each, with no session, also while the team is paused. RED.

- [ ] `feat(runtime): let the owner approve, return and end a marketing plan`

### Task 8: The queries

Files: `rpc.schema.json`, `gates.rs` (`query` at `gates.rs:133`, `waiting_row` at `gates.rs:113`), `store/src/waiting.rs` (`WaitingKind::MarketingPlan`), and in the same commit, since it matches `WaitingKind` exhaustively, `cli/src/waiting.rs` (the end-of-run line).

- `lists_and_gets_plans`: `marketing_plan.list` newest first with each `state`; `marketing_plan.get` the whole proposal and its decision; `not_found` for `MP-9`. RED.
- `waiting_lists_a_plan_to_approve`: a row of kind `marketing_plan` with `plan` and the line; gone once decided. RED.
- `a_run_says_which_plan_waits` (`cli/src/waiting.rs`): the end-of-run line for `MP-1`, and with `--json` its `plan` field. RED.

- [ ] `feat(runtime): list the marketing plans and the one waiting on the owner`

### Task 9: The command line

- `marketing_plan_approve_sends_the_decision` and `marketing_plan_return_needs_a_reason` (the parser refuses it without `--reason`); `marketing_plan_show_prints_the_list_and_one_plan` (`--json` stdout pure). RED each.

- [ ] `feat(cli): decide marketing plans`

### Task 10: The screens

As the approved mockups. `MarketingPlan.test.tsx`, `Today.test.tsx`.

- `today_shows_a_plan_to_approve_with_its_summary_and_budget`; `review_opens_the_plan_page`; `the_page_shows_the_budget_calendar_and_text_as_text` (markup in the text is inert); `approve_sends_the_decision`; `send_back_needs_a_reason`; `end_asks_first_then_sends`. RED each.

- [ ] `feat(web): approve a marketing plan on Today`

### Task 11: Spec and plan

`docs/SPEC.md` 6.5 (what was built: the mandate, the skills, `farik_save_media`, the plan's checks, numbers, events and decisions, with any change made in execution), 5.3 (`marketing_paths_owned`), 5.7 (a plan waits on the owner as a question does), 6.7 (`media_hosts`), 6.8 (the Designer reads the brand kit), 8.5 (the four kinds); the revision line. `docs/design/role-kits.md` (the Marketing row). Project plan row 08c.

- [ ] `docs(spec): record the brand and the marketing plan`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: a marketing task that writes the brand kit with one Recraft logo saved by `farik_save_media`, then proposes a two-week plan with one post slot; Today shows it; send it back with a reason; the next session's plan is approved; `farik marketing plan show` lists both; a Product Manager's task naming `docs/**` fails readiness with the rule's message.

## Execution notes

None yet.
