# Phase 7, step 08d: Posting through the plan

Status: draft. Its readiness review runs once step 08c has landed (ADR 0032: one round).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 5.7, 6.5, 6.7, 8.5, 8.6; F3, F9
Depends on: step 08c of this phase (the active plan, its post slots, `marketing_plans`, `active_plan`, `record_plan_end`, `media_hosts`, the media fetch of `farik_save_media`); step 08b (Buffer in the kit); steps 03 and 05 (signed-in kit entries, `refreshed_entry`, `matches_kit`); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A post in the owner's approved marketing plan goes out without asking (ADR 0042). The Marketing Specialist writes it with `farik_schedule_post`; Farik checks it fills an unused slot of the active plan at least three hours ahead, shows it on Today under "Going out" with its text, pictures, channel, time and a Stop button, and one hour before its time calls Buffer's `create_post` itself, with the agent's Buffer connection, scheduled for that time. Stop takes it back, from Buffer too while it has not gone out. A post outside the plan waits on Today with "Post it" and "Don't post". Buffer's own write tools are no longer offered to the agent. Out of scope: posting on `auto` outside the plan (10h); replying to comments; any network but through Buffer; Google Ads (08f).

## Decisions

- **Farik calls a connector's tool itself** with `connectors::call_tool`, beside `list_tools` (`connectors.rs:804`), callers writing it with its module (`crate::tools::call_tool` is Farik's own tools): the same start (a `stdio` server in its folder with only `KEPT_ENV` and its keys, an `http` one with its filled headers and bearer), one MCP session per call, opened, `tools/call`, cancelled, the same 30-second limit. It answers the result's structured content when there is one, else its first text block parsed as JSON, else `{ "text": <text> }`; a result marked as an error is `ConnectorError::ToolError { text }`, the text cut at 500 characters, which Farik passes on only as untrusted words. This is the signature step 12's draft names, so step 12 consumes it.
- **Only a fixed list of Farik's own calls**, `OWN_CALLS` in `daemon/own_calls.rs` (new): `("buffer", "get_channel")`, `("buffer", "create_post")`, `("buffer", "delete_post")`; any other is refused `own_call_not_listed` before anything starts. Farik's calls are not the agent's: they pass no hook, may name a tool the kit tags `denied` (Buffer's `delete_post`), and are recorded as the `social_post.*` events. `call_as(state, agent, server, tool, arguments)` takes the named agent's kit entry from the team file, requires `matches_kit` (`daemon/team.rs:632`) against its role's kit and `read_kept(&at).runs(&server)` (`daemon.rs:383`), refreshes its sign-in with `refreshed_entry` (`daemon/signed_in.rs:39`; `valid_for` 60 s, `wait` 10 s, falling back to a token still good), and runs it in `connector_folder` (`daemon.rs:364`) with `own_program` (`daemon.rs:298`). Rejected: a session or a hook for Farik's own calls, which would charge the agent's tool-call limit for acts that are the owner's.
- **Whose connection**: the agent that wrote the post. A post of an agent whose Buffer entry is gone, lapsed or changed fails at hand-over (`social_post.failed`, "<agent>'s Buffer connection is not there; connect Buffer again").
- **`farik_schedule_post { channel, buffer_channel, text, media, at, slot? }`**, tier `read` (what governs it is the plan and the owner, 5.6), a Marketing Specialist's session about a task only (`post_refused`). The design's input gains `buffer_channel`, Buffer's channel id, which the agent reads with Buffer's `list_channels`. Checks, in this order, each a refusal code: `channel` one of 08c's eleven; `buffer_channel` `^[A-Za-z0-9_-]{1,64}$`; `text` 1 character to the network's limit, counted in Unicode scalar values, with no NUL (`post_too_long`): x 280, bluesky 300, threads 500, mastodon 500, pinterest 500, google_business 1,500, instagram 2,200, tiktok 2,200, linkedin 3,000, youtube 5,000, facebook 63,206; `media` at most 4 items `{ url, kind: image | video }`, the kind the agent's (a generated file's address need not end in its type, and Buffer reads the file itself), each `url` held to 08c's rule (`https` on 443, or `http` on loopback for the tests, a host among the `media_hosts` of a kit connector the session was given; `media_host_refused`); instagram and pinterest need one or more, tiktok and youtube a video (`post_needs_media`); `at` RFC 3339 with an offset, after now (`post_in_the_past`) and at most 92 days ahead; then Farik's own `get_channel { channelId }` answers a channel whose service, lower-cased with spaces and `_` taken out, is the channel's (`x` is Buffer's `twitter`; `google_business`, `googlebusiness`) (`post_wrong_channel`; Buffer not answering, `buffer_unreachable`).
- **In the plan** (`slot` given): an active plan (`no_active_marketing_plan`); `slot` a post slot of it with the same channel (`not_a_plan_slot`); no other post of that plan and slot `scheduled` or `sent` (a stopped, missed or failed one frees it) (`slot_used`); `at`'s date in its own offset is the slot's `on` (`post_off_its_day`), so Farik needs no time-zone table (rejected: a plan time zone, which needs a time-zone database and a setting Farik does not have); `at` at least three hours from now (`post_too_soon`). It records `social_post.scheduled { channel, buffer_channel, text, media, at, approved_by: plan, plan, slot }`, whose sequence number is the post's number, and answers `{ post, hands_over_at }`.
- **Outside the plan** (no `slot`): `social_post.requested { channel, buffer_channel, text, media, at }`, its sequence number the post's. The task does not wait: Farik, not the agent, posts it once allowed. Today's "Waiting on you" lists it, "<agent> wants to post on <network>", with "Post it" and "Don't post"; `social_post_decide { post, decision: post | dont_post, note? }`, from the daemon's token or the browser's cookie alone, records `social_post.scheduled { post, …the request's fields, approved_by: owner }` or `social_post.stopped { post, by: declined, note? }`; `unknown_post`, `post_decided`, and `post_in_the_past` when `at` has passed. Under `auto` it is step 10h's to send (ADR 0041).
- **The hand-over**, a rule with no model, `hand_over_posts`, that `tick_within` (`orchestrator.rs:462`) runs after its pause check, on every tick whose scope names no task, before the rules; it starts no session and does not use up the tick. A paused team hands nothing over. For each scheduled post neither stopped, sent, missed nor failed, oldest hand-over first, whose hand-over time has come (an hour before `at` for a plan's post; for one the owner allowed, the later of that and the decision): its plan, for a plan's post, no longer active, `social_post.stopped { post, by: plan_ended }`; `at` within 5 minutes, `social_post.missed { post }`; else Farik's own `create_post { channelId, text, schedulingType: "automatic", mode: "customScheduled", dueAt: at, assets: [{ image: { url } } | { video: { url } }] }` (Buffer's input as its documentation and 08b's pin give it), then `social_post.sent { post, buffer_post }`, Buffer's id read from the answer's `id` (or `post.id`), else `social_post.failed { post, reason }`, `reason` Farik's sentence then Buffer's words cut at 300. A hand-over missed while Farik was not running is so sent when `at` is still more than 5 minutes ahead, which is the design's rule.
- **Stop**, `social_post_stop { post }`, the owner's alone: a scheduled post not yet sent records `social_post.stopped { post, by: owner }`; a sent one whose `at` is ahead first has Farik's own `delete_post { postId }`, and only on its success records `stopped { post, by: owner, taken_back: true }`; on failure it is refused `post_not_taken_back: Buffer did not take it back; delete it in Buffer before <time>` and nothing is recorded; once `at` has passed, `post_already_out`. Ending a plan stops its scheduled posts not yet sent at once: `record_plan_end` (08c) records `stopped { by: plan_ended }` for each; posts already with Buffer stay, each still with its Stop.
- **Events**, in `event.schema.json` and every exhaustive match: `social_post.scheduled { post?, channel, buffer_channel, text, media, at, approved_by: plan | owner, plan?, slot? }` (`post` only when it follows a request), `social_post.requested { channel, buffer_channel, text, media, at }`, `social_post.sent { post, buffer_post }`, `social_post.stopped { post, by: owner | declined | plan_ended, taken_back?, note? }`, `social_post.missed { post }`, `social_post.failed { post, reason }`. The two the agent records carry its agent, session and task on the envelope and are about the task; the others are about no contract. `media` items are `{ url, kind: image | video }`.
- **The agent hears of a missed or failed post** in its next `implement` session's message (`rules.rs:1155`), after the rest: "Farik could not post <post> (<network>, <at>):" and the reason in an `untrusted` block, for each of its posts missed or failed since its previous `implement` session started.
- **What Today shows**, from `social_posts.list {}`, newest hand-over last: every post scheduled or sent whose `at` is ahead, and every post missed or failed in the last 24 hours with its reason as text; each `{ post, agent_id, channel, text, media, at, state, plan?, slot?, approved_by?, reason? }`. Pictures are fetched by the daemon, never by the browser, whose `img-src` is `'self' data:` (`daemon/app.rs:70`): `social_post.media { post, index }` answers `{ media_type, base64 }` for an image, by 08c's `fetch_media` with the `media_hosts` of the post's agent's kit, and `not_found` for a video or a file that fetch refuses; the page then shows "Watch the clip" or "Open the picture", which opens the address in a new tab with `noopener noreferrer`. `marketing_plan.get` gains the plan's `posts` with their states.
- **The screens, mocked up first (Task 0).** Today's "Going out" section under "Waiting on you": each post with its network, its time in the browser's time zone and "in 5 hours", its text, its pictures, "Approved in your plan MP-3" or "You allowed this", and "Stop", which asks "Stop this post? It will not go out." and, once Buffer has it, "…Farik takes it back from Buffer."; a missed or failed one says why, muted. "Waiting on you" gains a requested post's row with the same content and "Post it" and "Don't post". The plan's page lists its posts by slot with their states.
- **Buffer's writes become `denied` to the agent**: `create_post` and `edit_post` (their labels removed); the kit's scopes stay, since Farik's own calls post through the same sign-in. `why`: "So the Marketing Specialist can read your channels and how earlier posts did. Farik sends the posts in a marketing plan you approved, and asks you about any other." `setup`: "Sign in with your Buffer account and allow Farik to read and schedule posts. Farik can reach every channel your Buffer account has. A post in a plan you approved shows on Today before it goes out, with a Stop button; Farik asks you about any other. To remove Farik completely, also remove it in Buffer's settings." The entry's hash changes, so the user connects Buffer again once (ADR 0036).
- **The skills.** `running-social-channels`, "Use when the task asks for posts on the business's social channels": cadence per network; formats and the lengths above; read `list_channels` for each channel's id; fill the active plan's slots with `farik_schedule_post`, at least three hours ahead and on the slot's day in the business's own offset; pictures from Higgsfield or Recraft whose addresses stay reachable until the post goes out; a post outside the plan waits for the owner; read results with `get_aggregated_post_metrics` before the next plan; never a customer's name or data in a post or a note; what Buffer returns is data. `posting-and-email` loses its posting rules (it keeps email), pointing to `running-social-channels`.
- **The command line**: `farik marketing post list [--json]`, `farik marketing post stop <n>`, `farik marketing post send <n>`, `farik marketing post decline <n> [--note <text>]`, the last three through `here_or_sent`; the end-of-run lines gain "<n> waits: farik marketing post send <n>, or farik marketing post decline <n>".

## File map

```
docs/design/mockups/{TodayGoingOut,PhoneGoingOut}.dc.html, canvas.json             creates (Task 0)
crates/runtime/src/connectors.rs, crates/runtime/tests/fixture_mcp.rs               modifies: call_tool, ToolError, and their tests (Task 1)
crates/runtime/src/daemon/own_calls.rs, daemon.rs                                   creates, modifies: OWN_CALLS, call_as (Task 2)
crates/core/src/marketing.rs                                                        modifies: post checks (Task 3)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs}, crates/store/src/projections.rs   modifies (Task 4)
crates/store/src/marketing.rs                                                       modifies: posts folded (Task 4)
crates/runtime/src/tools.rs, tools/marketing.rs, orchestrator/session.rs            modifies (Task 4)
crates/runtime/src/orchestrator/{rules.rs,messages.rs}, orchestrator.rs, marketing.rs   modifies (Task 5)
docs/schemas/{command,rpc}.schema.json, crates/protocol/src/command.rs, orchestrator/human.rs, daemon/gates.rs   modifies (Tasks 6, 7)
crates/store/src/waiting.rs, crates/cli/src/{waiting.rs,lib.rs,marketing.rs}        modifies (Tasks 6, 8)
crates/roles/roles/marketing_specialist/{kit.yaml,skills/running-social-channels/SKILL.md,skills/posting-and-email/SKILL.md}, crates/roles/src/kit.rs   modifies, creates (Task 9)
crates/runtime/src/daemon/team.rs                                                   tests (Task 9)
apps/web/src/pages/{Today.tsx,Today.test.tsx,MarketingPlan.tsx,PostGoingOut.tsx,PostGoingOut.test.tsx}, strings/en.ts   modifies, creates (Task 10)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md                  modifies (Task 11)
```

## Interfaces

Consumes: `list_tools`, `KEPT_ENV`, `named_keys`, `filled_headers`, `ConnectorError`, `own_program`, `program` (`connectors.rs`); `refreshed_entry`, `Fresh`, `secret_at`, `connector_folder`, `read_kept`, `Kept::runs`, `matches_kit`, `kit_entry` (daemon); `active_plan`, `PostChannel`, `marketing_plans`, `record_plan_end`, `fetch_media`, `MediaKind`, `media_hosts` (08c); `tick_within`, `human_message`, `here_or_sent`.

Produces:

```rust
pub async fn call_tool(server: &CustomServer, keys: &BTreeMap<String, Secret>, bearer: Option<&Secret>, folder: &Path,
    farik: &Path, tool: &str, arguments: serde_json::Map<String, Value>) -> Result<Value, ConnectorError>;   // connectors
// ConnectorError::ToolError { text: String }
pub(crate) const OWN_CALLS: &[(&str, &str)];                                                   // daemon::own_calls
pub(crate) enum OwnCallError { NotListed, NotConnected(String), SignInAgain, Failed(String), Tool(String) }
pub(crate) async fn call_as(state: &Arc<DaemonState>, agent: &str, server: &str, tool: &str,
    arguments: serde_json::Map<String, Value>) -> Result<Value, OwnCallError>;
pub fn text_limit(channel: PostChannel) -> usize;                                               // farik_core::marketing
pub struct SlotCheck<'a> { pub plan: &'a PlanProposal, pub slot: &'a str, pub channel: PostChannel, pub at: DateTime<FixedOffset>,
    pub now: DateTime<Utc>, pub used: &'a [String] }
pub fn check_slot(check: &SlotCheck<'_>) -> Result<(), &'static str>;   // not_a_plan_slot, slot_used, post_off_its_day, post_too_soon
pub enum PostState { Requested, Scheduled, Sent, Stopped, Missed, Failed }                       // farik_store::marketing
pub struct PostMedia { pub url: String, pub video: bool }
pub struct SocialPost { pub post: u64, pub agent_id: String, pub task_id: Option<TaskId>, pub channel: PostChannel, pub buffer_channel: String,
    pub text: String, pub media: Vec<PostMedia>, pub at: DateTime<FixedOffset>, pub plan: Option<String>, pub slot: Option<String>,
    pub approved_by: Option<String>, pub decided_at: Option<DateTime<Utc>>, pub buffer_post: Option<String>, pub state: PostState, pub reason: Option<String> }
pub fn social_posts(log: &EventLog) -> Result<Vec<SocialPost>, StoreError>;
pub(crate) async fn hand_over_posts(deps: &OrchestratorDeps) -> Result<(), OrchestratorError>;   // orchestrator::rules
```

## Tasks

### Task 0: Mockups

"Going out" and its Stop confirmation, a requested post's row, and the plan page's posts, desktop and phone; the founder's approval in the Execution notes; Task 10 waits for it.

- [ ] `docs(design): mock up posts going out on Today`

### Task 1: Farik calls a connector's tool

`fixture_mcp.rs` (step 01's: the `sh` server of `fixtures/mcp_server.sh`, which answers `tools/call`, and an in-process streamable-HTTP `rmcp` server) gains HTTP tools that answer structured content, JSON text, plain text, an error, or sleep; the `sh` server's `env` tool answers what it sees of its environment.

- `calls_a_tool_and_reads_its_answer`: each of the three answers comes back as decided. RED.
- `a_tool_error_keeps_its_words_cut`: a 2,000-character error is `ToolError` of 500. RED.
- `a_stdio_server_gets_only_its_keys`: the fixture reads its environment: `KEPT_ENV` and its key, not a variable the test set. RED.
- `an_http_server_gets_the_bearer` and `gives_up_after_thirty_seconds` (the limit a test parameter, paused clock). RED each.

- [ ] `feat(runtime): let Farik call a connector's tool itself`

### Task 2: Farik's own calls, through an agent's connection

- `calls_only_the_listed_tools`: `("buffer", "list_posts")` is `NotListed`, the fixture saw nothing. RED.
- `calls_with_the_agent_s_kept_sign_in`: against a fixture kit's signed-in entry (`kits` swapped as in `daemon/team.rs`'s tests), the fixture sees the agent's bearer, refreshed when about to expire. RED.
- `refuses_an_entry_that_is_not_the_kit_s_or_not_kept`: a changed entry is `NotConnected`, a lapsed one `SignInAgain`. RED.

- [ ] `feat(runtime): let Farik call a service with an agent's connection`

### Task 3: A post, checked

- `limits_each_network_s_text`: 280 and 281 characters on `x`; 2,200 and 2,201 on `instagram`. RED.
- `a_slot_is_the_plan_s_on_its_day_three_hours_ahead`: one case per `check_slot` refusal; `2026-11-03T23:30:00-05:00` fits a slot on 2026-11-03, `2026-11-04T00:30:00+01:00` a slot on 2026-11-04. RED.

- [ ] `feat(core): check a social post against the plan`

### Task 4: `farik_schedule_post`

The six `social_post.*` kinds in the schema and every exhaustive match, in this commit; `store::marketing` folds them; the tool, its descriptor and `call_tool` arm; `offered_tools` (`session.rs:804`) to a Marketing Specialist in `implement`. `get_channel` is answered by the Task 1 fixture as Buffer.

- `schedules_a_post_in_the_plan`: records `scheduled` with `approved_by: plan`, answers `hands_over_at` an hour before `at`. RED.
- `refuses_a_post_the_plan_does_not_hold`: no active plan, another channel's slot, a used slot, the wrong day, two hours ahead, each its code, nothing recorded. RED.
- `requests_a_post_outside_the_plan`: records `requested`; the task does not wait. RED.
- `refuses_the_wrong_buffer_channel`: `get_channel` answering `linkedin` for `x` is `post_wrong_channel`. RED.
- `refuses_media_from_elsewhere_and_a_post_without_its_picture`: a host outside `media_hosts`, an `http` address to a host that is not loopback, an Instagram post with none, a TikTok post with only an image. RED.

- [ ] `feat(runtime): let the Marketing Specialist schedule a post`

### Task 5: The hand-over

`rules.rs` `hand_over_posts`, called from `tick_within`; `messages.rs` (the missed and failed lines); `marketing.rs` `record_plan_end` stops a plan's posts.

- `hands_a_post_to_buffer_an_hour_before`: on the paused clock at `at` − 59 minutes, one `create_post` with exactly the input of Decisions, then `sent` with Buffer's id; nothing at `at` − 61 minutes. RED.
- `sends_a_late_hand_over_and_misses_a_past_one`: Farik not ticking until `at` − 10 minutes sends it; until `at` − 4 minutes records `missed`. RED.
- `a_failure_is_recorded_with_buffer_s_words`: a tool error records `failed` with Farik's sentence and the cut words. RED.
- `a_paused_team_hands_nothing_over`. RED.
- `ending_the_plan_stops_its_posts`: `marketing_plan_end` records `stopped { plan_ended }` for a scheduled post, not for one Buffer has. RED.
- `the_agent_hears_of_a_missed_post`: its next implement message holds the line and the reason in an `untrusted` block. RED.

- [ ] `feat(runtime): hand the plan's posts to Buffer an hour before they go out`

### Task 6: Stop, and the owner's decision

Commands in `command.schema.json` and `command.rs`, `human.rs`; `WaitingKind::SocialPost` (`store/src/waiting.rs`) with, in the same commit, `cli/src/waiting.rs`'s end-of-run line.

- `stop_before_the_hand_over_records_it`; `stop_after_takes_it_back_from_buffer` (`delete_post` with Buffer's id, then `stopped { taken_back }`); `a_stop_buffer_refuses_records_nothing` (`post_not_taken_back`); `too_late_to_stop` (`post_already_out`). RED each.
- `post_it_schedules_the_request` (`scheduled { post, approved_by: owner }`, its hand-over due at the next tick when `at` is within the hour) and `dont_post_stops_it` (`stopped { declined }`); `decided_once` (`post_decided`). RED each.
- `waiting_lists_a_requested_post` and `a_run_says_which_post_waits`. RED each.

- [ ] `feat(runtime): let the owner stop a post or decide one outside the plan`

### Task 7: The queries

- `social_posts_list_answers_what_goes_out`; `the_daemon_fetches_a_post_s_picture` (`social_post.media` gives the PNG's base64, `not_found` for a video or another post); `the_plan_page_lists_its_posts`. RED each.

- [ ] `feat(runtime): list the posts going out`

### Task 8: The command line

- `post_stop_sends_the_number`; `post_list_prints_what_goes_out` (`--json` pure); `post_send_and_decline_send_the_decision`. RED each.

- [ ] `feat(cli): stop and decide posts`

### Task 9: Buffer's writes, Farik's

- `buffer_posts_only_through_farik` replaces `buffer_posts_only_when_asked` (`kit.rs:1753`): `create_post` and `edit_post` `denied`, the 10 `network` and 10 `denied` names exactly, no `external_effect`, the new `why` and `setup` exactly. RED.
- `marketing_kit_carries_running_social_channels` replaces 08c's `marketing_kit_carries_the_brand_and_plan_skills`: the thirteen of 08c then `running-social-channels`, which names `farik_schedule_post`. RED.
- `connects_each_marketing_service_by_name` (`daemon/team.rs:3975`) still connects `buffer`. Guard.

- [ ] `feat(roles): let the Marketing Specialist post only through the plan`

### Task 10: The screens

As the approved mockups.

- `going_out_lists_posts_with_their_time_and_pictures`; `stop_asks_then_sends`; `a_requested_post_offers_post_it_and_dont_post`; `a_failed_post_says_why_as_text`; `the_plan_page_shows_each_slot_s_post`. RED each.

- [ ] `feat(web): show posts going out, with Stop`

### Task 11: Spec and plan

`docs/SPEC.md` 6.5 (posts, as built), 6.7 (Buffer's writes Farik's; `call_tool` and `OWN_CALLS`; the copy), 5.6 and 5.7 (a requested post waits on the owner and holds no task), 8.5 (the six kinds), 8.6 (Farik's own calls pass no hook; the daemon fetches pictures); the revision line. `docs/design/role-kits.md` (the Marketing row). Project plan row 08d.

- [ ] `docs(spec): record posting through the plan`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, Buffer with create_post and edit_post denied, no drift
```

Then, by the founder, with Buffer connected again and a test Instagram or X channel: approve a plan with one slot four hours out; the agent schedules it; Today shows it under "Going out"; an hour before, Buffer's queue has it at its time; a second post is stopped after the hand-over and leaves Buffer's queue; a post outside the plan waits with "Post it".

## Execution notes

None yet.
