//! The `farik` command line: the commands the first release ships, and the shape they share.
//!
//! Everything a command does is a function in one of the modules below, taking what it needs and
//! returning either a `Report` — the lines a person reads and the JSON a script reads — or the one
//! sentence that says why Farik would not do it. `run_cli` parses the arguments, picks the
//! function, and writes the answer to the streams it was given, so a test runs a command without
//! spawning a process (`docs/SPEC.md` sections 5.11, 5.16, F2, F3).

/// Taking a contract from the team, and giving it back.
pub mod contract;
/// Making a repository a Farik project.
pub mod init;
/// The project a command runs against.
pub mod project;
/// The governor's refusals in words.
pub mod refusal;
/// Filing a request.
pub mod task;
/// Sizing a request.
pub mod triage;

use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use farik_protocol::clock::Clock;
use serde_json::{Value, json};

pub use project::{Project, open_project};

/// Everything the command line needs from outside itself: where to write, where it is run, and what
/// time it is.
///
/// The streams are boxed writers rather than `std::io::stdout` so that a test reads what a command
/// printed, and they borrow for `'a` so that the buffer a test reads back is the test's own
/// `Vec<u8>`; the clock is injected for the same reason an event's `recorded_at` is (`docs/SPEC.md`
/// section 8.4).
pub struct CliIo<'a> {
    /// What a person or a script asked for.
    pub stdout: Box<dyn Write + 'a>,
    /// Why Farik would not do something, and nothing else.
    pub stderr: Box<dyn Write + 'a>,
    /// Where the command was run, which is how the project is found.
    pub cwd: PathBuf,
    /// The time every event this run records is stamped with.
    pub clock: Box<dyn Clock>,
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
}

#[derive(Subcommand)]
enum TaskCommands {
    /// File a contract as a draft request.
    Create {
        /// The YAML contract to file. Farik assigns the id.
        file: PathBuf,
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
    let now = io.clock.now();
    let outcome = match &parsed.command {
        Commands::Init => init::init(&io.cwd, now),
        Commands::Task {
            command: TaskCommands::Create { file },
        } => open_project(&io.cwd, now)
            .and_then(|project| task::create(&project, &io.cwd, file, now)),
        Commands::Triage {
            task_id,
            size,
            reason,
        } => open_project(&io.cwd, now)
            .and_then(|project| triage::triage(&project, task_id, (*size).into(), reason, now)),
        Commands::Contract {
            command: ContractCommands::Lock { task_id },
        } => open_project(&io.cwd, now)
            .and_then(|project| contract::hold(&project, task_id, true, now)),
        Commands::Contract {
            command: ContractCommands::Unlock { task_id },
        } => open_project(&io.cwd, now)
            .and_then(|project| contract::hold(&project, task_id, false, now)),
    };
    report(outcome, parsed.json, io)
}

/// Writes what the command did, or why it would not, and answers with the exit code.
fn report(outcome: Result<Report, String>, as_json: bool, io: &mut CliIo<'_>) -> i32 {
    match outcome {
        Ok(done) => {
            if as_json {
                say(&mut io.stdout, &format!("{}", done.json));
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
