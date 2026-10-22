//! `catervas serve` (`docs/SPEC.md` 4.1, 8.1; ADR 0059): `catervas run` for every team on the
//! computer at once, which keeps driving when a board is idle, until a stop or Ctrl-C. The port it
//! holds serves the hub: the first-run wizard, and each team's pages, routed to the daemon of that
//! team's drive. A team added in the browser starts beside the others, and one that leaves, or is
//! stopped with `catervas stop`, ends alone.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use catervas_runtime::daemon::hub::{Hub, TeamSlug};
use catervas_runtime::daemon::{
    DaemonHandle, DaemonState, PortChoice, SetupHost, candidates, serve_held,
};
use catervas_runtime::orchestrator::{TickRules, TickScope};
use serde_json::json;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::project::{open_project, repository_root};
use crate::run::{INTERRUPTED, OnIdle, Printer, finish, report_error, started, ticks};
use crate::setup::CliHost;
use crate::start::{
    RunLock, Shared, StartOptions, driven_elsewhere, listen, runtime, shared_web, start_holding,
    try_lock, web_sharing,
};
use crate::state::{forget, home_of, read_state, remember, state_dir};
use crate::{CliIo, Engine, Interrupts, say};

/// The port `serve` asks for when it is not told one.
const DEFAULT_PORT: u16 = 7420;

/// How a drive ended.
enum Driven {
    /// With this exit code: the team is no longer driven, and stays one of the teams.
    Ended(i32),
    /// The project at this root was left from the browser: it is no longer one of the teams.
    Left(PathBuf),
}

/// What a drive tells the loop besides its end.
enum Event {
    /// A line it wrote, whole, to standard output or, when `error`, standard error.
    Line { error: bool, text: String },
    /// The team is driven and its pages are routed to.
    Started,
}

/// A drive's end.
struct Finished {
    root: PathBuf,
    /// Whether the wizard chose the project, which goes back to the wizard when it cannot start.
    taking_on: bool,
    outcome: Result<Driven, String>,
}

/// A writer that hands the loop each whole line, so that the lines of teams driven at once are
/// never interleaved within a line.
struct Lines {
    error: bool,
    pending: Vec<u8>,
    to: mpsc::UnboundedSender<Event>,
}

impl Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line).into_owned();
            let _ = self.to.send(Event::Line {
                error: self.error,
                text,
            });
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What `serve` found to drive.
struct Found {
    /// Every team's project, in the order `state.json` lists them, with the one run in last.
    roots: Vec<PathBuf>,
    /// The projects `state.json` listed already.
    listed: BTreeSet<PathBuf>,
    /// The team `/rpc` answers for first.
    current: Option<PathBuf>,
    /// Whether Claude Code is not signed in, so that the teams wait on the account.
    no_account: bool,
}

/// What the wizard was started for, kept across a take-on that fails.
#[derive(Clone, Default)]
struct WizardContext {
    /// The project that waits on the AI account.
    waiting: Option<PathBuf>,
    /// The root a drive just left through `project.leave`.
    leaving: Option<PathBuf>,
}

/// The wizard and the hub on the held port, until it is shut down.
struct HubRun {
    handle: DaemonHandle,
    choice: watch::Receiver<Option<PathBuf>>,
}

/// Everything the loop keeps.
struct Serving<'a, 'io> {
    io: &'a mut CliIo<'io>,
    held: std::net::TcpListener,
    shared: Shared,
    hub: Arc<Hub>,
    /// The project of every team, driven or not.
    teams: Arc<Mutex<BTreeSet<PathBuf>>>,
    drives: JoinSet<Finished>,
    /// What stops each drive, as Ctrl-C does.
    stops: BTreeMap<PathBuf, mpsc::UnboundedSender<()>>,
    /// The slug of each driven team.
    slugs: BTreeMap<PathBuf, TeamSlug>,
    /// The projects another process drives, tried again.
    elsewhere: BTreeSet<PathBuf>,
    /// The projects waiting on the AI account.
    pending: Vec<PathBuf>,
    events: mpsc::UnboundedSender<Event>,
    state_dir: Option<PathBuf>,
    no_open: bool,
    /// Whether the link was printed.
    linked: bool,
    /// The worst exit code of the drives that ended.
    worst: i32,
    /// Whether Ctrl-C was pressed.
    presses: u32,
    /// Whether a team was driven at all.
    started: bool,
    /// The context the wizard has now.
    context: WizardContext,
    /// The context it had when it chose a project, which it goes back to if that fails.
    restore: WizardContext,
}

/// Serves every team of this computer: the project run in, and those `state.json` lists, else the
/// wizard until it chooses one. Drives each with every rule until a stop or Ctrl-C, whatever the
/// board. Answers the exit code: 130 after Ctrl-C, 1 after a refusal or a failed tick, 0 after
/// every team was stopped with `catervas stop`.
pub(crate) fn serve(port: Option<u16>, no_open: bool, io: &mut CliIo<'_>) -> i32 {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => return crate::run::refuse(io, false, &error),
    };
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run(port, no_open, io)))
}

#[allow(
    clippy::too_many_lines,
    reason = "one loop over the events a serve waits on, each handled in order"
)]
async fn run(port: Option<u16>, no_open: bool, io: &mut CliIo<'_>) -> i32 {
    // Listened for once, here, so that Ctrl-C is heard in setup mode and by every drive.
    let interrupts = std::mem::replace(&mut io.interrupts, Interrupts::CtrlC);
    let mut ctrl_c = match listen(interrupts) {
        Ok(receiver) => receiver,
        Err(error) => return crate::run::refuse(io, false, &error),
    };
    let found = found(io).await;
    // Bound once and held until serve ends: the hub is shut down and served again on it, so the
    // port the browser's tab is on is never free for another process to take meanwhile.
    let held = match hold(PortChoice::Preferred(port.unwrap_or(DEFAULT_PORT))) {
        Ok(held) => held,
        Err(error) => return crate::run::refuse(io, false, &error),
    };
    let bound = match port_of(&held) {
        Ok(bound) => bound,
        Err(error) => return crate::run::refuse(io, false, &error),
    };
    let shared = match shared_web(io, bound) {
        Ok(shared) => shared,
        Err(error) => return crate::run::refuse(io, false, &error),
    };
    let code = match shared.codes.issue() {
        Ok(code) => code,
        Err(error) => return crate::run::refuse(io, false, &error.to_string()),
    };
    let (events, mut heard) = mpsc::unbounded_channel();
    let mut serving = Serving {
        state_dir: state_dir(&io.env),
        io,
        held,
        shared,
        hub: Arc::new(Hub::default()),
        teams: Arc::new(Mutex::new(found.roots.iter().cloned().collect())),
        drives: JoinSet::new(),
        stops: BTreeMap::new(),
        slugs: BTreeMap::new(),
        elsewhere: BTreeSet::new(),
        pending: Vec::new(),
        events,
        no_open,
        linked: false,
        worst: 0,
        presses: 0,
        started: false,
        context: WizardContext::default(),
        restore: WizardContext::default(),
    };
    // Whether the hub serves the wizard: no team to drive, or one waiting on the account.
    let drivable: Vec<PathBuf> = found
        .roots
        .iter()
        .filter(|root| root.join(".catervas/team.yaml").exists())
        .cloned()
        .collect();
    for root in found.roots.iter().filter(|root| !drivable.contains(root)) {
        let message = format!(
            "catervas: {} has no team any more; it stays one of your teams",
            root.display()
        );
        say(&mut serving.io.stderr, &message);
    }
    let mut wizard = drivable.is_empty() || found.no_account;
    if found.no_account {
        serving.context.waiting = found.current.clone().or_else(|| drivable.first().cloned());
        serving.pending.clone_from(&drivable);
    }
    let mut hub_run = match serving.start_hub(None).await {
        Ok(hub_run) => hub_run,
        Err(error) => return crate::run::refuse(serving.io, false, &error),
    };
    if wizard {
        serving.announce_wizard();
        serving.print_link(&code);
    } else {
        for root in drivable {
            let new = !found.listed.contains(&root);
            let current = found.current.as_ref() == Some(&root);
            serving.launch(root, current, false, new);
        }
    }
    let retry = serving.io.elsewhere_retry;
    let mut retrying = tokio::time::interval(retry);
    retrying.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    retrying.tick().await;
    loop {
        if serving.drives.is_empty() && (!wizard || serving.presses > 0) {
            break;
        }
        tokio::select! {
            Some(()) = ctrl_c.recv() => {
                serving.presses += 1;
                if serving.drives.is_empty() {
                    serving.shut_down(hub_run).await;
                    return INTERRUPTED;
                }
                for stop in serving.stops.values() {
                    let _ = stop.send(());
                }
            }
            Some(event) = heard.recv() => {
                serving.heard(event, &code);
            }
            Ok(()) = hub_run.choice.changed() => {
                let chosen = hub_run.choice.borrow_and_update().clone();
                if let Some(root) = chosen {
                    serving.shut_down(hub_run).await;
                    let failed = serving.take_on(&root);
                    wizard = failed.is_some();
                    hub_run = match serving.start_hub(failed.clone()).await {
                        Ok(hub_run) => hub_run,
                        Err(error) => return crate::run::refuse(serving.io, false, &error),
                    };
                    if failed.is_some() {
                        serving.announce_wizard();
                    }
                }
            }
            Some(done) = serving.drives.join_next() => {
                let finished = match done {
                    Ok(finished) => finished,
                    Err(error) => {
                        let message = format!("a team's drive failed: {error}");
                        report_line(serving.io, &message);
                        serving.worst = 1;
                        continue;
                    }
                };
                // The lines a drive wrote before it ended are heard first.
                while let Ok(event) = heard.try_recv() {
                    serving.heard(event, &code);
                }
                if let Some(back) = serving.ended(finished) {
                    let error = match back {
                        Back::Left => None,
                        Back::TakeOnFailed(error) => Some(error),
                    };
                    serving.shut_down(hub_run).await;
                    wizard = true;
                    hub_run = match serving.start_hub(error).await {
                        Ok(hub_run) => hub_run,
                        Err(error) => return crate::run::refuse(serving.io, false, &error),
                    };
                    serving.announce_wizard();
                }
            }
            _ = retrying.tick(), if !serving.elsewhere.is_empty() => {
                for root in serving.elsewhere.clone() {
                    serving.launch(root, false, false, false);
                }
            }
        }
    }
    serving.shut_down(hub_run).await;
    while let Ok(event) = heard.try_recv() {
        serving.heard(event, &code);
    }
    if serving.presses > 0 {
        INTERRUPTED
    } else if !serving.started && !serving.elsewhere.is_empty() {
        // Every team is driven by another process: there is nothing for this one to do.
        1
    } else {
        serving.worst
    }
}

impl Serving<'_, '_> {
    /// Hears what a drive of `root` said.
    fn heard(&mut self, event: Event, code: &str) {
        match event {
            Event::Line { error, text } => {
                let stream: &mut dyn Write = if error {
                    &mut self.io.stderr
                } else {
                    &mut self.io.stdout
                };
                let _ = stream.write_all(text.as_bytes());
            }
            Event::Started => {
                self.started = true;
                if !self.linked {
                    self.print_link(code);
                }
            }
        }
    }

    /// Prints the browser's link, and opens it unless told not to.
    fn print_link(&mut self, code: &str) {
        self.linked = true;
        let link = format!("http://127.0.0.1:{}/connect#{code}", self.shared.port);
        say(&mut self.io.stdout, &format!("open {link} in your browser"));
        if !self.no_open
            && let Err(why) = (self.io.open_url)(&link)
        {
            say(
                &mut self.io.stderr,
                &format!("could not open a browser: {why}; open the link above yourself"),
            );
        }
    }

    /// Says what the wizard waits for.
    fn announce_wizard(&mut self) {
        if let Some(root) = &self.context.leaving {
            let message = format!(
                "the team left {}: choose its next project in the browser",
                root.display()
            );
            say(&mut self.io.stdout, &message);
        }
        let lacks = if self.context.waiting.is_some() {
            "no AI account yet: connect one in the browser"
        } else {
            "no project yet: catervas is set up in the browser"
        };
        let message = format!("{lacks}, on 127.0.0.1:{}", self.shared.port);
        say(&mut self.io.stdout, &message);
    }

    /// Serves the hub on the held port: the wizard and the routes to every team. `take_on_error`
    /// is why the project the wizard chose could not be taken on.
    async fn start_hub(&mut self, take_on_error: Option<String>) -> Result<HubRun, String> {
        let (chosen, choice) = watch::channel(None);
        let host: Arc<dyn SetupHost> = Arc::new(CliHost {
            env: self.io.env.clone(),
            home: home_of(self.io),
            clock: Arc::clone(&self.io.clock),
            chosen,
            waiting: self.context.waiting.clone(),
            leaving: self.context.leaving.clone(),
            secrets: Arc::clone(&self.io.connector_secrets),
            teams: Arc::clone(&self.teams),
        });
        let mut web = web_sharing(Path::new(""), self.io, &self.shared);
        web.take_on_error = Mutex::new(take_on_error);
        web.leaving = Mutex::new(self.context.leaving.clone());
        let state = Arc::new(DaemonState::setup(host, web));
        state.set_hub(Arc::clone(&self.hub));
        let handle = serve_held(&self.held, None, state)
            .await
            .map_err(|error| error.to_string())?;
        Ok(HubRun { handle, choice })
    }

    /// Shuts the hub down, closing the browsers' sockets.
    async fn shut_down(&mut self, hub_run: HubRun) {
        if let Err(error) = hub_run.handle.shutdown().await {
            say(&mut self.io.stderr, &format!("catervas: {error}"));
        }
    }

    /// Starts driving `root` on a task of its own, unless another process drives it, or it has no
    /// team. `current` makes its pages the ones `/rpc` answers, and `new` remembers it in
    /// `state.json`. Answers why a project the wizard chose (`taking_on`) cannot start, which
    /// sends it back to the wizard; any other that cannot is said, and the exit code is 1.
    fn launch(
        &mut self,
        root: PathBuf,
        current: bool,
        taking_on: bool,
        new: bool,
    ) -> Option<String> {
        let failed = |serving: &mut Self, error: String| {
            if taking_on {
                return Some(error);
            }
            say(
                &mut serving.io.stderr,
                &format!("catervas: {}: {error}", root.display()),
            );
            serving.worst = 1;
            None
        };
        if !root.join(".catervas/team.yaml").exists() {
            return failed(self, "there is no team here".to_string());
        }
        let lock = match try_lock(&root) {
            Ok(Some(lock)) => lock,
            Ok(None) if taking_on => return failed(self, driven_elsewhere(&root)),
            Ok(None) => {
                // Said once, and tried again every `elsewhere_retry`.
                if self.elsewhere.insert(root.clone()) {
                    let message = driven_elsewhere(&root);
                    say(
                        &mut self.io.stderr,
                        &format!("catervas: {}: {message}", root.display()),
                    );
                }
                return None;
            }
            Err(error) => return failed(self, error),
        };
        self.elsewhere.remove(&root);
        let (stop, interrupts) = mpsc::unbounded_channel();
        let lines = |error: bool| -> Box<dyn Write> {
            Box::new(Lines {
                error,
                pending: Vec::new(),
                to: self.events.clone(),
            })
        };
        let io = self.io.fork(lines(false), lines(true), interrupts);
        let slug = self.slug_of(&root);
        self.stops.insert(root.clone(), stop);
        self.slugs.insert(root.clone(), slug.clone());
        let launch = Launch {
            root,
            slug,
            lock,
            current,
            taking_on,
            remember: new.then(|| self.state_dir.clone()).flatten(),
        };
        let (shared, hub, events) = (
            self.shared.clone(),
            Arc::clone(&self.hub),
            self.events.clone(),
        );
        self.drives
            .spawn_local(drive_team(launch, io, shared, hub, events));
        None
    }

    /// The name `root`'s team has in an address: its team id, with `-2`, `-3` when another team
    /// has it.
    fn slug_of(&self, root: &Path) -> TeamSlug {
        let id = open_project(root, self.io.clock.now()).map_or_else(
            |_| {
                root.file_name().map_or_else(
                    || "team".to_string(),
                    |name| name.to_string_lossy().to_string(),
                )
            },
            |project| project.ids.team_id,
        );
        let taken: BTreeSet<&str> = self.slugs.values().map(TeamSlug::as_str).collect();
        let mut count = 1;
        loop {
            let candidate = if count == 1 {
                id.clone()
            } else {
                format!("{id}-{count}")
            };
            if !taken.contains(candidate.as_str()) {
                return TeamSlug::new(&candidate);
            }
            count += 1;
        }
    }

    /// Takes on the project the wizard chose, and with it those waiting on the account. Answers
    /// why it could not, which sends the wizard back up with the context it had.
    fn take_on(&mut self, root: &Path) -> Option<String> {
        self.restore = self.context.clone();
        let waiting = self.context.waiting.take();
        self.context.leaving = None;
        self.teams
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(root.to_path_buf());
        if let Some(error) = self.launch(root.to_path_buf(), true, true, true) {
            say(
                &mut self.io.stderr,
                &format!("catervas: the project could not be taken on: {error}"),
            );
            self.forget_team(root);
            self.context = self.restore.clone();
            return Some(error);
        }
        if waiting.as_deref() == Some(root) {
            for other in std::mem::take(&mut self.pending) {
                if other != root {
                    self.launch(other, false, false, false);
                }
            }
        }
        None
    }

    /// Takes `root` out of the teams this serve has.
    fn forget_team(&self, root: &Path) {
        self.teams
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(root);
    }

    /// What follows a drive's end. Answers why the wizard is to be served again, when it is.
    fn ended(&mut self, finished: Finished) -> Option<Back> {
        let Finished {
            root,
            taking_on,
            outcome,
        } = finished;
        self.stops.remove(&root);
        self.slugs.remove(&root);
        match outcome {
            Ok(Driven::Left(left)) => {
                if let Some(directory) = &self.state_dir
                    && let Err(error) = forget(directory, &left)
                {
                    say(&mut self.io.stderr, &format!("warning: {error}"));
                }
                self.forget_team(&left);
                if !self.drives.is_empty() {
                    return None;
                }
                self.context = WizardContext {
                    waiting: None,
                    leaving: Some(left),
                };
                Some(Back::Left)
            }
            Ok(Driven::Ended(code)) => {
                self.worst = self.worst.max(i32::from(code != 0 && code != INTERRUPTED));
                None
            }
            // A take-on that failed goes back to the wizard, which says why.
            Err(error) if taking_on => {
                say(
                    &mut self.io.stderr,
                    &format!("catervas: the project could not be taken on: {error}"),
                );
                self.forget_team(&root);
                self.context = self.restore.clone();
                Some(Back::TakeOnFailed(error))
            }
            Err(error) => {
                say(
                    &mut self.io.stderr,
                    &format!("catervas: {}: {error}", root.display()),
                );
                self.worst = 1;
                None
            }
        }
    }
}

/// Why the wizard is served again after a drive ended.
enum Back {
    /// The last team left its project.
    Left,
    /// The project the wizard chose could not be taken on, for this reason.
    TakeOnFailed(String),
}

/// Everything a drive needs of the loop.
struct Launch {
    root: PathBuf,
    slug: TeamSlug,
    lock: RunLock,
    /// Whether its pages are the ones `/rpc` answers.
    current: bool,
    taking_on: bool,
    /// The state folder to remember the project in, for a team that is new.
    remember: Option<PathBuf>,
}

/// Drives one team on this task until a stop, Ctrl-C or a leave.
async fn drive_team(
    launch: Launch,
    mut io: CliIo<'static>,
    shared: Shared,
    hub: Arc<Hub>,
    events: mpsc::UnboundedSender<Event>,
) -> Finished {
    let (root, taking_on) = (launch.root.clone(), launch.taking_on);
    let outcome = run_team(launch, &mut io, &shared, &hub, &events).await;
    Finished {
        root,
        taking_on,
        outcome,
    }
}

/// `drive_team`'s body: start, route the pages, tick, finish.
async fn run_team(
    launch: Launch,
    io: &mut CliIo<'static>,
    shared: &Shared,
    hub: &Arc<Hub>,
    events: &mpsc::UnboundedSender<Event>,
) -> Result<Driven, String> {
    let Launch {
        root,
        slug,
        lock,
        current,
        remember: state,
        ..
    } = launch;
    let project = open_project(&root, io.clock.now())?;
    let routed = |state: &Arc<DaemonState>| {
        hub.add(slug.clone(), Arc::clone(state));
        if current {
            hub.set_current(slug.as_str());
        }
    };
    let options = StartOptions {
        shared: Some(shared),
        listening: Some(&routed),
    };
    let mut driver = match start_holding(&project, io, lock, options).await {
        Ok(driver) => driver,
        Err(error) => {
            hub.remove(&slug);
            return Err(error);
        }
    };
    if let Some(directory) = state
        && let Err(error) = remember(&directory, &project.root)
    {
        say(&mut io.stderr, &format!("warning: {error}"));
    }
    let _ = events.send(Event::Started);
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
            shared.port
        ),
        &json!({}),
    );
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
    hub.remove(&slug);
    let code = finish(&project, driver, &mut printer, ended, presses).await;
    Ok(driven(left, code))
}

/// A line on standard error, as a refusal is written.
fn report_line(io: &mut CliIo<'_>, message: &str) {
    let mut printer = Printer {
        io,
        as_json: false,
        json_lines: true,
    };
    report_error(&mut printer, message);
}

/// Where `serve` starts: every project `state.json` lists, and the one run in. A project found
/// while Claude Code is not signed in waits on the account.
async fn found(io: &CliIo<'_>) -> Found {
    let has_team = |root: &PathBuf| root.join(".catervas/team.yaml").exists();
    let canonical = |root: PathBuf| std::fs::canonicalize(&root).unwrap_or(root);
    let state = state_dir(&io.env)
        .map(|directory| read_state(&directory))
        .unwrap_or_default();
    let here = repository_root(&io.cwd)
        .ok()
        .map(canonical)
        .filter(has_team);
    let mut roots = state.projects.clone();
    if let Some(here) = &here
        && !roots.contains(here)
    {
        roots.push(here.clone());
    }
    let current = here
        .or_else(|| state.last_project.clone().filter(has_team))
        .or_else(|| roots.iter().find(|root| has_team(root)).cloned());
    let no_account = !roots.is_empty()
        && matches!(io.engine, Engine::Claude)
        && !crate::start::account(io).status().await.signed_in;
    Found {
        roots,
        listed: state.projects.into_iter().collect(),
        current,
        no_account,
    }
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
        .ok_or_else(|| "no port is free on 127.0.0.1 for catervas serve".to_string())
}

/// The port `held` is bound to.
fn port_of(held: &std::net::TcpListener) -> Result<u16, String> {
    held.local_addr()
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

/// How a drive ended: out of the teams only for a leave that finished cleanly; any other code
/// (Ctrl-C pressed during the leave, a failed finish) ends the drive with it.
fn driven(left: Option<PathBuf>, code: i32) -> Driven {
    match left {
        Some(root) if code == 0 => Driven::Left(root),
        _ => Driven::Ended(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_leave_takes_the_team_out() {
        let left = Some(PathBuf::from("/old"));
        assert!(matches!(driven(left, 0), Driven::Left(root) if root == Path::new("/old")));
    }

    #[test]
    fn a_leave_that_finishes_with_a_code_ends_the_drive_with_it() {
        let left = Some(PathBuf::from("/old"));
        assert!(matches!(driven(left, 130), Driven::Ended(130)));
    }

    #[test]
    fn no_leave_ends_the_drive_with_its_code() {
        assert!(matches!(driven(None, 0), Driven::Ended(0)));
    }
}
