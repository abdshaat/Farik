//! The `farik` command line: the commands the first release ships, and the shape they share.
//!
//! Everything a command does is a function in one of the modules below, taking what it needs and
//! returning either a `Report` — the lines a person reads and the JSON a script reads — or the one
//! sentence that says why Farik would not do it. `run_cli` parses the arguments, picks the
//! function, and writes the answer to the streams it was given, so a test runs a command without
//! spawning a process (`docs/SPEC.md` sections 5.11, 5.16, F2, F3).

/// The lifecycle, one line per task.
pub mod board;
/// The team's channel.
pub mod channel;
/// The human's one-to-one chats.
pub mod chat;
/// `farik connect` and `farik disconnect`: one agent's MCP server, its keys kept by this process
/// and only names sent on (ADR 0030).
#[cfg(unix)]
mod connector;
/// `farik connector run` and `farik connector headers`: a custom connector's keys, from the
/// daemon to the server, never through a file (ADR 0030).
#[cfg(unix)]
pub mod connector_run;
/// Taking a contract from the team, and giving it back.
pub mod contract;
/// Writing a contract with the Product Manager at the terminal.
#[cfg(unix)]
mod contract_new;
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
pub mod marketing;
/// The harness metrics.
pub mod metrics;
/// Text as a terminal may be given it.
pub mod printable;
/// The project a command runs against.
pub mod project;
/// The governor's refusals in words.
pub mod refusal;
/// `farik run` and `farik plan`.
#[cfg(unix)]
mod run;
/// `farik serve`.
#[cfg(unix)]
mod serve;
/// What `farik serve` does for the first-run wizard before there is a project.
#[cfg(unix)]
mod setup;
/// One contract, and what happened to it.
pub mod show;

#[cfg(unix)]
mod skill;
/// One sprint, and how it went.
pub mod sprint;
/// Who drives a project, and how a command reaches it.
#[cfg(unix)]
mod start;
/// What outlives a project: the state folder.
#[cfg(unix)]
mod state;
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
use farik_runtime::connectors::{
    ConnectorSecretStores, ConnectorSecrets, KeychainConnectorSecrets, MemoryConnectorSecrets,
};
#[cfg(unix)]
use farik_runtime::credential::{CredentialStore, FileStore, KeychainStore, MemoryStore};
#[cfg(unix)]
use farik_runtime::daemon::DaemonState;
use farik_runtime::sleep::Sleeper;
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
    /// Whether `stdin` is a terminal, where a key is read with its echo off: `false` here, and
    /// what the process's standard input is in `main`.
    pub stdin_is_terminal: bool,
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
    /// What a driving process waits on while every agent with work is asleep: the machine's timer
    /// over `clock` when `None`, a test's own otherwise.
    pub sleeper: Option<Arc<dyn Sleeper>>,
    /// What `farik serve` opens its link with: nothing here, and the system's opener in `main`.
    pub open_url: Opener,
    /// Where the model credential is kept: one store in memory here, so that no test touches a
    /// real keychain, and the keychain then the file in `main`.
    #[cfg(unix)]
    pub credential_stores: CredentialStores,
    /// Where each role's kit comes from (ADR 0036): the shipped kits here and in `main`, which a
    /// test replaces with a fixture kit whose server is its own.
    pub kits: farik_runtime::KitSource,
    /// Where each agent's connector keys are kept (ADR 0030): in memory here, so that no test
    /// touches a real keychain, and the keychain then `connectors.json` in `main`.
    #[cfg(unix)]
    pub connector_secrets: Arc<dyn ConnectorSecrets>,
    /// Farik's own executable, which runs Farik's own connectors (ADR 0038): none here, so a
    /// test names the binary it built, and `std::env::current_exe()` in `main`.
    pub own_program: Option<PathBuf>,
    /// Whether `farik serve` lets a browser at `http://localhost:<port>` in without a code: the
    /// end-to-end server's `--preview` (step 12, D1). The release build has no such field.
    #[cfg(feature = "e2e")]
    pub admit_local_preview: bool,
    /// The image Docker's sandbox and the preview run in, in place of `SANDBOX_IMAGE`: the
    /// end-to-end server's `--sandbox-image`.
    #[cfg(feature = "e2e")]
    pub sandbox_image: Option<String>,
}

/// The places the model credential is kept, in the order they are tried.
#[cfg(unix)]
pub type CredentialStores = Arc<dyn Fn() -> Vec<Arc<dyn CredentialStore>> + Send + Sync>;

/// The computer's credential stores: its keychain when `keychain` is true, then
/// `credential.json` in the state folder of `env`, when there is one.
#[cfg(unix)]
#[must_use]
pub fn system_credential_stores(
    env: &BTreeMap<String, String>,
    keychain: bool,
) -> CredentialStores {
    let file = state::state_dir(env).map(|directory| directory.join("credential.json"));
    Arc::new(move || {
        let mut stores: Vec<Arc<dyn CredentialStore>> = Vec::new();
        if keychain {
            stores.push(Arc::new(KeychainStore));
        }
        if let Some(file) = &file {
            stores.push(Arc::new(FileStore::new(file.clone())));
        }
        stores
    })
}

/// The computer's connector key stores (ADR 0030): its keychain, then `connectors.json` in the
/// state folder of `env`, when there is one.
#[cfg(unix)]
#[must_use]
pub fn system_connector_secrets(env: &BTreeMap<String, String>) -> Arc<dyn ConnectorSecrets> {
    Arc::new(ConnectorSecretStores::new(
        Arc::new(KeychainConnectorSecrets::default()),
        state::state_dir(env).map(|directory| directory.join("connectors.json")),
    ))
}

/// Opens a link in a browser, or says why it could not.
pub type Opener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

impl<'a> CliIo<'a> {
    /// A harness writing to `stdout` and `stderr`, run in `cwd` at `clock`'s time, with nothing on
    /// standard input, an empty environment, the Claude Code engine, interrupts that never come,
    /// session ids `session-1`, `session-2`, and so on, and the machine's timer to wait on.
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
            stdin_is_terminal: false,
            stdout,
            stderr,
            cwd,
            clock,
            env: BTreeMap::new(),
            #[cfg(unix)]
            engine: Engine::Claude,
            interrupts: Interrupts::Channel(never),
            session_ids: Arc::new(SequentialIds::new()),
            sleeper: None,
            open_url: Arc::new(|_| Ok(())),
            #[cfg(unix)]
            credential_stores: {
                let memory: Arc<dyn CredentialStore> = Arc::new(MemoryStore::default());
                Arc::new(move || vec![Arc::clone(&memory)])
            },
            kits: Arc::new(farik_roles::load_kit),
            own_program: None,
            #[cfg(unix)]
            connector_secrets: Arc::new(MemoryConnectorSecrets::default()),
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
            #[cfg(feature = "e2e")]
            sandbox_image: None,
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
    /// Show the harness metrics over the whole project, or one sprint's, (F17).
    Metrics {
        /// Only this sprint's rows and costs.
        #[arg(long)]
        sprint: Option<String>,
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
    /// Start a custom connector, or fill its headers, with its keys from the daemon: what a
    /// session's `mcp.json` names (ADR 0030).
    #[cfg(unix)]
    Connector {
        #[command(subcommand)]
        command: ConnectorCommands,
    },
    /// Give one agent an MCP server, with its keys read from standard input, and label its tools
    /// (5.6, ADR 0030).
    #[cfg(unix)]
    #[command(group(clap::ArgGroup::new("start").required(false).args(["command", "url"])))]
    Connect {
        /// The agent's id.
        agent: String,
        /// The server's name: lower-case letters, digits and dashes.
        name: String,
        /// The program that starts the server.
        #[arg(long)]
        command: Option<String>,
        /// One argument to that program; repeat it for each.
        #[arg(long = "arg", allow_hyphen_values = true)]
        args: Vec<String>,
        /// The server's web address.
        #[arg(long)]
        url: Option<String>,
        /// A header, as 'Name: template', where {KEY} is a key's value; repeat it for each.
        #[arg(long = "header")]
        headers: Vec<String>,
        /// A key's name; its value is read from standard input. Repeat it for each.
        #[arg(long = "key")]
        keys: Vec<String>,
        /// Sign in to the server's service in your browser instead of giving a key (ADR 0033).
        #[arg(long, conflicts_with_all = ["keys", "command"])]
        sign_in: bool,
        /// The client the service's app registration gave Farik, when it offers no registration.
        #[arg(long)]
        client_id: Option<String>,
        /// The port that client's redirect address names (33418 when left out).
        #[arg(long)]
        callback_port: Option<u16>,
        /// What to ask the service for; repeat it for each. Left out, the service chooses.
        #[arg(long = "scope")]
        scopes: Vec<String>,
        /// A tool's label: `<tool>=network`, `<tool>=external_effect` or `<tool>=denied`. A tool
        /// left unlabelled is `external_effect`.
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// How many calls each sprint the agent makes of a kit service's spending tool without
        /// asking, `<tool>=<number>` from 0 to 1000; repeat it for each. Left out, the kit's number.
        #[arg(long = "allowance")]
        allowances: Vec<String>,
    },
    /// Take an MCP server from one agent, and delete its keys.
    #[cfg(unix)]
    Disconnect {
        /// The agent's id.
        agent: String,
        /// The server's name.
        name: String,
    },
    /// Give the team or one agent a skill in the Agent Skills format, read before it is added
    /// (6.7, ADR 0034).
    #[cfg(unix)]
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
    /// Drive the team until nothing needs doing, a stop, or Ctrl-C (8.2).
    Run,
    /// Drive the team and keep driving when the board is idle, until a stop or Ctrl-C (8.1).
    Serve {
        /// The port to listen on, instead of 7420 and the nine after it.
        #[arg(long)]
        port: Option<u16>,
        /// Print the link and do not open it in a browser.
        #[arg(long)]
        no_open: bool,
    },
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
    /// Send a result that waits for you back to its assignee, after its review (ADR 0024).
    SendBack {
        /// The task whose result it is.
        task_id: String,
        /// What is wrong, for the next attempt.
        message: String,
        /// A criterion the result fails; repeat it for each one.
        #[arg(long = "criterion")]
        criteria: Vec<String>,
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
    /// Allow or refuse one call an agent asked to make to a connector (5.7).
    Tool {
        #[command(subcommand)]
        command: ToolCommands,
    },
    /// Show the marketing plans, approve, send back or end one, and list, stop or allow the posts
    /// (6.5).
    Marketing {
        #[command(subcommand)]
        command: MarketingCommands,
    },
    /// Start, end, or show a sprint (5.5).
    Sprint {
        #[command(subcommand)]
        command: SprintCommands,
    },
    /// Pause the whole team: no rule runs and no session starts until `farik resume`.
    Pause,
    /// Resume a paused team.
    Resume,
    /// Say something in the team's channel; @<id> mentions an agent (5.9).
    Say {
        /// What you say.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        text: Vec<String>,
    },
    /// Say something to one agent in your one-to-one chat, or with no text show the chat (4.3).
    Chat {
        /// The agent's id.
        agent: String,
        /// What you say; nothing shows the chat, oldest first.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        text: Vec<String>,
    },
    /// Show the team's channel, oldest first (5.9).
    Channel {
        /// How many of the latest messages.
        #[arg(long, default_value_t = 50)]
        last: usize,
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

/// `--team` or `--agent <id>`: whose skill.
#[cfg(unix)]
#[derive(clap::Args)]
#[command(group(clap::ArgGroup::new("whom").required(true).args(["team", "agent"])))]
struct WhoseSkill {
    /// The team's skill.
    #[arg(long)]
    team: bool,
    /// One agent's skill, by id.
    #[arg(long)]
    agent: Option<String>,
}

#[cfg(unix)]
impl WhoseSkill {
    fn whom(&self) -> skill::Whom<'_> {
        self.agent
            .as_deref()
            .map_or(skill::Whom::Team, skill::Whom::Agent)
    }
}

/// What `farik skill` does.
#[cfg(unix)]
#[derive(Subcommand)]
enum SkillCommands {
    /// List the skills the team has, or with --agent what one agent has and how each stands.
    List {
        /// The agent's id.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Print every file of a skill, and the hash that confirms it.
    Show {
        /// The skill's name.
        name: String,
        #[command(flatten)]
        whose: WhoseSkill,
    },
    /// Read a skill folder, see all of it, and add it.
    Add {
        /// The folder: a SKILL.md and the files it refers to.
        folder: PathBuf,
        #[command(flatten)]
        whose: WhoseSkill,
        /// Add without asking, once you have read it.
        #[arg(long)]
        yes: bool,
        /// Let it replace a skill Farik ships with its name.
        #[arg(long)]
        replace: bool,
    },
    /// Remove a skill and its folder.
    Remove {
        /// The skill's name.
        name: String,
        #[command(flatten)]
        whose: WhoseSkill,
    },
    /// Confirm on this computer a skill as its folder is now: after a pull, a clone or an edit.
    Confirm {
        /// The skill's name.
        name: String,
        #[command(flatten)]
        whose: WhoseSkill,
        /// The Hash `farik skill show` printed: all of it, or its first 12 digits.
        hash: String,
        /// Let it replace a skill Farik ships with its name.
        #[arg(long)]
        replace: bool,
    },
}

/// Which session's server `farik connector` asks the daemon for.
#[derive(clap::Args)]
struct ConnectorAsk {
    /// The daemon's `daemon.json`.
    #[arg(long)]
    daemon: PathBuf,
    /// The session's id.
    #[arg(long)]
    session: String,
    /// The server's name.
    #[arg(long)]
    server: String,
}

#[derive(Subcommand)]
enum ConnectorCommands {
    /// Start the server with only its keys and the variables Farik keeps.
    Run(ConnectorAsk),
    /// Print the server's headers, filled with its keys, as one JSON object.
    Headers(ConnectorAsk),
    /// Look packages up in the open vulnerability database (used by the Architect's kit).
    Osv,
}

#[derive(Subcommand)]
enum SprintCommands {
    /// Start a sprint, which the team plans from the ready backlog.
    Start {
        /// What it may spend, in dollars. Left out, it has no budget of its own.
        #[arg(long)]
        budget: Option<f64>,
    },
    /// End the open sprint; its unfinished tasks leave it and keep their status.
    End,
    /// Show the sprint named, else the open one, else the latest.
    Show {
        /// The sprint, as S<n>.
        sprint_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum MarketingCommands {
    /// The Marketing Specialist's plans.
    Plan {
        #[command(subcommand)]
        command: PlanCommands,
    },
    /// The posts it writes: what goes out, and stopping or allowing one.
    Post {
        #[command(subcommand)]
        command: PostCommands,
    },
}

#[derive(Subcommand)]
enum PostCommands {
    /// List the posts going out, soonest first, then those that did not go out in the last day.
    List,
    /// Stop a post going out; one Buffer has is taken back from Buffer first.
    Stop {
        /// The post's number, as the list prints it.
        post: u64,
    },
    /// Allow a post the Marketing Specialist asked about, which is not in the plan.
    Send {
        /// The post's number, as farik run prints it.
        post: u64,
    },
    /// Do not allow a post the Marketing Specialist asked about.
    Decline {
        /// The post's number, as farik run prints it.
        post: u64,
        /// A note for the Marketing Specialist's next session.
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum PlanCommands {
    /// List the plans, newest first, or show the one named.
    Show {
        /// The plan, as MP-<n>.
        plan: Option<String>,
    },
    /// Approve a plan the Marketing Specialist proposed; it spends and posts only as it says.
    Approve {
        /// The plan, as MP-<n>.
        plan: String,
        /// A note for the Marketing Specialist's next session.
        #[arg(long)]
        note: Option<String>,
    },
    /// Send a plan back, with the reason the Marketing Specialist reads.
    Return {
        /// The plan, as MP-<n>.
        plan: String,
        /// Why; the Marketing Specialist answers it in its next version.
        #[arg(long)]
        reason: String,
    },
    /// End an approved plan now.
    End {
        /// The plan, as MP-<n>.
        plan: String,
        /// Why, when you say.
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum ToolCommands {
    /// Allow the call once, with exactly the input the agent asked with.
    Approve {
        /// The approval's number, as farik run prints it.
        approval: u64,
        /// A note for the agent's next session.
        #[arg(long)]
        note: Option<String>,
    },
    /// Refuse the call.
    Refuse {
        /// The approval's number, as farik run prints it.
        approval: u64,
        /// Why, for the agent's next session.
        #[arg(long)]
        note: Option<String>,
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
    /// File a request from a brief or an issue and write its contract with the Product Manager,
    /// answering its questions here (5.13).
    #[command(group(clap::ArgGroup::new("source").required(true).args(["brief", "from"])))]
    New {
        /// What you want, in your words: its first line is the title, the whole the intent.
        #[arg(long)]
        brief: Option<String>,
        /// An issue on the forge to file instead, read with gh.
        #[arg(long)]
        from: Option<String>,
        /// Size it now: large becomes an epic, small a standalone task (5.16).
        #[arg(long)]
        size: Option<SizeArgument>,
        /// Take the contract once the Product Manager has written it (5.11).
        #[arg(long)]
        lock: bool,
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
#[allow(clippy::too_many_lines, reason = "one arm per command")]
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
    #[cfg(unix)]
    if let Commands::Connector { command } = &parsed.command {
        return match command {
            ConnectorCommands::Run(ask) => {
                connector_run::run(&io.cwd.join(&ask.daemon), &ask.session, &ask.server, io)
            }
            ConnectorCommands::Headers(ask) => {
                connector_run::headers(&io.cwd.join(&ask.daemon), &ask.session, &ask.server, io)
            }
            ConnectorCommands::Osv => connector_run::osv(io),
        };
    }
    if parsed.json && matches!(parsed.command, Commands::Serve { .. }) {
        say(
            &mut io.stderr,
            "farik: farik serve prints lines for a person; --json is not available for it",
        );
        return MISUSE;
    }
    let now = io.clock.now();
    if let Commands::Run
    | Commands::Serve { .. }
    | Commands::Plan
    | Commands::Contract {
        command: ContractCommands::New { .. },
    } = &parsed.command
    {
        return drive(&parsed.command, parsed.json, io);
    }
    let outcome = match &parsed.command {
        Commands::Init => init::init(&io.cwd, now),
        Commands::Task {
            command: TaskCommands::Create { file, parent },
        } => open_project(&io.cwd, now)
            .and_then(|project| task::create(&project, io, file, parent.as_deref(), now)),
        Commands::Triage { .. }
        | Commands::Contract {
            command: ContractCommands::Lock { .. } | ContractCommands::Unlock { .. },
        } => open_project(&io.cwd, now)
            .and_then(|project| phase_two_write(&parsed.command, &project, now)),
        Commands::Chat { agent, text } if text.is_empty() => {
            open_project(&io.cwd, now).and_then(|project| chat::chat(&project, agent))
        }
        Commands::Approve { .. }
        | Commands::Chat { .. }
        | Commands::Accept { .. }
        | Commands::SendBack { .. }
        | Commands::Answer { .. }
        | Commands::Tool { .. }
        | Commands::Integrate { .. }
        | Commands::Resolve { .. }
        | Commands::Cancel { .. }
        | Commands::Say { .. }
        | Commands::Pause
        | Commands::Resume
        | Commands::Sprint {
            command: SprintCommands::Start { .. } | SprintCommands::End,
        } => open_project(&io.cwd, now).and_then(|project| {
            let (name, command) = humans(&parsed.command)?;
            human_command(&project, command, name, io)
        }),
        Commands::Marketing {
            command:
                MarketingCommands::Plan {
                    command: PlanCommands::Show { plan },
                },
        } => open_project(&io.cwd, now)
            .and_then(|project| marketing::show(&project, plan.as_deref(), now)),
        Commands::Marketing {
            command:
                MarketingCommands::Post {
                    command: PostCommands::List,
                },
        } => open_project(&io.cwd, now).and_then(|project| marketing::posts(&project, now)),
        Commands::Marketing { .. } => open_project(&io.cwd, now).and_then(|project| {
            let (name, command) = humans(&parsed.command)?;
            human_command(&project, command, name, io)
        }),
        Commands::Stop { target } => {
            open_project(&io.cwd, now).and_then(|project| stop(&project, target.as_deref()))
        }
        Commands::Task {
            command: TaskCommands::Show { task_id, diff },
        } => open_project(&io.cwd, now).and_then(|project| show::show(&project, task_id, *diff)),
        Commands::Sprint {
            command: SprintCommands::Show { sprint_id },
        } => open_project(&io.cwd, now)
            .and_then(|project| sprint::show(&project, sprint_id.as_deref())),
        Commands::Channel { last } => {
            open_project(&io.cwd, now).and_then(|project| channel::channel(&project, *last))
        }
        Commands::Board => open_project(&io.cwd, now).and_then(|project| board::board(&project)),
        Commands::Log { task, kind, limit } => open_project(&io.cwd, now)
            .and_then(|project| log::log(&project, task.as_ref(), kind.as_ref(), *limit)),
        Commands::Metrics { sprint } => open_project(&io.cwd, now)
            .and_then(|project| metrics::metrics(&project, sprint.as_deref())),
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
        #[cfg(unix)]
        Commands::Connect {
            agent,
            name,
            command,
            args,
            url,
            headers,
            keys,
            sign_in,
            client_id,
            callback_port,
            scopes,
            tags,
            allowances,
        } => open_project(&io.cwd, now).and_then(|project| {
            connector::connect(
                &project,
                &connector::Asked {
                    agent,
                    name,
                    command: command.as_deref(),
                    args,
                    url: url.as_deref(),
                    headers,
                    keys,
                    tags,
                    allowances,
                    sign_in: *sign_in,
                    client_id: client_id.as_deref(),
                    callback_port: *callback_port,
                    scopes,
                },
                io,
            )
        }),
        #[cfg(unix)]
        Commands::Disconnect { agent, name } => open_project(&io.cwd, now)
            .and_then(|project| connector::disconnect(&project, agent, name, io)),
        #[cfg(unix)]
        Commands::Skill { command } => {
            open_project(&io.cwd, now).and_then(|project| match command {
                SkillCommands::List { agent } => skill::list(&project, agent.as_deref(), io),
                SkillCommands::Show { name, whose } => skill::show(&project, name, &whose.whom()),
                SkillCommands::Add {
                    folder,
                    whose,
                    yes,
                    replace,
                } => skill::add(&project, folder, &whose.whom(), *yes, *replace, io),
                SkillCommands::Remove { name, whose } => {
                    skill::remove(&project, name, &whose.whom(), io)
                }
                SkillCommands::Confirm {
                    name,
                    whose,
                    hash,
                    replace,
                } => skill::confirm(&project, name, &whose.whom(), hash, *replace, io),
            })
        }
        #[cfg(unix)]
        Commands::Connector { .. } => unreachable!("a connector command returned above"),
        Commands::Hook { .. }
        | Commands::Run
        | Commands::Serve { .. }
        | Commands::Plan
        | Commands::Contract {
            command: ContractCommands::New { .. },
        } => unreachable!("a hook, run, plan, or contract new command returned above"),
    };
    report(outcome, parsed.json, io)
}

/// One of phase 2's writes, which reach the process driving the project when one does.
fn phase_two_write(
    command: &Commands,
    project: &Project,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Report, String> {
    match command {
        Commands::Triage {
            task_id,
            size,
            reason,
        } => here_or_sent(
            project,
            || triage::triage(project, task_id, (*size).into(), reason, now),
            || triage::command_of(task_id, (*size).into(), reason),
        ),
        Commands::Contract {
            command: ContractCommands::Lock { task_id },
        } => here_or_sent(
            project,
            || contract::hold(project, task_id, true, now),
            || {
                Ok(Command::ContractLock {
                    task_id: task(task_id)?,
                })
            },
        ),
        Commands::Contract {
            command: ContractCommands::Unlock { task_id },
        } => here_or_sent(
            project,
            || contract::hold(project, task_id, false, now),
            || {
                Ok(Command::ContractUnlock {
                    task_id: task(task_id)?,
                })
            },
        ),
        _ => Err("this is not one of phase 2's writes".to_string()),
    }
}

/// A task id a person typed.
pub(crate) fn task(task_id: &str) -> Result<TaskId, String> {
    task_id
        .parse()
        .map_err(|error| format!("{task_id} is not a task id: {error}"))
}

/// The human's command a subcommand stands for, and its name as typed.
#[allow(clippy::too_many_lines, reason = "one arm per command")]
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
        Commands::SendBack {
            task_id,
            message,
            criteria,
        } => (
            "send-back",
            Command::HumanSendBack {
                task_id: task(task_id)?,
                subject: AcceptSubject::Result,
                message: message.clone(),
                failed_criteria: criteria.clone(),
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
        Commands::Tool {
            command: ToolCommands::Approve { approval, note },
        } => (
            "tool approve",
            Command::ToolApprove {
                approval: *approval,
                note: note.clone(),
            },
        ),
        Commands::Tool {
            command: ToolCommands::Refuse { approval, note },
        } => (
            "tool refuse",
            Command::ToolRefuse {
                approval: *approval,
                note: note.clone(),
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
                extra_tries: None,
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
        Commands::Sprint {
            command: SprintCommands::Start { budget },
        } => (
            "sprint start",
            Command::SprintStart {
                budget_usd: *budget,
            },
        ),
        Commands::Sprint {
            command: SprintCommands::End,
        } => ("sprint end", Command::SprintEnd),
        Commands::Pause => ("pause", Command::TeamPause),
        Commands::Resume => ("resume", Command::TeamResume),
        Commands::Say { text } => (
            "say",
            Command::MessagePost {
                text: text.join(" "),
            },
        ),
        Commands::Chat { agent, text } => (
            "chat",
            Command::ChatMessagePost {
                agent_id: agent.clone(),
                text: text.join(" "),
            },
        ),
        Commands::Marketing {
            command: MarketingCommands::Plan { command },
        } => match command {
            PlanCommands::Approve { plan, note } => (
                "marketing plan approve",
                Command::MarketingPlanDecide {
                    plan: plan.clone(),
                    approve: true,
                    note: note.clone(),
                },
            ),
            PlanCommands::Return { plan, reason } => (
                "marketing plan return",
                Command::MarketingPlanDecide {
                    plan: plan.clone(),
                    approve: false,
                    note: Some(reason.clone()),
                },
            ),
            PlanCommands::End { plan, note } => (
                "marketing plan end",
                Command::MarketingPlanEnd {
                    plan: plan.clone(),
                    note: note.clone(),
                },
            ),
            PlanCommands::Show { .. } => {
                return Err("farik marketing plan show only reads".to_string());
            }
        },
        Commands::Marketing {
            command: MarketingCommands::Post { command },
        } => match command {
            PostCommands::Stop { post } => (
                "marketing post stop",
                Command::SocialPostStop { post: *post },
            ),
            PostCommands::Send { post } => (
                "marketing post send",
                Command::SocialPostDecide {
                    post: *post,
                    post_it: true,
                    note: None,
                },
            ),
            PostCommands::Decline { post, note } => (
                "marketing post decline",
                Command::SocialPostDecide {
                    post: *post,
                    post_it: false,
                    note: note.clone(),
                },
            ),
            PostCommands::List => {
                return Err("farik marketing post list only reads".to_string());
            }
        },
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
    human::said(start::command(project, command, name, io))
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

/// `farik run`, `farik plan`, or `farik contract new`, which drive the project, write as they go,
/// and answer their own exit code.
#[cfg(unix)]
fn drive(command: &Commands, as_json: bool, io: &mut CliIo<'_>) -> i32 {
    use farik_runtime::orchestrator::TickRules;

    if let Commands::Serve { port, no_open } = command {
        return serve::serve(*port, *no_open, io);
    }
    let project = match open_project(&io.cwd, io.clock.now()) {
        Ok(project) => project,
        Err(error) => return run::refuse(io, as_json, &error),
    };
    match command {
        Commands::Contract {
            command:
                ContractCommands::New {
                    brief,
                    from,
                    size,
                    lock,
                },
        } => {
            let source = match (brief, from) {
                (Some(brief), _) => contract_new::Source::Brief(brief),
                (None, Some(url)) => contract_new::Source::Issue(url),
                (None, None) => {
                    return run::refuse(io, as_json, "give --brief or --from");
                }
            };
            contract_new::contract_new(
                &project,
                &contract_new::Asked {
                    source,
                    size: size.map(Into::into),
                    lock: *lock,
                },
                io,
                as_json,
            )
        }
        Commands::Plan => run::drive(&project, TickRules::Planning, io, as_json),
        _ => run::drive(&project, TickRules::All, io, as_json),
    }
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

/// One line to a stream, its control characters escaped (`printable`), since much of what the
/// command line prints an agent wrote. A stream that cannot be written to is a pipe that was
/// closed, which is not something to tell the person about on the stream that just closed.
fn say(stream: &mut Box<dyn Write + '_>, line: &str) {
    let _ = writeln!(stream, "{}", printable::printable(line));
}
