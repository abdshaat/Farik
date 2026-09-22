# Phase 3, step 01: Runtime adapter and recorded transcripts

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` section 8.2, F6
Depends on: phase 2 (merged in #6), project plan revision 8 (committed as 2317d36)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (one round, against `docs/standards/workflow.md` stage 2; its findings folded in)

## Goal

`farik-runtime` exists, and a Claude Code session is something the rest of Farik can start, read, talk to, and stop through one trait, without a model behind it. Every line the Claude Code program prints in `stream-json` becomes a `SessionEvent`, and a recorded adapter replays real transcripts through the same trait, so every later step tests sessions without the program, the network, or money. Out of scope: spawning the real program (step 08), the events a session writes to the log (step 08), cost (step 03).

## Decisions

- The crate is `farik-runtime` in `crates/runtime`, depending on `farik-core`, `serde`, `serde_json`, and `tokio` =1.53.1 with only the `sync` feature; later steps widen the features they use. Chose `tokio` over `std::sync::mpsc` because the daemon (step 07) and the orchestrator (step 11) are `async`, and a blocking receiver in an async task stalls the runtime.
- Effort is `farik_core::team::Effort`, the team schema's own `low | medium | high`, not a second enum in the runtime: the value comes from the agent's `model.effort` and one type means no mapping (revision 8 recorded `Effort` in this step; this replaces it).
- Parsing is a `StreamParser` with state, not the stateless `parse_stream_json_line` revision 8 sketched. Measured on `claude` 2.1.280 on 2026-09-22: a `tool_result` names only its `tool_use_id`, so `ToolReturned { tool }` needs the name remembered from the `tool_use`; and a `result` line yields both `UsageReported` and `Ended`, so a line yields a `Vec` (a message may also hold several content blocks, though every recorded line holds one). Content blocks other than `text` and `tool_use` (`thinking`, `redacted_thinking`, whatever comes next) yield nothing. A `user` line whose `content` is a string (an echoed prompt) yields nothing. A `tool_result` whose `tool_use_id` no earlier `tool_use` named is `RuntimeError::Protocol` naming `tool_use_id`.
- Usage is read from the `result` line only. Measured: every assistant line repeats its whole message's `usage`, once per content block, so summing lines counts a message several times. `result.usage` maps `input_tokens`, `output_tokens`, `cache_read_input_tokens` to `cache_read_tokens`, and `cache_creation_input_tokens` to `cache_write_tokens` of `farik_core::pricing::Usage`.
- A denial is the `system` line with subtype `permission_denied`: it yields `ToolDenied { tool: tool_name, reason: decision_reason }`, and the `tool_result` for the same id that follows is not reported again as `ToolReturned`. Any other `tool_result`, error or not, is `ToolReturned` with its text. Only a `safetyCheck` denial was recorded; whether a denial by the `PreToolUse` hook or the permission-prompt tool also prints a `permission_denied` line is unmeasured, and step 08 records one of each and adjusts the parser if not. The daemon records its own denials either way (step 07), so the log does not depend on this line.
- `result` ends the session: subtype `success` is `EndReason::Completed`; `error_max_turns` is `Limit`; any other subtype is `Error`; `detail` is the `errors` array joined with `; `, or, when `errors` is absent, null, or empty, the `result` text, or empty when that is absent or null too. `Aborted` is never parsed; the adapter reports it when `abort` was called.
- Lines of a `type` the parser does not know (`rate_limit_event` today, whatever the program adds tomorrow) and `system` lines other than `permission_denied` yield nothing, because the program adds line types between minor versions and a new one must not end a session. A line that is not JSON, or a known line missing a field the parser reads, is `RuntimeError::Protocol` naming the field.
- Tool output is a `String`: a `content` string as it is, a `content` array as its `text` blocks joined with newlines.
- Transcripts are fixtures under `crates/runtime/src/recorded/transcripts/*.jsonl`, exported by `crates/runtime/src/recorded/fixtures.rs` as builder functions (`docs/standards/code.md`, "Fixtures"), not under `tests/fixtures/` as revision 8 sketched, because the orchestrator's unit tests in `src/` replay them too. They were recorded from `claude` 2.1.280 with `--model haiku` on 2026-09-22 and trimmed: hook and `thinking_tokens` lines dropped, the `init` line cut to eight fields, every thinking-only assistant line dropped except the first of `reads_a_file.jsonl` (its signature emptied), which stays because it repeats the next line's `usage`, and the recording directory rewritten to `/workspace`; everything the parser reads is kept byte for byte. They are committed in Task 2 and are the only copy the asserted numbers come from.
- `RecordedAdapter` plays its transcripts in order, one per `start_session` or `resume`, and remembers every `SessionSpec` and every `send` it was given, so a test asserts what the caller asked for. When it has none left it answers `RuntimeError::Spawn`. Its channel holds every event of the transcript, so replay never blocks and its tests are plain `#[test]`s reading with `try_recv`. `abort` sets a shared flag; the next `events` call (which has `&mut self`) replaces the receiver with one holding only `Ended { reason: Aborted, detail: "aborted" }` and no sender. `resume(session_id, prompt)` hands out a handle reporting the given `session_id` and records `prompt` in `sent()`. A transcript line that fails to parse makes `start_session` or `resume` answer that `RuntimeError::Protocol`, since a fixture that does not parse is a broken test. `RuntimeError::Aborted` and `RuntimeError::Limit` are not returned in this step; step 08 returns them from `send` on a session that ended that way.
- `RuntimeAdapter: Send + Sync` and `SessionHandle: Send`, because both are held across `await` points in `tokio` tasks from step 07 on.
- `RuntimeError`'s `Display` and `Error` are hand-written (ADR 0006).
- Task 1 amends what this step changes elsewhere: the project plan's step 01 interface (`Effort`, `parse_stream_json_line`, `tests/fixtures/transcripts`) with a "changed 2026-09-22 by the step 01 plan" note, and `docs/schemas/team.schema.json`'s `effort` description, which promises a runtime `Effort` that no longer arrives.

## File map

```
Cargo.toml                                         modifies: `farik-runtime` and `tokio` in the workspace
crates/runtime/Cargo.toml                          creates: the crate
crates/runtime/src/lib.rs                          creates: the modules and the crate doc
crates/runtime/src/session.rs                      creates: SessionSpec, SessionPurpose, McpServerConfig, McpTransport, SessionEvent, EndReason, RuntimeError, SessionHandle, RuntimeAdapter
crates/runtime/src/stream.rs                       creates: StreamParser; tests in `mod tests`
crates/runtime/src/recorded.rs                     creates (Task 2): Transcript and `pub mod fixtures`; modifies (Task 3): RecordedAdapter, RecordedSession; tests in `mod tests`
docs/plans/project-plan.md, docs/schemas/team.schema.json   modifies (Task 1): the notes above
crates/runtime/src/recorded/fixtures.rs            creates: the transcript builders and `a_session_spec()`
crates/runtime/src/recorded/transcripts/reads_a_file.jsonl       creates: a Read call, its result, text, a successful result
crates/runtime/src/recorded/transcripts/write_denied.jsonl       creates: a Write call denied, text, a successful result
crates/runtime/src/recorded/transcripts/hits_the_turn_limit.jsonl creates: a Read call and an `error_max_turns` result
```

## Interfaces

Consumes: `Usage` (`farik-core::pricing`, on main), `SessionLimits` (`farik-core::budget`, on main), `Effort` (`farik-core::team`, on main), `TaskId` (`farik-core::contract`, on main).

Produces:

```rust
pub enum SessionPurpose { Triage, Refine, Plan, Implement, Verify, Ceremony, Conversation }   // serde snake_case
pub enum McpTransport { Http { url: String }, Stdio { command: String, args: Vec<String> } }
pub struct McpServerConfig { pub name: String, pub transport: McpTransport, pub headers: BTreeMap<String, String> }
pub struct SessionSpec {
    pub session_id: String, pub agent_id: String, pub task_id: Option<TaskId>, pub purpose: SessionPurpose,
    pub system_prompt: String, pub model: String, pub effort: Effort, pub farik_tools: Vec<String>,
    pub disallowed_builtin_tools: Vec<String>, pub mcp_servers: Vec<McpServerConfig>, pub cwd: PathBuf,
    pub limits: SessionLimits, pub initial_prompt: String,
}
pub enum EndReason { Completed, Aborted, Limit, Error }
pub enum SessionEvent {
    ToolCalled { tool: String, input: Value }, ToolReturned { tool: String, output: String },
    ToolDenied { tool: String, reason: String }, UsageReported(Usage), TextProduced(String),
    Ended { reason: EndReason, detail: String },
}
pub enum RuntimeError { Spawn { detail: String }, Protocol { detail: String }, VersionTooOld { found: String, required: String }, Aborted, Limit }
pub trait SessionHandle: Send {
    fn session_id(&self) -> &str;
    fn events(&mut self) -> &mut tokio::sync::mpsc::Receiver<SessionEvent>;
    fn send(&self, text: &str) -> Result<(), RuntimeError>;
    fn abort(&self) -> Result<(), RuntimeError>;
}
pub trait RuntimeAdapter: Send + Sync {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError>;
    fn resume(&self, session_id: &str, prompt: &str) -> Result<Box<dyn SessionHandle>, RuntimeError>;
}
#[derive(Default)] pub struct StreamParser { /* tool names by tool_use_id, denied ids */ }
impl StreamParser { pub fn parse_line(&mut self, line: &str) -> Result<Vec<SessionEvent>, RuntimeError>; }
pub struct Transcript { /* lines */ }
impl Transcript { pub fn from_jsonl(text: &str) -> Transcript; pub fn lines(&self) -> impl Iterator<Item = &str>; }
pub struct RecordedAdapter { /* transcripts, started specs, sent texts */ }
impl RecordedAdapter {
    pub fn new(transcripts: Vec<Transcript>) -> RecordedAdapter;
    pub fn started(&self) -> Vec<SessionSpec>;
    pub fn sent(&self) -> Vec<String>;
}
// recorded::fixtures
pub fn reads_a_file() -> Transcript; pub fn write_denied() -> Transcript; pub fn hits_the_turn_limit() -> Transcript;
pub fn a_session_spec() -> SessionSpec;
```

## Tasks

### Task 1: the crate and the session vocabulary

Files: modified `Cargo.toml`, `docs/plans/project-plan.md`, `docs/schemas/team.schema.json`; created `crates/runtime/Cargo.toml`, `crates/runtime/src/lib.rs`, `crates/runtime/src/session.rs`, tested in `session.rs`
Produces: every type in `session.rs` above
Consumes: `Usage`, `SessionLimits`, `Effort`, `TaskId`

Tests:

- `displays_a_version_too_old_error_with_both_versions` — asserts that `RuntimeError::VersionTooOld { found: "2.1.200", required: "2.1.272" }` displays a sentence containing both version strings.
- `serialises_session_purposes_in_snake_case` — asserts that `serde_json::to_value(SessionPurpose::Implement)` is `"implement"` and that each of the seven round-trips.

- [x] `feat(runtime): add the session vocabulary and the runtime crate`

### Task 2: the stream-json parser

Files: created `crates/runtime/src/stream.rs`, `crates/runtime/src/recorded.rs` (`Transcript` and `pub mod fixtures` only), the three transcripts, and `crates/runtime/src/recorded/fixtures.rs` (the transcript builders only), tested in `stream.rs`
Produces: `StreamParser`, `Transcript::{from_jsonl, lines}`, the three transcript builders
Consumes: `SessionEvent`, `EndReason`, `RuntimeError` from Task 1

Tests, each driving a fresh parser through a whole fixture unless it says otherwise:

- `reports_a_tool_call_its_result_and_the_text_of_a_completed_session` — asserts that `reads_a_file()` yields, in order, `ToolCalled { tool: "Read", input }` with `input["file_path"] == "/workspace/note.txt"`, `ToolReturned { tool: "Read", output }` with `output` containing `hello fixture`, `TextProduced("hello fixture")`, `UsageReported(_)`, `Ended { reason: Completed, .. }`, and nothing else.
- `reads_usage_from_the_result_line_only` — asserts that the one `UsageReported` from `reads_a_file()` equals the fixture's `result.usage` (`input_tokens` 18, `output_tokens` 368, `cache_read_tokens` 33806, `cache_write_tokens` 10215), not a sum of the assistant lines; `output_tokens` is the value that tells them apart (the assistant lines' own sum is 10, since the kept thinking line repeats the tool call's `usage`).
- `reports_a_denial_once_with_its_reason` — asserts that `write_denied()` yields `ToolDenied { tool: "Write", reason }` with the fixture's `decision_reason`, and no `ToolReturned` for that call.
- `ends_with_limit_when_the_turn_limit_is_reached` — asserts that `hits_the_turn_limit()` ends with `Ended { reason: Limit, detail }` where `detail` is `Reached maximum number of turns (1)`.
- `ends_with_error_on_any_other_error_subtype` — one line, `{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["boom"],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}`: asserts `Ended { reason: Error, detail: "boom" }`.
- `ignores_line_types_it_does_not_know` — asserts that `{"type":"rate_limit_event"}` and `{"type":"something_new"}` and `{"type":"system","subtype":"init"}` and a `thinking` block each yield an empty `Vec`.
- `refuses_a_result_for_a_tool_nobody_called` — asserts that a `user` `tool_result` line naming `toolu_unknown` with no `tool_use` before it is `Err(RuntimeError::Protocol { detail })` with `detail` naming `tool_use_id`.
- `refuses_a_line_that_is_not_json` — asserts `Err(RuntimeError::Protocol { .. })` for `not json`.
- `refuses_a_result_without_usage` — asserts `Err(RuntimeError::Protocol { detail })` with `detail` naming `usage`.
- `joins_the_text_blocks_of_a_tool_result_array` — one `tool_use` line then a `user` line whose `content` is `[{"type":"text","text":"a"},{"type":"text","text":"b"}]`: asserts `ToolReturned { output: "a\nb" }`.

- [x] `feat(runtime): parse claude code stream-json into session events`

### Task 3: the recorded adapter

Files: created `crates/runtime/src/recorded.rs`, modified `crates/runtime/src/recorded/fixtures.rs` (`a_session_spec`), tested in `recorded.rs`
Produces: `RecordedAdapter`, its `SessionHandle`, `a_session_spec()`
Consumes: `StreamParser`, `Transcript`, the transcript builders from Task 2; the traits from Task 1

Tests (plain `#[test]`, reading with `try_recv`):

- `replays_a_transcript_as_session_events` — asserts that `start_session(a_session_spec())` on an adapter holding `reads_a_file()` delivers on `events()` the same sequence the parser yields for it, then the channel closes.
- `plays_transcripts_in_order_across_start_and_resume` — with `[reads_a_file(), write_denied()]`, asserts the first session's events include `ToolCalled { tool: "Read" }` and the resumed session's include `ToolDenied { tool: "Write" }`.
- `answers_spawn_when_no_transcript_is_left` — asserts that a second `start_session` on an adapter holding one transcript is `Err(RuntimeError::Spawn { .. })`.
- `remembers_the_specs_it_started_and_the_texts_it_was_sent` — asserts that `started()` holds the spec given, with its `session_id`, and `sent()` holds `"go on"` after `handle.send("go on")`.
- `ends_an_aborted_session_with_aborted` — asserts that after `abort()`, the next event read is `Ended { reason: Aborted, .. }` and the channel then closes, whatever the transcript held after that point.
- `hands_out_the_session_id_of_the_spec` — asserts `handle.session_id() == spec.session_id`, and that a handle from `resume("s-9", "continue")` reports `s-9` and puts `continue` in `sent()`.

- [x] `feat(runtime): replay recorded transcripts through the adapter trait`

## Verification

```
cargo test -p farik-runtime
# expected: every test above passes; test result: ok.
cargo xtask check
# expected: xtask check: ok
```
