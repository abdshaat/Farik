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
            "setup_pending": false,
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

#[cfg(feature = "e2e")]
/// The raw answer to `GET <path>` sent to the daemon on `port` with `Host: <host>`, and `cookie`
/// when there is one.
fn get_as(port: u16, host: &str, path: &str, cookie: Option<&str>) -> String {
    use std::io::Read as _;

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connects");
    let cookie = cookie
        .map(|cookie| format!("Cookie: {cookie}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\n{cookie}Connection: close\r\n\r\n"
    )
    .expect("the request is sent");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("the answer");
    answer
}

#[cfg(feature = "e2e")]
/// The `name=value` of the session cookie an answer sets, if it sets one.
fn cookie_set(answer: &str) -> Option<String> {
    answer.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("set-cookie")
            .then(|| {
                value
                    .trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .filter(|pair| {
                pair.starts_with("farik_session=") && pair.len() > "farik_session=".len()
            })
    })
}

#[cfg(feature = "e2e")]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn admits_a_local_browser_without_a_code_in_preview_mode() {
    use std::io::{BufRead as _, BufReader};
    use std::process::{Command, Stdio};

    // Run outside any project: `--preview` makes its own.
    let folder = scratch("serve-preview");
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_farik-e2e-serve"))
        .args(["--preview", "--port", &port])
        .current_dir(&folder)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary starts");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let port: u16 = loop {
        let line = lines.next().expect("the link is printed").expect("a line");
        if let Some((port, _)) = link_of(&line) {
            break port;
        }
    };

    // The preview's own project (`TempRepo::new("preview")` on the binary's main thread) does not
    // plan in sprints, so a request filed in it flows without one.
    let team = std::fs::read_to_string(
        std::env::temp_dir()
            .join(format!("farik-git-preview-{}-ThreadId(1)", child.id()))
            .join(".farik/team.yaml"),
    )
    .unwrap_or_default();
    let admitted = get_as(port, &format!("localhost:{port}"), "/", None);
    let cookie = cookie_set(&admitted);
    let session = cookie
        .as_deref()
        .map(|cookie| get_as(port, &format!("localhost:{port}"), "/session", Some(cookie)));
    let other_port = get_as(port, &format!("localhost:{}", port + 1), "/", None);
    let foreign = get_as(port, "evil.example", "/", None);
    let interrupted = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill runs");
    let status = child.wait().expect("the binary ends");

    assert!(admitted.starts_with("HTTP/1.1 200"), "{admitted}");
    assert!(
        admitted
            .to_ascii_lowercase()
            .contains("content-type: text/html"),
        "{admitted}"
    );
    assert!(cookie.is_some(), "no session cookie: {admitted}");
    let session = session.unwrap_or_default();
    assert!(session.starts_with("HTTP/1.1 204"), "{session}");
    assert!(other_port.starts_with("HTTP/1.1 403"), "{other_port}");
    assert!(cookie_set(&other_port).is_none(), "{other_port}");
    assert!(foreign.starts_with("HTTP/1.1 403"), "{foreign}");
    assert!(cookie_set(&foreign).is_none(), "{foreign}");
    assert!(interrupted.success());
    // What a serve that Ctrl-C ended exits with.
    assert_eq!(status.code(), Some(130), "{status:?}");
    assert!(team.contains("plan_in_sprints: false"), "{team}");
}

#[cfg(not(feature = "e2e"))]
#[test]
fn refuses_preview_without_the_e2e_feature() {
    let folder = scratch("serve-no-preview");
    // A port no serve can take, so that a parser that took `--preview` refuses the port rather
    // than serving for ever.
    let ran = run(&folder, &["serve", "--preview", "--port", "none"]);
    assert_eq!(ran.code, 2, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.err.contains("unexpected argument '--preview'"),
        "{}",
        ran.err
    );
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
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match try_socket(port, cookie).await {
            Ok(socket) => return socket,
            Err(error) => assert!(Instant::now() < deadline, "the socket opens: {error}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The browser's socket on `port`, with the session `cookie`, tried once. The handshake is given
/// up after 5 s: a port that is held but never served takes the connection and never answers.
async fn try_socket(port: u16, cookie: &str) -> Result<Socket, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

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
    tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "no handshake within 5 s".to_string())?
    .map(|(socket, _)| socket)
    .map_err(|error| error.to_string())
}

/// Sends `method` with `params` and answers the reply.
async fn ask(socket: &mut Socket, id: u64, method: &str, params: Value) -> Value {
    try_ask(socket, id, method, params)
        .await
        .unwrap_or_else(|why| panic!("{method}: {why}"))
}

/// `ask`, answering why when the daemon cut the socket before it replied.
async fn try_ask(
    socket: &mut Socket,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;

    let asked = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    socket
        .send(Message::Text(asked.to_string().into()))
        .await
        .map_err(|error| format!("not sent: {error}"))?;
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(60), socket.next())
            .await
            .expect("an answer in time")
            .ok_or("the socket closed")?
            .map_err(|error| format!("no frame: {error}"))?;
        if let Message::Text(text) = frame {
            return Ok(serde_json::from_str(text.as_str()).expect("JSON"));
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

/// `serve.status` while serve may be restarting: `None` when no daemon answers on `port` now, or
/// the one it reached was shutting down and cut the socket before it replied. Tried once; the
/// caller tries again.
fn serve_status_across_a_restart(port: u16, cookie: &str) -> Option<Value> {
    a_runtime().block_on(async {
        let mut socket = try_socket(port, cookie).await.ok()?;
        let status = json!({ "name": "serve.status", "params": {} });
        let answer = try_ask(&mut socket, 1, "query", status).await.ok()?;
        Some(answer["result"].clone())
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
    let mut env = setup_env(home, state);
    env.insert(
        "ANTHROPIC_API_KEY".to_string(),
        "sk-ant-api03-test".to_string(),
    );
    serving_in(cwd, env, true)
}

/// `PATH`, `HOME` at `home`, and the state folder at `state`: no credential.
fn setup_env(home: &Path, state: &Path) -> std::collections::BTreeMap<String, String> {
    let mut env = project::a_bare_env();
    env.insert("HOME".to_string(), home.display().to_string());
    env.insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
    env
}

/// `farik serve` in `cwd` with `env` alone, on the recorded engine when `recorded_engine`, else on
/// Claude Code's, with interrupts the test sends.
fn serving_in(
    cwd: &Path,
    env: std::collections::BTreeMap<String, String>,
    recorded_engine: bool,
) -> Serving {
    let (out, err) = (SharedOut::default(), SharedOut::default());
    let (interrupt, interrupts) = tokio::sync::mpsc::unbounded_channel();
    let (cwd, port) = (cwd.to_path_buf(), free_port());
    let (shared_out, shared_err) = (out.clone(), err.clone());
    let thread = std::thread::spawn(move || {
        run_with(&cwd, &["serve", "--port", &port], |io| {
            if recorded_engine {
                io.engine = recorded(Vec::new());
            }
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
    let prober = Prober::on(serving.port);

    let opened = call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&repository.path), "no_sandbox": false }),
    );
    assert!(opened["result"]["project_root"].is_string(), "{opened}");
    // Meanwhile the setup daemon shuts down and another starts, and a status asked of the one
    // shutting down is cut off unanswered.
    let mut status = Value::Null;
    until("serve is back in setup mode with the reason", || {
        status = serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
        status["take_on_error"].is_string()
    });
    let took = prober.stopped();
    let port = serving.port;
    let (ran, out, err) = serving.interrupted();
    assert_eq!(status["project_root"], Value::Null, "{status}");
    assert!(
        status["take_on_error"]
            .as_str()
            .is_some_and(|why| why.contains("prices.json")),
        "{status}"
    );
    assert!(!state.join("farik/state.json").exists());
    // Serve never lets its port go, so nothing else can take it: setup is back on the port the
    // browser's tab is on, with no second link.
    assert!(
        !took,
        "something took serve's port during the take-on\n{out}"
    );
    let setups: Vec<&str> = out
        .lines()
        .filter(|line| line.starts_with("no project yet"))
        .collect();
    assert_eq!(setups.len(), 2, "{out}");
    for setup in setups {
        assert!(setup.ends_with(&format!("127.0.0.1:{port}")), "{out}");
    }
    let ports: Vec<u16> = links(&out).iter().map(|(port, _)| *port).collect();
    assert_eq!(ports, vec![port], "{out}");
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

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_paths_outside_home_and_bad_names() {
    let (cwd, state) = setup_folders("setup-guards");
    let home = scratch("setup-guards-home");
    let shop = home.join("shop");
    std::fs::create_dir_all(shop.join("src")).expect("the folders");
    farik_store::git::fixtures::git_in(&shop, &["init", "-b", "main"]);
    // No credential kept, and none in the environment.
    let serving = serving_in(&cwd, setup_env(&home, &state), true);
    let description = "A shop for bread, with an order page and a daily menu.";
    let create = |parent: &str, name: &str| json!({ "parent": parent, "name": name, "description": description, "no_sandbox": false });
    let open = |path: &str| json!({ "path": path, "no_sandbox": false });
    let named = "a project's name is lowercase letters, digits, and -, up to 64 characters";
    let shop_root = shop.canonicalize().expect("the shop");
    let inside = format!(
        "that folder is inside a git project; choose {} instead",
        shop_root.display()
    );
    let escaped = format!("../{}-out", name_of(&home));
    let asked = [
        (
            "project.open",
            open("../"),
            "that folder is outside your home folder",
        ),
        ("project.create", create("", &escaped), named),
        ("project.create", create("", "Bad Name"), named),
        ("project.open", open("shop/src"), inside.as_str()),
        (
            "project.create",
            create("", "shop"),
            "a folder with that name is already there",
        ),
        (
            "project.open",
            open("shop"),
            "connect your AI account first",
        ),
        (
            "project.create",
            create("", "bakery"),
            "connect your AI account first",
        ),
    ];
    let answers: Vec<Value> = asked
        .iter()
        .map(|(method, params, _)| call(serving.port, &serving.cookie, method, params.clone()))
        .collect();
    let (ran, out, err) = serving.interrupted();
    for ((method, params, sentence), answer) in asked.iter().zip(&answers) {
        assert_eq!(
            answer["error"]["code"], -32005,
            "{method} {params}: {answer}"
        );
        assert_eq!(answer["error"]["message"], *sentence, "{method} {params}");
    }
    assert!(!home.join("bakery").exists());
    assert!(
        !home
            .parent()
            .expect("a parent")
            .join(&escaped[3..])
            .exists()
    );
    assert!(!shop.join(".farik/team.yaml").exists());
    assert!(!state.join("farik/state.json").exists());
    assert_eq!(ran.code, 130, "{out}\n{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn connecting_the_account_takes_the_waiting_project_on() {
    let repository = a_team("setup-connect-takes-on");
    let (_home, state) = setup_folders("setup-connect-takes-on");
    let (_claude, path) = project::a_claude_saying("setup-connect-claude", "2.1.300 (Claude Code)");
    let mut env = setup_env(&repository.path, &state);
    env.insert("PATH".to_string(), path);
    // Run in a project with no credential, on Claude Code's engine: setup waits on the account.
    let serving = serving_in(&repository.path, env, false);
    let before = serve_status(serving.port, &serving.cookie);
    assert_eq!(before["project_root"], Value::Null, "{before}");

    let connected = call(
        serving.port,
        &serving.cookie,
        "account.connect",
        json!({ "kind": "api_key", "secret": "sk-ant-api03-test" }),
    );
    assert_eq!(
        connected["result"],
        json!({ "stored_in": "keychain", "taking_on": true }),
        "{connected}"
    );
    until("the team is driven", || daemon_file(&repository).exists());
    let status = serve_status(serving.port, &serving.cookie);
    let port = serving.port;
    let (ran, out, err) = serving.interrupted();
    let root = repository.path.canonicalize().expect("the root");
    assert_eq!(
        status["project_root"],
        json!(root.display().to_string()),
        "{status}"
    );
    assert_eq!(ran.code, 130, "{out}\n{err}");
    // It found the project: what it lacks is the account, and it says so.
    let first = out.lines().next().unwrap_or_default();
    assert_eq!(
        first,
        format!("no AI account yet: connect one in the browser, on 127.0.0.1:{port}"),
        "{out}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_waiting_on_the_project_after_a_failed_take_on() {
    let repository = a_team("setup-waits-again");
    std::fs::write(repository.path.join(".farik/prices.json"), "not JSON").expect("written");
    let (_home, state) = setup_folders("setup-waits-again");
    let (_claude, path) = project::a_claude_saying("setup-waits-claude", "2.1.300 (Claude Code)");
    let mut env = setup_env(&repository.path, &state);
    env.insert("PATH".to_string(), path);
    let serving = serving_in(&repository.path, env, false);
    let connect = json!({ "kind": "api_key", "secret": "sk-ant-api03-test" });
    let prober = Prober::on(serving.port);

    let first = call(
        serving.port,
        &serving.cookie,
        "account.connect",
        connect.clone(),
    );
    assert_eq!(first["result"]["taking_on"], true, "{first}");
    let mut status = Value::Null;
    until("serve is back in setup mode with the reason", || {
        status = serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
        status["take_on_error"].is_string()
    });
    let took = prober.stopped();
    // The project is still the one setup waits on: connecting again takes it on again.
    let again = call(serving.port, &serving.cookie, "account.connect", connect);
    let (ran, out, err) = serving.interrupted();
    assert_eq!(again["result"]["taking_on"], true, "{again}");
    assert_eq!(ran.code, 130, "{out}\n{err}");
    assert!(
        !took,
        "something took serve's port during the take-on\n{out}"
    );
    assert_eq!(links(&out).len(), 1, "{out}");
}

/// A thread that tries, over and over, to take `port` on `127.0.0.1`, as any other process on the
/// machine may at any moment: a parallel test asking the system for a free port among them.
struct Prober {
    done: Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<bool>,
}

impl Prober {
    fn on(port: u16) -> Self {
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::clone(&done);
        let thread = std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                    return true;
                }
                std::thread::yield_now();
            }
            false
        });
        Self { done, thread }
    }

    /// Stops trying, and answers whether it ever took the port.
    fn stopped(self) -> bool {
        self.done.store(true, std::sync::atomic::Ordering::SeqCst);
        self.thread.join().expect("the prober ends")
    }
}
