//! The `farik` command line: the commands the first release ships, and the shape they share.
//!
//! Everything a command does is a function in one of the modules below, taking what it needs and
//! returning either a `Report` — the lines a person reads and the JSON a script reads — or the one
//! sentence that says why Farik would not do it. `run_cli` parses the arguments, picks the
//! function, and writes the answer to the streams it was given, so a test runs a command without
//! spawning a process (`docs/SPEC.md` sections 5.11, 5.16, F2, F3).

/// The lifecycle, one line per task.
pub mod board;
/// Taking a contract from the team, and giving it back.
pub mod contract;
/// One exchange with the daemon a `daemon.json` names.
pub mod daemon_client;
/// Everything this project disagrees with itself about.
pub mod doctor;
/// The hooks Claude Code runs around a tool call, carried to the daemon and back.
pub mod hook;
/// The human's commands, from any terminal.
#[cfg(unix)]
pub mod human;
/// The wall clock and the session ids the binary hands the command line.
pub mod ids;
/// Making a repository a Farik project.
pub mod init;
/// The event log, filtered and exported.
pub mod log;
/// The project a command runs against.
pub mod project;
/// The governor's refusals in words.
pub mod refusal;
/// `farik run` and `farik plan`.
#[cfg(unix)]
mod run;
/// One contract, and what happened to it.
pub mod show;
/// Who drives a project, and how a command reaches it.
#[cfg(unix)]
mod start;
/// Filing a request.
pub mod task;
/// The team's rules and its criterion library.
pub mod team;
/// Sizing a request.
pub mod triage;
/// What waits on the human.
#[cfg(unix)]
mod waiting;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use farik_core::contract::{TaskId, TaskStatus};
use farik_protocol::clock::{Clock, IdSource, SequentialIds};
use farik_protocol::command::{AcceptSubject, Command};
#[cfg(unix)]
use farik_runtime::RuntimeAdapter;
#[cfg(unix)]
use farik_runtime::daemon::DaemonState;
use serde_json::{Value, json};

pub use project::{Project, open_project};

/// What makes the adapter a driving process's sessions start through, given its daemon: the
/// recorded adapter over the daemon's tools in a test.
#[cfg(unix)]
pub type AdapterFactory = Arc<dyn Fn(Arc<DaemonState>) -> Arc<dyn RuntimeAdapter> + Send + Sync>;

/// What runs a driving process's sessions.
#[cfg(unix)]
pub enum Engine {
    /// The Claude Code program, with the credential in the environment: what `farik` runs.
    Claude,
    /// The adapter the factory makes: what a test runs. Nothing a user sets makes `farik` replay.
    Given(AdapterFactory),
}

/// Where a driving process hears that the person wants it to stop.
pub enum Interrupts {
    /// Ctrl-C at the terminal.
    CtrlC,
    /// One `()` per interrupt, from a test; a closed channel never fires.
    Channel(tokio::sync::mpsc::UnboundedReceiver<()>),
}

/// Everything the command line needs from outside itself: where to write, where it is run, and what
/// time it is.
///
/// The streams are boxed writers rather than `std::io::stdout` so that a test reads what a command
/// printed, and they borrow for `'a` so that the buffer a test reads back is the test's own
/// `Vec<u8>`; the clock is injected for the same reason an event's `recorded_at` is (`docs/SPEC.md`
/// section 8.4).
pub struct CliIo<'a> {
    /// What a command reads: the hook commands' JSON from Claude Code, and the answers
    /// `farik contract new` asks for. Owned, so that one reader thread can hold it.
    pub stdin: Box<dyn Read + Send>,
    /// What a person or a script asked for.
    pub stdout: Box<dyn Write + 'a>,
    /// Why Farik would not do something, and warnings.
    pub stderr: Box<dyn Write + 'a>,
    /// Where the command was run, which is how the project is found.
    pub cwd: PathBuf,
    /// The time every event this run records is stamped with.
    pub clock: Arc<dyn Clock + Send + Sync>,
    /// The environment: the credential, `PATH`, and what a session is given. A test passes its
    /// own, since setting a process's variable is `unsafe`.
    pub env: BTreeMap<String, String>,
    /// What runs a driving process's sessions.
    #[cfg(unix)]
    pub engine: Engine,
    /// Where a driving process hears Ctrl-C.
    pub interrupts: Interrupts,
    /// Where session ids come from.
    pub session_ids: Arc<dyn IdSource + Send + Sync>,
}

impl<'a> CliIo<'a> {
    /// A harness writing to `stdout` and `stderr`, run in `cwd` at `clock`'s time, with nothing on
    /// standard input, an empty environment, the Claude Code engine, interrupts that never come,
    /// and session ids `session-1`, `session-2`, and so on.
    #[must_use]
    pub fn new(
        cwd: PathBuf,
        stdout: Box<dyn Write + 'a>,
        stderr: Box<dyn Write + 'a>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> CliIo<'a> {
        let (_, never) = tokio::sync::mpsc::unbounded_channel();
        CliIo {
            stdin: Box::new(std::io::empty()),
            stdout,
            stderr,
            cwd,
            clock,
            env: BTreeMap::new(),
            #[cfg(unix)]
            engine: Engine::Claude,
            interrupts: Interrupts::Channel(never),
            session_ids: Arc::new(SequentialIds::new()),
        }
    }
}

/// What a command did: the lines a person reads, and the same thing as JSON for `--json`.
///
/// Both are built whatever the flag says, because a command that told a person one thing and a
/// script another would be two commands.
pub struct Report {
    /// One line per thing that happened, in the order it happened.
    pub lines: Vec<String>,
    /// The same, as an object a script can read.
    pub json: Value,
    /// When this is set, `--json` prints one of these per line instead of `json`. `farik log` is the
    /// one command that uses it: F11 calls the log an export, and an export read a line at a time is
    /// what survives being large.
    pub json_lines: Option<Vec<Value>>,
}

/// The command ran and did what it said.
const OK: i32 = 0;
/// Farik refused: a project that is not there, a file that is not a contract, a rule in section 5.
const REFUSED: i32 = 1;
/// The command line itself was wrong. Two is what clap exits with, and the shape of a wrong
/// invocation is not ours to redefine.
const MISUSE: i32 = 2;

/// Who the command line acts as. Every command here is run by a person at a terminal, so every
/// event it records says `human` wrote it; an agent's events come from the runtime (phase 3).
pub const HUMAN: &str = "human";

#[derive(Parser)]
#[command(
    name = "farik",
    version,
    about = "An operating system for a small team of AI agents.",
    long_about = None
)]
struct Cli {
    /// Print what happened as JSON rather than as lines a person reads.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Make the repository this is run in a Farik project.
    Init,
    /// Work with one task.
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },
    /// Record how big a request is, or overrule the triage that did (5.16).
    Triage {
        /// The request being sized.
        task_id: String,
        /// Large becomes an epic; small becomes one standalone task.
        size: SizeArgument,
        /// Why, in your own words. The log keeps it.
        #[arg(long)]
        reason: String,
    },
    /// Take a contract from the team, or give it back (5.11).
    Contract {
        #[command(subcommand)]
        command: ContractCommands,
    },
    /// Show the lifecycle, one line per task.
    Board,
    /// Show the event log, oldest first. With --json, one JSON object per line.
    Log {
        /// Only events about this task.
        #[arg(long)]
        task: Option<String>,
        /// Only events of this kind.
        #[arg(long)]
        kind: Option<String>,
        /// At most this many, from the oldest up.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Say where the files and the log disagree, and what else this project got wrong.
    Doctor,
    /// Show the team's rules (5.12).
    Rules {
        #[command(subcommand)]
        command: RulesCommands,
    },
    /// Show the criterion library (5.13).
    Criteria {
        #[command(subcommand)]
        command: CriteriaCommands,
    },
    /// Carry a Claude Code hook's JSON to the daemon and its answer back (8.2).
    Hook {
        #[command(subcommand)]
        command: HookCommands,
    },
    /// Drive the team until nothing needs doing, a stop, or Ctrl-C (8.2).
    Run,
    /// Plan without doing: triage, contracts, breakdowns, and assignments, and no work (8.2).
    Plan,
    /// Approve a contract that awaits your approval (5.16).
    Approve {
        /// The task whose contract it is.
        task_id: String,
    },
    /// Accept a result that waits for you (5.4).
    Accept {
        /// The task whose result it is.
        task_id: String,
        /// Your review, which an epic's result needs.
        #[arg(long)]
        message: Option<String>,
    },
    /// Answer a question an agent asked (5.7).
    Answer {
        /// The question's number, as farik run prints it.
        question_id: u64,
        /// Your answer.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        answer: Vec<String>,
    },
    /// Integrate an accepted task now (5.14).
    Integrate {
        /// The accepted task.
        task_id: String,
    },
    /// Resolve an escalation by moving the task, with a message for the next session (5.7).
    Resolve {
        /// The escalated task.
        task_id: String,
        /// Where it goes.
        status: StatusArgument,
        /// What the next session about it is told.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        message: Vec<String>,
    },
    /// Cancel a task (5.2).
    Cancel {
        /// The task.
        task_id: String,
        /// Why.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        reason: Vec<String>,
    },
    /// Stop the process driving this project after its session, or stop one session now (5.2).
    Stop {
        /// A session id, or a task whose running session to stop.
        target: Option<String>,
    },
}

/// A status, as a person types it: the wire's spelling.
#[derive(Clone, Copy, ValueEnum)]
#[value(rename_all = "snake_case")]
enum StatusArgument {
    Draft,
    Refining,
    Ready,
    Assigned,
    InProgress,
    Blocked,
    Verifying,
    Rejected,
    Accepted,
    Escalated,
    Cancelled,
}

impl From<StatusArgument> for TaskStatus {
    fn from(status: StatusArgument) -> Self {
        match status {
            StatusArgument::Draft => Self::Draft,
            StatusArgument::Refining => Self::Refining,
            StatusArgument::Ready => Self::Ready,
            StatusArgument::Assigned => Self::Assigned,
            StatusArgument::InProgress => Self::InProgress,
            StatusArgument::Blocked => Self::Blocked,
            StatusArgument::Verifying => Self::Verifying,
            StatusArgument::Rejected => Self::Rejected,
            StatusArgument::Accepted => Self::Accepted,
            StatusArgument::Escalated => Self::Escalated,
            StatusArgument::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Subcommand)]
enum HookCommands {
    /// Ask the daemon whether a tool call may go ahead, and print its answer. Denies when it
    /// cannot ask.
    PreToolUse {
        /// The daemon's `daemon.json`.
        #[arg(long)]
        daemon: PathBuf,
    },
    /// Tell the daemon what a tool call returned. Prints nothing.
    PostToolUse {
        /// The daemon's `daemon.json`.
        #[arg(long)]
        daemon: PathBuf,
    },
}

#[derive(Subcommand)]
enum RulesCommands {
    /// Print the rules every command and every path check is held to.
    Show,
}

#[derive(Subcommand)]
enum CriteriaCommands {
    /// Print every criterion a contract may refer to by name.
    List,
}

#[derive(Subcommand)]
enum TaskCommands {
    /// File a contract as a draft request, or as a task of an epic in progress.
    Create {
        /// The YAML contract to file. Farik assigns the id.
        file: PathBuf,
        /// The epic the task is filed under (5.16).
        #[arg(long)]
        parent: Option<String>,
    },
    /// Show one contract and what happened to it.
    Show {
        /// The task to show.
        task_id: String,
        /// Also show its branch's diff.
        #[arg(long)]
        diff: bool,
    },
}

#[derive(Subcommand)]
enum ContractCommands {
    /// Take the contract: from now on it is yours, and agents may only record results and notes.
    Lock {
        /// The task whose contract it is.
        task_id: String,
    },
    /// Give the contract back to the team.
    Unlock {
        /// The task whose contract it is.
        task_id: String,
    },
}

/// How big triage found a request, as a person types it.
#[derive(Clone, Copy, ValueEnum)]
enum SizeArgument {
    /// A large request, which becomes an epic.
    Large,
    /// A small request, which becomes one standalone task.
    Small,
}

impl From<SizeArgument> for farik_protocol::command::RequestSize {
    fn from(size: SizeArgument) -> Self {
        match size {
            SizeArgument::Large => Self::Large,
            SizeArgument::Small => Self::Small,
        }
    }
}

/// Runs one command and returns the code the process should exit with: 0 when it did what it said,
/// 1 when Farik refused, 2 when the command line itself was wrong.
///
/// `args` is the whole invocation, program name first, as `std::env::args` gives it.
#[must_use]
pub fn run_cli(args: &[String], io: &mut CliIo<'_>) -> i32 {
    let parsed = match Cli::try_parse_from(args) {
        Ok(parsed) => parsed,
        Err(error) => return usage(&error, io),
    };
    if let Commands::Hook { command } = &parsed.command {
        return match command {
            HookCommands::PreToolUse { daemon } => hook::pre_tool_use(&io.cwd.join(daemon), io),
            HookCommands::PostToolUse { daemon } => hook::post_tool_use(&io.cwd.join(daemon), io),
        };
    }
    let now = io.clock.now();
    if let Commands::Run | Commands::Plan = &parsed.command {
        return drive(&parsed.command, parsed.json, io);
    }
    let outcome = match &parsed.command {
        Commands::Init => init::init(&io.cwd, now),
        Commands::Task {
            command: TaskCommands::Create { file, parent },
        } => open_project(&io.cwd, now)
            .and_then(|project| task::create(&project, &io.cwd, file, parent.as_deref(), now)),
        Commands::Triage {
            task_id,
            size,
            reason,
        } => open_project(&io.cwd, now).and_then(|project| {
            here_or_sent(
                &project,
                || triage::triage(&project, task_id, (*size).into(), reason, now),
                || triage::command_of(task_id, (*size).into(), reason),
            )
        }),
        Commands::Contract {
            command: ContractCommands::Lock { task_id },
        } => open_project(&io.cwd, now).and_then(|project| {
            here_or_sent(
                &project,
                || contract::hold(&project, task_id, true, now),
                || {
                    Ok(Command::ContractLock {
                        task_id: task(task_id)?,
                    })
                },
            )
        }),
        Commands::Contract {
            command: ContractCommands::Unlock { task_id },
        } => open_project(&io.cwd, now).and_then(|project| {
            here_or_sent(
                &project,
                || contract::hold(&project, task_id, false, now),
                || {
                    Ok(Command::ContractUnlock {
                        task_id: task(task_id)?,
                    })
                },
            )
        }),
        Commands::Approve { .. }
        | Commands::Accept { .. }
        | Commands::Answer { .. }
        | Commands::Integrate { .. }
        | Commands::Resolve { .. }
        | Commands::Cancel { .. } => open_project(&io.cwd, now).and_then(|project| {
            let (name, command) = humans(&parsed.command)?;
            human_command(&project, command, name, io)
        }),
        Commands::Stop { target } => {
            open_project(&io.cwd, now).and_then(|project| stop(&project, target.as_deref()))
        }
        Commands::Task {
            command: TaskCommands::Show { task_id, diff },
        } => open_project(&io.cwd, now).and_then(|project| show::show(&project, task_id, *diff)),
        Commands::Board => open_project(&io.cwd, now).and_then(|project| board::board(&project)),
        Commands::Log { task, kind, limit } => open_project(&io.cwd, now)
            .and_then(|project| log::log(&project, task.as_ref(), kind.as_ref(), *limit)),
        Commands::Doctor => {
            let found =
                open_project(&io.cwd, now).and_then(|project| doctor::doctor(&project, now));
            // `doctor` exits 1 when it found something, so that a script can gate on it. That is not
            // a refusal, so the report goes to stdout as any other command's does.
            let told = match &found {
                Ok(report) => doctor::found_something(report),
                Err(_) => false,
            };
            let code = report(found, parsed.json, io);
            return if told && code == OK { REFUSED } else { code };
        }
        Commands::Rules {
            command: RulesCommands::Show,
        } => open_project(&io.cwd, now).and_then(|project| team::rules(&project)),
        Commands::Criteria {
            command: CriteriaCommands::List,
        } => open_project(&io.cwd, now).and_then(|project| team::criteria(&project)),
        Commands::Hook { .. } | Commands::Run | Commands::Plan => {
            unreachable!("a hook, run, or plan command returned above")
        }
    };
    report(outcome, parsed.json, io)
}

/// A task id a person typed.
fn task(task_id: &str) -> Result<TaskId, String> {
    task_id
        .parse()
        .map_err(|error| format!("{task_id} is not a task id: {error}"))
}

/// The human's command a subcommand stands for, and its name as typed.
fn humans(command: &Commands) -> Result<(&'static str, Command), String> {
    Ok(match command {
        Commands::Approve { task_id } => (
            "approve",
            Command::HumanAccept {
                task_id: task(task_id)?,
                subject: AcceptSubject::Contract,
                message: None,
            },
        ),
        Commands::Accept { task_id, message } => (
            "accept",
            Command::HumanAccept {
                task_id: task(task_id)?,
                subject: AcceptSubject::Result,
                message: message.clone(),
            },
        ),
        Commands::Answer {
            question_id,
            answer,
        } => (
            "answer",
            Command::QuestionAnswer {
                question_id: *question_id,
                answer: answer.join(" "),
            },
        ),
        Commands::Integrate { task_id } => (
            "integrate",
            Command::TaskIntegrate {
                task_id: task(task_id)?,
            },
        ),
        Commands::Resolve {
            task_id,
            status,
            message,
        } => (
            "resolve",
            Command::EscalationResolve {
                task_id: task(task_id)?,
                to: (*status).into(),
                message: message.join(" "),
            },
        ),
        Commands::Cancel { task_id, reason } => (
            "cancel",
            Command::TaskTransition {
                task_id: task(task_id)?,
                to: TaskStatus::Cancelled,
                reason: reason.join(" "),
            },
        ),
        _ => return Err("this is not one of the human's commands".to_string()),
    })
}

#[cfg(unix)]
fn here_or_sent(
    project: &Project,
    here: impl FnOnce() -> Result<Report, String>,
    command: impl FnOnce() -> Result<Command, String>,
) -> Result<Report, String> {
    start::here_or_sent(project, here, command)
}

#[cfg(not(unix))]
fn here_or_sent(
    _project: &Project,
    here: impl FnOnce() -> Result<Report, String>,
    _command: impl FnOnce() -> Result<Command, String>,
) -> Result<Report, String> {
    here()
}

#[cfg(unix)]
fn human_command(
    project: &Project,
    command: Command,
    name: &str,
    io: &CliIo<'_>,
) -> Result<Report, String> {
    human::human(project, command, name, io)
}

#[cfg(not(unix))]
fn human_command(
    _project: &Project,
    _command: Command,
    name: &str,
    _io: &CliIo<'_>,
) -> Result<Report, String> {
    Err(format!(
        "farik {name} needs the daemon, which runs on Linux and macOS"
    ))
}

/// `farik run` or `farik plan`, which write as they go and answer their own exit code.
#[cfg(unix)]
fn drive(command: &Commands, as_json: bool, io: &mut CliIo<'_>) -> i32 {
    use farik_runtime::orchestrator::TickRules;

    let project = match open_project(&io.cwd, io.clock.now()) {
        Ok(project) => project,
        Err(error) => return run::refuse(io, as_json, &error),
    };
    let rules = match command {
        Commands::Plan => TickRules::Planning,
        _ => TickRules::All,
    };
    run::drive(&project, rules, io, as_json)
}

#[cfg(not(unix))]
fn drive(_command: &Commands, as_json: bool, io: &mut CliIo<'_>) -> i32 {
    report(
        Err("farik run needs the daemon, which runs on Linux and macOS".to_string()),
        as_json,
        io,
    )
}

#[cfg(unix)]
fn stop(project: &Project, target: Option<&str>) -> Result<Report, String> {
    human::stop(project, target)
}

#[cfg(not(unix))]
fn stop(_project: &Project, _target: Option<&str>) -> Result<Report, String> {
    Err("farik stop needs the daemon, which runs on Linux and macOS".to_string())
}

/// Writes what the command did, or why it would not, and answers with the exit code.
fn report(outcome: Result<Report, String>, as_json: bool, io: &mut CliIo<'_>) -> i32 {
    match outcome {
        Ok(done) => {
            if as_json {
                match &done.json_lines {
                    Some(lines) => {
                        for line in lines {
                            say(&mut io.stdout, &format!("{line}"));
                        }
                    }
                    None => say(&mut io.stdout, &format!("{}", done.json)),
                }
            } else {
                for line in &done.lines {
                    say(&mut io.stdout, line);
                }
            }
            OK
        }
        Err(refusal) => {
            if as_json {
                say(&mut io.stderr, &format!("{}", json!({ "error": refusal })));
            } else {
                say(&mut io.stderr, &format!("farik: {refusal}"));
            }
            REFUSED
        }
    }
}

/// What clap has to say about an invocation it could not use. `--help` and `--version` are the two
/// it reports as errors and a person asked for, so they go to stdout and the run succeeded.
fn usage(error: &clap::Error, io: &mut CliIo<'_>) -> i32 {
    let text = error.render().to_string();
    if error.use_stderr() {
        say(&mut io.stderr, text.trim_end());
        MISUSE
    } else {
        say(&mut io.stdout, text.trim_end());
        OK
    }
}

/// One line to a stream. A stream that cannot be written to is a pipe that was closed, which is not
/// something to tell the person about on the stream that just closed.
fn say(stream: &mut Box<dyn Write + '_>, line: &str) {
    let _ = writeln!(stream, "{line}");
}
