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
use farik_protocol::clock::FixedClock;
use farik_protocol::event::{EventBody, EventKind};
use farik_runtime::recorded::fixtures::{refine_asks_frk_1, triage_frk_1_large};
use farik_runtime::sleep::Sleeper;
use farik_store::git::fixtures::TempRepo;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use project::{
    Ran, a_team, events, filed, hold_the_run_lock, joined, recorded, run, run_with, scratch,
};

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

/// A sleeper that says it was entered, and until when, and then returns only when the test
/// releases it, once.
struct GatedSleeper {
    entered: Mutex<Sender<DateTime<Utc>>>,
    gate: Arc<Semaphore>,
}

impl Sleeper for GatedSleeper {
    fn sleep_until(&self, until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let _ = self.entered.lock().expect("the sender").send(until);
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
    // A state folder already there with a looser mode is tightened.
    std::fs::create_dir_all(state.join("farik")).expect("the folder is made");
    std::fs::set_permissions(state.join("farik"), std::fs::Permissions::from_mode(0o755))
        .expect("the mode is set");
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
fn writes_no_state_before_the_driver_starts() {
    let repository = a_team("serve-refused");
    let state = scratch("serve-refused-state");
    let _lock = hold_the_run_lock(&repository);
    let ran = run_with(&repository.path, &["serve", "--port", &free_port()], |io| {
        io.engine = recorded(Vec::new());
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
    });
    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);
    assert!(!state.join("farik/state.json").exists(), "{}", ran.err);
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
            // The triage makes FRK-1 an epic, so serve refines it next; the refine asks the human
            // and leaves the board waiting on them, so serve goes back to its idle wait.
            io.engine = recorded(vec![triage_frk_1_large(), refine_asks_frk_1()]);
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

    // Back in its idle wait, with no permit left, so the stop lands before any other tick.
    waiting
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or_else(|_| panic!("serve waits again once the board is idle\n{}", out.text()));
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    // The idle line is said again after the ticks that acted, though its reason is the same.
    let seen = out.idle_lines();
    assert_eq!(
        seen.iter().filter(|line| **line == idle[0]).count(),
        2,
        "{seen:?}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn waits_on_the_injected_clock_when_idle() {
    let repository = a_team("serve-clock");
    let port = free_port();
    // A clock a year from the machine's, so that a wait reckoned from the machine's time shows.
    let now = project::at() + chrono::Duration::days(365);
    let (entered, waiting) = channel();
    let gate = Arc::new(Semaphore::new(0));
    let sleeper = Arc::new(GatedSleeper {
        entered: Mutex::new(entered),
        gate: Arc::clone(&gate),
    });
    let root = repository.path.clone();
    let serving = std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port], |io| {
            io.engine = recorded(Vec::new());
            io.clock = Arc::new(FixedClock::new(now));
            io.sleeper = Some(sleeper);
        })
    });

    let until = waiting
        .recv_timeout(Duration::from_secs(30))
        .expect("serve waits on its sleeper when the board is idle");
    // The day's wait, capped at the minute's recheck, both from the clock serve was given.
    assert_eq!(until, now + chrono::Duration::seconds(60));
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
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

/// The link a line is, as `(port, code)`: `open http://127.0.0.1:<port>/connect#<code> in your
/// browser`, the code sixty-four lowercase hex digits.
fn link_of(line: &str) -> Option<(u16, String)> {
    let rest = line
        .strip_prefix("open http://127.0.0.1:")?
        .strip_suffix(" in your browser")?;
    let (port, code) = rest.split_once("/connect#")?;
    let hex = code.len() == 64
        && code
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    (!port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && hex)
        .then(|| (port.parse().ok(), code.to_string()))
        .and_then(|(port, code)| Some((port?, code)))
}

fn links(text: &str) -> Vec<(u16, String)> {
    text.lines().filter_map(link_of).collect()
}

/// `farik serve` on a thread, printing into `out`.
fn serving_into(root: &Path, out: &SharedOut) -> std::thread::JoinHandle<Ran> {
    let root = root.to_path_buf();
    let port = free_port();
    let shared = out.clone();
    std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port], |io| {
            io.engine = recorded(Vec::new());
            io.stdout = Box::new(shared);
        })
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_a_one_time_link() {
    let repository = a_team("serve-link");
    let out = SharedOut::default();
    let serving = serving_into(&repository.path, &out);
    until("the link is printed", || !links(&out.text()).is_empty());
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let printed = links(&out.text());
    assert_eq!(printed.len(), 1, "{}", out.text());
    assert!(
        out.text()
            .lines()
            .filter(|line| line.starts_with("open "))
            .count()
            == 1,
        "{}",
        out.text()
    );

    let ran = run_with(&repository.path, &["run"], |io| {
        io.engine = recorded(Vec::new());
    });
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(links(&ran.out).is_empty(), "{}", ran.out);
    assert!(!ran.out.contains("/connect#"), "{}", ran.out);
}

/// Trades `code` at `POST /connect` for the session cookie's `name=value`.
fn connected(port: u16, code: &str) -> String {
    use std::io::Read as _;

    let body = json!({ "code": code }).to_string();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connects");
    write!(
        stream,
        "POST /connect HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("sent");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("read");
    assert!(answer.starts_with("HTTP/1.1 204"), "{answer}");
    answer
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("set-cookie").then(|| {
                value
                    .trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
        })
        .expect("a cookie is set")
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn serve_status_has_no_credential_under_a_given_engine() {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let repository = a_team("serve-status");
    let out = SharedOut::default();
    let serving = serving_into(&repository.path, &out);
    until("the link is printed", || !links(&out.text()).is_empty());
    let (port, code) = links(&out.text()).remove(0);
    let cookie = connected(port, &code);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let status = runtime.block_on(async {
        let mut request = format!("ws://127.0.0.1:{port}/rpc")
            .into_client_request()
            .expect("a request");
        let headers = request.headers_mut();
        headers.insert(
            "Origin",
            format!("http://127.0.0.1:{port}")
                .parse()
                .expect("a header"),
        );
        headers.insert("Cookie", cookie.parse().expect("a header"));
        let (mut socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .expect("the socket opens");
        let asked = json!({
            "jsonrpc": "2.0", "id": 1, "method": "query",
            "params": { "name": "serve.status", "params": {} }
        });
        socket
            .send(Message::Text(asked.to_string().into()))
            .await
            .expect("sent");
        let frame = tokio::time::timeout(Duration::from_secs(30), socket.next())
            .await
            .expect("an answer in time")
            .expect("the socket is open")
            .expect("a frame");
        serde_json::from_str::<Value>(frame.to_text().expect("text")).expect("JSON")
    });

    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let root = repository.path.canonicalize().expect("the root");
    assert_eq!(
        status["result"],
        json!({
            "project_root": root.display().to_string(),
            "paused": false,
            "credential": null,
            "port": port,
        }),
        "{status}"
    );
}
