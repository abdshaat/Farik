# Phase 7, step 08d: Posting through the plan

Status: ready
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 5.7, 6.5, 6.7, 8.5, 8.6; F3, F9
Depends on: step 08c of this phase (the active plan, its post slots, `marketing_plans`, `active_plan`, `PLANS` and `hold_plans`, `record_plan_end`, `media_hosts`, `fetch_media`; the line numbers cited here move when it lands); step 08b (Buffer in the kit); steps 03 and 05 (signed-in kit entries, `refreshed_entry`, `matches_kit`); phase 6 (merged in #19). ADR 0042, with its amendment on `OWN_CALLS`.
Changed 2026-10-06: step 08c's Task 10 was dropped by the founder (the brand kit's pictures are the user's own). The founder then decided a post's pictures may be ones the agent made with its creative services or the owner's own at any public web address ("Generated, and mine"). So this step pins no media hosts: a post's `media` is up to four `https` addresses, each checked at scheduling and again at hand-over to answer with an image or a video (content type and size, by a bounded `GET`), never a loopback or private address; Today shows each picture before it goes out, with Stop; `running-social-channels` says to use only the business's own pictures, ones the owner gave, or ones the agent made, never another's. Wherever this plan names `media_hosts`, `media_url_allowed` with hosts, or 08c's `fetch_media`, read that check instead; the code kept aside from 08c (its bounded fetch, content-type and SVG checks, and loopback fixture) is reused for it, without the host list.
Readiness confirmed by: a fresh Opus session, 2026-10-05 (one round, against docs/standards/workflow.md stage 2): three Blocking, each a decision, folded below with its Should items; the controller confirmed the three sentences went in
Mockups approved by: the founder, 2026-10-05, as drawn (Task 0's boards on the canvas's "Marketing plan and posts" page)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A post in the owner's approved marketing plan goes out without asking (ADR 0042). The Marketing Specialist writes it with `farik_schedule_post`; Farik checks it fills an unused slot of the active plan at least three hours ahead, shows it on Today under "Going out" with its text, pictures, channel, time and a Stop button, and one hour before its time calls Buffer's `create_post` itself, with the agent's Buffer connection, scheduled for that time. Stop takes it back, from Buffer too while it has not gone out. A post outside the plan waits on Today with "Post it" and "Don't post". Buffer's own write tools are no longer offered to the agent. Out of scope: posting on `auto` outside the plan (10h); replying to comments; any network but through Buffer; Google Ads (08f).

## Decisions

- **Who builds what.** 08d builds `connectors::call_tool`, `ConnectorError::ToolError`, `OWN_CALLS` and `call_as` (ADR 0042's amendment); every later step that has Farik call a service itself (08g, 11c, 12) adds its pairs to `OWN_CALLS` and calls through `call_as`, never beside it, and step 12 consumes `call_tool` (Task 11 edits its draft). `approved_by: plan | owner` on `social_post.scheduled` is 08d's own wire enum, and 10h adds `auto` to it; `ApprovedBy` on `tool.called` is 10h's.
- **Farik calls a connector's tool itself** with `connectors::call_tool`, beside `list_tools` (`connectors.rs:804`), callers writing it with its module (`crate::tools::call_tool` is Farik's own tools): the same start (a `stdio` server in its folder with only `KEPT_ENV` and its keys, an `http` one with its filled headers and bearer), one MCP session per call, opened, `tools/call`, cancelled, within the 30 seconds of `LISTING_TIMEOUT` (`connectors.rs:605`; past them `ConnectorError::Timeout`). It answers the result's structured content when there is one, else its first text block parsed as JSON, else `{ "text": <text> }`. A result marked as an error, and a JSON-RPC error to the call, are `ConnectorError::ToolError { text }`, the text cut at 500 characters, which Farik passes on only as untrusted words; Farik's arguments hold no key, so these words may be kept where `list_tools` keeps none.
- **Only a fixed list of Farik's own calls**, `OWN_CALLS` in `daemon/own_calls.rs` (new): `("buffer", "get_channel")`, `("buffer", "create_post")`, `("buffer", "delete_post")`; any other is `NotListed` before anything starts. Farik's calls are not the agent's: they pass no hook, may name a tool the kit tags `denied` (Buffer's `delete_post`), and are recorded as the `social_post.*` events. `call_as(state, agent, server, tool, arguments)` takes the named agent's kit entry from the team file, requires `matches_kit` (`daemon/team.rs:632`) against its role's kit and `read_kept(&at).runs(&server)` (`daemon.rs:383`) (`NotConnected` otherwise), and runs it in `connector_folder` (`daemon.rs:364`) with `own_program` (`daemon.rs:298`). It refreshes only a signed-in entry, with `refreshed_entry` (`daemon/signed_in.rs:39`; `valid_for` 60 s, `wait` 10 s, falling back to a token still good; `Lapsed` is `SignInAgain`, `NotConfirmed` `NotConnected`, the others `Failed`); a keys entry is used as kept, since `refreshed_entry` answers `NotConfirmed` for one. `ConnectorError::Timeout` is `Timeout`, `KeyMissing` `NotConnected`, `Failed` `Failed`, `ToolError` `Tool`. Rejected: a session or a hook for Farik's own calls, which would charge the agent's tool-call limit for acts that are the owner's.
- **Whose connection**: the agent that wrote the post. A post of an agent whose Buffer entry is gone, lapsed or changed fails at hand-over ("<agent>'s Buffer connection is not there; connect Buffer again.").
- **The tool reaches the daemon** through `ToolContext.daemon: Weak<DaemonState>` (`tools.rs:106`), set by `tool_context(self: &Arc<Self>, …)` (`daemon.rs:570`); a strong `Arc` would be a cycle through `sessions`. A failed upgrade answers `buffer_unreachable`; `tools/fixtures.rs`'s `context` sets `Weak::new()`.
- **`farik_schedule_post { channel, buffer_channel, text, media, at, details?, slot? }`**, tier `read` (what governs it is the plan and the owner, 5.6), offered only to a Marketing Specialist's `implement` session about a task (`offered_tools`, `session.rs:804`) and `post_refused` from any other. The design's input gains `buffer_channel`, Buffer's channel id, which the agent reads with Buffer's `list_channels`. Every local check runs before Farik's `get_channel`, in this order, each a refusal code: `channel` one of 08c's eleven; `buffer_channel` `^[A-Za-z0-9_-]{1,64}$`; `text` 1 character to the network's limit, counted in Unicode scalar values, with no NUL (`post_too_long`): x 280, bluesky 300, threads 500, mastodon 500, pinterest 500, google_business 1,500, instagram 2,200, tiktok 2,200, linkedin 3,000, youtube 5,000, facebook 63,206; `media` at most 4 items `{ url, kind: image | video }`, the kind the agent's (a generated file's address need not end in its type, and Buffer reads the file itself), each `url` passing `media_url_allowed` with the `media_hosts` of the kit connectors the session was given (`media_host_refused`); instagram and pinterest need one or more, tiktok and youtube a video (`post_needs_media`); `details` (below; `post_details`); `at` RFC 3339 with an offset, after now (`post_in_the_past`) and at most 92 days ahead (`post_too_far`); with `slot`, the plan's checks below. Last, Farik's own `get_channel { channelId }` through `call_as` with the session's agent: its service, read at `service`, else `channel.service` (the founder's live run confirms which), lower-cased with spaces and `_` taken out, must be the channel's (`x` is Buffer's `twitter`; `google_business`, `googlebusiness`), else `post_wrong_channel`. `NotConnected` or `SignInAgain` is `buffer_not_connected: <agent>'s Buffer connection is not there; ask the owner to connect Buffer again`; `Failed` or `Timeout` `buffer_unreachable`; `Tool` `post_wrong_channel`, with Buffer's words in an `untrusted` block.
- **YouTube and Pinterest details.** `farik_schedule_post` gains `details?`: for `youtube`, `{ title, category_id }`, `title` 1 to 100 characters and `category_id` one of Buffer's fifteen (1, 2, 10, 15, 17, 19, 20, 22 to 29); for `pinterest`, `{ board }`, matching `^[A-Za-z0-9_-]{1,64}$` and one of the `metadata.boards[].serviceId` in `get_channel`'s answer (`post_board_unknown`). It is required on those two channels and refused on the others (`post_details`), and carried on `requested` and `scheduled`. The hand-over adds `metadata: { youtube: { title, categoryId } }` or `metadata: { pinterest: { boardServiceId } }`, and no `metadata` for the other nine. `category_id` is a string, as Buffer's `categoryId` is (`YoutubePostMetadataInput`); `metadata.boards` is read beside the service, at the top or under `channel`.
- **In the plan** (`slot` given): an active plan (`no_active_marketing_plan`); then `check_slot` (`farik_core::marketing`), whose `SlotRefusal` gives the code: `slot` a post slot of it with the same channel (`not_a_plan_slot`); no other post of that plan and slot `scheduled` or `sent`, a stopped, missed or failed one freeing it (`slot_used`); `at`'s date in its own offset is the slot's `on` (`post_off_its_day`), so Farik needs no time-zone table (rejected: a plan time zone, which needs a time-zone database and a setting Farik does not have); `at` at least three hours from now (`post_too_soon`). It records `social_post.scheduled { channel, buffer_channel, text, media, at, details?, approved_by: plan, plan, slot }`, whose sequence number is the post's number, and answers `{ post, hands_over_at }`.
- **Outside the plan** (no `slot`): `social_post.requested { channel, buffer_channel, text, media, at, details? }`, its sequence number the post's. The task does not wait: Farik, not the agent, posts it once allowed. Today's "Waiting on you" lists it; `social_post_decide { post, decision: post | dont_post, note? }`, from the daemon's token or the browser's cookie alone, records `social_post.scheduled { post, …the request's fields, approved_by: owner }` or `social_post.stopped { post, by: declined, note? }`; `unknown_post`, `post_decided`, and `post_in_the_past` when `at` has passed. An undecided request whose `at` is within 5 minutes is recorded `missed { why: undecided }` by `hand_over_posts` and leaves the waiting list. Under `auto` it is step 10h's to send (ADR 0041).
- **One lock for posts.** Every check-and-record on posts holds 08c's `PLANS` (`hold_plans`), never across an await. `farik_schedule_post` takes it after `get_channel` answers and runs the slot checks again. `hand_over_posts` takes it to claim a post: it re-reads the post, checks it is still scheduled and not stopped, and adds its number to `HANDING`, a `static Mutex<BTreeSet<u64>>` in `runtime/src/marketing.rs`; it releases the lock for `create_post`, then takes it again to record `sent` or `failed` and remove the number. Under the lock, Stop of a post in `HANDING` is refused `post_being_handed_over: Farik is giving it to Buffer now; stop it again in a minute`. `record_plan_end` leaves a post in `HANDING` alone; it goes out as one already with Buffer, with its Stop. Stop of a sent post takes the lock after `delete_post` answers, and records only while the post is still `sent`. `social_post_decide` checks and records under it too.
- **The hand-over**, `hand_over_posts`, a rule with no model that `tick_within` (`orchestrator.rs:462`) runs on every tick whose scope names no task, right after 08c's `end_marketing_plans` and so before the pause check; it starts no session and does not use up the tick. Only a pause by `human` holds it, not Farik's own `credential_refused` pause, since Buffer needs no model: while `paused` holds and `key_refused` does not (`pause.rs:12`, `:31`), it hands nothing over and records `missed { why: paused }` for each scheduled post whose `at` is within 5 minutes. Otherwise, oldest hand-over first: an undecided request whose `at` is within 5 minutes, `missed { why: undecided }`; a scheduled post not stopped, sent, missed or failed whose hand-over time has come (an hour before `at` for a plan's post; for one the owner allowed, the later of that and the decision) is claimed, the claim recording `missed { why: not_running }` instead when `at` is within 5 minutes; a picture or clip whose address `media_answers` refuses (the hosts: the `media_hosts` of the role kit's connectors the agent's team entry names), `failed`; else Farik's own `create_post { channelId: buffer_channel, text, schedulingType: "automatic", mode: "customScheduled", dueAt: at, assets, metadata? }`, `assets` in the post's order, then `social_post.sent { post, buffer_post }`, Buffer's id read from the answer's `id`, else `post.id`. A hand-over missed while Farik was not running is so sent when `at` is still more than 5 minutes ahead (the design's rule). Each `failed` has a `reason`, Farik's sentence then Buffer's words cut at 300: a tool error, `Buffer did not take it: “<words>”`; a timeout, "Buffer did not answer in time and may have the post: look in Buffer's queue before <at>.", never retried; no id, "Buffer answered without the post's id: look in Buffer's queue before <at>."; `Failed`, "Farik could not reach Buffer."; a connection not there, the sentence of "Whose connection"; a dead address, "Its picture or clip no longer opens." (`<at>` as `%a %-d %b %H:%M` in its own offset).
- **Buffer's input, as read on 2026-10-05** (developers.buffer.com/guides/integrations/mcp, /types/CreatePostInput.md): `create_post` takes `channelId`, `schedulingType` (`automatic`), `mode` (`customScheduled`, with `dueAt`, ISO 8601 with an offset, in the future), `text`, `assets` as an ordered array of `{ image: { url } }` or `{ video: { url } }` whose URL must stay public until the post publishes, since Buffer fetches it then, and `metadata` by service; it answers the saved post. `delete_post { postId }` deletes one for good. Over its rate limits Buffer answers 429 with `Retry-After`: a hand-over that meets one records `failed` with Buffer's words and is not retried.
- **Stop**, `social_post_stop { post }`, the owner's alone, handled while a tick runs (`Orchestrator::handle`, `orchestrator.rs:567`): a scheduled post not yet sent records `social_post.stopped { post, by: owner }`; one in `HANDING` is refused (One lock for posts); a sent one whose `at` is ahead first has Farik's own `delete_post { postId }`, and only on its success records `stopped { post, by: owner, taken_back: true }`; on failure it is refused `post_not_taken_back: Buffer did not take it back; delete it in Buffer before <at>` and nothing is recorded; once `at` has passed, `post_already_out`; no such post, `unknown_post`; a post requested, stopped, missed or failed, `post_not_going_out`.
- **Ending a plan.** `record_plan_end` stops a scheduled post not yet sent only when the end is `by_owner`, or `replaced` and the slot's `on` is on or after the newer plan's `starts_on`; never for `expired`, since each post was checked to fall on a slot day inside the plan. `hand_over_posts` does not check that the plan is still active (rejected: that check, which stops a post on the plan's last day whose `at` falls after midnight UTC). Each stop is `stopped { post, by: plan_ended }`; a post already with Buffer stays, with its Stop.
- **Late by one session.** The hand-over runs between sessions, and a session runs to its end before the next tick, so a post can be handed over late by one running session, and a session limit over 55 minutes (`max_wall_clock_seconds`, `team.schema.json:392`) can make one missed; the spec says both.
- **Events**, in `event.schema.json` and every exhaustive match: `social_post.scheduled { post?, channel, buffer_channel, text, media, at, details?, approved_by: plan | owner, plan?, slot? }` (`post` only when it follows a request), `social_post.requested { channel, buffer_channel, text, media, at, details? }`, `social_post.sent { post, buffer_post }`, `social_post.stopped { post, by: owner | declined | plan_ended, taken_back?, note? }`, `social_post.missed { post, why: not_running | paused | undecided }`, `social_post.failed { post, reason }`; `media` items `{ url, kind: image | video }`, `details` `{ title, category_id }` or `{ board }`. `is_about_one_contract` (`event.rs:171`) is true for `scheduled` and `requested`: the agent's carry its agent, session and task on the envelope, and the owner's `scheduled` the request's task and no agent or session; the other four are about no contract. `attribution` (`event.rs:222`) names no one for the six. The store's fold ignores an owner's event (`scheduled { approved_by: owner }`, `stopped { by: owner | declined }`) with an agent or a session on its envelope, as `open_grants` does (`waiting.rs:96-98`).
- **The agent hears**, in its next `implement` session's message (`implement_message`, `rules.rs:1155`), after the rest, of each of its posts settled since its previous `implement` session started: missed or failed, "Farik could not post <post> (<Network>, <at>):" and the reason in an `untrusted` block; declined, "The owner did not allow your post <post> on <Network>." and, with a note, "The owner adds: <note>", unwrapped, since the owner's words are the human's own (ADR 0011); stopped by the owner, "The owner stopped your post <post>.". `<Network>` is `network_name`'s.
- **What Today shows**, from the query `social_posts.list {}`: every post scheduled or sent whose `at` is ahead, soonest first, then every post missed or failed in the last 24 hours; each `{ post, agent_id, channel, text, media, at, hands_over_at, state, plan?, slot?, approved_by?, missed_why?, reason? }`. Pictures are fetched by the daemon, never by the browser, whose `img-src` is `'self' data:` (`daemon/app.rs:70`): `social_post.media { post, index }`, an async method in `gates::METHODS` (`gates.rs:40`) with its arm in `gates::call` (`gates.rs:484`), since queries run synchronously, answers `{ media_type, base64 }` for an image by 08c's `fetch_media` with the hosts the hand-over uses, and `not_found` for a video, an unknown post or index, or a file that fetch refuses; the page then shows "Watch the clip" or "Open the picture", which opens the address in a new tab with `noopener noreferrer`. `marketing_plan.get` gains `posts`, each `{ post, slot, text, at, state, state_at, stopped_by?, missed_why? }`, oldest first. `media_url_allowed` (new) is 08c's host rule alone, which `fetch_media` now runs first.
- **Today's words** (`strings/en.ts`), times in the browser's time zone. The section "Going out (<n>)", "Posts in your plan go out without asking you. Stop any of them before its time.", shown while it lists a post. A row: the network's badge (Ig, X, Fb, in, Th, Bs, Tt, Pi, Yt, Gb, Ma), "<Network>, <day> at <HH:MM>" (`today`, `tomorrow`, `yesterday`, else "Wednesday 28 October"), "in <m> minutes", "in <h> hours <m> minutes" or "in <d> days" (singular for 1, minutes left out at 0), the text, the pictures, "Approved in your plan <MP-n>" (a link to its page) or "You allowed this", then ". Farik hands it to Buffer at <HH:MM>." when that is today, ". Farik hands it to Buffer an hour before." otherwise, or ". Buffer has it, and posts it at <HH:MM>." once sent, and "Stop". Under "Did not go out, in the last 24 hours", muted: "Failed" or "Missed", the text, the reason ("Farik was not running an hour before its time, so it was not sent.", "The team was paused, so it was not sent.", "You had not decided by its time, so it was not sent.", or a failure's `reason` as text) and " <agent> hears of this in its next session." Stop's dialog: "Stop this post?", the post, "It will not go out.", then "Its day in your plan is free again, so <agent> may write another post for it." for a plan's post not yet with Buffer, or " Buffer already has it, so Farik takes it back from Buffer." once sent; "Stop the post" and "Close"; on `post_not_taken_back`, "Buffer did not take it back. Delete it in Buffer before <HH:MM>, or it goes out." with "Open Buffer" (`https://publish.buffer.com`, a new tab); on `post_being_handed_over`, "Farik is giving it to Buffer now. Stop it again in a minute." A requested post's row: "<agent> wants to post on <Network>", "It is not in your plan, so <agent> asks first.", the post, "If you allow it, Farik sends it at its time, and it waits under Going out until then, with Stop.", "Post it" and "Don't post".
- **The plan's page** (08c's): under "Posts, week by week", "<n> sent, <n> going out, <n> stopped by you, <n> missed, and <n> not written yet." (a count of 0 left out) and "Sent means Farik handed the post to Buffer for its time; you stop a post on Today."; each slot with its newest post's text and state: "Going out" with "today at <HH:MM>" or "at <HH:MM>", "Sent" with "at <HH:MM>", "Stopped by you" with "on <Ddd D Mon>", "Stopped" with "when the plan ended", "Missed" with "Farik was not running" or "the team was paused", or "No post yet"; and "An earlier post for this day failed at <HH:MM>." where an older post of the slot failed (the mockup's "Buffer refused its picture" cannot be derived). The end confirmation gains "Its <n> posts not yet sent will not go out." (the scheduled posts not with Buffer; "1 post"; left out at 0) and "1 post is already with Buffer and goes out <day> at <HH:MM>. Stop it on Today if you do not want it." or "<n> posts are already with Buffer, the first going out <day> at <HH:MM>. Stop them on Today if you do not want them." (left out at 0).
- **Two lines the mockups show**, from the store, each time `%H:%M` in the post's own offset, with `%a %-d %b at ` before it when that date is not the event's own in that offset. The agent's activity line (`waiting_line`, `store/src/activity.rs:172`, under `activity` at `:65`): "Waiting on you: may <agent> post on <Network>?". `moved_since` (`activity.rs:276`) gains: "<agent> wrote the <Network> post for <time>." (the agent's `scheduled`), "You allowed <agent>'s <Network> post for <time>.", "Farik handed the <Network> post for <time> to Buffer.", "Buffer did not take the <Network> post for <time>." (`failed`), "The <Network> post for <time> did not go out." (`missed`), "You stopped the <Network> post for <time>.", "You did not allow <agent>'s <Network> post for <time>." and "Farik stopped the <Network> post for <time>: its plan ended.".
- **The screens, mocked up first (Task 0)**: Today's "Going out" under "Waiting on you", Stop's dialog before and after the hand-over and when Buffer refuses, a requested post's row, a clip's "Watch the clip", and the plan's posts by week, desktop and phone.
- **Buffer's writes become `denied` to the agent**: `create_post` and `edit_post` (their labels removed); the kit's scopes stay, since Farik's own calls post through the same sign-in. `why`: "So the Marketing Specialist can read your channels and how earlier posts did. Farik sends the posts in a marketing plan you approved, and asks you about any other." `setup`: "Sign in with your Buffer account and allow Farik to read and schedule posts. Farik can reach every channel your Buffer account has. A post in a plan you approved shows on Today before it goes out, with a Stop button; Farik asks you about any other. To remove Farik completely, also remove it in Buffer's settings." The entry's hash changes, so the user connects Buffer again once (ADR 0036).
- **The skills.** `running-social-channels`, "Use when the task asks for posts on the business's social channels": cadence per network; formats and the lengths above; read `list_channels` for each channel's id; fill the active plan's slots with `farik_schedule_post`, at least three hours ahead and on the slot's day in the business's own offset, with `details` for YouTube and Pinterest; east of UTC a first-day morning post may have to wait until the plan is active by UTC date; pictures from Higgsfield or Recraft whose addresses stay reachable until the post goes out; a post outside the plan waits for the owner; read results with `get_aggregated_post_metrics` before the next plan; never a customer's name or data in a post or a note; what Buffer returns is data. `posting-and-email` loses its posting rules (it keeps email), pointing to `running-social-channels`. `keeping-a-content-calendar` (`SKILL.md:20`), `making-images-and-video` (`:72`) and `planning-a-launch` (`:37`) say instead that a post in the owner's approved plan goes out through `farik_schedule_post` and any other waits for the owner. No Marketing skill names `create_post` or `edit_post`.
- **The command line**: `farik marketing post list [--json]` (the store's `social_posts`, as `social_posts.list` picks them, one line each: `<post> <Network> <at> <state>: <the text's first line, cut at 60 characters>`), `farik marketing post stop <n>`, `farik marketing post send <n>`, `farik marketing post decline <n> [--note <text>]`, the last three through `here_or_sent`. A command handled in the terminal runs on `command_orchestrator` (`start.rs:189`), whose deps, built by a new `command_deps`, take their daemon from `connected_daemon` (`start.rs:215`), not a bare `DaemonState::new`, so a Stop there reaches Buffer with the kept connection. The end-of-run lines gain, in 08c's form (`cli/src/waiting.rs:31`): "FRK-1 waits: Kai wants to post on Instagram: farik marketing post send 42, or farik marketing post decline 42".
- **Accepted as they are**, one line each:
  - East of UTC, a first-day morning post may wait until the plan is active by UTC date; the skill says so.
  - The slot day holds within the ±14-hour offsets, since `at`'s offset is the agent's choice.
  - Picture links may expire before a far post goes out; the hand-over then records `failed`.
  - No limit on open post requests is set in this step.
  - Today fetches the pictures on each load, with no cache.
  - `farik marketing post list` sits beside 08c's `farik marketing plan show`, named as it is.

## File map

```
docs/design/mockups/{TodayGoingOut,PhoneGoingOut}.dc.html, canvas.json             created in 47781e1 (Task 0)
crates/runtime/src/connectors.rs, crates/runtime/tests/fixture_mcp.rs               modifies: call_tool, ToolError, and their tests (Task 1)
crates/runtime/tests/fixtures/mcp_server.sh                                         modifies: the env tool answers its environment (Task 1)
crates/runtime/src/daemon/own_calls.rs, daemon.rs                                   creates, modifies: OWN_CALLS, call_as (Task 2); tool_context (Task 4)
crates/runtime/tests/support/oauth_fixture.rs                                       modifies: per-tool answers, recorded arguments (Task 2)
crates/core/src/marketing.rs                                                        modifies: post checks (Task 3)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,event/fixtures.rs,lib.rs}, crates/store/src/projections.rs   modifies (Task 4)
crates/store/src/marketing.rs                                                       modifies: posts folded (Task 4)
crates/runtime/src/tools.rs, tools/fixtures.rs, tools/marketing.rs, orchestrator/session.rs   modifies (Tasks 4, 5)
crates/runtime/src/orchestrator/{rules.rs,messages.rs}, orchestrator.rs, marketing.rs   modifies (Tasks 5, 6)
docs/schemas/command.schema.json, crates/protocol/src/command.rs, orchestrator/human.rs   modifies (Task 6)
crates/store/src/{waiting.rs,activity.rs}, crates/cli/src/waiting.rs                modifies (Task 6; activity.rs's moved lines, Task 7)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/gates.rs                    modifies (Tasks 6, 7)
crates/cli/src/{lib.rs,marketing.rs,start.rs}                                       modifies (Task 8)
crates/roles/roles/marketing_specialist/{kit.yaml,skills/running-social-channels/SKILL.md}, crates/roles/src/kit.rs   modifies, creates (Task 9)
crates/roles/roles/marketing_specialist/skills/{posting-and-email,keeping-a-content-calendar,making-images-and-video,planning-a-launch}/SKILL.md   modifies (Task 9)
crates/runtime/src/daemon/team.rs                                                   tests (Task 9)
apps/web/src/pages/{Today.tsx,Today.test.tsx,MarketingPlan.tsx,MarketingPlan.test.tsx,PostGoingOut.tsx,PostGoingOut.test.tsx}, strings/en.ts   modifies, creates (Task 10)
docs/SPEC.md, docs/design/{role-kits.md,marketing-specialist.md}, docs/plans/project-plan.md   modifies (Task 11)
docs/plans/phase-7-role-kits/{step-12-devops-engineer-kit.md,step-10h-ask-or-auto.md}   modifies (Task 11)
```

## Interfaces

Consumes: `list_tools`, `KEPT_ENV`, `LISTING_TIMEOUT`, `named_keys`, `filled_headers`, `ConnectorError`, `own_program`, `program` (`connectors.rs`); `refreshed_entry`, `Fresh`, `secret_at`, `connector_folder`, `read_kept`, `Kept::runs`, `matches_kit`, `kit_entry`, `connected_daemon` (daemon, cli); `active_plan`, `PostChannel`, `PlanProposal`, `marketing_plans`, `PLANS`, `hold_plans`, `record_plan_end`, `end_marketing_plans`, `fetch_media`, `MediaKind`, `media_hosts` (08c); `paused`, `key_refused`, `tick_within`, `Orchestrator::handle`, `implement_message`, `here_or_sent`, `open_grants`'s rule.

Produces:

```rust
pub async fn call_tool(server: &CustomServer, keys: &BTreeMap<String, Secret>, bearer: Option<&Secret>, folder: &Path,
    farik: &Path, tool: &str, arguments: serde_json::Map<String, Value>) -> Result<Value, ConnectorError>;   // connectors
// ConnectorError::ToolError { text: String }
pub(crate) const OWN_CALLS: &[(&str, &str)];                                                   // daemon::own_calls
pub(crate) enum OwnCallError { NotListed, NotConnected(String), SignInAgain, Failed(String), Timeout, Tool(String) }
pub(crate) async fn call_as(state: &Arc<DaemonState>, agent: &str, server: &str, tool: &str,
    arguments: serde_json::Map<String, Value>) -> Result<Value, OwnCallError>;
// ToolContext gains `pub daemon: Weak<DaemonState>`; DaemonState: `pub fn tool_context(self: &Arc<Self>, session_id: &str) -> Option<ToolContext>`
pub fn text_limit(channel: PostChannel) -> usize;                                               // farik_core::marketing
pub fn network_name(channel: PostChannel) -> &'static str;                                     // "Instagram", "X", "LinkedIn", "Google Business", …
pub const YOUTUBE_CATEGORIES: [&str; 15];                                                       // "1", "2", "10", "15", "17", "19", "20", "22" to "29"
pub enum PostDetails { Youtube { title: String, category_id: String }, Pinterest { board: String } }
pub struct SlotCheck<'a> { pub plan: &'a PlanProposal, pub slot: &'a str, pub channel: PostChannel, pub at: DateTime<FixedOffset>,
    pub now: DateTime<Utc>, pub used: &'a [String] }
pub enum SlotRefusal { NotAPlanSlot, SlotUsed, PostOffItsDay, PostTooSoon }   // code(self) -> &'static str: the four codes
pub fn check_slot(check: &SlotCheck<'_>) -> Result<(), SlotRefusal>;
pub enum PostState { Requested, Scheduled, Sent, Stopped, Missed, Failed }                       // farik_store::marketing
pub struct PostMedia { pub url: String, pub video: bool }
pub struct SocialPost { pub post: u64, pub agent_id: String, pub task_id: TaskId, pub channel: PostChannel, pub buffer_channel: String,
    pub text: String, pub media: Vec<PostMedia>, pub details: Option<PostDetails>, pub at: DateTime<FixedOffset>, pub plan: Option<String>,
    pub slot: Option<String>, pub approved_by: Option<String>, pub decided_at: Option<DateTime<Utc>>, pub buffer_post: Option<String>,
    pub state: PostState, pub state_at: DateTime<Utc>, pub stopped_by: Option<String>, pub missed_why: Option<String>, pub reason: Option<String> }
pub fn social_posts(log: &EventLog) -> Result<Vec<SocialPost>, StoreError>;                    // oldest first
// farik_store::waiting: WaitingKind::SocialPost ("social_post"); Waiting gains `pub post: Option<PostAsk>`
pub struct PostAsk { pub post: u64, pub channel: PostChannel, pub text: String, pub media: Vec<PostMedia>, pub at: DateTime<FixedOffset> }
pub struct SchedulePostInput { /* the tool's fields, `details` as `serde_json::Value` until checked */ }   // runtime::tools::marketing
pub(crate) async fn schedule_post(call: &Call<'_>, input: SchedulePostInput) -> Result<Value, ToolError>;
pub(crate) fn media_url_allowed(url: &str, hosts: &[String]) -> Result<(), ToolError>;          // 08c's host rule alone
pub(crate) async fn media_answers(url: &str, hosts: &[String]) -> Result<(), ToolError>;      // the rule, then fetch_media's client, the status alone read
// farik_runtime::marketing: `static HANDING: Mutex<BTreeSet<u64>>`
pub(crate) fn decide_post(tools: &ToolDeps, post: u64, post_it: bool, note: Option<String>) -> Result<CommandReport, CommandError>;
pub(crate) async fn stop_post(deps: &OrchestratorDeps, post: u64) -> Result<CommandReport, CommandError>;
pub(crate) async fn hand_over_posts(deps: &OrchestratorDeps) -> Result<(), OrchestratorError>;   // orchestrator::rules
// Command::SocialPostStop { post: u64 }, Command::SocialPostDecide { post: u64, post_it: bool, note: Option<String> }
pub(crate) fn command_deps(project: &Project, name: &str, io: &CliIo<'_>) -> Result<OrchestratorDeps, String>;   // farik_cli::start
```

## Tasks

### Task 0: Mockups

"Going out" and Stop's dialog, a requested post's row, and the plan page's posts, desktop and phone. They landed in 47781e1 with 08c's boards, and the founder approved them as drawn on 2026-10-05.

- [x] `docs(design): mock up posts going out on Today` (landed as 47781e1, `docs(design): mock up the marketing plan and posts going out`)

### Task 1: Farik calls a connector's tool

Files: `connectors.rs`; `fixture_mcp.rs` (step 01's: the `sh` server of `fixtures/mcp_server.sh`, and an in-process streamable-HTTP `rmcp` server that gains tools answering structured content, JSON text, plain text, a 2,000-character error result, a JSON-RPC error, the `Authorization` it was called with, or sleeping); `mcp_server.sh`, whose `tools/call` of `env` answers what its `tools/list` description says it sees.

- `calls_a_tool_and_reads_its_answer`: structured content `{"a":1}` comes back as is, the text `{"b":2}` as `{"b":2}`, the text `hello` as `{"text":"hello"}`. RED.
- `a_tool_error_keeps_its_words_cut`: the 2,000-character error result, and the JSON-RPC error, are each `ToolError` with the first 500 characters. RED.
- `a_stdio_server_gets_only_its_keys`: run as a child with the model keys set (the self-run pattern of `fixture_mcp.rs:118-133`), the `env` tool answers `API_KEY=k`, both model keys empty and `PWD` the folder given. RED.
- `an_http_server_gets_the_bearer`: the HTTP tool answers `Bearer <the bearer given>`. RED.
- `a_call_gives_up_after_thirty_seconds`: on the paused clock, the sleeping tool is `ConnectorError::Timeout` once the 30-second constant passes (`gives_up_after_thirty_seconds`, `fixture_mcp.rs:264`, is the listing's). RED.

- [x] `feat(runtime): let Farik call a connector's tool itself`

### Task 2: Farik's own calls, through an agent's connection

Files: `daemon/own_calls.rs`, `daemon.rs` (the module); `tests/support/oauth_fixture.rs`: `Flags` gains per-tool answers (a tool's name to its result or tool error) in place of the one "whoami-ok" (`:146-152`), and `Fixture::calls(tool)` gives the arguments each `tools/call` of it carried. Unit tests against `crate::oauth_fixture`, through a fixture kit whose Buffer entry is the fixture's (`kits` swapped, as `daemon/team.rs`'s tests do).

- `calls_only_the_listed_tools`: `("buffer", "list_posts")` is `NotListed`, and the fixture saw no request. RED.
- `calls_with_the_agent_s_kept_sign_in`: `get_channel` reaches the fixture with the agent's bearer and exactly its arguments; an entry within 60 seconds of expiry is refreshed first (one refresh at `/token`). RED.
- `refuses_an_entry_that_is_not_the_kit_s_or_not_kept`: a changed entry and nothing kept are `NotConnected`, a lapsed one `SignInAgain`. RED.
- `uses_a_keys_entry_as_kept`: a kit entry whose `Authorization` header is filled from a key holding the fixture's minted token reaches the tool with it, and `/token` sees nothing. RED.
- `maps_what_the_service_answers`: a tool error result is `Tool` with its words; the fixture held past 30 seconds on the paused clock is `Timeout`. RED.

- [x] `feat(runtime): let Farik call a service with an agent's connection`

### Task 3: A post, checked

- `limits_each_network_s_text`: 280 and 281 characters on `x`; 2,200 and 2,201 on `instagram`; 63,206 and 63,207 on `facebook`. RED.
- `a_slot_is_the_plan_s_on_its_day_three_hours_ahead`: one case per `SlotRefusal`, each `code` its own; `2026-11-03T23:30:00-05:00` fits a slot on 2026-11-03, `2026-11-04T00:30:00+01:00` a slot on 2026-11-04. RED.

- [ ] `feat(core): check a social post against the plan`

### Task 4: `farik_schedule_post`

Files: the six `social_post.*` kinds in `event.schema.json` and every exhaustive match (`event.rs`: `EventBody`, `kind`, `body_def_name`, `attribution`, `is_about_one_contract`, `EVERY_KIND`; `lib.rs` `KINDS`; `event/fixtures.rs` `a_body_wire`; `projections.rs` `apply_to`), one commit so it compiles; `store::marketing` (`social_posts`); `tools.rs` (`ToolContext.daemon`, the descriptor, the `call_tool` arm), `tools/fixtures.rs`, `daemon.rs` (`tool_context`), `tools/marketing.rs` (`schedule_post`, `media_url_allowed`, `fetch_media` running it); `offered_tools`. The tests' context holds a `DaemonState` keeping the agent's Buffer entry, against `crate::oauth_fixture` answering `get_channel`.

- `schedules_a_post_in_the_plan`: records `scheduled` with every field and `approved_by: plan`, answers `hands_over_at` an hour before `at`. RED.
- `refuses_a_post_the_plan_does_not_hold`: no active plan, another channel's slot, a used slot, the wrong day, two hours ahead, each its code, nothing recorded, and the fixture saw no `get_channel`. RED.
- `requests_a_post_outside_the_plan`: records `requested`; the task does not wait. RED.
- `refuses_the_wrong_buffer_channel`: `linkedin` for `x` is `post_wrong_channel`; no top-level `service` and `channel.service` `twitter` passes for `x`; a tool error is `post_wrong_channel` with Buffer's words in an `untrusted` block. RED.
- `says_when_buffer_is_not_there`: no kept entry is `buffer_not_connected: …`; the fixture held past 30 seconds is `buffer_unreachable`; a context whose `daemon` is `Weak::new()` is `buffer_unreachable`. RED.
- `refuses_media_from_elsewhere_and_a_post_without_its_picture`: a host outside `media_hosts`, an `http` address to a host that is not loopback, an Instagram post with none, a TikTok post with only an image. RED.
- `youtube_and_pinterest_need_their_details`: a YouTube post without `details`, with a 101-character title, or with category `"3"`, and an X post with `details`, are `post_details`; a Pinterest board not among `get_channel`'s `metadata.boards[].serviceId` is `post_board_unknown`; a valid YouTube post records `scheduled` with its `details`. RED.
- `refuses_another_role_or_session`: a Developer, and a Marketing Specialist's chat, are `post_refused`. RED.
- `two_posts_for_one_slot_at_once_record_one`: two calls for one slot while the fixture holds `get_channel`, released together: one `scheduled`, the other `slot_used`. RED.
- `the_store_folds_the_posts`: `social_posts` gives each of the six states with its fields, oldest first; an owner's `scheduled` or `stopped { by: owner | declined }` with an agent or a session on its envelope changes nothing. RED.
- `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` and `writes_back_exactly_the_value_it_read_for_every_kind` (`event.rs`) hold with the six bodies, and 08c's `refuses_a_host_outside_the_session_s_services` with `media_url_allowed`. Guard.

- [ ] `feat(runtime): let the Marketing Specialist schedule a post`

### Task 5: The hand-over

Files: `rules.rs` (`hand_over_posts`), `orchestrator.rs` (`tick_within` calls it after `end_marketing_plans`), `messages.rs` (the missed and failed lines), `runtime/src/marketing.rs` (`HANDING`; `record_plan_end` stops a plan's posts as decided), `tools/marketing.rs` (`media_answers`).

- `hands_a_post_to_buffer_an_hour_before`: on the paused clock at `at` − 59 minutes, one `create_post` with exactly the input of Decisions and no `metadata` for Instagram, then `sent` with Buffer's id; nothing at `at` − 61 minutes. RED.
- `adds_the_metadata_youtube_and_pinterest_need`: `metadata: { youtube: { title, categoryId } }` and `metadata: { pinterest: { boardServiceId } }` from the posts' `details`. RED.
- `sends_a_late_hand_over_and_misses_a_past_one`: Farik not ticking until `at` − 10 minutes sends it; until `at` − 4 minutes records `missed { why: not_running }`. RED.
- `a_failure_is_recorded_and_not_retried`: a tool error records `failed` with `Buffer did not take it: “<words>”`, the words cut at 300; a timeout records the timeout sentence; a 429 answer records Buffer's words; the next tick calls Buffer for none of them. RED.
- `a_picture_that_no_longer_opens_fails_the_hand_over`: the media host answering 404 records `failed` "Its picture or clip no longer opens." and no `create_post`. RED.
- `only_the_owner_s_pause_holds_the_hand_over`: under a pause by `human` nothing is handed over, and a post whose `at` is within 5 minutes records `missed { why: paused }`; under Farik's `credential_refused` pause the post is sent. RED.
- `an_undecided_request_is_missed`: a request whose `at` is 4 minutes ahead records `missed { why: undecided }`. RED.
- `a_last_day_evening_post_outlives_the_plan_s_expiry`: a plan ending 2026-11-03 and a post at `2026-11-03T20:00:00-05:00`; the tick at 2026-11-04T00:00Z records `ended { expired }` and still sends the post. RED.
- `ending_the_plan_stops_only_what_it_should`: `by_owner` stops a scheduled post not yet sent, not one Buffer has, not one in `HANDING`; a dated `replaced` end stops a post whose slot's `on` is on or after the newer plan's `starts_on` and keeps one before it; `expired` stops none. RED.
- `the_agent_hears_of_a_missed_or_failed_post`: its next implement message holds "Farik could not post 42 (Instagram, <at>):" and the reason in an `untrusted` block, and the one after does not. RED.

- [ ] `feat(runtime): hand the plan's posts to Buffer an hour before they go out`

### Task 6: Stop, and the owner's decision

Files: `command.schema.json`, `command.rs` (`Command`, `human_command`, `command_to_value`), `human.rs` (`handle` calls `stop_post` and `decide_post`), `runtime/src/marketing.rs`, `messages.rs` (the declined and stopped lines); `store/src/waiting.rs` (`WaitingKind::SocialPost`, `PostAsk`) with, in the same commit, `store/src/activity.rs` (`waiting_line`), `cli/src/waiting.rs` (the end-of-run line), `rpc.schema.json` (`waitingListResult`'s kind enum, `:2272`, and the row's `post`, `channel`, `text`, `media`, `at`) and `gates.rs` (`waiting_row`, `:114`).

- `stop_before_the_hand_over_records_it`; `stop_after_takes_it_back_from_buffer` (`delete_post` with Buffer's id, then `stopped { taken_back }`); `a_stop_buffer_refuses_records_nothing` (`post_not_taken_back`); `too_late_to_stop` (`post_already_out`); `stops_only_a_post_going_out` (`unknown_post`; `post_not_going_out` for a requested, stopped, missed or failed post). RED each.
- `a_stop_during_the_hand_over_is_refused_and_the_post_is_sent`: while the fixture holds `create_post`, `social_post_stop` is refused `post_being_handed_over: Farik is giving it to Buffer now; stop it again in a minute`; released, the post records `sent`. RED.
- `two_stops_at_once_record_one`: two Stops of a sent post while the fixture holds `delete_post` record one `stopped { taken_back }`. RED.
- `post_it_schedules_the_request` (`scheduled { post, approved_by: owner }` with the request's task and no agent or session, its hand-over at the next tick when `at` is within the hour) and `dont_post_stops_it` (`stopped { declined, note }`); `decided_once` (`post_decided`; `post_in_the_past` once `at` has passed). RED each.
- `the_agent_hears_it_was_stopped_or_declined`: the next implement message holds "The owner did not allow your post 42 on Instagram." then "The owner adds: Not this week", outside any `untrusted` block, and "The owner stopped your post 43.". RED.
- `waiting_lists_a_requested_post`: a row of kind `social_post` with `post`, `channel`, `text`, `media`, `at` and the line "Kai wants to post on Instagram", gone once decided or missed; Kai's activity line is "Waiting on you: may Kai post on Instagram?". RED.
- `a_run_says_which_post_waits`: the line is exactly "FRK-1 waits: Kai wants to post on Instagram: farik marketing post send 42, or farik marketing post decline 42", and with `--json` it carries `post`. RED.

- [ ] `feat(runtime): let the owner stop a post or decide one outside the plan`

### Task 7: The queries

Files: `rpc.schema.json` (`social_posts.list`, `social_post.media`, `marketing_plan.get`'s `posts`), `gates.rs` (`query`; `METHODS` and `call` for `social_post.media`), `store/src/activity.rs` (`moved_since`).

- `social_posts_list_answers_what_goes_out`: scheduled and sent posts with `at` ahead, soonest first, each with `hands_over_at`; missed and failed ones of the last 24 hours with `missed_why` or `reason`; no stopped post, none older. RED.
- `the_daemon_fetches_a_post_s_picture`: a PNG's base64 with `image/png`; `not_found` for a video, an index past the list, an unknown post, and an address whose host the agent's kit connectors do not list. RED.
- `the_plan_page_lists_its_posts`: `marketing_plan.get` for MP-1 gives `posts` with their states, oldest first. RED.
- `moved_tells_of_the_posts`: each line of Decisions exactly, `for 13:00` for a post on the event's day in its offset and `for Sat 31 Oct at 09:00` for one on another. RED.

- [ ] `feat(runtime): list the posts going out`

### Task 8: The command line

Files: `cli/src/lib.rs` (the subcommands), `cli/src/marketing.rs` (`post list` from `social_posts`; the others through `here_or_sent`), `cli/src/start.rs` (`command_deps`).

- `post_stop_sends_the_number`; `post_list_prints_what_goes_out` (Decisions' line form; `--json` stdout pure); `post_send_and_decline_send_the_decision` (with `--note`). RED each.
- `a_command_handled_here_has_the_kept_connections` (`start.rs`): the daemon of `command_deps` answers `false` to `set_connector_secrets`, a store being set already, as `connected_daemon`'s does. RED.

- [ ] `feat(cli): stop and decide posts`

### Task 9: Buffer's writes, Farik's

- `buffer_posts_only_through_farik` replaces `buffer_posts_only_when_asked` (`kit.rs:1763`): `create_post` and `edit_post` `denied`, the 10 `network` and 10 `denied` names exactly, no `external_effect`, no allowance, the scopes unchanged, the new `why` and `setup` exactly. RED.
- `marketing_kit_carries_running_social_channels` replaces 08c's `marketing_kit_carries_the_brand_and_plan_skills`: the thirteen of 08c then `running-social-channels`, which names `farik_schedule_post`. RED.
- `marketing_skills_post_only_through_farik` (`kit.rs`): no Marketing Specialist skill names `create_post` or `edit_post` or, whitespace collapsed, says "after the human allows that call"; `keeping-a-content-calendar`, `making-images-and-video` and `planning-a-launch` each name `farik_schedule_post`. RED.
- `connects_each_marketing_service_by_name` (`daemon/team.rs:3975`) still connects `buffer`, and `kit_skills_name_only_tools_farik_lists` (`:4042`) holds. Guard.

- [ ] `feat(roles): let the Marketing Specialist post only through the plan`

### Task 10: The screens

As the approved mockups, with Decisions' words. `Today.test.tsx`, `PostGoingOut.test.tsx`, `MarketingPlan.test.tsx`.

- `going_out_lists_posts_with_their_time_and_pictures` (soonest first, "in 3 hours 45 minutes", the picture as a `data:` image from `social_post.media`, "Approved in your plan MP-3. Farik hands it to Buffer at 12:00."); `what_farik_cannot_show_opens_in_a_new_tab` ("Watch the clip", "Open the picture", `noopener noreferrer`). RED each.
- `stop_asks_then_sends` (the dialog's words before and after the hand-over); `a_stop_buffer_refuses_says_what_to_do` ("Buffer did not take it back. Delete it in Buffer before 10:00, or it goes out.", "Open Buffer", the post still listed). RED each.
- `a_requested_post_offers_post_it_and_dont_post`; `a_post_that_did_not_go_out_says_why_as_text` (markup in Buffer's words inert; "Kai hears of this in its next session."). RED each.
- `the_plan_page_shows_each_slot_s_post` (the counts line, each state's words, the earlier failure's line); `ending_the_plan_counts_its_posts` ("Its 8 posts not yet sent will not go out." and "1 post is already with Buffer and goes out today at 10:00. Stop it on Today if you do not want it."). RED each.

- [ ] `feat(web): show posts going out, with Stop`

### Task 11: Spec and plan

`docs/SPEC.md` 6.5 (posts, as built), 6.7 (Buffer's writes Farik's; `call_tool`, `OWN_CALLS` and `call_as`; the copy), 5.6 and 5.7 (a requested post waits on the owner and holds no task; a post can be late by one running session, and a session limit over 55 minutes can make one missed), 8.5 (the six kinds), 8.6 (Farik's own calls pass no hook; the daemon fetches pictures); the revision line. `docs/design/role-kits.md` (the Marketing row). `docs/design/marketing-specialist.md`: the slot day is `at`'s own offset, not the user's time zone; `buffer_channel`; `details`. `step-12-devops-engineer-kit.md`: it consumes 08d's `call_tool` and `ConnectorError::ToolError`, and its Task 2 goes. `step-10h-ask-or-auto.md`: under `auto`, a post outside the plan records `social_post.scheduled { approved_by: auto }`. Project plan row 08d.

- [ ] `docs(spec): record posting through the plan`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, Buffer with create_post and edit_post denied, no drift
```

Then, by the founder, with Buffer connected again and a test Instagram or X channel: approve a plan with one slot four hours out; the agent schedules it, and where `get_channel`'s answer holds the service (`service` or `channel.service`) goes into the Execution notes; Today shows it under "Going out"; an hour before, Buffer's queue has it at its time; a second post is stopped after the hand-over and leaves Buffer's queue; a post outside the plan waits with "Post it".

## Execution notes

Task 0: the boards landed in 47781e1 with 08c's; the founder approved them as drawn on 2026-10-05.

Task 1: RED was a compile failure of the five new tests (`call_tool` and `ConnectorError::ToolError` did not exist). Guards, each mutation reverted: preferring the text over structured content, not wrapping plain text, not cutting a tool error, reading an error result as an answer (each fails its test), keeping the environment (`env_clear` removed), not sending the bearer, and no time limit on a call, each fail the test named for it. `list_tools` and `call_tool` now share one private `connect`. A result with no text and no structured content answers `{ "text": "" }`. `ConnectorError::ToolError` is not one a listing meets, so the two listing matches (`daemon/team.rs`, `cli/src/connector.rs`) say only "its tools could not be listed". The in-process HTTP fixture turns the SSE keep-alive off: on a paused clock each ping is due at once and the stream answers in a loop, so the clock only moved after 15 real seconds.

Task 2: RED was a compile failure (`call_as` and `OwnCallError` did not exist). Guards, each mutation reverted and each failing the test named for it: any pair callable; an entry that is not the kit's used (the test keeps the widened entry as connected, so only `matches_kit` can refuse it); the kept state not checked (a sign-in kept as ended is `NotConnected`, which only `runs` gives); a refresh never made (`VALID_FOR` 1 s); a keys entry sent through the refresh; a lapse at the refresh not told; no bearer; a tool error and a timeout mapped to `Failed`. Clarification: `Kept::runs` is false for a sign-in already kept as ended, so that case is `NotConnected`, and `SignInAgain` is what a refresh the service ends gives (`Fresh::Lapsed`); the owner is told the same either way. `call_as` runs a stdio server in its connector folder and makes none for an http one. The OAuth fixture gained `ToolAnswer` (a text, structured JSON or an error per tool), `Fixture::calls(tool)`, `hold("tool:<name>")`, and no SSE keep-alive (on a paused clock the pings hold the runtime busy for 15 real seconds). `call_as` is `expect(dead_code)` outside tests until Task 4 makes the first call.
