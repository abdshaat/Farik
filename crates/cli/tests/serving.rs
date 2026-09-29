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
            "take_on_error": null,
        }),
        "{status}"
    );
}

/// `farik serve <extra>` on a thread whose opener records what it is asked to open, and answers
/// `opened`.
fn serving_opening(
    root: &Path,
    extra: &[&str],
    out: &SharedOut,
    err: &SharedOut,
    asked: &Arc<Mutex<Vec<String>>>,
    opened: Result<(), String>,
) -> std::thread::JoinHandle<Ran> {
    let root = root.to_path_buf();
    let mut args = vec!["serve".to_string(), "--port".to_string(), free_port()];
    args.extend(extra.iter().map(ToString::to_string));
    let (out, err, asked) = (out.clone(), err.clone(), Arc::clone(asked));
    std::thread::spawn(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        run_with(&root, &args, |io| {
            io.engine = recorded(Vec::new());
            io.stdout = Box::new(out);
            io.stderr = Box::new(err);
            io.open_url = Arc::new(move |url| {
                asked.lock().expect("the urls").push(url.to_string());
                opened.clone()
            });
        })
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn opens_the_link_in_a_browser() {
    let repository = a_team("serve-opens");
    let (out, err) = (SharedOut::default(), SharedOut::default());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let serving = serving_opening(&repository.path, &[], &out, &err, &asked, Ok(()));
    until("the link is printed", || !links(&out.text()).is_empty());
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    joined(serving, "the serve");
    let (port, code) = links(&out.text()).remove(0);
    assert_eq!(
        *asked.lock().expect("the urls"),
        vec![format!("http://127.0.0.1:{port}/connect#{code}")]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn does_not_open_with_no_open() {
    let repository = a_team("serve-no-open");
    let (out, err) = (SharedOut::default(), SharedOut::default());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let serving = serving_opening(&repository.path, &["--no-open"], &out, &err, &asked, Ok(()));
    until("the link is printed", || !links(&out.text()).is_empty());
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    joined(serving, "the serve");
    assert!(asked.lock().expect("the urls").is_empty());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_serving_when_no_browser_opens() {
    let repository = a_team("serve-no-browser");
    let (out, err) = (SharedOut::default(), SharedOut::default());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let serving = serving_opening(
        &repository.path,
        &[],
        &out,
        &err,
        &asked,
        Err("no display".to_string()),
    );
    until("the link is printed", || !links(&out.text()).is_empty());
    until("the warning is printed", || !err.text().is_empty());
    assert!(!serving.is_finished());
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(
        err.text()
            .contains("could not open a browser: no display; open the link above yourself"),
        "{}",
        err.text()
    );
}

#[cfg(feature = "e2e")]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn the_e2e_binary_serves_with_recorded_sessions() {
    use std::io::{BufRead as _, BufReader, Read as _};
    use std::process::{Command, Stdio};

    let repository = a_team("serve-e2e");
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_farik-e2e-serve"))
        .args(["--port", &port])
        .current_dir(&repository.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary starts");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let printed = loop {
        let line = lines.next().expect("the link is printed").expect("a line");
        if let Some(link) = link_of(&line) {
            break link;
        }
    };
    assert_eq!(printed.0.to_string(), port);
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", printed.0)).expect("connects");
    write!(
        stream,
        "GET /session HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is sent");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("the answer");
    let stopped = run(&repository.path, &["stop"]);
    let status = child.wait().expect("the binary ends");
    assert!(answer.starts_with("HTTP/1.1 401"), "{answer}");
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    assert!(status.success());
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A runtime for a test's side of the socket.
fn a_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

/// The browser's socket on `port`, with the session `cookie`; retried for 30 s, since across a
/// take-on the daemon is briefly not there.
async fn socket(port: u16, cookie: &str) -> Socket {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
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
        match tokio_tungstenite::connect_async(request).await {
            Ok((socket, _)) => return socket,
            Err(error) => assert!(Instant::now() < deadline, "the socket opens: {error}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Sends `method` with `params` and answers the reply.
async fn ask(socket: &mut Socket, id: u64, method: &str, params: Value) -> Value {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;

    let asked = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    socket
        .send(Message::Text(asked.to_string().into()))
        .await
        .expect("sent");
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(60), socket.next())
            .await
            .expect("an answer in time")
            .expect("the socket is open")
            .expect("a frame");
        if let Message::Text(text) = frame {
            return serde_json::from_str(text.as_str()).expect("JSON");
        }
    }
}

/// `serve.status`, on a socket of its own.
fn serve_status(port: u16, cookie: &str) -> Value {
    a_runtime().block_on(async {
        let mut socket = socket(port, cookie).await;
        let status = json!({ "name": "serve.status", "params": {} });
        ask(&mut socket, 1, "query", status).await["result"].clone()
    })
}

/// `method` on a socket of its own, answering the reply.
fn call(port: u16, cookie: &str, method: &str, params: Value) -> Value {
    a_runtime().block_on(async {
        let mut socket = socket(port, cookie).await;
        ask(&mut socket, 1, method, params).await
    })
}

/// `call`, also answering whether the daemon then closed the socket within 30 s.
fn call_then_closed(port: u16, cookie: &str, method: &str, params: Value) -> (Value, bool) {
    use futures_util::StreamExt as _;
    use tokio_tungstenite::tungstenite::Message;

    a_runtime().block_on(async {
        let mut socket = socket(port, cookie).await;
        let answer = ask(&mut socket, 1, method, params).await;
        let closed = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match socket.next().await {
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    Some(Ok(_)) => {}
                }
            }
        })
        .await
        .is_ok();
        (answer, closed)
    })
}

/// `farik serve` in setup mode or not, on a thread, with its link traded for a cookie.
struct Serving {
    out: SharedOut,
    err: SharedOut,
    interrupt: tokio::sync::mpsc::UnboundedSender<()>,
    thread: std::thread::JoinHandle<Ran>,
    port: u16,
    cookie: String,
}

/// `farik serve` in `cwd`, with `PATH`, `HOME` at `home`, the state folder at `state`, an API key
/// in the environment, and interrupts the test sends.
fn serving_setup(cwd: &Path, home: &Path, state: &Path) -> Serving {
    let (out, err) = (SharedOut::default(), SharedOut::default());
    let (interrupt, interrupts) = tokio::sync::mpsc::unbounded_channel();
    let mut env = project::a_bare_env();
    env.insert("HOME".to_string(), home.display().to_string());
    env.insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
    env.insert(
        "ANTHROPIC_API_KEY".to_string(),
        "sk-ant-api03-test".to_string(),
    );
    let (cwd, port) = (cwd.to_path_buf(), free_port());
    let (shared_out, shared_err) = (out.clone(), err.clone());
    let thread = std::thread::spawn(move || {
        run_with(&cwd, &["serve", "--port", &port], |io| {
            io.engine = recorded(Vec::new());
            io.env = env;
            io.stdout = Box::new(shared_out);
            io.stderr = Box::new(shared_err);
            io.interrupts = farik::Interrupts::Channel(interrupts);
        })
    });
    until("the link is printed", || {
        !links(&out.text()).is_empty() || thread.is_finished()
    });
    let Some((port, code)) = links(&out.text()).first().cloned() else {
        panic!("no link\n{}\n{}", out.text(), err.text());
    };
    let cookie = connected(port, &code);
    Serving {
        out,
        err,
        interrupt,
        thread,
        port,
        cookie,
    }
}

impl Serving {
    /// Interrupts serve once and answers how it ended.
    fn interrupted(self) -> (Ran, String, String) {
        self.interrupt.send(()).expect("serve listens");
        let ran = joined(self.thread, "the serve");
        (ran, self.out.text(), self.err.text())
    }
}

/// The folders a setup test runs in: an empty one to run in, and a state folder.
fn setup_folders(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    (
        scratch(&format!("{name}-cwd")),
        scratch(&format!("{name}-state")),
    )
}

/// The name `path` has inside its parent, which the tests make home.
fn name_of(path: &Path) -> String {
    path.file_name()
        .expect("a name")
        .to_string_lossy()
        .to_string()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn serves_setup_outside_a_project() {
    let (cwd, state) = setup_folders("setup-outside");
    let serving = serving_setup(&cwd, &cwd, &state);
    let status = serve_status(serving.port, &serving.cookie);
    let (ran, out, err) = serving.interrupted();
    assert_eq!(status["project_root"], Value::Null, "{status}");
    assert_eq!(ran.code, 130, "{out}\n{err}");
    assert!(!cwd.join(".farik").exists(), "no daemon.json, nor anything");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reopens_the_last_project() {
    let repository = a_team("setup-reopens");
    let (cwd, state) = setup_folders("setup-reopens");
    std::fs::create_dir_all(state.join("farik")).expect("the folder");
    let root = repository.path.canonicalize().expect("the root");
    std::fs::write(
        state.join("farik/state.json"),
        json!({ "last_project": root.display().to_string() }).to_string(),
    )
    .expect("state.json");
    let serving = serving_setup(&cwd, &cwd, &state);
    let status = serve_status(serving.port, &serving.cookie);
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
    assert_eq!(
        status["project_root"],
        json!(root.display().to_string()),
        "{status}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn creates_a_project_paused_with_its_first_request() {
    let (home, state) = setup_folders("setup-creates");
    let serving = serving_setup(&home, &home, &state);
    let description = "A shop for bread, with an order page and a daily menu.";
    let create = |description: &str| json!({ "parent": "", "name": "bakery", "description": description, "no_sandbox": false });

    let short = call(
        serving.port,
        &serving.cookie,
        "project.create",
        create("seventeen chars!!"),
    );
    assert_eq!(short["error"]["code"], -32005, "{short}");
    assert_eq!(
        short["error"]["message"],
        "say a little more about the project: at least 20 characters"
    );
    assert!(!home.join("bakery").exists());

    let made = call(
        serving.port,
        &serving.cookie,
        "project.create",
        create(description),
    );
    let root = home.join("bakery").canonicalize().expect("bakery is made");
    assert_eq!(
        made["result"]["project_root"],
        json!(root.display().to_string()),
        "{made}"
    );
    until("the team is driven", || {
        root.join(".farik/local/daemon.json").exists()
    });
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");

    assert!(root.join(".git").is_dir());
    let readme = std::fs::read_to_string(root.join("README.md")).expect("a README");
    assert!(readme.contains(description), "{readme}");
    assert!(root.join(".farik/team.yaml").is_file());
    assert!(root.join(".farik/local/setup-pending").is_file());
    let log = farik_store::open_event_log(&root.join(".farik/local/farik.db"), project::at())
        .expect("the log opens");
    let paused = log
        .read(&farik_store::EventQuery {
            kinds: vec![EventKind::TeamPaused],
            ..farik_store::EventQuery::default()
        })
        .expect("the log reads");
    assert_eq!(paused.len(), 1);
    let contract = farik_store::files::ProjectFiles::open(root.clone())
        .read_contract(&"FRK-1".parse().expect("an id"))
        .expect("the first request is filed");
    assert_eq!(contract.intent.as_str(), description);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn takes_on_the_chosen_project_on_the_same_port() {
    let repository = TempRepo::new("setup-takes-on");
    let home = repository.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-takes-on");
    let serving = serving_setup(&cwd, &home, &state);

    let (opened, closed) = call_then_closed(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&repository.path), "no_sandbox": false }),
    );
    let root = repository.path.canonicalize().expect("the root");
    assert_eq!(
        opened["result"]["project_root"],
        json!(root.display().to_string()),
        "{opened}"
    );
    assert!(closed, "the setup daemon closes the socket");
    until("the team is driven", || {
        root.join(".farik/local/daemon.json").exists()
    });
    let status = serve_status(serving.port, &serving.cookie);
    let stopped = run(&root, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());

    assert_eq!(
        status["project_root"],
        json!(root.display().to_string()),
        "{status}"
    );
    assert_eq!(status["paused"], true, "{status}");
    assert_eq!(status["port"], serving.port, "{status}");
    assert_eq!(
        links(&serving.out.text()).len(),
        1,
        "{}",
        serving.out.text()
    );
    let written: Value = serde_json::from_str(
        &std::fs::read_to_string(state.join("farik/state.json")).expect("state.json"),
    )
    .expect("JSON");
    assert_eq!(written["last_project"], json!(root.display().to_string()));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stays_in_setup_when_the_project_is_busy() {
    let repository = a_team("setup-busy");
    let home = repository.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-busy");
    let _lock = hold_the_run_lock(&repository);
    let serving = serving_setup(&cwd, &home, &state);

    let refused = call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&repository.path), "no_sandbox": false }),
    );
    let status = serve_status(serving.port, &serving.cookie);
    let (ran, out, err) = serving.interrupted();
    assert_eq!(refused["error"]["code"], -32005, "{refused}");
    assert_eq!(
        refused["error"]["message"],
        "another farik is already running this project"
    );
    assert_eq!(status["project_root"], Value::Null, "{status}");
    assert!(!state.join("farik/state.json").exists());
    assert_eq!(ran.code, 130, "{out}\n{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn goes_back_to_setup_when_the_driver_cannot_start() {
    let repository = a_team("setup-cannot-start");
    std::fs::write(repository.path.join(".farik/prices.json"), "not JSON").expect("written");
    let home = repository.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-cannot-start");
    let serving = serving_setup(&cwd, &home, &state);

    let opened = call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&repository.path), "no_sandbox": false }),
    );
    assert!(opened["result"]["project_root"].is_string(), "{opened}");
    let mut status = Value::Null;
    until("serve is back in setup mode with the reason", || {
        status = serve_status(serving.port, &serving.cookie);
        status["take_on_error"].is_string()
    });
    let (ran, out, err) = serving.interrupted();
    assert_eq!(status["project_root"], Value::Null, "{status}");
    assert!(
        status["take_on_error"]
            .as_str()
            .is_some_and(|why| why.contains("prices.json")),
        "{status}"
    );
    assert!(!state.join("farik/state.json").exists());
    assert_eq!(ran.code, 130, "{out}\n{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_ctrl_c_after_the_take_on() {
    let repository = a_team("setup-ctrl-c");
    let home = repository.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-ctrl-c");
    let serving = serving_setup(&cwd, &home, &state);

    call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&repository.path), "no_sandbox": false }),
    );
    until("the team is driven", || daemon_file(&repository).exists());
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");
    assert!(!daemon_file(&repository).exists());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn writes_no_sandbox_into_the_project() {
    let repository = TempRepo::new("setup-no-sandbox");
    let home = repository.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-no-sandbox");
    let serving = serving_setup(&cwd, &home, &state);

    call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&repository.path), "no_sandbox": true }),
    );
    until("the team is driven", || daemon_file(&repository).exists());
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(repository.path.join(".farik/local/settings.json"))
            .expect("settings.json"),
    )
    .expect("JSON");
    assert_eq!(settings, json!({ "sandbox": "none" }));
    assert!(err.contains("warning: no-sandbox mode"), "{err}");
}
