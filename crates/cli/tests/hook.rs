//! The hook commands against a served daemon, and against no daemon at all.
//!
//! A test that serves a daemon needs a repository, and so the `git` program, and is `#[ignore]`d
//! and run by `cargo xtask check --integration`; the ones that find no daemon need only a
//! temporary directory.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeZone, Utc};
use farik::{CliIo, run_cli};
use farik_core::budget::DEFAULT_SESSION_LIMITS;
use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
use farik_core::team::validate_team;
use farik_protocol::clock::{Clock, FixedClock};
use farik_protocol::event::{EventIds, EventKind};
use farik_runtime::ToolDeps;
use farik_runtime::daemon::{DaemonConfig, DaemonHandle, DaemonState, SessionRegistration, serve};
use farik_runtime::transitions::Transitions;
use farik_store::files::ProjectFiles;
use farik_store::git::fixtures::TempRepo;
use farik_store::{EventLog, EventQuery, IN_MEMORY, open_event_log, open_projections};
use serde_json::{Value, json};

const SESSION: &str = "3f1c2a9e-8b7d-4e6f-9a01-2b3c4d5e6f70";
const PRE_READ: &str = include_str!("../../runtime/src/daemon/fixtures/pre_tool_use_read.json");
const POST_READ: &str = include_str!("../../runtime/src/daemon/fixtures/post_tool_use_read.json");

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0)
        .single()
        .expect("a real time")
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

/// Runs `farik hook <which> --daemon <daemon_file>` with `input` on its standard input.
fn hook(which: &str, daemon_file: &Path, input: &str) -> Ran {
    hook_with(
        which,
        &daemon_file.display().to_string(),
        std::env::temp_dir(),
        Box::new(std::io::Cursor::new(input.to_string())),
    )
}

/// Runs `farik hook <which> --daemon <daemon>` in `cwd`, reading `stdin`.
fn hook_with(which: &str, daemon: &str, cwd: PathBuf, stdin: Box<dyn Read + Send>) -> Ran {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = {
        let mut io = CliIo::new(
            cwd,
            Box::new(&mut out),
            Box::new(&mut err),
            Arc::new(FixedClock::new(at())),
        );
        io.stdin = stdin;
        let arguments: Vec<String> = ["farik", "hook", which, "--daemon", daemon]
            .map(ToString::to_string)
            .to_vec();
        run_cli(&arguments, &mut io)
    };
    Ran {
        code,
        out: String::from_utf8(out).expect("text"),
        err: String::from_utf8(err).expect("text"),
    }
}

/// A daemon on a repository, with `dev-a`'s session registered in the repository itself, and
/// the runtime that serves it.
struct Served {
    repo: TempRepo,
    log: Arc<EventLog>,
    daemon_file: PathBuf,
    handle: Option<DaemonHandle>,
    runtime: tokio::runtime::Runtime,
}

impl Served {
    fn new(name: &str) -> Self {
        let repo = TempRepo::new(name);
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("pm", "product_manager"),
            an_agent_wire("dev-a", "software_developer"),
        ]);
        let team = validate_team(&wire).expect("a team");
        let files = Arc::new(ProjectFiles::open(repo.path.clone()));
        files.init(&team).expect(".farik/ is made");
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        let projections = Arc::new(open_projections(Arc::clone(&log)).expect("projections"));
        let clock: Arc<dyn Clock + Send + Sync> = Arc::new(FixedClock::new(at()));
        let ids = EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            ..EventIds::default()
        };
        let transitions = Arc::new(Transitions::new(
            Arc::clone(&log),
            Arc::clone(&projections),
            Arc::clone(&files),
            repo.adapter(),
            Arc::clone(&clock),
            ids.clone(),
        ));
        let state = Arc::new(DaemonState::new(Arc::new(ToolDeps {
            log: Arc::clone(&log),
            projections,
            files,
            transitions,
            git: repo.adapter(),
            clock,
            ids,
        })));
        state.register_session(SessionRegistration {
            session_id: SESSION.to_string(),
            agent_id: "dev-a".to_string(),
            task_id: None,
            cwd: repo.path.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
        });
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let daemon_file = repo.path.join(".farik/local/daemon.json");
        let handle = runtime
            .block_on(serve(
                DaemonConfig {
                    port: None,
                    daemon_file: daemon_file.clone(),
                },
                state,
            ))
            .expect("the daemon is up");
        Self {
            repo,
            log,
            daemon_file,
            handle: Some(handle),
            runtime,
        }
    }

    /// A recorded hook input with `/workspace` made the repository.
    fn recorded(&self, fixture: &str) -> String {
        fixture.replace("/workspace", &self.repo.path.display().to_string())
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = self.runtime.block_on(handle.shutdown());
        }
    }
}

/// What a deny from the hook says, or a panic naming what it said instead.
fn denied_reason(out: &str) -> String {
    let answer: Value = serde_json::from_str(out.trim()).expect("the hook prints JSON");
    let output = &answer["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "PreToolUse", "{answer}");
    assert_eq!(output["permissionDecision"], "deny", "{answer}");
    output["permissionDecisionReason"]
        .as_str()
        .expect("a reason")
        .to_string()
}

/// A directory of its own under the temporary directory.
fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("farik-hook-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

/// A daemon file naming `port` on this machine.
fn daemon_file_for(directory: &Path, port: u16) -> PathBuf {
    let path = directory.join("daemon.json");
    std::fs::write(
        &path,
        json!({ "port": port, "token": "a-token", "pid": 1 }).to_string(),
    )
    .expect("the file is written");
    path
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn carries_a_pre_tool_use_hook_to_the_daemon_and_back() {
    let served = Served::new("hook-pre");
    let ran = hook(
        "pre-tool-use",
        &served.daemon_file,
        &served.recorded(PRE_READ),
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let answer: Value = serde_json::from_str(ran.out.trim()).expect("JSON");
    assert_eq!(
        answer["hookSpecificOutput"]["permissionDecision"], "allow",
        "{answer}"
    );
    assert_eq!(answer["hookSpecificOutput"]["hookEventName"], "PreToolUse");
}

#[test]
fn fails_closed_when_the_daemon_is_not_there() {
    let directory = scratch("closed");
    let missing = hook("pre-tool-use", &directory.join("nothing.json"), PRE_READ);
    assert_eq!(missing.code, 0, "{}", missing.err);
    let reason = denied_reason(&missing.out);
    assert!(reason.contains("nothing.json"), "{reason}");

    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        listener.local_addr().expect("an address").port()
    };
    let refused = hook("pre-tool-use", &daemon_file_for(&directory, port), PRE_READ);
    assert_eq!(refused.code, 0, "{}", refused.err);
    let reason = denied_reason(&refused.out);
    assert!(reason.contains("cannot be reached"), "{reason}");

    let silent = TcpListener::bind("127.0.0.1:0").expect("a port");
    let silent_port = silent.local_addr().expect("an address").port();
    let holder = std::thread::spawn(move || {
        // Accepts and holds every connection, answering nothing.
        let held: Vec<_> = silent.incoming().take(1).collect();
        std::thread::sleep(Duration::from_secs(13));
        drop(held);
    });
    let started = Instant::now();
    let unanswered = hook(
        "pre-tool-use",
        &daemon_file_for(&directory, silent_port),
        PRE_READ,
    );
    assert!(
        started.elapsed() < Duration::from_secs(12),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(unanswered.code, 0, "{}", unanswered.err);
    let reason = denied_reason(&unanswered.out);
    assert!(reason.contains("did not answer"), "{reason}");
    drop(holder);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_nothing_after_a_post_tool_use() {
    let served = Served::new("hook-post");
    let ran = hook(
        "post-tool-use",
        &served.daemon_file,
        &served.recorded(POST_READ),
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out, "");
    let returned = served
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::ToolReturned],
            ..EventQuery::default()
        })
        .expect("the log reads");
    assert_eq!(returned.len(), 1);
    assert_eq!(
        returned[0].envelope.ids.session_id.as_deref(),
        Some(SESSION)
    );
}

/// A listener on this machine that reads one request whole and answers it with `status` and
/// `body`, and the thread that serves it, which panics if no request comes within ten seconds, so
/// that a hook that never asks fails the test rather than hanging it.
fn answering(status: &'static str, body: &'static str) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    listener.set_nonblocking(true).expect("the listener polls");
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "no request came");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("the listener failed: {error}"),
            }
        };
        stream.set_nonblocking(false).expect("the stream blocks");
        let mut seen = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let text = String::from_utf8_lossy(&seen).to_string();
            if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                let length = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if rest.len() >= length {
                    break;
                }
            }
            let read = stream.read(&mut buffer).expect("the request reads");
            if read == 0 {
                break;
            }
            seen.extend_from_slice(&buffer[..read]);
        }
        let answer = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(answer.as_bytes())
            .expect("the answer is written");
    });
    (port, server)
}

/// An allow, in Claude Code's shape.
const AN_ALLOW: &str = r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","permissionDecisionReason":"allowed by the governor"}}"#;

#[test]
fn fails_closed_on_an_answer_that_is_not_a_decision() {
    let directory = scratch("not-a-decision");
    let (port, server) = answering("200 OK", "{}");
    let ran = hook("pre-tool-use", &daemon_file_for(&directory, port), PRE_READ);
    server.join().expect("the listener answered");
    assert_eq!(ran.code, 0, "{}", ran.err);
    let reason = denied_reason(&ran.out);
    assert!(reason.starts_with("hook_failed: "), "{reason}");
    assert!(reason.contains("not a decision"), "{reason}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn fails_closed_on_an_answer_that_is_not_a_200() {
    let directory = scratch("not-a-200");
    // The body is an allow, so that only the status can make this a deny.
    let (port, server) = answering("500 Internal Server Error", AN_ALLOW);
    let ran = hook("pre-tool-use", &daemon_file_for(&directory, port), PRE_READ);
    server.join().expect("the listener answered");
    assert_eq!(ran.code, 0, "{}", ran.err);
    let reason = denied_reason(&ran.out);
    assert!(reason.starts_with("hook_failed: "), "{reason}");
    assert!(reason.contains("500"), "{reason}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn reads_a_relative_daemon_path_from_the_directory_it_runs_in() {
    let directory = scratch("relative");
    let (port, server) = answering("200 OK", AN_ALLOW);
    daemon_file_for(&directory, port);
    let ran = hook_with(
        "pre-tool-use",
        "daemon.json",
        directory.clone(),
        Box::new(PRE_READ.as_bytes()),
    );
    server.join().expect("the listener answered");
    assert_eq!(ran.code, 0, "{}", ran.err);
    let answer: Value = serde_json::from_str(ran.out.trim()).expect("JSON");
    assert_eq!(
        answer["hookSpecificOutput"]["permissionDecision"], "allow",
        "{answer}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// A standard input that panics when it is read.
struct Panicking;

impl Read for Panicking {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        panic!("the input broke");
    }
}

#[test]
fn exits_2_with_the_reason_when_the_hook_panics() {
    let ran = hook_with(
        "pre-tool-use",
        "daemon.json",
        std::env::temp_dir(),
        Box::new(Panicking),
    );
    assert_eq!(ran.code, 2, "{}", ran.out);
    assert_eq!(ran.out, "");
    assert!(ran.err.contains("the input broke"), "{}", ran.err);
}
