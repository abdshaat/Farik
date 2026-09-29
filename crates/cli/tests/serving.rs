//! `farik serve` (`docs/SPEC.md` 8.1): the process that keeps driving when the board is idle,
//! remembers its project, and ends as `farik run` does. Driven by the recorded adapter through
//! the harness's engine.
//!
//! Every test here passes a port the operating system assigned, never 7420, because tests run in
//! parallel and a CI host may hold 7420. Each needs the `git` program, and is `#[ignore]`d and
//! run by `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use std::io::Write;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::pin::Pin;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use farik_protocol::event::{EventBody, EventKind};
use farik_runtime::recorded::fixtures::triage_frk_1_large;
use farik_runtime::sleep::Sleeper;
use farik_store::git::fixtures::TempRepo;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use project::{Ran, a_team, events, filed, joined, recorded, run, run_with, scratch};

/// A port the operating system gave out and nothing holds now.
fn free_port() -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port is bound");
    listener
        .local_addr()
        .expect("an address")
        .port()
        .to_string()
}

fn daemon_file(repository: &TempRepo) -> std::path::PathBuf {
    repository.path.join(".farik/local/daemon.json")
}

/// Waits until `done` is true, and fails the test rather than hang when it is not within 30 s.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "{what} did not happen in time");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What a serving process printed, readable while it runs.
#[derive(Clone, Default)]
struct SharedOut(Arc<Mutex<Vec<u8>>>);

impl SharedOut {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("the output")).to_string()
    }

    fn idle_lines(&self) -> Vec<String> {
        self.text()
            .lines()
            .filter(|line| line.starts_with("idle: "))
            .map(ToString::to_string)
            .collect()
    }
}

impl Write for SharedOut {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("the output").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A sleeper that says it was entered and then returns only when the test releases it, once.
struct GatedSleeper {
    entered: Mutex<Sender<()>>,
    gate: Arc<Semaphore>,
}

impl Sleeper for GatedSleeper {
    fn sleep_until(&self, _until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let _ = self.entered.lock().expect("the sender").send(());
        Box::pin(async move {
            if let Ok(permit) = self.gate.acquire().await {
                permit.forget();
            }
        })
    }
}

/// `farik serve --port <port>` in `root` on a thread, on an engine with no transcripts.
fn serving(root: &Path, port: &str, env: Vec<(&str, String)>) -> std::thread::JoinHandle<Ran> {
    let root = root.to_path_buf();
    let port = port.to_string();
    let env: Vec<(String, String)> = env.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port], |io| {
            io.engine = recorded(Vec::new());
            io.env.extend(env);
        })
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn remembers_the_project_it_serves() {
    let repository = a_team("serve-remembers");
    let state = scratch("serve-state");
    let serving = serving(
        &repository.path,
        &free_port(),
        vec![("XDG_CONFIG_HOME", state.display().to_string())],
    );
    let file = state.join("farik/state.json");
    until("state.json is written", || file.exists());

    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&state.join("farik")), 0o700);
    assert_eq!(mode(&file), 0o600);
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("state.json reads"))
            .expect("state.json is JSON");
    let root = repository.path.canonicalize().expect("the root");
    assert_eq!(
        written,
        json!({ "last_project": root.display().to_string() })
    );

    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_serving_when_the_board_is_idle() {
    let repository = a_team("serve-idle");
    let port = free_port();
    let out = SharedOut::default();
    let (entered, waiting) = channel();
    let gate = Arc::new(Semaphore::new(0));
    let sleeper = Arc::new(GatedSleeper {
        entered: Mutex::new(entered),
        gate: Arc::clone(&gate),
    });
    let root = repository.path.clone();
    let shared = out.clone();
    let serving = std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port], |io| {
            io.engine = recorded(vec![triage_frk_1_large()]);
            io.stdout = Box::new(shared);
            io.sleeper = Some(sleeper);
        })
    });

    // Blocked in its idle wait, having said so once, and not exited.
    waiting
        .recv_timeout(Duration::from_secs(30))
        .expect("serve waits on its sleeper when the board is idle");
    let idle = out.idle_lines();
    assert_eq!(idle.len(), 1, "{}", out.text());
    assert!(!serving.is_finished(), "{}", out.text());

    // A request filed from another process is picked up, and the gate is released once.
    filed(&repository, "Add done.txt");
    gate.add_permits(1);
    until("the triage session starts", || {
        events(&repository, &[EventKind::SessionStarted])
            .iter()
            .any(|event| match &event.body {
                EventBody::SessionStarted(body) => body.purpose.to_string() == "triage",
                _ => false,
            })
    });

    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let seen = out.idle_lines();
    assert_eq!(
        seen.iter().filter(|line| **line == idle[0]).count(),
        1,
        "{seen:?}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stops_on_farik_stop() {
    let repository = a_team("serve-stop");
    let serving = serving(&repository.path, &free_port(), Vec::new());
    until("the daemon is up", || daemon_file(&repository).exists());

    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(!daemon_file(&repository).exists());
}
