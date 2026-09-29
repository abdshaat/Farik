//! `farik serve` (`docs/SPEC.md` 4.1, 8.1): `farik run` that keeps driving when the board is idle,
//! until a stop or Ctrl-C, and remembers the project it serves. With no project to serve it serves
//! the first-run wizard, and takes on the project the wizard chooses, on the same port.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use farik_runtime::credential::load_credential;
use farik_runtime::daemon::{DaemonConfig, DaemonState, PortChoice, SetupHost, candidates};
use farik_runtime::orchestrator::{TickRules, TickScope};
use serde_json::json;
use tokio::sync::watch;

use crate::project::{open_project, repository_root};
use crate::run::{INTERRUPTED, OnIdle, Printer, finish, refuse, started, ticks};
use crate::setup::CliHost;
use crate::start::{StartOptions, listen, runtime, start, web};
use crate::state::{last_project, remember, state_dir};
use crate::{CliIo, Engine, Interrupts, say};

/// The port `serve` asks for when it is not told one.
const DEFAULT_PORT: u16 = 7420;
/// How many more times the wizard's daemon tries a port after the one it found was taken.
const BIND_TRIES: u32 = 3;

/// What `serve` does next.
enum Mode {
    /// Serve the wizard; with the project already chosen when it only waits on the credential.
    Setup(Option<PathBuf>),
    /// Drive this project.
    Drive(PathBuf),
}

/// Serves the project found here, else the one served last, else the wizard until it chooses one,
/// and drives it with every rule until a stop or Ctrl-C, whatever the board. Answers the exit
/// code: 130 after Ctrl-C, 1 after a refusal or a failed tick, 0 after `farik stop`.
pub(crate) fn serve(port: Option<u16>, no_open: bool, io: &mut CliIo<'_>) -> i32 {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => return refuse(io, false, &error),
    };
    runtime.block_on(async {
        // Listened for once, here, so that Ctrl-C is heard in setup mode and by every driver.
        let interrupts = std::mem::replace(&mut io.interrupts, Interrupts::CtrlC);
        match listen(interrupts) {
            Ok(receiver) => io.interrupts = Interrupts::Channel(receiver),
            Err(error) => return refuse(io, false, &error),
        }
        let mut mode = found(io);
        let mut port = PortChoice::Preferred(port.unwrap_or(DEFAULT_PORT));
        let mut linked = None;
        let (mut take_on_error, mut taking_on) = (None, false);
        // The project setup waits on the credential for, kept across a take-on that fails.
        let mut waited_on = None;
        loop {
            mode = match mode {
                Mode::Setup(waiting) => {
                    waited_on.clone_from(&waiting);
                    let set_up = set_up(
                        io,
                        port,
                        waiting,
                        take_on_error.take(),
                        no_open,
                        &mut linked,
                    );
                    match set_up.await {
                        Ok(Some((root, bound))) => {
                            port = PortChoice::Exact(bound);
                            taking_on = true;
                            Mode::Drive(root)
                        }
                        Ok(None) => return INTERRUPTED,
                        Err(error) => return refuse(io, false, &error),
                    }
                }
                Mode::Drive(root) => match drive(&root, port, no_open, &mut linked, io).await {
                    Ok(code) => return code,
                    // A take-on that failed goes back to the wizard, which says why.
                    Err(error) if taking_on => {
                        taking_on = false;
                        say(
                            &mut io.stderr,
                            &format!("farik: the project could not be taken on: {error}"),
                        );
                        take_on_error = Some(error);
                        Mode::Setup(waited_on.take())
                    }
                    Err(error) => return refuse(io, false, &error),
                },
            };
        }
    })
}

/// Where `serve` starts: the project it is run in, else the one `state.json` remembers, else the
/// wizard. A project found with no credential for Claude Code starts on the wizard, chosen.
fn found(io: &CliIo<'_>) -> Mode {
    let project = |root: PathBuf| root.join(".farik/team.yaml").exists().then_some(root);
    let root = repository_root(&io.cwd).ok().and_then(project).or_else(|| {
        state_dir(&io.env)
            .and_then(|directory| last_project(&directory))
            .and_then(project)
    });
    let no_account = matches!(io.engine, Engine::Claude)
        && load_credential(&io.env, &(io.credential_stores)()).is_none();
    match root {
        Some(root) if no_account => Mode::Setup(Some(root)),
        Some(root) => Mode::Drive(root),
        None => Mode::Setup(None),
    }
}

/// Serves the wizard on `port` until it chooses a project, answered with the port it was served
/// on, or until Ctrl-C, answered `None`. The link is printed when this port has had none.
async fn set_up(
    io: &mut CliIo<'_>,
    port: PortChoice,
    waiting: Option<PathBuf>,
    take_on_error: Option<String>,
    no_open: bool,
    linked: &mut Option<u16>,
) -> Result<Option<(PathBuf, u16)>, String> {
    let (chosen, mut choice) = watch::channel(None);
    let host: Arc<dyn SetupHost> = Arc::new(CliHost {
        env: io.env.clone(),
        home: io
            .env
            .get("HOME")
            .filter(|home| !home.is_empty())
            .map_or_else(|| io.cwd.clone(), PathBuf::from),
        clock: Arc::clone(&io.clock),
        stores: (io.credential_stores)(),
        chosen,
        waiting,
    });
    // The web state needs the port before the daemon binds it, so a free one is found first, and
    // found again when something takes it in between. An exact port that cannot be had gives way
    // to any, whose new link is printed.
    let mut wanted = port;
    let mut tries = 0;
    let (handle, bound, code) = loop {
        let bound = free_port(wanted).or_else(|_| free_port(PortChoice::Any))?;
        let (mut web, code) = web(Path::new(""), io, None)?;
        web.port = bound;
        web.take_on_error = std::sync::Mutex::new(take_on_error.clone());
        let config = DaemonConfig {
            port: PortChoice::Exact(bound),
            daemon_file: None,
        };
        let state = Arc::new(DaemonState::setup(Arc::clone(&host), web));
        match farik_runtime::daemon::serve(config, state).await {
            Ok(handle) => break (handle, bound, code),
            Err(_) if tries < BIND_TRIES => {
                tries += 1;
                if let PortChoice::Exact(_) = wanted {
                    wanted = PortChoice::Any;
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    say(
        &mut io.stdout,
        &format!("no project yet: farik is set up in the browser, on 127.0.0.1:{bound}"),
    );
    if *linked != Some(bound) {
        print_link(io, bound, &code, no_open);
        *linked = Some(bound);
    }
    let Interrupts::Channel(interrupts) = &mut io.interrupts else {
        return Err("Ctrl-C is not listened for".to_string());
    };
    let chosen = tokio::select! {
        chosen = choice.wait_for(Option::is_some) => chosen.ok().and_then(|root| root.clone()),
        Some(()) = interrupts.recv() => None,
    };
    handle.shutdown().await.map_err(|error| error.to_string())?;
    Ok(chosen.map(|root| (root, bound)))
}

/// The first port of `choice` that is free on `127.0.0.1` now.
///
/// # Errors
///
/// A sentence saying none is.
// ponytail: another process may take the port between this check and the bind, which then fails;
// passing the port to the web state after the bind removes the race if it ever bites.
fn free_port(choice: PortChoice) -> Result<u16, String> {
    candidates(choice)
        .into_iter()
        .find_map(|port| {
            let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).ok()?;
            listener.local_addr().ok().map(|address| address.port())
        })
        .ok_or_else(|| "no port is free on 127.0.0.1 for farik serve".to_string())
}

/// Prints the browser's link, and opens it unless told not to.
fn print_link(io: &mut CliIo<'_>, port: u16, code: &str, no_open: bool) {
    let link = format!("http://127.0.0.1:{port}/connect#{code}");
    say(&mut io.stdout, &format!("open {link} in your browser"));
    if !no_open && let Err(why) = (io.open_url)(&link) {
        say(
            &mut io.stderr,
            &format!("could not open a browser: {why}; open the link above yourself"),
        );
    }
}

/// Drives the project at `root` until a stop or Ctrl-C, answering the exit code, or the sentence
/// of a start that refused, with nothing left running and nothing remembered.
async fn drive(
    root: &Path,
    port: PortChoice,
    no_open: bool,
    linked: &mut Option<u16>,
    io: &mut CliIo<'_>,
) -> Result<i32, String> {
    let project = open_project(root, io.clock.now())?;
    let options = StartOptions { port, web: true };
    let mut driver = start(&project, io, options).await?;
    match state_dir(&io.env) {
        Some(directory) => {
            if let Err(error) = remember(&directory, &project.root) {
                say(&mut io.stderr, &format!("warning: {error}"));
            }
        }
        None => say(
            &mut io.stderr,
            "warning: XDG_CONFIG_HOME, HOME, and APPDATA are all unset, so the project is not \
             remembered",
        ),
    }
    let mut printer = Printer {
        io,
        as_json: false,
        json_lines: true,
    };
    started(&mut printer, &driver);
    printer.line(
        &format!(
            "serving {} on 127.0.0.1:{}",
            project.root.display(),
            driver.port()
        ),
        &json!({}),
    );
    if let Some(code) = &driver.connect_code
        && *linked != Some(driver.port())
    {
        print_link(printer.io, driver.port(), code, no_open);
        *linked = Some(driver.port());
    }
    let scope = TickScope {
        task_id: None,
        rules: TickRules::All,
    };
    let mut presses = 0;
    let ended = ticks(
        &mut driver,
        &scope,
        &mut printer,
        &mut presses,
        OnIdle::Wait,
        |_| {},
    )
    .await;
    Ok(finish(&project, driver, &mut printer, ended, presses).await)
}
