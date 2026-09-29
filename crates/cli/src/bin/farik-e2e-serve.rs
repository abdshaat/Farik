//! `farik serve` on recorded sessions, for the browser suites: `farik-e2e-serve --port <p>
//! [--transcripts <name>,...] [--no-keychain]`, run in a project directory, or anywhere else for
//! the first-run wizard. `--no-keychain` keeps the credential in the state folder's file alone. Built only with the `e2e` feature, so
//! the shipped `farik` has no path to replay.

use std::path::PathBuf;
use std::sync::Arc;

use farik::ids::SystemClock;
use farik::{CliIo, Engine, Interrupts, run_cli};
use farik_runtime::recorded::fixtures::{tool_runner, triage_frk_1_large};
use farik_runtime::{RecordedAdapter, RuntimeAdapter, Transcript};

/// The transcript a suite names.
fn transcript(name: &str) -> Option<Transcript> {
    match name {
        "triage_frk_1_large" => Some(triage_frk_1_large()),
        _ => None,
    }
}

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (mut port, mut names, mut keychain) = (None, String::new(), true);
    while let Some(flag) = arguments.next() {
        if flag == "--no-keychain" {
            keychain = false;
            continue;
        }
        match (flag.as_str(), arguments.next()) {
            ("--port", Some(value)) => port = Some(value),
            ("--transcripts", Some(value)) => names = value,
            _ => return usage(),
        }
    }
    let Some(port) = port else {
        return usage();
    };
    let mut transcripts = Vec::new();
    for name in names.split(',').filter(|name| !name.is_empty()) {
        let Some(found) = transcript(name) else {
            eprintln!("unknown transcript: {name}");
            return std::process::ExitCode::from(2);
        };
        transcripts.push(found);
    }

    let mut io = CliIo::new(
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        Box::new(std::io::stdout()),
        Box::new(std::io::stderr()),
        Arc::new(SystemClock),
    );
    io.env = std::env::vars_os()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    io.engine = Engine::Given(Arc::new(move |daemon| {
        let adapter: Arc<dyn RuntimeAdapter> = Arc::new(RecordedAdapter::with_tools(
            transcripts.clone(),
            tool_runner(daemon),
        ));
        adapter
    }));
    io.interrupts = Interrupts::CtrlC;
    io.credential_stores = farik::system_credential_stores(&io.env, keychain);
    let arguments = ["farik", "serve", "--no-open", "--port", &port].map(String::from);
    let code = run_cli(&arguments, &mut io);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

fn usage() -> std::process::ExitCode {
    eprintln!("usage: farik-e2e-serve --port <p> [--transcripts <name>,...] [--no-keychain]");
    std::process::ExitCode::from(2)
}
