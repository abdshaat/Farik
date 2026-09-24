//! What the tests of the human's commands, of driving the team, and of `farik contract new` share:
//! a project with a team, the command line run in it, the log read and written behind its back,
//! and a stand-in for another process driving the project.
//!
//! Each test file includes this with `#[path]`, so a helper one of them does not use is not dead
//! code in the others.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::{DateTime, Utc};
use farik::{CliIo, run_cli};
use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
use farik_core::team::validate_team;
use farik_protocol::clock::{Clock, FixedClock};
use farik_protocol::command::Command;
use farik_protocol::event::{EventIds, EventKind, FarikEvent, NewEvent, event_from_value};
use farik_runtime::ToolDeps;
use farik_runtime::daemon::{DaemonConfig, DaemonHandle, DaemonState, serve};
use farik_runtime::orchestrator::{CommandError, CommandReport};
use farik_runtime::transitions::Transitions;
use farik_store::files::{LocalSettings, ProjectFiles, Sandbox};
use farik_store::git::fixtures::TempRepo;
use farik_store::{EventLog, EventQuery, open_event_log, open_projections};
use serde_json::{Value, json};

/// The moment every test in a run is stamped with: the day's budget is read against it.
static AT: LazyLock<DateTime<Utc>> = LazyLock::new(Utc::now);

/// The time the command line is told it is.
pub fn at() -> DateTime<Utc> {
    *AT
}

/// What one run of the command line did.
pub struct Ran {
    pub code: i32,
    pub out: String,
    pub err: String,
}

/// Runs `farik <args>` in `cwd` on `CliIo::new`.
pub fn run(cwd: &Path, args: &[&str]) -> Ran {
    run_with(cwd, args, |_| {})
}

/// Runs `farik <args>` in `cwd` on `CliIo::new`, with `set` applied to the harness first.
pub fn run_with(cwd: &Path, args: &[&str], set: impl FnOnce(&mut CliIo<'_>)) -> Ran {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = {
        let mut io = CliIo::new(
            cwd.to_path_buf(),
            Box::new(&mut out),
            Box::new(&mut err),
            Arc::new(FixedClock::new(at())),
        );
        set(&mut io);
        let arguments: Vec<String> = std::iter::once("farik")
            .chain(args.iter().copied())
            .map(ToString::to_string)
            .collect();
        run_cli(&arguments, &mut io)
    };
    Ran {
        code,
        out: String::from_utf8(out).expect("the command line writes text"),
        err: String::from_utf8(err).expect("the command line writes text"),
    }
}

/// A repository made a Farik project by `farik init`.
pub fn a_project(name: &str) -> TempRepo {
    let repository = TempRepo::new(name);
    let ran = run(&repository.path, &["init"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    repository
}

/// A project whose team is step 11's three agents, `pm`, `dev-a`, and `dev-b`, with a WIP limit
/// of one and the `manual` integration policy, running in no-sandbox mode.
pub fn a_team(name: &str) -> TempRepo {
    a_team_with(name, |_| {})
}

/// `a_team`, with `change` applied to the team's wire last.
pub fn a_team_with(name: &str, change: impl FnOnce(&mut Value)) -> TempRepo {
    let repository = a_project(name);
    let mut wire = a_team_wire();
    wire["agents"] = json!([
        an_agent_wire("pm", "product_manager"),
        an_agent_wire("dev-a", "software_developer"),
        an_agent_wire("dev-b", "software_developer"),
    ]);
    wire["policy"]["wip_limit_per_agent"] = json!(1);
    wire["policy"]["integration"] = json!("manual");
    change(&mut wire);
    let files = files_of(&repository);
    files
        .write_team(&validate_team(&wire).expect("the fixture is a team"))
        .expect("the team is written");
    no_sandbox(&repository);
    repository
}

/// Says in `.farik/local/settings.json` that this machine runs no sandbox.
pub fn no_sandbox(repository: &TempRepo) {
    files_of(repository)
        .write_settings(&LocalSettings {
            sandbox: Sandbox::None,
        })
        .expect("the settings are written");
}

pub fn files_of(repository: &TempRepo) -> ProjectFiles {
    ProjectFiles::open(repository.path.clone())
}

/// The project's log, opened as another process would.
pub fn log_of(repository: &TempRepo) -> EventLog {
    open_event_log(&repository.path.join(".farik/local/farik.db"), at()).expect("the log opens")
}

/// Every event of these kinds, oldest first; every event when `kinds` is empty.
pub fn events(repository: &TempRepo, kinds: &[EventKind]) -> Vec<FarikEvent> {
    log_of(repository)
        .read(&EventQuery {
            kinds: kinds.to_vec(),
            ..EventQuery::default()
        })
        .expect("the log reads")
}

/// The status the board gives `task`.
pub fn status_of(repository: &TempRepo, task: &str) -> String {
    let log = Arc::new(log_of(repository));
    let id = task.parse().expect("a task id");
    open_projections(log)
        .expect("the projections open")
        .task(&id)
        .expect("the board reads")
        .map(|row| row.status.to_string())
        .unwrap_or_default()
}

/// Appends one event about `task` (or none when `task` is empty), stamped with the project's ids,
/// as a command in another process would.
pub fn record(repository: &TempRepo, task: &str, kind: &str, body: &Value) -> FarikEvent {
    record_as(repository, task, None, kind, body)
}

/// `record`, from `agent`'s session `session` when one is named.
pub fn record_as(
    repository: &TempRepo,
    task: &str,
    session: Option<(&str, &str)>,
    kind: &str,
    body: &Value,
) -> FarikEvent {
    record_on(repository, task, session, kind, body, at())
}

/// `record_as`, stamped `recorded_at` rather than now.
pub fn record_on(
    repository: &TempRepo,
    task: &str,
    session: Option<(&str, &str)>,
    kind: &str,
    body: &Value,
    recorded_at: DateTime<Utc>,
) -> FarikEvent {
    let log = log_of(repository);
    let first = log
        .read(&EventQuery {
            limit: Some(1),
            ..EventQuery::default()
        })
        .expect("the log reads");
    let ids = &first.first().expect("init recorded something").envelope.ids;
    let mut wire = json!({
        "seq": 1,
        "recorded_at": recorded_at.to_rfc3339(),
        "team_id": ids.team_id,
        "project_id": ids.project_id,
        "kind": kind,
        "body": body,
    });
    if !task.is_empty() {
        wire["task_id"] = json!(task);
    }
    if let Some((agent, session)) = session {
        wire["agent_id"] = json!(agent);
        wire["session_id"] = json!(session);
    }
    let event = event_from_value(&wire).expect("the fixture is schema-valid");
    let appended = log
        .append(&NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        })
        .expect("appends");
    // The board is brought up to the log by whoever opens it next.
    appended
}

/// Records `task`'s move from `from` to `to`, by the governor unless `extra` says otherwise.
pub fn moved(repository: &TempRepo, task: &str, from: &str, to: &str, extra: &Value) {
    let mut body = json!({
        "from": from,
        "to": to,
        "actor": "governor",
        "requested_by": "governor",
        "gate": "none",
        "effects": [],
        "iteration": 0
    });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            body.insert(key.clone(), value.clone());
        }
    }
    record(repository, task, "task.transitioned", &body);
}

/// Walks `task` from `draft` through `path`, as the governor's moves.
pub fn walked(repository: &TempRepo, task: &str, path: &[&str]) {
    let mut from = "draft";
    for to in path {
        moved(repository, task, from, to, &json!({}));
        from = to;
    }
}

/// A contract a person would write, as YAML, for `done.txt`: C1 runs `test -f done.txt`.
pub fn a_request(title: &str) -> String {
    format!(
        r"title: {title}
intent: The repository has a done.txt at its root, so that a run can be checked for it.
scope:
  in_scope:
    - done.txt
  out_of_scope:
    - what done.txt says
requirements:
  - id: R1
    text: done.txt is at the root.
exit_criteria:
  - id: C1
    text: done.txt exists.
    satisfies:
      - R1
    verification:
      method: command
      command: test -f done.txt
      expect:
        exit_code: 0
assignee_role: software_developer
reviewer_role: software_developer
risk: low
budget:
  max_cost_usd: 5
allowed_paths:
  - done.txt
"
    )
}

/// Files `a_request(title)` with `farik task create`, and answers the id it was given.
pub fn filed(repository: &TempRepo, title: &str) -> String {
    let path = repository.path.join(format!("{}.yaml", title.len()));
    std::fs::write(&path, a_request(title)).expect("the request is written");
    let ran = run(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let _ = std::fs::remove_file(&path);
    ran.out
        .split_whitespace()
        .next()
        .expect("the id first")
        .to_string()
}

/// Files `a_request(title)` of risk `high`, which waits for the human's acceptance, and walks it
/// to `verifying` as `dev-a`'s, reviewed by `dev-b`. Answers its id.
pub fn a_high_risk_task_verifying(repository: &TempRepo, title: &str) -> String {
    let path = repository.path.join(format!("high-{}.yaml", title.len()));
    std::fs::write(&path, a_request(title).replace("risk: low", "risk: high"))
        .expect("the request is written");
    let ran = run(
        &repository.path,
        &["task", "create", path.to_str().expect("a path")],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let _ = std::fs::remove_file(&path);
    let task = ran
        .out
        .split_whitespace()
        .next()
        .expect("the id first")
        .to_string();
    record(
        repository,
        &task,
        "request.triaged",
        &json!({ "size": "small", "reason": "One file.", "triaged_by": "human" }),
    );
    let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
    for (from, to) in [
        ("draft", "refining"),
        ("refining", "ready"),
        ("ready", "assigned"),
        ("assigned", "in_progress"),
        ("in_progress", "verifying"),
    ] {
        moved(repository, &task, from, to, &people);
    }
    task
}

/// Holds this project's run lock for as long as it lives, as a process driving it does.
pub fn hold_the_run_lock(repository: &TempRepo) -> File {
    let path = repository.path.join(".farik/local/run.lock");
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .expect("the lock file opens");
    file.try_lock().expect("nothing else holds the lock");
    file
}

/// Waits for this project's run lock to be free, then gives it back at once: proof that nothing
/// is left holding it, without racing a holder that only just let go.
///
/// A `try_lock` right after the process driving the project dropped its hold can answer
/// `WouldBlock` even though nothing means to hold the lock: a `flock` belongs to the open file
/// description, and a child another test thread forks while that description is still open
/// keeps a copy of it until it execs, which can outlast the real holder's own drop. Blocking
/// waits out that cloexec'd duplicate instead of racing it; a lock a bug genuinely left held
/// blocks here forever, which still fails the test, just not as fast.
pub fn the_run_lock_frees(repository: &TempRepo) {
    let path = repository.path.join(".farik/local/run.lock");
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .expect("the lock file opens");
    file.lock().expect("the lock can be taken");
}

/// The project's tools, as a process driving it holds them.
pub fn tool_deps(repository: &TempRepo) -> Arc<ToolDeps> {
    let log = Arc::new(log_of(repository));
    let projections = Arc::new(open_projections(Arc::clone(&log)).expect("the projections open"));
    let files = Arc::new(files_of(repository));
    let clock: Arc<dyn Clock + Send + Sync> = Arc::new(FixedClock::new(at()));
    let first = log
        .read(&EventQuery {
            limit: Some(1),
            ..EventQuery::default()
        })
        .expect("the log reads");
    let ids = EventIds {
        task_id: None,
        agent_id: None,
        session_id: None,
        ..first
            .first()
            .expect("init recorded something")
            .envelope
            .ids
            .clone()
    };
    let transitions = Arc::new(Transitions::new(
        Arc::clone(&log),
        Arc::clone(&projections),
        Arc::clone(&files),
        repository.adapter(),
        Arc::clone(&clock),
        ids.clone(),
    ));
    Arc::new(ToolDeps {
        log,
        projections,
        files,
        transitions,
        git: repository.adapter(),
        clock,
        ids,
    })
}

/// Another process driving the project: it holds the run lock and serves a daemon on
/// `.farik/local/daemon.json` whose handler records each command and answers `handled by the
/// run`.
pub struct LiveDriver {
    pub commands: Arc<Mutex<Vec<Command>>>,
    pub state: Arc<DaemonState>,
    handle: Option<DaemonHandle>,
    runtime: tokio::runtime::Runtime,
    _lock: File,
}

impl LiveDriver {
    pub fn new(repository: &TempRepo) -> LiveDriver {
        LiveDriver::answering(
            repository,
            Ok(CommandReport {
                said: "handled by the run".to_string(),
                events: Vec::new(),
            }),
        )
    }

    /// A driver whose handler records each command and answers `answer`.
    pub fn answering(
        repository: &TempRepo,
        answer: Result<CommandReport, CommandError>,
    ) -> LiveDriver {
        let lock = hold_the_run_lock(repository);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("a runtime");
        let state = Arc::new(DaemonState::new(tool_deps(repository)));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&commands);
        state.set_command_handler(Arc::new(move |command| {
            recorded
                .lock()
                .expect("no test panics holding it")
                .push(command);
            let answer = answer.clone();
            Box::pin(async move { answer })
        }));
        let handle = runtime
            .block_on(serve(
                DaemonConfig {
                    port: None,
                    daemon_file: repository.path.join(".farik/local/daemon.json"),
                },
                Arc::clone(&state),
            ))
            .expect("the daemon is up");
        LiveDriver {
            commands,
            state,
            handle: Some(handle),
            runtime,
            _lock: lock,
        }
    }

    /// Every command the driver was sent, in order.
    pub fn commands(&self) -> Vec<Command> {
        self.commands
            .lock()
            .expect("no test panics holding it")
            .clone()
    }

    /// The driver's pid, as its `daemon.json` says.
    pub fn pid(&self) -> u32 {
        self.handle.as_ref().expect("serving").info.pid
    }
}

impl Drop for LiveDriver {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = self.runtime.block_on(handle.shutdown());
        }
    }
}

/// The environment a test gives the command line: nothing but `PATH`, as `PATH` is here.
pub fn a_bare_env() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )])
}

/// A directory of its own under the temporary directory, removed first.
pub fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "farik-cli-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

/// A directory holding a `claude` that prints `version` whatever it is asked, and the `PATH` that
/// finds it first.
pub fn a_claude_saying(name: &str, version: &str) -> (PathBuf, String) {
    use std::os::unix::fs::PermissionsExt;

    let directory = scratch(name);
    let source = directory.join("claude.txt");
    std::fs::write(&source, format!("#!/bin/sh\necho '{version}'\n")).expect("written");
    // Copied rather than written in place: a file this process holds open for writing is
    // inherited by whatever another test thread forks meanwhile, and running it then fails with
    // "text file busy".
    let program = directory.join("claude");
    let copied = std::process::Command::new("cp")
        .arg(&source)
        .arg(&program)
        .status()
        .expect("cp runs");
    assert!(copied.success());
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let path = format!(
        "{}:{}",
        directory.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (directory, path)
}

/// Waits for `handle`'s thread to end, and fails the test rather than hang when it does not end
/// within a minute.
pub fn joined<T>(handle: std::thread::JoinHandle<T>, what: &str) -> T {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !handle.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "{what} did not end in time"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    handle.join().unwrap_or_else(|_| panic!("{what} panicked"))
}
