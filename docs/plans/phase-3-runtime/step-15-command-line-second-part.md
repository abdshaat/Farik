# Phase 3, step 15: Command line, second part

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 3, 5.7, 5.13, 5.16, 8.2, 8.3, F3, F6, F11, F14
Depends on: step 14 of this phase (`Orchestrator::handle`, the human's `Command` variants, `CommandReport`, `CommandError`, `triage_by_human`, `hold_contract`, `DaemonState::{request_stop, sessions_of}`, its transcripts), a start gate: Task 1 does not begin until step 14's last commit is on this branch; step 13 (`recover`, `RecoveryReport`, `Forge`, `fixtures::FakeGh`); steps 11 and 12 (`Orchestrator`, `tick`, `stop`, `TickReport`, the rules, `orchestrator::fixtures` with `tool_runner` and `UsageThenWaitAdapter`, their transcripts); step 08 (`ClaudeAdapter`, `ClaudeConfig`, `credential_from_env`, `MIN_CLAUDE_VERSION`); step 07 (`serve`, `DaemonConfig`, `DaemonHandle`, `DaemonState`, `hook.rs`'s exchange); step 02 (the sandbox factories, `SANDBOX_IMAGE`); phase 2 (the command line and its harness, `read_settings`, `file_request`, on main)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (one round; findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A person drives the team from a terminal. `farik run` starts the daemon and the orchestrator in the one process that drives the project, names its credential and sandbox mode, and runs until nothing needs doing, a stop, or Ctrl-C; `farik plan` triages, contracts, breaks down, and assigns, and does no work. `farik contract new` files a request from a brief or an issue and writes its contract with the Product Manager, asking its questions at the terminal. `approve`, `accept`, `answer`, `integrate`, `resolve`, `cancel`, and `stop` are step 14's commands and reach the process driving the project from any terminal. `farik task show` gains event summaries, cost, children, and the diff; `farik task create --parent` files a task under an epic. Out of scope: metrics (step 16); pausing an agent from the command line (phase 5's team builder); a run that waits after it is idle (phase 5); Windows (the daemon is Unix-only).

## Decisions

The run lock and the second terminal:
- One process drives a project: `run`, `plan`, and `contract new` hold an exclusive `File::try_lock` on `.farik/local/run.lock` for their lifetime; a command handled in-process holds it while it runs. Held elsewhere, a second driver refuses "another farik process is driving this project (pid <pid> in .farik/local/daemon.json)". The process's end releases it. Rejected: probing `daemon.json`, which a crash leaves and two starting runs both find missing.
- A command typed in another terminal (step 14's open question): with the lock free it is handled in this process by `handle`; with the lock held it is sent as the command wire to `POST /command` on the holder's daemon, token from `daemon.json`, and the reply printed. Every command thus reaches the one orchestrator whose sessions, stop flag, and `Transitions` lock (step 04, per process) it concerns, and `SessionStop`/`RunStop` work from any terminal. The lock held with no `daemon.json` refuses "another farik process holds this project's run lock and serves no daemon yet: try again in a moment". An unreachable daemon, an answer that is not a reply, or none in 120 s refuses, saying the command may still take effect and `farik log` shows whether it did; never retried in-process, which could apply it twice. Rejected: appending to the log for the next tick (step 14's path), which reaches no session or stop flag and lets two processes ask the governor about one task; routing only the stops, which keeps that race. ADR 0014 records it: phase 5's front end sends the same commands to the same daemon.
- Phase 2's human writes join the routing: `farik triage`, `farik contract lock|unlock`, and `contract new --size` send `RequestTriage`, `ContractLock`, `ContractUnlock` through `/command` when the lock is held (printing `said`); with it free they keep phase 2's direct store call and output, holding the lock while they write, so phase 2's tests pass unchanged. Filing a request (`task create`, `contract new`'s filing) stays a direct `file_request` in any process: it creates a new task with an id from the log's counter, so it races with nothing.
- The route: `POST /command` on step 07's router behind its token middleware; body read by `command_from_value`; 200 with the reply wire whenever the token passes: `{ "said", "events" }` or `{ "error": { "kind": invalid | refused | not_found | failed, "detail" } }` (a schema refusal is `invalid`, errors joined by `; `). `DaemonState::set_command_handler` sets the handler after the orchestrator exists (the adapter needs the served port and token, the orchestrator the adapter, the handler the orchestrator); it answers `true`, or `false` and keeps the first when one is already set. Unset, the route answers `failed`, "this daemon takes no commands". The reply is `$defs/commandReply` in `command.schema.json` (code.md: what leaves a process has a schema); `command_to_value` is `command_from_value`'s inverse. The client is step 07's exchange moved to `daemon_client.rs`, timeout a parameter (10 s hooks, 120 s commands, since `integrate` may push), with a `NoDaemon` error the hooks still turn into a deny.
- Commands with the lock free run on an orchestrator whose daemon state is not served and whose adapter is `NoSessions` (`start_session`/`resume` answer `Spawn { "farik <command> starts no sessions" }`; `handle` starts none). It prints no credential and no sandbox warning. Rejected: the Claude adapter (a credential and `claude` to answer a question); an empty `RecordedAdapter` (a test double in the binary).

Tests, `CliIo`, and the binary:
- Tests get the recorded adapter through `CliIo`, never the binary: `engine: Engine`, `Engine::Claude` from `main.rs`, `Engine::Given(factory)` from a test (daemon state to adapter: `RecordedAdapter::with_tools(transcripts, tool_runner(daemon))` or `UsageThenWaitAdapter`). Nothing a user sets makes `farik` replay. Rejected: a hidden `FARIK_RECORDED_TRANSCRIPTS` variable (fake sessions with real tool calls in the shipped binary); a Cargo feature (code.md forbids them; feature unification would build it into the checked binary). Cost: no real SIGINT in tests, and the Claude path is tested to its version check with a fake `claude` on the test's `PATH`.
- `CliIo` gains `env: BTreeMap<String, String>` (a test passes its own; setting a process variable is `unsafe` in edition 2024), `interrupts: Interrupts` (`CtrlC` is `tokio::signal::ctrl_c`; `Channel` a test's `UnboundedReceiver<()>`, a closed one never firing), `session_ids`; `clock` becomes an `Arc`; `stdin` becomes an owned `Box<dyn Read + Send>`, so one reader thread can hold it (below). `main.rs` passes `Engine::Claude`, `Interrupts::CtrlC`, `RandomSessionIds`, `SystemClock`, and `env` from `std::env::vars_os()`, each name and value converted lossily. `CliIo::new` gives `Engine::Claude`, an empty env, a closed channel, `SequentialIds`, and `std::io::empty()`; the existing test helpers use it. `SystemClock` moves to `ids.rs`.
- `RandomSessionIds`: UUID v4 from two `std::hash::RandomState` hashers over a process-wide counter, version and variant bits set; unique, not secret (the daemon token is the secret). Rejected: the `uuid` crate; `/dev/urandom` per id, a failure `IdSource` cannot return.
- `run_cli` stays synchronous; a command needing the orchestrator builds a multi-thread `tokio` runtime and blocks on it, so tests stay plain `#[test]`. `farik` depends on `farik-runtime`, and its `tokio` features are `rt-multi-thread`, `macros`, `signal`, `sync`, `time`. The new commands are `#[cfg(unix)]`, as the daemon is.

Driving (`run`, `plan`, `contract new`), shared in `start.rs` and `run.rs`:
- Start, in order, each failure refusing with the lock released and nothing left running: the lock; the interrupt listener (a Ctrl-C before it kills the process with nothing started); `read_settings`; with `Engine::Claude`, `credential_from_env(&io.env)`, API key first, else token, neither refusing "no credential for Claude Code: set ANTHROPIC_API_KEY to an API key, or CLAUDE_CODE_OAUTH_TOKEN to the token claude setup-token prints", then printing `credential: ANTHROPIC_API_KEY (an API key)` or `credential: CLAUDE_CODE_OAUTH_TOKEN (a subscription token)`, never a value (`Engine::Given` prints none; JSON `credential: null`); `claude`, the first executable of that name on `io.env`'s `PATH` (none: "Claude Code is not installed: there is no claude on PATH"); `serve` on `.farik/local/daemon.json`; `ClaudeAdapter::new` with `ClaudeConfig { claude_path, hook_command: current_exe(), daemon_file, daemon, sessions_dir: .farik/local/sessions, team_file, env: PATH, HOME, USER, LANG, TERM, TMPDIR from io.env }`, whose error shuts the daemon down and refuses in its words; the orchestrator and its handler; `recover()` under `spawn_blocking`, printing `recovered: sessions interrupted <a>, worktrees removed <b>, tasks resumed <c>` when any is non-zero. Sandbox `docker`: `DockerSandboxFactory` with `SANDBOX_IMAGE`, not probed (a missing Docker or image fails the first sandboxed tick in `SandboxError`'s words; rejected: a probe on every start). `none`: `HostSandboxFactory` and on stderr at every start: "warning: no-sandbox mode (.farik/local/settings.json says sandbox: none). Agents' commands run on this machine as you, with your HOME: they can read your credential files (~/.git-credentials, ~/.ssh, ~/.claude/.credentials.json, the gh configuration) and push with a git hidden in a script, which farik_exec's check does not see. The governor still checks every path and permission it is asked about." Forge: the `gh` on `io.env`'s `PATH`, else `"gh"`.
- The loop, the same for all three: `tick_within(scope)` until `Idle`, `is_stopped()`, or an error; lines `FRK-1: <what>`, `idle: <why>`, or `stopped`. `farik stop` reaches whichever of the three holds the lock, through `stop()` and the same `is_stopped()` check. Then what waits on the human, `shutdown` (removing `daemon.json`), the lock. Exit 0 when idle or stopped, 1 on a tick's error (`farik: <OrchestratorError>`), 130 after Ctrl-C. It exits when idle: every command works without it. Rejected: `--watch` (phase 5).
- Ctrl-C, the same for all three: the first calls `stop()` and prints "stopping after the current session; Ctrl-C again aborts it"; the second calls `request_stop(id, "interrupted by Ctrl-C")` for each of `DaemonState::session_ids()`, which step 14's session loop aborts at once; the task keeps its status and the next run resumes it (5.15), no escalation (unlike `farik stop <session>`, the human's row of 5.2); later ones print "already stopping". Rejected: a third press exiting at once, leaving Claude Code processes spending.
- `farik run`: `TickScope::default()`. `--json`: a first line `{"credential","sandbox","recovered"}`, one per tick (`{"task_id","what"}`, `{"idle"}`, `{"stopped": true}`), a last `{"waiting_on_you"}`.
- `farik plan` means plan without doing (the founder, 2026-09-22): `TickRules::Planning` runs rule 10 (triage sessions; `draft → refining`), rule 9 (refine sessions; the governor's judgment to `ready` or `escalated`), rule 8 (plan sessions that assign; an approved epic's assignment to its Product Manager), and rule 6 for an epic only (its breakdown and close-out `plan` sessions); it passes over rules 1 to 5 and 7 and every `implement` or `verify` session: no cleanup, integration, merge or push, rejection or blocked-age move, criterion run, worktree, or start of work. Output as `run`'s.
- `TickScope { task_id: Option<TaskId>, rules: TickRules }`: `task_id` confines every rule to that task; `TickRules::All` is every rule, `Planning` as above, `Refining` rules 9 and 10 only. A rule outside the set passes its task over, as a spent budget does (step 11). `tick()` is `tick_within(&TickScope::default())`.
- What waits on the human (`waiting.rs`), printed when a driver ends, in these groups, ids ascending, nothing when nothing waits: open questions (`question.asked` with no `question.answered` naming its seq): `question <seq> on FRK-1 from pm: <question>` then `  farik answer <seq> <your answer>`; awaiting approval: `FRK-1 awaits your approval: farik approve FRK-1, or farik resolve FRK-1 refining <why>`; other `escalated`, with the last escalation's reason: `FRK-1 is escalated (<reason>): farik resolve FRK-1 <status> <message>`; `verifying` epics and `high` risk tasks: `FRK-1 may need your acceptance: farik accept FRK-1 --message <your review>`; accepted and awaiting integration under `manual`: `FRK-1 waits for you to integrate it: farik integrate FRK-1`. JSON `waiting_on_you`: `{ task_id, what, command }` each.

The human's commands:
- Ids parsed before sending, as `farik triage` does; free text is the remaining words joined by one space. `approve <task>`: `HumanAccept { contract, None }`; `accept <task> [--message <text>]`: `HumanAccept { result }`; `answer <question-id> <answer...>`: `QuestionAnswer`; `integrate <task>`: `TaskIntegrate`; `resolve <task> <status> <message...>`: `EscalationResolve`, status a clap value enum of the eleven wire spellings; `cancel <task> <reason...>`: `TaskTransition { cancelled }`; `stop`: `RunStop`; `stop <session-or-task>`: `SessionStop`, a `FRK-<n>` naming its last `session.started` with no `session.ended` (none: "FRK-<n> has no session running"). With the lock free, `stop` refuses "no farik process is driving this project, so there is nothing to stop". A report prints `said` (`--json`: `{ "said", "events" }`), exit 0; a `CommandError` exits 1, stderr `farik: ` and `Invalid`'s detail, `Refused`'s reason, `<what> is not in this project`, or `Failed`'s detail (`--json`: `{"error": <that text>}`).

`farik contract new (--brief <text> | --from <url>) [--size large|small] [--lock]`:
- The request: title the brief's first line cut at 120 characters (under 3 refused), intent the whole brief (under 20 refused, the schema's minimum), checked before anything is filed or started. Every other field is a placeholder the refine session replaces: `scope` one item in and one out, `requirements` R1, `exit_criteria` C1 `review` satisfying R1 with one rubric line, each text `(placeholder, for the Product Manager to write)`; both roles `software_developer`; `risk: low`; `budget.max_cost_usd` the team's `max_task_budget_usd` (amended 2026-09-23 by step 17: 20 dollars when the team sets none, since ADR 0015 leaves the cap unset by default); `allowed_paths` the placeholder text, a glob no file matches. Filed by `human` through `file_request`, printing `FRK-1 filed as a draft request: <title>`. Rejected: a brief with no contract (the schema refuses it); prompting for each field (the editor's, phase 5).
- `--from <url>`: `Forge::issue` (`gh issue view <url> --json title,body,url`); title the issue's; brief `<title>`, blank line, `<body>`, blank line, `From <url>`; `gh`'s refusal in its words; no `gh` on `PATH`: "farik contract new --from reads the issue with gh, and there is no gh on PATH". Rejected: the Product Manager's `WebFetch`, which cannot read Milestone 0's private issues (D9).
- `--size`: `RequestTriage` with reason "sized by the human with farik contract new", printing `FRK-1 sized large by you, so it is an epic` or `FRK-1 sized small by you, so it is a standalone task`.
- With the lock held elsewhere: `--lock` is refused before anything is filed ("--lock waits for the Product Manager's contract, which the process driving this project writes: run farik contract lock FRK-<n> once it has"), because a lock taken now would refuse the Product Manager's writes; otherwise the request is filed (and sized, routed), then `the farik process driving this project (pid <pid>) takes it from here; answer its questions with farik answer`, exit 0.
- Otherwise: start, then the loop with `TickScope { Some(<id>), TickRules::Refining }`, printing after each tick the task's new events of kinds `request.triaged`, `question.asked`, `contract.written`, `contract.evaluated`, `escalation.raised`, `task.transitioned` as `task show` prints them. `Idle` with a question open on the task prints it and `answer> `, takes the next line (a blank one asks again), `handle(QuestionAnswer)`, and ticks on. Stdin is read by one thread, started at the first prompt, that owns one `BufReader` over it for the whole loop and sends each line on a channel; the loop awaits that channel and `interrupts` together, so an interrupt at the prompt is seen at once, and the reader, still blocked, ends with the process. End of input, an interrupt at the prompt, or a stop with a question open ends with the question open and `the question stays open: farik answer <seq> <your answer>, then farik run` (130 on an interrupt). `Idle` with none prints the contract as `task show` prints its body, then one readiness line: `readiness: passed`; `readiness: failed` with each failure of the last `contract.evaluated` since the last move into `refining`; `readiness: passed the structural checks` when none and escalated for approval; or `readiness: not judged yet (<the idle report's why>)` otherwise (a spent daily budget, the task's sessions run out); then what waits on the human; then `--lock`'s `hold_contract`, printing `locked: the contract is yours (5.11)`, last so the Product Manager can write first.
- `--json`: the question and `answer> ` go to stderr; stdout holds one object at the end: `{ task_id, status, readiness: { state: passed | failed | structural | not_judged, failures?, why? }, question_open: <seq> | null, locked, waiting_on_you }`.
- Exit 0 when the loop ends, 1 on a refusal or a tick's error, 130 after an interrupt.

`farik task show [--diff]` and `farik task create --parent`:
- Event lines: `seq time kind`, then ` — ` and, for these kinds, a summary cut at 200 characters with `…`: `task.transitioned` `<from> -> <to> by <requested_by>` (`: <reason>` when set); `request.triaged` `<size> by <triaged_by>: <reason>`; `question.asked` `question <seq> from <asked_by>: <question>`; `question.answered` `answer to <question_id> by <answered_by>: <answer>`; `escalation.raised` `<reason>: <detail>`; `escalation.resolved` `to <to> by <resolved_by>: <message>`; `human.accepted` `<subject> by <accepted_by>` (`: <message>`); `criterion.recorded` `<id> passed|failed, run by <run_by>`; `note.written` `<kind>: <first line>`; `contract.evaluated` `<gate> passed` or `<gate> failed: <failures joined by ; >`; `session.started` `<purpose> session <session id> for <agent id>`; `session.ended` `<reason>`; `cost.recorded` `$<two places>`; `task.integrated` `<12 of sha> into <into> by <integrated_by>`; `pull_request.opened` `<url>`.
- Then `cost: $<usd> of $<max_cost_usd>; sessions: <n>; tokens: <in> in, <out> out` from `costs(CostScope::Task)`, or `cost: nothing yet`. An epic lists `children`: `  FRK-2 accepted Add done.txt` per board row whose parent it is.
- `--diff`, printed last: `Git::diff(<integration branch>, farik/<id>)` before integration; after, base `<sha>^1` of the last `task.integrated` when that sha is a merge commit; when it is not (a hand merge `integrate` recorded), `Git::diff(<integration branch>, farik/<id>)`, an empty result printing `farik/<id> is wholly in <into>; its merge is not a commit farik can diff against`. An epic refuses "FRK-1 is an epic and has no branch: farik task show <task> --diff shows each of its tasks'"; no branch: "FRK-1 has no branch yet: its work starts at assignment". JSON gains `cost` (`usd`, `sessions`, `input_tokens`, `output_tokens`), `children` (`board_row_json`), `diff`.
- `task create <file> --parent <epic>`: the parent is an epic on the board (else "<id> is not an epic: a task is filed under an epic") and `check_child_creation` passes with the human as actor (for whom it asks only `in_progress`), its reasons the refusal; then `file_request(.., HUMAN, Some(parent), ..)`, which triages it small, printing `FRK-2 filed as a task of FRK-1: <title>` and `farik run judges it against the Definition of Ready, and FRK-1's assignee assigns it`.

Changes in execution:
- Changed 2026-09-23 in execution (Task 1): `writes_every_command_back_as_the_wire_it_was_read_from` reads `task_create` from `a_full_contract_wire()`, because the reader fills a contract's defaults and only a contract with every default written comes back as it went in. The reply's two shapes are `$defs/commandDone` and `$defs/commandRefusal` under `commandReply`'s `oneOf`, and `reply_from_value` validates against `commandReply` with the schema's own `$defs`.
- Changed 2026-09-23 in execution (Task 2): the route runs the handler on a `tokio` task of its own, so that a client that goes away does not cut a command off half done. A body that is not JSON at all is refused by the JSON extractor before the route, as the hooks' bodies are; a JSON body that is not a command is the `invalid` reply. `lists_every_registered_session` uses a daemon state of its own, since the fixture's already answers for a session. `plans_without_running_criteria_or_merging` sets the WIP limit to 3, so that FRK-4 has an assignee with room beside FRK-1 and FRK-3.
- Changed 2026-09-23 in execution (Task 3): the three new command line test files share `crates/cli/tests/shared/project.rs`, included with `#[path]` (a subdirectory of `tests/` is not a test crate of its own, and `code.md` allows no `mod.rs`). `daemon_client.rs` also reads a `daemon.json` alone (`read_daemon_file`, `DaemonAddress` with the pid). The orchestrator a command runs on in this process has host sandboxes, because `handle` makes and removes none. A routed failure (no daemon, unreachable, not a reply) is printed as `CommandError::Failed`'s detail. `stop` takes the run lock only to learn that it is free, and gives it back before refusing.
- Changed 2026-09-23 in execution (Task 4): the command line's tests need the recorded adapter's tool runner and `UsageThenWaitAdapter`, which were `#[cfg(test)]` inside the runtime crate, so both moved to the public `farik_runtime::recorded::fixtures` (the tool runner `#[cfg(unix)]`, as the daemon is), re-exported by `orchestrator::fixtures` so step 11 to 14's tests are unchanged; the adapter gained `complete()`, which ends every waiting session `completed`, for `stops_a_plan_through_farik_stop`. Ctrl-C is heard through `tokio::signal::unix::signal(SignalKind::interrupt())`, registered as the start's second step and forwarded to the same channel a test's interrupts arrive on, since `ctrl_c()` registers only when first polled. The Ctrl-C lines go to standard error, so that `--json`'s standard output stays JSON; the second press says "aborting the current session". A lock held with no `daemon.json` refuses a second driver with "another farik process is driving this project, and it serves no daemon yet". The credential and recovery lines are printed once the start has succeeded, and what waits on the human is printed after a failed tick too.
- Changed 2026-09-23 in execution (Task 5): `--diff` asks whether a branch exists, and whether an integration's commit is a merge, with `Git::merge_base(x, x)` (for `farik/<id>` and `<sha>^2`), which answers a name's commit and refuses one that names none, rather than with new `Git` methods; the integration branch is `farik_runtime::transitions::integration_branch`, made `pub` for this. The summaries are read from each event's wire (`event_to_value`), and a task's children are listed only for an epic, while the JSON's `children` is every row whose parent is the task. `reading.rs` and `commands.rs` include the shared test harness for its log helpers.

## File map

```
docs/schemas/command.schema.json, crates/protocol/src/command.rs   modifies: commandReply; command_to_value, CommandReply, reply_to_value, reply_from_value; tests
crates/runtime/src/forge.rs                         modifies: Issue, Forge::issue; tests against FakeGh
crates/runtime/src/daemon.rs                        modifies: POST /command, CommandHandler, set_command_handler, session_ids; tests
crates/runtime/src/orchestrator.rs                  modifies: TickScope, TickRules, tick_within, is_stopped, command_handler, reply_of, result_of
crates/runtime/src/orchestrator/rules.rs            modifies: the scope in every rule; tests
docs/decisions/0014-commands-reach-a-running-farik-through-its-daemon.md   creates
Cargo.toml, crates/cli/Cargo.toml                   modifies: farik-runtime a dependency; tokio features
crates/cli/src/lib.rs, main.rs                      modifies: CliIo, Engine, Interrupts, the new subcommands
crates/cli/src/ids.rs                               creates: SystemClock, RandomSessionIds; tests
crates/cli/src/daemon_client.rs, hook.rs            creates, modifies: the exchange moved, its timeout, NoDaemon
crates/cli/src/start.rs                             creates: the run lock, routing, NoSessions, the command orchestrator; a driver's start
crates/cli/src/human.rs, triage.rs, contract.rs     creates, modifies: the human's commands; phase 2's writes routed when the lock is held
crates/cli/src/run.rs, waiting.rs                   creates: the loop, run, plan, Ctrl-C; what waits on the human
crates/cli/src/show.rs, task.rs                     modifies: event summaries, cost, children, --diff; --parent
crates/cli/src/contract_new.rs                      creates: farik contract new
crates/cli/tests/commands.rs, reading.rs, hook.rs   modifies: CliIo::new
crates/cli/tests/human.rs, running.rs, contract_new.rs   creates
docs/SPEC.md                                        modifies: 3 (the commands), 5.7 (answering from any terminal; what waits on the human), 5.13 (contract new), 8.2 (the credential named, the run lock, the command route, Ctrl-C, plan), 8.3 (the warning's words), F3 (task show; a task under an epic)
docs/plans/project-plan.md                          modifies: step 15's interface line
```

## Interfaces

Consumes: `handle`, `Command`, `CommandReport`, `CommandError`, `triage_by_human`, `hold_contract`, `request_stop` (step 14); `recover`, `Forge`, `ForgeError`, `FakeGh` (step 13); `Orchestrator`, `OrchestratorDeps`, `TickReport`, `stop`, the rules, `tool_runner`, `UsageThenWaitAdapter` (steps 11, 12); `ClaudeAdapter`, `ClaudeConfig`, `credential_from_env` (step 08); `serve`, `DaemonConfig`, `DaemonHandle`, `DaemonState` (step 07); `ToolDeps` (step 05); `Transitions` (step 04); the sandbox factories (step 02); `RecordedAdapter`, `SessionPurpose` (step 01); `check_child_creation`, `ParentEpic` (`farik-core`); `file_request`, `board_row_json`, `Projections::costs`, `Git::diff`, `read_settings`, `run_cli`, `open_project` (main).

Produces:

```rust
// farik-protocol
pub fn command_to_value(command: &Command) -> Value;
pub enum ReplyKind { Invalid, Refused, NotFound, Failed }
pub enum CommandReply { Done { said: String, events: Vec<u64> }, Error { kind: ReplyKind, detail: String } }
pub fn reply_to_value(reply: &CommandReply) -> Value;
pub fn reply_from_value(value: &Value) -> Result<CommandReply, Vec<ValidationError>>;
// farik-runtime
pub struct Issue { pub title: String, pub body: String, pub url: String }
impl Forge { pub fn issue(&self, url: &str) -> Result<Issue, ForgeError>; }
pub type CommandHandler = Arc<dyn Fn(Command) -> Pin<Box<dyn Future<Output = Result<CommandReport, CommandError>> + Send>> + Send + Sync>;
impl DaemonState { pub fn set_command_handler(&self, handler: CommandHandler) -> bool; pub fn session_ids(&self) -> Vec<String>; }
#[derive(Default)] pub enum TickRules { #[default] All, Planning, Refining }
#[derive(Default)] pub struct TickScope { pub task_id: Option<TaskId>, pub rules: TickRules }
impl Orchestrator { pub async fn tick_within(&self, scope: &TickScope) -> Result<TickReport, OrchestratorError>; pub fn is_stopped(&self) -> bool; }
pub fn command_handler(orchestrator: Arc<Orchestrator>) -> CommandHandler;
pub fn reply_of(result: Result<CommandReport, CommandError>) -> CommandReply;
pub fn result_of(reply: CommandReply) -> Result<CommandReport, CommandError>;
// farik
pub type AdapterFactory = Arc<dyn Fn(Arc<DaemonState>) -> Arc<dyn RuntimeAdapter> + Send + Sync>;
pub enum Engine { Claude, Given(AdapterFactory) }
pub enum Interrupts { CtrlC, Channel(tokio::sync::mpsc::UnboundedReceiver<()>) }
pub struct CliIo<'a> { pub stdin: Box<dyn Read + Send>, .., pub clock: Arc<dyn Clock + Send + Sync>, pub env: BTreeMap<String, String>, pub engine: Engine, pub interrupts: Interrupts, pub session_ids: Arc<dyn IdSource + Send + Sync> }
impl<'a> CliIo<'a> { pub fn new(cwd: PathBuf, stdout: Box<dyn Write + 'a>, stderr: Box<dyn Write + 'a>, clock: Arc<dyn Clock + Send + Sync>) -> CliIo<'a>; }
pub struct SystemClock;       // impl Clock
pub struct RandomSessionIds;  // impl IdSource
pub(crate) fn brief_from_issue(issue: &Issue) -> (String, String);   // contract_new: (title, brief)
pub(crate) fn request_from_brief(title: &str, brief: &str, max_cost_usd: f64) -> Result<Value, String>;   // contract_new
```

## Tasks

A command line test that runs git or serves a daemon is `#[ignore = "needs the git program: cargo xtask check --integration"]`. "A team": `farik init`, then step 11's three agents (`pm`, `dev-a`, `dev-b`, WIP limit 1, integration `manual`) written with `ProjectFiles::write_team`, and `sandbox: none` in `.farik/local/settings.json` unless a test says otherwise. "Recorded": `Engine::Given` over `RecordedAdapter::with_tools` with the named transcripts of steps 11, 12, and 14. "A live driver": the test holds the run lock and serves a daemon on `.farik/local/daemon.json` whose handler records each command and answers `said: "handled by the run"`. A test that waits for a session polls the log for its `session.started`.

### Task 1: the command wire both ways

Files: `command.schema.json`, `command.rs`

- `writes_every_command_back_as_the_wire_it_was_read_from` — for the two phase 2 wires and step 14's ten, `command_to_value(command_from_value(w))` equals `w`; `human_accept` read without `message` is written without it.
- `reads_a_reply_either_way_and_refuses_one_that_is_neither` — `{said: "x", events: [3, 4]}` is `Done`; `{error: {kind: "not_found", detail: "question 9"}}` is `Error { NotFound, .. }`; each goes back through `reply_to_value` unchanged; `{said: "x"}` and `{error: {kind: "lost", detail: ""}}` are `Err`.

- [x] `feat(protocol): write a command as its wire and read a command's reply`

### Task 2: the daemon takes commands, and a tick takes a scope

Files: `daemon.rs`, `orchestrator.rs`, `orchestrator/rules.rs`, `forge.rs`, ADR 0014 (runtime tests as steps 11 to 14 run theirs)

- `answers_a_command_on_the_daemon` — served with `command_handler(orchestrator)`, a `question_answer` of an open question: 200, `{said, events: [n]}`, the `question.answered` logged; `answer: 7`: `error.kind == "invalid"`; no token: 401; no handler: `failed`, "this daemon takes no commands"; a second `set_command_handler` answers `false`.
- `lists_every_registered_session` — after registering `s-2` and `s-1`, `session_ids()` is `["s-1", "s-2"]`; after `end_session("s-1")`, `["s-2"]`.
- `acts_only_on_the_task_in_scope` — FRK-1 `ready`, FRK-2 `assigned`: scoped to FRK-1, the tick starts FRK-1's plan session and FRK-2 stays `assigned`.
- `refines_and_nothing_else_under_the_refining_rules` — FRK-1 `ready`, FRK-2 triaged `draft`: `Refining` records FRK-2's `draft → refining`; with FRK-2 then waiting on a question, the next tick is `Idle` and no plan session starts.
- `plans_without_running_criteria_or_merging` — `Planning`, with FRK-1 `verifying` and C1 unrun, FRK-2 accepted and awaiting integration under `auto_merge`, FRK-3 `assigned`, FRK-4 `ready` (its transcript `plan_assigns_frk_1` with `FRK-1` rewritten to `FRK-4` by the test): ticks until `Idle` start one session, FRK-4's plan; the log holds no `criterion.recorded` and no `task.integrated`; FRK-3 is `assigned` with no worktree.
- `says_whether_it_was_stopped` — `is_stopped()` false, then true after `stop()`.
- `reads_an_issue_with_gh` — `FakeGh` answering `view` with `{"title":"T","body":"B","url":"u"}`: `Issue { T, B, u }`, one call `issue view <url> --json title,body,url`; exit 1 with stderr `not found`: `Failed` containing it.

- [x] `feat(runtime): take the human's commands on the daemon, and tick within a scope`

### Task 3: the human's commands from any terminal

Files: `Cargo.toml`, `crates/cli/Cargo.toml`, `lib.rs`, `main.rs`, `ids.rs`, `daemon_client.rs`, `hook.rs`, `start.rs` (the lock, routing, `NoSessions`, the command orchestrator), `human.rs`, `triage.rs`, `contract.rs`, the three existing test files, `crates/cli/tests/human.rs`

- Every phase 2 and step 07 command line test passes on `CliIo::new`.
- `hands_out_distinct_version_4_uuids` (`ids.rs`) — a thousand ids are distinct, 36 characters, character 14 `4`, character 19 one of `8 9 a b`.
- `answers_a_question_with_no_driver` — `question.asked` at seq n on FRK-1: `farik answer <n> No, one line.` exits 0 printing `said`; the log holds `question.answered { answer: "No, one line.", answered_by: human }`; a second `farik --json answer <n> Again` exits 1 and the `error` string of the JSON on stderr starts `already_answered`.
- `refuses_with_the_words_of_handle` — `farik approve FRK-1` on a draft: exit 1, stderr starts `farik: not_awaiting_approval`; `farik answer 999 Yes`: stderr contains `999` and `is not in this project`.
- `cancels_and_resolves_for_the_human` — `farik cancel FRK-1 Not needed any more`: `cancelled`, the move's `reason` that text; escalated FRK-2, `farik resolve FRK-2 refining Split it by page.`: `escalation.resolved { to: refining, message: "Split it by page." }`; `farik resolve FRK-2 finished x`: exit 2.
- `sends_a_command_to_the_driving_process` — a live driver: `farik answer <n> Yes.` prints "handled by the run", the handler got `QuestionAnswer { n, "Yes." }`, no `question.answered` logged; `farik triage FRK-1 large --reason Big.` and `farik contract lock FRK-1` reach the handler as `RequestTriage` and `ContractLock`, and the contract file is unchanged.
- `stops_only_a_driving_process` — lock free: `farik stop` exits 1, "no farik process is driving this project"; a live driver and `session.started` for FRK-1 in `s-1` with no end: `farik stop` sends `RunStop`, `farik stop FRK-1` `SessionStop { s-1 }`, `farik stop s-9` `SessionStop { s-9 }`, `farik stop FRK-2` exits 1, "FRK-2 has no session running".
- `refuses_while_the_lock_holder_serves_no_daemon` — lock held, no `daemon.json`: exit 1, "serves no daemon yet".

- [x] `feat(cli): send the human's commands to the driving process, or handle them here`

### Task 4: farik run and farik plan

Files: `start.rs` (a driver's start), `run.rs`, `waiting.rs`, `lib.rs`, `crates/cli/tests/running.rs`, `docs/SPEC.md` (3, 5.7, 8.2, 8.3)

- `refuses_to_run_without_a_credential` — `Engine::Claude`, `env` with `PATH` only: exit 1, stderr names both variables; no `daemon.json`; the lock free; nothing logged.
- `refuses_a_claude_code_older_than_the_minimum` — `ANTHROPIC_API_KEY=sk-test`, `PATH` holding a `claude` script printing `2.1.200 (Claude Code)`: exit 1, stderr contains `2.1.272`; no `daemon.json`.
- `names_the_credential_it_chose_and_never_prints_it` — the script printing `2.1.280 (Claude Code)`, an empty board: both variables set: exit 0, stdout `credential: ANTHROPIC_API_KEY (an API key)`, neither value in stdout or stderr; only `CLAUDE_CODE_OAUTH_TOKEN`: `credential: CLAUDE_CODE_OAUTH_TOKEN (a subscription token)`.
- `warns_on_every_start_in_no_sandbox_mode` — recorded, empty board: with `sandbox: none`, two runs each print the warning to stderr, containing `~/.git-credentials` and `a git hidden in a script`; no settings file: none; `farik answer` of an open question under `sandbox: none` prints none.
- `refuses_a_second_driver` — the test holds the lock: `farik run` and `farik plan` exit 1, "another farik process is driving this project".
- `runs_a_task_to_acceptance_and_says_what_waits` — a team; FRK-1 filed by `farik task create` from the harness's `a_request("Add done.txt")` YAML, sized by `farik triage FRK-1 small --reason One file.`; recorded `refine_writes_task_frk_1`, `plan_assigns_frk_1`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`: exit 0; stdout holds a `FRK-1: ` line per acting tick, `idle: nothing on the board needs doing`, `FRK-1 waits for you to integrate it: farik integrate FRK-1`; five `session.started` (refine, plan, implement, verify, verify); no `daemon.json` afterwards. `farik integrate FRK-1` then exits 0 with `merged`, and `main` holds `done.txt`.
- `recovers_before_the_first_tick` — a seeded `session.started` with no end: stdout contains `recovered: sessions interrupted 1,`; the log holds its `session.ended { aborted }`.
- `stops_after_the_session_then_aborts_it_on_ctrl_c` — FRK-1 sized small, `UsageThenWaitAdapter` waiting in the refine session, `run_cli` on a thread, the first interrupt sent once that `session.started` is logged: "stopping after the current session", `abort` not called; the second: `abort` once; exit 130; `session.ended { aborted }`, no `escalation.raised`; FRK-1 `refining`; no `daemon.json`; lock free.
- `stops_a_plan_through_farik_stop` — `farik plan` with `UsageThenWaitAdapter` waiting; once its `session.started` is logged, `farik stop` from another thread (routed), then the adapter told to complete: the plan prints `stopped`, exits 0, and starts no second session.
- `plans_without_starting_work` — FRK-1 `ready`, recorded `plan_assigns_frk_1`: `farik plan` exits 0, FRK-1 `assigned` with no worktree, one `session.started`, purpose `plan`.
- `lists_what_waits_on_the_human` — an open question on FRK-1, epic FRK-2 awaiting approval, FRK-3 escalated for `iterations`: after `idle:`, in order, `question <n> on FRK-1 from pm: `, `  farik answer <n> <your answer>`, `FRK-2 awaits your approval: farik approve FRK-2`, `FRK-3 is escalated (iterations)`; `--json`'s last line has three `waiting_on_you` entries.

- [x] `feat(cli): run and plan the team from the terminal`

### Task 5: a task's whole story, and a task under an epic

Files: `show.rs`, `task.rs`, `lib.rs`, `crates/cli/tests/reading.rs`, `crates/cli/tests/commands.rs`, `docs/SPEC.md` (F3)

- `shows_a_tasks_events_cost_and_children` — epic FRK-1 `in_progress`, child FRK-2, a $0.50 `cost.recorded` on FRK-1: stdout contains `request.triaged — large by human: `, `cost: $0.50 of $5.00; sessions: 1;`, `children`, `  FRK-2 draft `; JSON `cost.usd == 0.5`, `children[0].task_id == "FRK-2"`.
- `shows_a_tasks_diff_before_and_after_integration` — `farik/FRK-1` committing `done.txt`: `--diff` contains `+++ b/done.txt`; after a merge commit into `main` and its `task.integrated`, the same; after a fast-forward recorded as `task.integrated` at `main`'s head, the `is wholly in main` line; an epic: exit 1, "is an epic and has no branch"; a draft: exit 1, "has no branch yet".
- `files_a_task_under_an_epic_in_progress` — `farik task create child.yaml --parent FRK-1`: FRK-2 `parent: FRK-1`, `kind: task`, `request.triaged { small, human, "a task of FRK-1" }`, stdout `FRK-2 filed as a task of FRK-1: `.
- `refuses_a_parent_that_is_not_an_epic_in_progress` — a standalone parent: "is not an epic"; a `ready` epic: "the epic is ready and its tasks are written while it is in progress"; nothing appended, no FRK-2 file.

- [x] `feat(cli): show a task's events, cost, children, and diff, and file a task under an epic`

### Task 6: farik contract new

Files: `contract_new.rs`, `lib.rs`, `crates/cli/tests/contract_new.rs`, `docs/SPEC.md` (5.13), `docs/plans/project-plan.md`

- `builds_a_request_the_store_files` (unit) — `request_from_brief("Add done.txt", <40-character brief>, 5.0)` passes `validate_contract` once given an id and a status, every placeholder text `(placeholder, for the Product Manager to write)`; a 19-character brief and a 2-character title are `Err`.
- `reads_a_request_from_an_issue` (unit) — `brief_from_issue(Issue { "Add done.txt", "It should exist.", "https://github.com/o/r/issues/3" })` is `("Add done.txt", "Add done.txt\n\nIt should exist.\n\nFrom https://github.com/o/r/issues/3")`.
- `writes_a_small_request_with_the_product_manager` — a team; `--brief "Add done.txt and a check that it exists." --size small`, recorded `refine_writes_task_frk_1`: exit 0; stdout in order `FRK-1 filed as a draft request: Add done.txt and a check that it exists.`, `FRK-1 sized small by you, so it is a standalone task`, a `contract.written` line, the contract's `C1`, `readiness: passed`; FRK-1 `ready`; one `session.started`, purpose `refine`.
- `asks_its_questions_at_the_terminal` — no `--size`; recorded `triage_frk_1_large`, `refine_asks_frk_1`, `refine_writes_epic_frk_1`; stdin `"\nNo, one line.\n"`: stdout contains `large by pm: A file and the check that it exists.`, `question <n> from pm: Should done.txt be empty?`, `answer> ` twice, `readiness: passed the structural checks`, `FRK-1 awaits your approval`; `question.answered { answer: "No, one line." }` logged; FRK-1 escalated awaiting approval. With `--json`, stdout is one object with `readiness.state == "structural"` and `question_open == null`.
- `leaves_the_question_open_when_input_ends` — empty stdin: exit 0, `the question stays open: farik answer <n>`; no `question.answered`; FRK-1 `refining`.
- `ends_at_the_prompt_on_an_interrupt` — stdin the read end of a pipe the test keeps open; the interrupt sent once `question.asked` is logged: exit 130, `the question stays open: farik answer <n>` in stdout, no `question.answered`, the lock free.
- `says_why_it_stopped_before_a_contract` — the day's budget spent by a seeded `cost.recorded`: `readiness: not judged yet (the team's daily budget is spent)`, no `session.started`.
- `locks_the_contract_it_wrote_when_asked` — the small request with `--lock`: the last event is `contract.locked { human }` and the file says `locked: true`.
- `refuses_a_brief_too_short_to_be_an_intent` — `--brief "Fix it"`: exit 1; no event, contract file, or `daemon.json`.
- `hands_the_request_to_the_driving_process` — a live driver, `--size large`: FRK-1 filed, the handler got `RequestTriage { FRK-1, large }`, stdout contains `(pid ` and `takes it from here`, no `session.started`; with `--lock` instead: exit 1 and nothing filed.

- [ ] `feat(cli): write a contract with the product manager at the terminal`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
