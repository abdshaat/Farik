//! `farik serve` (`docs/SPEC.md` 4.1, 8.1): `farik run` that keeps driving when the board is idle,
//! until a stop or Ctrl-C, and remembers the project it serves. With no project to serve it serves
//! the first-run wizard, and takes on the project the wizard chooses, on the same port.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use farik_runtime::credential::load_credential;
use farik_runtime::daemon::{DaemonState, PortChoice, SetupHost, candidates, serve_held};
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

/// What `serve` does next.
enum Mode {
    /// Serve the wizard; `waiting` is the project already chosen when it only waits on the
    /// credential, `leaving` the root a drive just left through `project.leave`.
    Setup {
        waiting: Option<PathBuf>,
        leaving: Option<PathBuf>,
    },
    /// Drive this project.
    Drive(PathBuf),
}

/// How a drive ended.
enum Driven {
    /// With this exit code: serve ends.
    Ended(i32),
    /// The project at this root was left from the browser: serve goes back to the wizard.
    Left(PathBuf),
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
        // Bound once and held until serve ends: the wizard's daemon and the driver's each listen
        // on it in turn, so the port the browser's tab is on is never free for another process to
        // take while one stops and the next starts.
        let held = match hold(PortChoice::Preferred(port.unwrap_or(DEFAULT_PORT))) {
            Ok(held) => held,
            Err(error) => return refuse(io, false, &error),
        };
        let mut linked = false;
        let (mut take_on_error, mut taking_on) = (None, false);
        // The project setup waits on the credential for, kept across a take-on that fails.
        let mut waited_on = None;
        // The root a drive left, kept across a take-on that fails so Stay on still works.
        let mut left_from = None;
        loop {
            mode = match mode {
                Mode::Setup { waiting, leaving } => {
                    waited_on.clone_from(&waiting);
                    left_from.clone_from(&leaving);
                    let set_up = set_up(
                        io,
                        &held,
                        waiting,
                        leaving,
                        take_on_error.take(),
                        no_open,
                        &mut linked,
                    );
                    match set_up.await {
                        Ok(Some(root)) => {
                            taking_on = true;
                            Mode::Drive(root)
                        }
                        Ok(None) => return INTERRUPTED,
                        Err(error) => return refuse(io, false, &error),
                    }
                }
                Mode::Drive(root) => match drive(&root, &held, no_open, &mut linked, io).await {
                    Ok(Driven::Ended(code)) => return code,
                    Ok(Driven::Left(root)) => Mode::Setup {
                        waiting: None,
                        leaving: Some(root),
                    },
                    // A take-on that failed goes back to the wizard, which says why.
                    Err(error) if taking_on => {
                        taking_on = false;
                        say(
                            &mut io.stderr,
                            &format!("farik: the project could not be taken on: {error}"),
                        );
                        take_on_error = Some(error);
                        Mode::Setup {
                            waiting: waited_on.take(),
                            leaving: left_from.take(),
                        }
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
        Some(root) if no_account => Mode::Setup {
            waiting: Some(root),
            leaving: None,
        },
        Some(root) => Mode::Drive(root),
        None => Mode::Setup {
            waiting: None,
            leaving: None,
        },
    }
}

/// Serves the wizard on `held` until it chooses a project, answered with it, or until Ctrl-C,
/// answered `None`. The link is printed when serve has printed none.
async fn set_up(
    io: &mut CliIo<'_>,
    held: &std::net::TcpListener,
    waiting: Option<PathBuf>,
    leaving: Option<PathBuf>,
    take_on_error: Option<String>,
    no_open: bool,
    linked: &mut bool,
) -> Result<Option<PathBuf>, String> {
    // A project found with no account waits on the account, not on a project.
    let lacks = if waiting.is_some() {
        "no AI account yet: connect one in the browser"
    } else {
        "no project yet: farik is set up in the browser"
    };
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
        leaving: leaving.clone(),
    });
    let bound = port_of(held)?;
    let (mut web, code) = web(Path::new(""), io, None)?;
    web.port = bound;
    web.take_on_error = std::sync::Mutex::new(take_on_error);
    web.leaving = std::sync::Mutex::new(leaving.clone());
    let state = Arc::new(DaemonState::setup(Arc::clone(&host), web));
    let handle = serve_held(held, None, state)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(root) = &leaving {
        say(
            &mut io.stdout,
            &format!(
                "the team left {}: choose its next project in the browser",
                root.display()
            ),
        );
    }
    say(&mut io.stdout, &format!("{lacks}, on 127.0.0.1:{bound}"));
    if !*linked {
        print_link(io, bound, &code, no_open);
        *linked = true;
    }
    let Interrupts::Channel(interrupts) = &mut io.interrupts else {
        return Err("Ctrl-C is not listened for".to_string());
    };
    let chosen = tokio::select! {
        chosen = choice.wait_for(Option::is_some) => chosen.ok().and_then(|root| root.clone()),
        Some(()) = interrupts.recv() => None,
    };
    handle.shutdown().await.map_err(|error| error.to_string())?;
    Ok(chosen)
}

/// The first port of `choice` that is free on `127.0.0.1`, bound.
///
/// # Errors
///
/// A sentence saying none is.
fn hold(choice: PortChoice) -> Result<std::net::TcpListener, String> {
    candidates(choice)
        .into_iter()
        .find_map(|port| std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).ok())
        .ok_or_else(|| "no port is free on 127.0.0.1 for farik serve".to_string())
}

/// The port `held` is bound to.
fn port_of(held: &std::net::TcpListener) -> Result<u16, String> {
    held.local_addr()
        .map(|address| address.port())
        .map_err(|error| error.to_string())
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
    held: &std::net::TcpListener,
    no_open: bool,
    linked: &mut bool,
    io: &mut CliIo<'_>,
) -> Result<Driven, String> {
    let project = open_project(root, io.clock.now())?;
    let options = StartOptions {
        listener: Some(held),
        web: true,
    };
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
        && !*linked
    {
        print_link(printer.io, driver.port(), code, no_open);
        *linked = true;
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
    let left = driver.daemon.left();
    if left.is_some() {
        // The wizard after a leave listens on `io`: give it the receiver Ctrl-C reaches, as
        // `start_holding` does on a refusal.
        let dropped = tokio::sync::mpsc::unbounded_channel().1;
        let interrupts = std::mem::replace(&mut driver.interrupts, dropped);
        printer.io.interrupts = Interrupts::Channel(interrupts);
    }
    let code = finish(&project, driver, &mut printer, ended, presses).await;
    Ok(match left {
        Some(root) if code == 0 => Driven::Left(root),
        _ => Driven::Ended(code),
    })
}
