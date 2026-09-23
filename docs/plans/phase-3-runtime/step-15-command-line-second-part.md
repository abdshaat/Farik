# Phase 3, step 15: Command line, second part

Status: draft (readiness review pending)
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 3, 5.7, 5.13, 5.16, 8.2, 8.3, F3, F6, F11, F14
Depends on: step 14 of this phase (`Orchestrator::handle`, the human's `Command` variants, `CommandReport`, `CommandError`, `triage_by_human`, `hold_contract`, `DaemonState::{request_stop, sessions_of}`, its transcripts), a start gate: Task 1 does not begin until step 14's last commit is on this branch; step 13 (`recover`, `RecoveryReport`, `Forge`, `fixtures::FakeGh`); steps 11 and 12 (`Orchestrator`, `tick`, `stop`, `TickReport`, `orchestrator::fixtures` with `tool_runner` and `UsageThenWaitAdapter`, their transcripts); step 08 (`ClaudeAdapter`, `ClaudeConfig`, `credential_from_env`, `MIN_CLAUDE_VERSION`); step 07 (`serve`, `DaemonConfig`, `DaemonHandle`, `DaemonState`, `crates/cli/src/hook.rs`'s exchange); step 02 (`DockerSandboxFactory`, `HostSandboxFactory`, `SANDBOX_IMAGE`); phase 2 (the command line and its test harness, `read_settings`, `file_request`, on main)
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A person drives the team from a terminal. `farik run` starts the daemon and the orchestrator in the one process that owns the project, with the credential and sandbox mode it names, and runs until nothing needs doing, a stop, or Ctrl-C; `farik plan` does the same for triage, contracts, breakdowns, and assignments only. `farik contract new` files a request from a brief or an issue and writes its contract with the Product Manager, asking its questions at the terminal. `farik approve`, `accept`, `answer`, `integrate`, `resolve`, `cancel`, and `stop` are the human's commands of step 14, and reach a run going in another terminal. `farik task show` gains readable events, cost, children, and the diff; `farik task create --parent` files a task under an epic. Out of scope: the metrics (step 16); pausing an agent from the command line (the team builder's, phase 5); a run that waits for commands after it is idle (phase 5's daemon with the front end); Windows (the daemon is Unix-only, step 07).

## Decisions

- One process drives a project at a time. `farik run`, `farik plan`, and `farik contract new` hold an exclusive `File::try_lock` on `.farik/local/run.lock` for their lifetime, and a human command handled in-process holds it while it runs; a lock held elsewhere refuses a second run with "another farik process is driving this project (see .farik/local/daemon.json); farik stop stops a run". Rejected: probing `daemon.json`, which a crash leaves behind and which two runs starting together both find missing. The lock is released by the process ending, so a killed run leaves nothing to clean.
- How a command typed in another terminal reaches a running `farik run` (the question step 14 left). A human command first tries the run lock. Free: it is handled in this process by `handle` on an orchestrator built for commands. Held: it goes to the process holding it as the command wire in `POST /command` on that process's daemon, with the token from `.farik/local/daemon.json`, and its reply is printed. So every command reaches the one orchestrator whose sessions, stop flag, and `Transitions` lock (step 04: one process) it concerns, and `SessionStop` and `RunStop` work from any terminal. The lock held with no `daemon.json` is refused "a farik run is starting in this project; try again in a moment". A daemon that refuses the connection, answers anything but a reply, or has not answered in 120 s is refused, saying the command may still take effect and `farik log` shows whether it did; it is never retried in-process, because a command the run applied and answered late would be applied twice. Rejected: appending to the log for the next tick to catch up (step 14's path), which cannot reach a session or the run's stop flag and leaves two processes asking the governor about one task at once; routing only the stops, which keeps that race for every other command. ADR 0014 records it, because phase 5's front end sends the same commands to the same daemon.
- The route. `POST /command` joins step 07's router behind its token middleware. Its body is the command wire read by `command_from_value`; it answers 200 with the reply wire whenever the token passes: `{ "said", "events" }`, or `{ "error": { "kind": invalid | refused | not_found | failed, "detail" } }`, a body the schema refuses being `invalid` with its errors joined by `; `. The handler is set after the orchestrator exists (`DaemonState::set_command_handler`), because the adapter needs the served daemon's port and token, the orchestrator needs the adapter, and the handler needs the orchestrator; until it is set the route answers `failed` with "this daemon takes no commands". The reply is `$defs/commandReply` in `command.schema.json`, because what leaves a process has a schema (code.md); `command_to_value` is added as `command_from_value`'s inverse, so the command line sends exactly what the daemon reads. The client is step 07's forty-line exchange, moved from `hook.rs` to `daemon_client.rs` with the timeout as a parameter (10 s for the hooks, 120 s for commands, because `integrate` may push and open a pull request) and a `NoDaemon` error for a missing file or a refused connection, which the hooks still turn into a deny.
- The orchestrator for commands, with no run live: `OrchestratorDeps` built as a run builds them (below), except that the daemon state is not served and the adapter is the command line's `NoSessions`, whose `start_session` and `resume` answer `RuntimeError::Spawn { detail: "farik <command> starts no sessions" }`; `handle` starts none (step 14). Rejected: the Claude adapter, which would demand a credential and a `claude` program to answer a question; an empty `RecordedAdapter`, a test double in the shipped binary.
- How tests get the recorded adapter: through `CliIo`, not the binary. `CliIo` gains `engine: Engine`; `main.rs` passes `Engine::Claude`; a test passes `Engine::Given(factory)`, a function from the daemon state to an adapter (`RecordedAdapter::with_tools(transcripts, tool_runner(daemon))`, or step 11's `UsageThenWaitAdapter`). Nothing a user can set makes `farik` replay sessions. Phase 2's tests already drive `run_cli` in-process, so no test needs the binary to replay. Rejected: a hidden `FARIK_RECORDED_TRANSCRIPTS` variable, a door in the shipped binary to fake sessions whose tool calls are real; a Cargo feature, which code.md rules out for the first release and which feature unification would compile into the binary `cargo xtask check` builds. The cost: no test sends a real SIGINT (Ctrl-C arrives through `interrupts`, below), and the Claude path is tested up to its version check with a fake `claude` on the test's `PATH`.
- `CliIo` also gains `env: BTreeMap<String, String>` (the process environment, which `main.rs` fills; a test passes its own, because setting a process variable is `unsafe` in edition 2024 and races other tests), `interrupts: Interrupts` (`CtrlC` listens with `tokio::signal::ctrl_c`; `Channel` is a test's `UnboundedReceiver<()>`, and a closed channel never interrupts), and `session_ids: Arc<dyn IdSource + Send + Sync>`; `clock` becomes `Arc<dyn Clock + Send + Sync>`, because the orchestrator shares it. `CliIo::new` gives `Engine::Claude`, an empty environment, a closed channel, and `SequentialIds`, and phase 2's and step 07's test helpers build it that way. `SystemClock` moves from `main.rs` to `ids.rs`.
- `RandomSessionIds` (`ids.rs`) hands out UUID v4: 128 bits from two `std::hash::RandomState` hashers over a process-wide counter, with the version and variant bits set. A session id must be unique, not secret; the daemon's token is the secret. Rejected: the `uuid` crate, a dependency for twenty lines; reading `/dev/urandom` per id, a failure `IdSource` cannot return.
- `run_cli` stays synchronous: a command that needs the orchestrator builds a multi-thread `tokio` runtime and blocks on it, so the command line's tests stay plain `#[test]`. `farik-runtime` becomes a dependency of `farik`, and its `tokio` gains `macros`, `signal`, `sync`, and `time`. The new commands are `#[cfg(unix)]`, as the daemon is.
- Starting a run (`start.rs`, shared by `run`, `plan`, and `contract new`), in this order, each failure refusing with the lock released and nothing left running: the run lock; `read_settings` (an unreadable file refuses); with `Engine::Claude`, `credential_from_env(&io.env)`, the API key first, else the token, neither refusing "no credential for Claude Code: set ANTHROPIC_API_KEY to an API key, or CLAUDE_CODE_OAUTH_TOKEN to the token claude setup-token prints", and then the line `credential: ANTHROPIC_API_KEY (an API key)` or `credential: CLAUDE_CODE_OAUTH_TOKEN (a subscription token)`, never a value; `claude`, the first executable of that name in a directory of `io.env`'s `PATH` (none refuses "Claude Code is not installed: there is no claude on PATH"); the daemon (`serve` on `.farik/local/daemon.json`); the adapter, `ClaudeAdapter::new` with `ClaudeConfig { claude_path, hook_command: current_exe(), daemon_file, daemon: info, sessions_dir: .farik/local/sessions, team_file, env: PATH, HOME, USER, LANG, TERM, TMPDIR from io.env }`, whose `VersionTooOld` or `Spawn` shuts the daemon down and refuses with its words; the orchestrator and its command handler; `recover()` under `spawn_blocking`, printing `recovered: sessions interrupted <a>, worktrees removed <b>, tasks resumed <c>` when any count is not zero. Sandbox: `docker` is `DockerSandboxFactory` with `SANDBOX_IMAGE`; `none` is `HostSandboxFactory` and this warning on stderr at every start: "warning: no-sandbox mode (.farik/local/settings.json says sandbox: none). Agents' commands run on this machine as you, with your HOME: they can read your credential files (~/.git-credentials, ~/.ssh, ~/.claude/.credentials.json, the gh configuration) and push with a git hidden in a script, which farik_exec's check does not see. The governor still checks every path and permission it is asked about." Docker is not probed at start: a missing Docker or image fails the first sandboxed session's tick with `SandboxError`'s words. Rejected: probing, a second on every start of a run that may implement nothing. The forge is `Forge { program: the gh on io.env's PATH, else "gh", root }`.
- `farik run`: after starting, `tick_within(&TickScope::default())` until a tick is `Idle`, `is_stopped()`, or an error. Each report is one line, `FRK-1: <what>` or `idle: <why>`, and a loop the stop flag ended prints `stopped`; with `--json`, one JSON line each (`{"task_id","what"}`, `{"idle"}`) after a first `{"credential","sandbox","recovered"}`. Then what waits on the human; then `shutdown` (which removes `daemon.json`) and the lock. Exit 0 when idle or stopped by `farik stop`, 1 on a tick's error (`farik: <OrchestratorError>` on stderr), 130 after Ctrl-C. It exits when idle rather than waiting, because every command works without a run and a waiting run holds the lock for nothing; the human runs it again after answering. Rejected: a `--watch` loop, which phase 5 brings with the front end.
- Ctrl-C. The first calls `stop()` and prints "stopping after the current session; Ctrl-C again aborts it". The second calls `request_stop(id, "interrupted by Ctrl-C")` for each of `DaemonState::sessions()`, so step 14's session loop aborts at once; the task keeps its status and the next run resumes it (5.15), with no escalation, unlike `farik stop <session>`, which is the human's row of 5.2. Later ones print "already stopping". Rejected: a third press exiting at once, which would leave Claude Code processes spending.
- `farik plan`: the same start and loop with `TickScope { task_id: None, purposes: Some(vec![Triage, Refine, Plan]) }`: requests are triaged, contracts written, epics broken down, and tasks assigned, and nothing is implemented or verified; rules that start no session run as in `farik run`, because they spend nothing. The name had no definition in the spec or the project plan; this is the cheap half of a run, which a person wants to see before paying for the work, and phase 4's planning ceremony takes the name over. Rejected: a dry run printing what `farik run` would do, which would split every rule of steps 11 to 14 into a finder and a doer (see Open for the founder).
- `TickScope`: `task_id` confines every rule to that task; `purposes` makes a rule that would start a session of another purpose pass its task over, as a spent budget does (step 11). `tick()` is `tick_within(&TickScope::default())`.
- What waits on the human (`waiting.rs`), printed when a run, a plan, or `contract new` ends, grouped in this order, task ids ascending within a group, nothing when nothing waits: each open question, `question.asked` with no `question.answered` naming its seq (`question <seq> on FRK-1 from pm: <question>`, then `  farik answer <seq> <your answer>`); each task awaiting approval (`FRK-1 awaits your approval: farik approve FRK-1, or farik resolve FRK-1 refining <why>`); each other `escalated` task with its last escalation's reason (`FRK-1 is escalated (<reason>): farik resolve FRK-1 <status> <message>`); each `verifying` epic and `high` risk task (`FRK-1 may need your acceptance: farik accept FRK-1 --message <your review>`); each accepted task awaiting integration under `manual` (`FRK-1 waits for you to integrate it: farik integrate FRK-1`). JSON: `waiting_on_you`, a list of `{ task_id, what, command }`.
- The human's commands. Ids are parsed before anything is sent, as `farik triage` parses its own; free text after the ids is the remaining words joined by one space. `approve <task>` is `HumanAccept { contract, message: None }`; `accept <task> [--message <text>]` is `HumanAccept { result }`; `answer <question-id> <answer...>` is `QuestionAnswer`; `integrate <task>` is `TaskIntegrate`; `resolve <task> <status> <message...>` is `EscalationResolve`, the status a clap value enum of the eleven wire spellings; `cancel <task> <reason...>` is `TaskTransition { to: cancelled }`; `stop` is `RunStop`, and `stop <session-or-task>` is `SessionStop`, a `FRK-<n>` naming the session of that task's last `session.started` with no `session.ended` (none refuses "FRK-<n> has no session running"). With no run live, `stop` refuses "no farik run is running in this project, so there is nothing to stop", because a stop reaches only a registered session. A report prints `said`, or `{ "said", "events" }` with `--json`, exit 0; a `CommandError` exits 1 with `farik: ` and `Invalid`'s detail, `Refused`'s reason, `<what> is not in this project` for `NotFound`, or `Failed`'s detail.
- `farik contract new (--brief <text> | --from <url>) [--size large|small] [--lock]`, one of the first two required:
  - The request. Its title is the brief's first line cut at 120 characters (under 3 refused) and its intent the whole brief (under 20 characters refused, the schema's minimum), both checked before anything is filed or started. Every other field is a placeholder the refine session replaces: `scope` one item in and one out, `requirements` R1, `exit_criteria` C1 a `review` criterion satisfying R1 with one rubric line, each text `(placeholder, for the Product Manager to write)`; both roles `software_developer`; `risk: low`; `budget.max_cost_usd` the team's `max_task_budget_usd`; `allowed_paths` that same placeholder text, a glob no file matches. Filed by `human` through `file_request`. Rejected: filing a brief with no contract, which the schema refuses; prompting for each field, which is the editor's work (phase 5).
  - `--from <url>`: `Forge::issue(url)` (`gh issue view <url> --json title,body,url`); the title is the issue's, the brief `<title>`, a blank line, `<body>`, a blank line, `From <url>`; `gh`'s refusal is refused in its words, and no `gh` on `PATH` refuses "farik contract new --from reads the issue with gh, and there is no gh on PATH". Rejected: handing the link to the Product Manager's `WebFetch`, which cannot read a private repository's issues, and Milestone 0's are (D9).
  - `--size` records `triage_by_human` with the reason "sized by the human with farik contract new".
  - With no run live: start, then `tick_within(TickScope { task_id: Some(<id>), purposes: Some(vec![Triage, Refine]) })` until `Idle`, printing after each tick the task's new events of the kinds `request.triaged`, `question.asked`, `contract.written`, `contract.evaluated`, `escalation.raised`, `task.transitioned`, as `task show` prints them. `Idle` with a question open on the task prints it and the prompt `answer> `, reads one line from `stdin` (a blank one asks again), hands it to `handle(QuestionAnswer)`, and ticks on. End of input, or Ctrl-C at the prompt, ends with the question open and the line `the question stays open: farik answer <seq> <your answer>, then farik run`. `Idle` with none prints the contract as `task show` prints its body, then `readiness: passed`, or `readiness: failed` and each failure of the last `contract.evaluated` since the last move into `refining`, or, with none and the task escalated for approval, `readiness: passed the structural checks`; then what waits on the human. The scope's purposes stop it at `ready` (the next session is a plan) or `escalated`.
  - `--lock` locks the contract when the loop ends (`hold_contract`, printing `locked: the contract is yours (5.11)`), so that the Product Manager can write it first; to edit before locking, leave `--lock` off and run `farik contract lock` after.
  - With a run live, the request is filed (and sized, with `--size`), and the line `farik run is running in this project and takes it from here; answer its questions with farik answer` replaces the loop. Rejected: refusing, which would make a person stop the team to file a request.
  - Exit 0 when the loop or the hand-over ends, 1 on a refusal or a tick's error, 130 after Ctrl-C.
- `farik task show [--diff]`. Each event line is `seq time kind`, then ` — ` and a summary for these kinds, cut at 200 characters with `…`: `task.transitioned` `<from> -> <to> by <requested_by>` and `: <reason>` when set; `request.triaged` `<size> by <triaged_by>: <reason>`; `question.asked` `question <seq> from <asked_by>: <question>`; `question.answered` `answer to <question_id> by <answered_by>: <answer>`; `escalation.raised` `<reason>: <detail>`; `escalation.resolved` `to <to> by <resolved_by>: <message>`; `human.accepted` `<subject> by <accepted_by>` and `: <message>` when set; `criterion.recorded` `<criterion_id> passed|failed, run by <run_by>`; `note.written` `<kind>: <first line>`; `contract.evaluated` `<gate> passed` or `<gate> failed: <failures joined by ; >`; `session.started` `<purpose> session <session id> for <agent id>`; `session.ended` `<reason>`; `cost.recorded` `$<dollars, two places>`; `task.integrated` `<first 12 of sha> into <into> by <integrated_by>`; `pull_request.opened` `<url>`. Then `cost: $<usd> of $<max_cost_usd>; sessions: <n>; tokens: <in> in, <out> out` from `costs(CostScope::Task)`, dollars to two places, or `cost: nothing yet`. An epic then lists `children`, one line per board row whose parent it is: `  FRK-2 accepted Add done.txt`. `--diff` prints last `Git::diff(<integration branch>, farik/<id>)`, or, once integrated, with base `<sha>^1` of its last `task.integrated`, because the branch is by then in the integration branch; an epic refuses "FRK-1 is an epic and has no branch: farik task show <task> --diff shows each of its tasks'", and a task with no branch "FRK-1 has no branch yet: its work starts at assignment". JSON gains `cost` (`usd`, `sessions`, `input_tokens`, `output_tokens`), `children` (`board_row_json` each), and, with `--diff`, `diff`.
- `farik task create <file> [--parent <epic>]`: the parent must be an epic on the board (else "<id> is not an epic: a task is filed under an epic"), and `check_child_creation` is asked with the human as actor, for whom it asks only that the epic is `in_progress`; its reasons are the refusal. Then `file_request(.., HUMAN, Some(parent), ..)`, which triages it small, and the lines `FRK-2 filed as a task of FRK-1: <title>` and `farik run judges it against the Definition of Ready, and FRK-1's assignee assigns it`. No run lock: filing is a store write, safe across processes as phase 2's is.

## File map

```
docs/schemas/command.schema.json, crates/protocol/src/command.rs   modifies: commandReply; command_to_value, CommandReply, reply_to_value, reply_from_value; tests
crates/runtime/src/forge.rs                         modifies: Issue, Forge::issue; tests against FakeGh
crates/runtime/src/daemon.rs                        modifies: POST /command, CommandHandler, set_command_handler, sessions; tests
crates/runtime/src/orchestrator.rs                  modifies: TickScope, tick_within, is_stopped, command_handler, reply_of, result_of
crates/runtime/src/orchestrator/rules.rs            modifies: the scope in every rule; tests
docs/decisions/0014-commands-reach-a-running-farik-through-its-daemon.md   creates
Cargo.toml, crates/cli/Cargo.toml                   modifies: farik-runtime a dependency; tokio features
crates/cli/src/lib.rs, main.rs                      modifies: CliIo, Engine, Interrupts, the new subcommands
crates/cli/src/ids.rs                               creates: SystemClock, RandomSessionIds; tests
crates/cli/src/daemon_client.rs, hook.rs            creates, modifies: the exchange moved, its timeout, NoDaemon
crates/cli/src/start.rs                             creates: the run lock, credential, claude, sandbox, daemon, adapter, orchestrator, recover; NoSessions
crates/cli/src/human.rs                             creates: approve, accept, answer, integrate, resolve, cancel, stop
crates/cli/src/run.rs, waiting.rs                   creates: farik run, farik plan, Ctrl-C; what waits on the human
crates/cli/src/show.rs, task.rs                     modifies: event summaries, cost, children, --diff; --parent
crates/cli/src/contract_new.rs                      creates: farik contract new
crates/cli/tests/commands.rs, reading.rs, hook.rs   modifies: CliIo::new
crates/cli/tests/human.rs, running.rs, contract_new.rs   creates
docs/SPEC.md                                        modifies: 3 (the commands), 5.7 (answering from any terminal; what waits on the human), 5.13 (contract new's placeholder request, --from, the hand-over, --lock), 8.2 (farik run: the credential named, the run lock, the command route, Ctrl-C), 8.3 (the warning's words), F3 (task show's cost, children, diff; a task under an epic)
docs/plans/project-plan.md                          modifies: step 15's interface line
```

## Interfaces

Consumes: `handle`, `Command`, `CommandReport`, `CommandError`, `AcceptSubject`, `triage_by_human`, `hold_contract`, `request_stop` (step 14); `recover`, `Forge`, `ForgeError`, `FakeGh` (step 13); `Orchestrator`, `OrchestratorDeps`, `TickReport`, `stop`, `tool_runner`, `UsageThenWaitAdapter` (step 11); `ClaudeAdapter`, `ClaudeConfig`, `credential_from_env` (step 08); `serve`, `DaemonConfig`, `DaemonHandle`, `DaemonState` (step 07); `ToolDeps` (step 05); `Transitions` (step 04); the sandbox factories (step 02); `RecordedAdapter`, `Transcript`, `SessionPurpose` (step 01); `check_child_creation`, `ParentEpic` (`farik-core`); `file_request`, `board_row_json`, `Projections::costs`, `Git::diff`, `read_settings`, `run_cli`, `open_project` (main).

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
impl DaemonState { pub fn set_command_handler(&self, handler: CommandHandler) -> bool; pub fn sessions(&self) -> Vec<String>; }
#[derive(Default)] pub struct TickScope { pub task_id: Option<TaskId>, pub purposes: Option<Vec<SessionPurpose>> }
impl Orchestrator { pub async fn tick_within(&self, scope: &TickScope) -> Result<TickReport, OrchestratorError>; pub fn is_stopped(&self) -> bool; }
pub fn command_handler(orchestrator: Arc<Orchestrator>) -> CommandHandler;
pub fn reply_of(result: Result<CommandReport, CommandError>) -> CommandReply;
pub fn result_of(reply: CommandReply) -> Result<CommandReport, CommandError>;
// farik
pub type AdapterFactory = Arc<dyn Fn(Arc<DaemonState>) -> Arc<dyn RuntimeAdapter> + Send + Sync>;
pub enum Engine { Claude, Given(AdapterFactory) }
pub enum Interrupts { CtrlC, Channel(tokio::sync::mpsc::UnboundedReceiver<()>) }
pub struct CliIo<'a> { .., pub clock: Arc<dyn Clock + Send + Sync>, pub env: BTreeMap<String, String>, pub engine: Engine, pub interrupts: Interrupts, pub session_ids: Arc<dyn IdSource + Send + Sync> }
impl<'a> CliIo<'a> { pub fn new(cwd: PathBuf, stdin: Box<dyn Read + 'a>, stdout: Box<dyn Write + 'a>, stderr: Box<dyn Write + 'a>, clock: Arc<dyn Clock + Send + Sync>) -> CliIo<'a>; }
pub struct SystemClock;       // impl Clock
pub struct RandomSessionIds;  // impl IdSource
pub(crate) fn brief_from_issue(issue: &Issue) -> (String, String);   // contract_new: (title, brief)
pub(crate) fn request_from_brief(title: &str, brief: &str, max_cost_usd: f64) -> Result<Value, String>;   // contract_new: the placeholder request, or the refusal
```

## Tasks

Command line tests are `#[ignore = "needs the git program: cargo xtask check --integration"]` plain `#[test]`s through `run_cli`, as phase 2's are. "A team" is `farik init` followed by writing step 11's three agents (`pm`, `dev-a`, `dev-b`, WIP limit 1, integration `manual`) with `ProjectFiles::write_team`, and `.farik/local/settings.json` saying `sandbox: none` unless a test says otherwise. "Recorded" is `Engine::Given` over `RecordedAdapter::with_tools` with the named transcripts of steps 11, 12, and 14; "a live run" is the test holding the run lock and serving a daemon on `.farik/local/daemon.json` whose handler records each command it gets and answers `said: "handled by the run"`.

### Task 1: the command wire both ways

Files: `command.schema.json`, `command.rs`

- `writes_every_command_back_as_the_wire_it_was_read_from` — for the two phase 2 wires and step 14's ten, `command_to_value(command_from_value(w))` equals `w`; a `human_accept` read without `message` is written without the key.
- `reads_a_reply_either_way_and_refuses_one_that_is_neither` — `{said: "x", events: [3, 4]}` reads as `Done`; `{error: {kind: "not_found", detail: "question 9"}}` as `Error { NotFound, .. }`; each goes back through `reply_to_value` to its own value; `{said: "x"}` and `{error: {kind: "lost", detail: ""}}` are `Err`.

- [ ] `feat(protocol): write a command as its wire and read a command's reply`

### Task 2: the daemon takes commands, and a tick takes a scope

Files: `daemon.rs`, `orchestrator.rs`, `orchestrator/rules.rs`, `forge.rs`, ADR 0014 (runtime tests as steps 11 to 14 run theirs)

- `answers_a_command_on_the_daemon` — served, with `command_handler(orchestrator)` set, a `question_answer` of an open question: 200, body `{said, events: [n]}`, and the log holds the `question.answered`; a `question_answer` with `answer: 7`: 200 with `error.kind == "invalid"`; no token: 401; with no handler set: `error.kind == "failed"` and detail "this daemon takes no commands".
- `lists_every_registered_session` — after registering `s-2` and `s-1`, `sessions()` is `["s-1", "s-2"]`; after `end_session("s-1")`, `["s-2"]`.
- `acts_only_on_the_task_in_scope` — FRK-1 `ready`, FRK-2 `assigned`: `tick_within` scoped to FRK-1 starts FRK-1's plan session and FRK-2 stays `assigned`.
- `passes_over_a_session_of_a_purpose_out_of_scope` — FRK-1 `in_progress` and FRK-2 `assigned`, purposes `[Triage, Refine, Plan]`: the first tick moves FRK-2 to `in_progress`, the second is `Idle { why: "nothing on the board needs doing" }`, and the adapter started nothing.
- `says_whether_it_was_stopped` — `is_stopped()` false, then true after `stop()`.
- `reads_an_issue_with_gh` — `FakeGh` answering `view` with `{"title":"T","body":"B","url":"u"}`: `Issue { T, B, u }` and one call `issue view <url> --json title,body,url`; exit 1 with stderr `not found`: `Failed` containing `not found`.

- [ ] `feat(runtime): take the human's commands on the daemon, and tick within a scope`

### Task 3: the human's commands from any terminal

Files: `lib.rs`, `main.rs`, `ids.rs`, `daemon_client.rs`, `hook.rs`, `start.rs` (the lock, `NoSessions`, the command orchestrator), `human.rs`, the three existing test files, `crates/cli/tests/human.rs`, `docs/SPEC.md` (5.7)

- Every phase 2 and step 07 command line test passes on `CliIo::new`.
- `hands_out_distinct_version_4_uuids` (`ids.rs`) — a thousand ids are distinct, each 36 characters, character 14 is `4` and character 19 one of `8`, `9`, `a`, `b`.
- `answers_a_question_with_no_run_going` — `question.asked` at seq n on FRK-1: `farik answer <n> No, one line.` exits 0 printing `handle`'s `said`, and the log holds `question.answered { answer: "No, one line.", answered_by: human }`; with `--json` a second answer's refusal is `{"error": ..}` on stderr starting `already_answered`.
- `refuses_with_the_words_of_handle` — `farik approve FRK-1` on a draft: exit 1, stderr starts `farik: not_awaiting_approval`; `farik answer 999 Yes`: stderr contains `999` and `is not in this project`.
- `cancels_and_resolves_for_the_human` — `farik cancel FRK-1 Not needed any more`: `cancelled`, the move's `reason` "Not needed any more"; an escalated FRK-2 with `farik resolve FRK-2 refining Split it by page.`: `escalation.resolved { to: refining, message: "Split it by page." }`; `farik resolve FRK-2 finished x`: exit 2.
- `sends_a_command_to_the_running_farik` — a live run: `farik answer <n> Yes.` prints "handled by the run", the handler got `QuestionAnswer { n, "Yes." }`, and the log holds no `question.answered`.
- `stops_only_a_running_farik` — with no run: `farik stop` exits 1 with "no farik run is running in this project"; with a live run and a `session.started` for FRK-1 in session `s-1` with no end: `farik stop` sends `RunStop`, `farik stop FRK-1` sends `SessionStop { s-1 }`, `farik stop s-9` sends `SessionStop { s-9 }`, and `farik stop FRK-2` exits 1 with "FRK-2 has no session running".
- `refuses_while_a_run_is_starting` — the lock held and no `daemon.json`: exit 1, "a farik run is starting".

- [ ] `feat(cli): send the human's commands to the running farik, or handle them here`

### Task 4: farik run and farik plan

Files: `start.rs`, `run.rs`, `waiting.rs`, `lib.rs`, `crates/cli/tests/running.rs`, `docs/SPEC.md` (3, 8.2, 8.3)

- `refuses_to_run_without_a_credential` — `Engine::Claude`, `env` with `PATH` only: exit 1, stderr names both variables; no `daemon.json`; the run lock is free; the log gained nothing.
- `refuses_a_claude_code_older_than_the_minimum` — `ANTHROPIC_API_KEY=sk-test` and `PATH` a directory holding a `claude` script printing `2.1.200 (Claude Code)`: exit 1, stderr contains `2.1.272`; no `daemon.json`.
- `names_the_credential_it_chose_and_never_prints_it` — the script printing `2.1.280 (Claude Code)`, an empty board: with both variables set, exit 0, stdout contains `credential: ANTHROPIC_API_KEY (an API key)` and neither value appears in stdout or stderr; with only `CLAUDE_CODE_OAUTH_TOKEN`, `credential: CLAUDE_CODE_OAUTH_TOKEN (a subscription token)`.
- `warns_on_every_start_in_no_sandbox_mode` — recorded, nothing on the board: with `sandbox: none` two runs each print the warning to stderr, containing `~/.git-credentials` and `a git hidden in a script`; with no settings file, none.
- `refuses_a_second_run_in_the_same_project` — the lock held by the test: exit 1, "another farik process is driving this project".
- `runs_a_task_to_acceptance_and_says_what_waits` — a team; FRK-1 filed by `farik task create` and sized by `farik triage FRK-1 small`; recorded `refine_writes_task_frk_1`, `plan_assigns_frk_1`, `implement_finishes_frk_1`, `review_writes_note`, `accept_frk_1`: exit 0; stdout holds one `FRK-1: ` line per acting tick, then `idle: nothing on the board needs doing`, then `FRK-1 waits for you to integrate it: farik integrate FRK-1`; five `session.started` (refine, plan, implement, verify, verify); no `daemon.json` afterwards. Then `farik integrate FRK-1` exits 0 with `merged` in stdout, and `main` holds `done.txt`.
- `recovers_before_the_first_tick` — a `session.started` with no end seeded: stdout contains `recovered: sessions interrupted 1,` and the log holds its `session.ended { reason: aborted }`.
- `stops_after_the_session_then_aborts_it_on_ctrl_c` — FRK-1 sized small, `UsageThenWaitAdapter` waiting in the refine session, `run_cli` on a thread: one interrupt prints "stopping after the current session" and `abort` is not called; a second calls `abort` once; exit 130; the log holds `session.ended { aborted }` and no `escalation.raised`; FRK-1 is `refining`; no `daemon.json`; the lock is free.
- `plans_without_starting_work` — FRK-1 `ready`, recorded `plan_assigns_frk_1`: `farik plan` exits 0, FRK-1 is `in_progress`, and the log holds exactly one `session.started`, purpose `plan`.
- `lists_what_waits_on_the_human` — an open question on FRK-1, an epic FRK-2 awaiting approval, and FRK-3 escalated for `iterations`: stdout after `idle:` holds, in this order, `question <n> on FRK-1 from pm: `, `  farik answer <n> <your answer>`, `FRK-2 awaits your approval: farik approve FRK-2`, `FRK-3 is escalated (iterations)`; `--json`'s last line has three `waiting_on_you` entries.

- [ ] `feat(cli): run and plan the team from the terminal`

### Task 5: a task's whole story, and a task under an epic

Files: `show.rs`, `task.rs`, `lib.rs`, `crates/cli/tests/reading.rs`, `crates/cli/tests/commands.rs`, `docs/SPEC.md` (F3)

- `shows_a_tasks_events_cost_and_children` — epic FRK-1 `in_progress` with child FRK-2 and a `cost.recorded` of $0.50 on FRK-1: stdout contains `request.triaged — large by human: `, `cost: $0.50 of $5.00; sessions: 1;`, `children`, `  FRK-2 draft `; JSON `cost.usd == 0.5` and `children[0].task_id == "FRK-2"`.
- `shows_a_tasks_diff_before_and_after_integration` — `farik/FRK-1` committing `done.txt`: `--diff` contains `+++ b/done.txt`; after a merge into `main` and its `task.integrated`, the same; an epic: exit 1, "is an epic and has no branch"; a draft: exit 1, "has no branch yet".
- `files_a_task_under_an_epic_in_progress` — `farik task create child.yaml --parent FRK-1`: FRK-2 with `parent: FRK-1` and `kind: task`, `request.triaged { small, triaged_by: human, reason: "a task of FRK-1" }`, stdout `FRK-2 filed as a task of FRK-1: `.
- `refuses_a_parent_that_is_not_an_epic_in_progress` — a standalone parent: "is not an epic"; a `ready` epic: contains "the epic is ready and its tasks are written while it is in progress"; either way nothing appended and no FRK-2 file.

- [ ] `feat(cli): show a task's events, cost, children, and diff, and file a task under an epic`

### Task 6: farik contract new

Files: `contract_new.rs`, `lib.rs`, `crates/cli/tests/contract_new.rs`, `docs/SPEC.md` (5.13), `docs/plans/project-plan.md`

- `writes_a_small_request_with_the_product_manager` — a team; `--brief "Add done.txt and a check that it exists." --size small`, recorded `refine_writes_task_frk_1`: exit 0; stdout in order `FRK-1 filed as a draft request`, a `request.triaged — small by human` line, a `contract.written` line, `readiness: passed`, the contract's `C1`; FRK-1 `ready`; one `session.started`, purpose `refine`.
- `asks_its_questions_at_the_terminal` — no `--size`; recorded `triage_frk_1_large`, `refine_asks_frk_1`, `refine_writes_epic_frk_1`; stdin `"\nNo, one line.\n"`: stdout contains `large by pm: A file and the check that it exists.`, `question <n> from pm: Should done.txt be empty?`, `answer> ` twice, `readiness: passed the structural checks`, `FRK-1 awaits your approval`; the log holds `question.answered { answer: "No, one line." }`; FRK-1 is escalated awaiting approval.
- `leaves_the_question_open_when_input_ends` — the same with empty stdin: exit 0, stdout contains `the question stays open: farik answer <n>`; no `question.answered`; FRK-1 `refining`.
- `locks_the_contract_it_wrote_when_asked` — the first test with `--lock`: the last event is `contract.locked { locked_by: human }` and the file says `locked: true`.
- `refuses_a_brief_too_short_to_be_an_intent` — `--brief "Fix it"`: exit 1, no event, no contract file, no `daemon.json`.
- `hands_the_request_to_a_running_farik` — a live run, `--size large`: FRK-1 filed and sized, stdout contains "takes it from here", no `session.started`.
- `reads_a_request_from_an_issue` (unit) — `brief_from_issue` of `Issue { "Add done.txt", "It should exist.", "https://github.com/o/r/issues/3" }` is `("Add done.txt", "Add done.txt\n\nIt should exist.\n\nFrom https://github.com/o/r/issues/3")`.
- `builds_a_request_the_store_files` (unit) — `request_from_brief("Add done.txt", <a 40-character brief>, 5.0)` passes `validate_contract` once given an id and a status, with `allowed_paths` and each placeholder text `(placeholder, for the Product Manager to write)`; a 19-character brief and a 2-character title are `Err`.

- [ ] `feat(cli): write a contract with the product manager at the terminal`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```

## Open for the founder

- What `farik plan` means. Decided here as a run that triages, refines, breaks down, and assigns but implements and verifies nothing, because neither the spec nor the project plan defines it; the alternative is a dry run that starts nothing. Execution does not wait on this: a reversal is Task 4's `farik plan` alone.
