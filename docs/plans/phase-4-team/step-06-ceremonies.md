# Phase 4, step 06: Ceremonies and escalation hygiene

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 5.7, 5.8, 5.9, 6.2, 8.5; F7
Depends on: phase 3 (merged in #11); steps 01 to 05 of this phase (committed on this branch before this step starts)
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the rule order the first found undecided; its findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The team stops and looks up. The sprint planning session becomes the planning ceremony. The Scrum Master (the Product Manager without one) posts the plan and a digest of every open escalation in the channel's planning thread, informed by what the last retro learned. A standup is posted once a day while a sprint moves. When a sprint ends, a review says what it delivered, and a retro says what to keep and change, appended to `team/retro.md` for the next planning. An escalation the human leaves waiting past a set age is recorded and said in the channel. Each ceremony is one session, the founder's choice (2026-09-24). Out of scope: notifications on the desktop (phase 6), the agent's own memory and decisions (step 07).

## Decisions

- A ceremony is one session of the ceremony runner: the active Scrum Master, else the active Product Manager. With neither active, no ceremony runs. While the runner sleeps (step 04), a ceremony waits and feeds `Idle.until`; it never falls back to the other role.
- The session:
  - Its purpose is `ceremony`. It runs on `claude-sonnet-5` at `medium` whatever the runner's model, overriding it as triage does (revision 11: channel sessions use Sonnet 5; SPEC 5.9's ceremonies run in the channel). It is about no task, `cwd` the files' root, and gets the read tier's built-ins alone. No ceremony starts on a spent day (step 03's day-only check).
  - Its thread is one of `planning`, `standup`, `review`, `retro`, registered with the session: `SessionRegistration` and `ToolContext` gain `thread: Option<Thread>`, and `session.started` gains an optional `thread` (a shared `$defs/thread`, the type step 05's `message.posted` uses), so the log says which ceremony a session was.
  - Its Farik tools are given by `SessionAsk::tools` (step 05): the reading tools and `farik_post_message`; planning adds `farik_plan_sprint`, retro adds `farik_append_retro`.
  - Its first message carries the ceremony's facts and the channel summary, each in an `untrusted` block cut at 16 KiB.
  - Its `This session` text is `CEREMONY_INSTRUCTIONS[thread]`, passed as `PromptInput::closing`. Each tells it to mention no one: the channel is read by all.
  - The tick reports it as `TickReport::Sprint { sprint_id, what }` (step 03).
- `farik_post_message` in a ceremony session posts `kind: ceremony` with the session's thread, at most three per session, a fourth refused `channel_limit`. This is the ceremony branch step 05 left. `pending_mentions` (step 05) also leaves out `ceremony` messages, so a ceremony's words start no conversation.
- Every ceremony rule and the aged rule run only when the tick has no `task_id` scope, as step 03's and step 05's rules do.
- Order in the tick: the ceremony rules sit with step 03's sprint rules, before rule 1, in this order:
  1. sprint end (step 03)
  2. review
  3. retro
  4. planning
  5. standup

  Each is bounded (below), so none starves work, and work cannot starve them. The aged rule sits after step 05's channel rule.
- A ceremony has run when a `session.started` with its thread follows its anchor, and ended `completed`, `aborted`, or `error`; or when three such sessions have, whatever their ends. This is step 04's planning bound, now matched on the thread instead of purpose `plan`, and applied to every ceremony; for planning, a task-less `plan` session with no thread also counts, so a sprint planned by a log from before this step is not planned again. The anchor is:
  - for planning: the sprint's `sprint.started`
  - for review and retro: the sprint's `sprint.ended`
  - for a standup: the start of the UTC day
- Planning is step 03's sprint planning session, now a ceremony (`thread: planning`), under the rules above and `TickRules::All` and `TickRules::Planning`.
  - Step 03's `SPRINT_PLAN_INSTRUCTION` and its choice by `only_tool` are removed in favour of `CEREMONY_INSTRUCTIONS[planning]`. `sprint_plan_message` becomes `planning_message(sprint, candidates, budget_left, digest, retro)`.
  - The digest's facts are:
    - each open escalation's task, title, reason, detail, and hours waiting;
    - each `budget.exhausted` of `DayUsd` or `SprintUsd` since the previous planning (the founder: a spent budget is listed in the digest).
  - Its first message also holds the last 16 KiB of `team/retro.md`.
  - Its instruction: post the plan and the escalation digest, then plan the sprint with `farik_plan_sprint`.
- An open escalation is a task in `escalated`, with the reason and detail of its last `escalation.raised`, waiting since then. An accepted task is one too while its last `escalation.raised` with reason `integration` since its move into `accepted` has no `task.integrated` after it, as `since_accepted` reads it.
- Standup, under `TickRules::All` only: while a sprint is open, once per UTC day, when a `task.transitioned` of a task in the sprint was recorded in its window: from the start of the UTC day of the last standup that has run under the bound above (else the sprint's start) up to the start of the current UTC day. It reports the days before today, a standup that hit its limit is retried, and no move between midnight and a standup's own time is dropped.
  - Its first message holds each such move (task, from, to, who), each blocked task of the sprint with its blocker, and the open escalations.
  - Its instruction: post one standup summary.
- Review and retro run under `TickRules::All` only, for the latest ended sprint while no newer sprint has started. Their tasks are the sprint file's `task_ids`, not the projections' `sprint`, which is cleared for tasks that left. Their events are those about those tasks with a seq between the sprint's `sprint.started` and `sprint.ended`.
  - Review comes first. Its first message holds each task with its status, the first line of its completion note, and its cost in this sprint (`cost_records.sprint`), then the sprint's budget and spent. Its instruction: post what the sprint delivered and what it did not.
  - Retro follows once review has run. Its first message holds the sprint's rejections (task, failed criteria), escalations (task, reason), blocks (task, blocker), iterations, and the last 16 KiB of `team/retro.md`. Its instruction: post the retro, then record what the next planning should know with `farik_append_retro`.
  - A sprint followed at once by another gets no review or retro; the log keeps what happened.
- `farik_append_retro { text }`, tier `read`, is accepted only in a `retro` ceremony session. Anywhere else it is refused with `retro_refused: only the retro ceremony writes team/retro.md`.
  - The text is non-empty and at most 4,000 characters.
  - It writes under the latest ended sprint: it appends to `.farik/team/retro.md` a section `## <sprint id> (<date, UTC>)` and the text. When the file is missing it is created with a `# Retro` title.
  - It records `retro.appended { sprint_id, text, appended_by }`, attributed to `appended_by`, not about one contract.
  - One append per session; a second is refused as `retro_refused: <why>`.
  - `ProjectFiles::{read_retro, append_retro}` hold the file.
- `escalation.aged`: `policy.escalation_age_hours` is in the team schema, default 24, 1 to 720.
  - The aged rule, under `TickRules::All` only, takes one open escalation per tick: the oldest that is past the limit and not yet aged.
  - It records `escalation.aged { raised_seq, hours }`, with `hours` rounded down. The event is about the escalation's task and has no attribution.
  - It posts a system line `<id> has waited <hours> hours on the human: <reason>`. The line is for the human to read and carries no mention (step 05). The project plan's "mentioning the human" is corrected to this.
  - The tick reports it as `Acted { task_id, what }`.
  - Aging happens only while something ticks: `farik run` ends when the board is idle, which the spec says.
  - Desktop notifications of it are phase 6's.
- The project plan's step 07 line "`team/retro.md` in planning sessions" is done here; step 07's line is corrected when its plan is written.

## File map

```
docs/schemas/event.schema.json, docs/schemas/team.schema.json   modifies: session.started's thread, retro.appended, escalation.aged; policy.escalation_age_hours
crates/core/src/team.rs                          modifies: escalation_age_hours with its default
crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies
crates/store/src/files.rs                        modifies: read_retro, append_retro; tests
crates/runtime/src/daemon.rs, crates/runtime/src/tools.rs, crates/runtime/src/sessions.rs   modifies: thread in the registration, the tool context, and session.started
crates/runtime/src/tools/channel.rs, crates/runtime/src/tools/retro.rs, crates/runtime/src/tools/refusal.rs, crates/runtime/src/daemon/mcp.rs   modifies / creates: the ceremony branch; farik_append_retro
crates/runtime/src/prompt.rs                     modifies: CEREMONY_INSTRUCTIONS
crates/runtime/src/ceremonies.rs, crates/runtime/src/lib.rs   creates / modifies: open_escalations, the facts each ceremony is given
crates/runtime/src/orchestrator/{rules,messages,session}.rs   modifies: the ceremony rules, their first messages, the aged rule
crates/runtime/src/recorded/transcripts/planning_ceremony_frk_1.jsonl, standup.jsonl, review.jsonl, retro.jsonl   creates
docs/SPEC.md, docs/plans/project-plan.md         modifies
```

## Interfaces

Consumes: step 03's sprint rules, `plan_sprint`, `TickReport::Sprint`; step 04's `asleep` and day-only check; step 05's `farik_post_message`, `post`, `SessionAsk::tools`, `channel_summary`, `Thread`, `MessageKind::Ceremony`.

Produces:

```rust
// farik-protocol: SessionStartedBody gains thread: Option<Thread>; EventBody::{RetroAppended(RetroAppendedBody { sprint_id, text, appended_by }), EscalationAged(EscalationAgedBody { raised_seq: u64, hours: u32 })}
// farik-core::team: TeamPolicy::escalation_age_hours: u32 (default 24)
// farik-store
impl ProjectFiles { pub fn read_retro(&self) -> Result<Option<String>, FilesError>; pub fn append_retro(&self, sprint_id: &str, date: NaiveDate, text: &str) -> Result<(), FilesError>; }
// farik-runtime::ceremonies
pub struct OpenEscalation { pub task_id: TaskId, pub title: String, pub reason: String, pub detail: String, pub raised_seq: u64, pub raised_at: DateTime<Utc> }
pub fn open_escalations(log: &EventLog, projections: &Projections) -> Result<Vec<OpenEscalation>, StoreError>;
// farik-runtime::prompt
pub const CEREMONY_INSTRUCTIONS: [(Thread, &str); 4];
// farik-runtime::orchestrator: planning_message(sprint, candidates, budget_left, digest, retro) replaces sprint_plan_message
```

## Tasks

### Task 1: the ceremony session and its posts

Files: event schema (`session.started`'s thread), protocol, `daemon.rs`, `tools.rs`, `sessions.rs`, `tools/channel.rs`, `prompt.rs`, `orchestrator/session.rs`
- `records_a_ceremonys_thread_on_its_start` — a session registered with `thread: Some(Standup)`: its `session.started` has `thread: standup`.
- `posts_a_ceremony_message_in_its_thread` — `farik_post_message` three times in a `standup` ceremony session: three `message.posted { kind: ceremony, thread: standup }`, none refused.
- `caps_a_ceremonys_posts` — a fourth post in a ceremony session: refused `channel_limit`.
- `starts_no_conversation_for_a_ceremonys_mention` (guard) — a ceremony post naming `@dev-a`: no conversation session for dev-a.
- `runs_a_ceremony_on_sonnet` — a ceremony run by the Product Manager (no Scrum Master): its spec's model is `claude-sonnet-5` at `medium`.
- `closes_each_ceremony_with_its_own_instruction` — for each thread, the prompt's `This session` text is that thread's `CEREMONY_INSTRUCTIONS` entry.

- [x] `feat(runtime): run a ceremony as one session that posts in its thread`

### Task 2: planning as a ceremony, with the digest

Files: `ceremonies.rs`, `orchestrator/rules.rs`, `orchestrator/messages.rs`, `prompt.rs` (`SPRINT_PLAN_INSTRUCTION` removed), the planning transcript, the step 03 and 04 tests that name purpose `plan` for the planning session (`plans_a_sprint_again_after_its_planner_slept`, `plans_a_sprint_at_most_three_times`, `plans_a_sprint_once`)
- `lists_the_open_escalations` — FRK-1 `escalated` for `iterations` 30 hours ago, FRK-2 accepted with an unresolved `integration` escalation, FRK-3 escalated then resolved: `open_escalations` gives FRK-1 and FRK-2 with their reasons.
- `lists_the_spent_budgets_in_the_digest` — a `budget.exhausted` of `SprintUsd` since the last planning: the planning message names it.
- `plans_the_sprint_in_a_ceremony` — S1 open and empty: the Scrum Master's session has purpose `ceremony`, thread `planning`, tools including `farik_plan_sprint` and `farik_post_message`. Its first message holds the candidates, the open escalations, and the retro's text. After the replay, the channel holds its ceremony posts and S1 holds the planned task.
- `plans_once_per_sprint_as_a_ceremony` (changed from step 03's) — a completed planning ceremony: no second one.

- [x] `feat(runtime): plan a sprint in a ceremony with the escalation digest`

### Task 3: standup

Files: `orchestrator/rules.rs`, `orchestrator/messages.rs`, `ceremonies.rs`, the standup transcript
- `holds_a_standup_when_the_sprint_moved` — S1 open, FRK-1 moved `assigned → in_progress` yesterday (UTC): a `standup` ceremony whose first message names that move; after it, a second tick the same day holds none; a move made today waits for tomorrow's.
- `holds_the_standup_before_work` — S1 open, yesterday's moves, FRK-2 `in_progress` with work to do: the tick's session is the standup, and the next tick's is FRK-2's implement session.
- `holds_no_standup_on_a_still_day` (guard) — nothing moved since yesterday's standup: none.
- `holds_the_next_days_standup` — yesterday's standup, then a move later yesterday: today's first tick holds a standup naming it.
- `reports_a_move_made_before_the_days_standup` — a standup ran at 00:05 on day D, and a move was made at 00:02 on D: day D+1's standup names it.
- `retries_a_standup_that_hit_its_limit` — today's standup ended `Limit`: the next tick holds another.
- `holds_no_standup_without_a_sprint` (guard) — no sprint open, moves recorded: none.

- [x] `feat(runtime): hold a standup each day the sprint moved`

### Task 4: review and retro

Files: `files.rs`, event schema (`retro.appended`), protocol, `tools/retro.rs`, `tools/refusal.rs`, `daemon/mcp.rs`, `orchestrator/rules.rs`, `orchestrator/messages.rs`, the review and retro transcripts
- `appends_to_the_retro_file` (store) — two appends: `.farik/team/retro.md` holds `# Retro`, then `## S1 (…)` and its text, then `## S2 (…)`; `read_retro` returns it.
- `reviews_an_ended_sprint` — S1 ended with FRK-1 accepted: a `review` ceremony whose first message names FRK-1, its completion note's first line, and the sprint's spent; the next tick holds the retro.
- `appends_the_retro` — the retro replay calls `farik_append_retro`: the file gains S1's section and `retro.appended` is recorded; a second call in it is refused `retro_refused`.
- `retries_a_retro_that_hit_a_limit` — the retro session ended `ProviderLimit` before appending: after the sleep, a second retro session runs.
- `refuses_a_retro_outside_a_retro_ceremony` — `farik_append_retro` in an implement session: refused.
- `holds_no_review_once_a_new_sprint_started` (guard) — S1 ended and S2 started: no review of S1.

- [x] `feat(runtime): review and look back on a sprint when it ends`

### Task 5: aged escalations

Files: team schema, `team.rs`, event schema (`escalation.aged`), protocol, `orchestrator/rules.rs`
- `ages_an_escalation_past_its_limit` — FRK-1 escalated 25 hours ago, the default limit: `escalation.aged { raised_seq, hours: 25 }` and a system line holding "waited 25 hours".
- `ages_an_escalation_once` — a second tick: nothing more.
- `ages_one_escalation_per_tick` — two escalations past the limit: the first tick ages the older and reports it against its task, the second the other.
- `ages_a_new_escalation_of_the_same_task_again` — resolved, escalated again, 25 hours later: a second `escalation.aged` for the new `raised_seq`.
- `leaves_a_young_escalation` (guard) — 23 hours: nothing.
- `defaults_the_escalation_age` (core) — a team file without it: 24.

- [x] `feat(runtime): say when an escalation has waited too long on the human`

### Task 6: the spec

Revision 0.15 in the header, naming each change:
- 5.7: the digest in planning, and `escalation.aged` with its policy and line.
- 5.8: `team/retro.md` appended by the retro ceremony and read by planning.
- 5.9: ceremonies as one session each, their threads, when each runs.
- 6.2: the Scrum Master runs them.
- 8.5: `retro.appended`, `escalation.aged`, `session.started`'s thread.
- F7: ceremony threads.

The step's interface line, and step 07's line (retro done here), are written in the project plan.

- [ ] `docs(docs): record the ceremonies and aged escalations in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
