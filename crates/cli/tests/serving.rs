//! `catervas serve` (`docs/SPEC.md` 8.1): the process that keeps driving when the board is idle,
//! remembers its project, and ends as `catervas run` does. Driven by the recorded adapter through
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

use catervas_core::governor::gates::DesignerBrowser;
use catervas_protocol::clock::FixedClock;
use catervas_protocol::event::{EventBody, EventKind};
use catervas_runtime::recorded::fixtures::{refine_asks_ctv_1, triage_ctv_1_large};
use catervas_runtime::sleep::Sleeper;
use catervas_store::git::fixtures::TempRepo;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use project::{
    Ran, a_team, events, filed, hold_the_run_lock, joined, record, recorded, run, run_with,
    scratch, the_run_lock_frees, walked,
};

#[path = "../../runtime/tests/support/ports.rs"]
mod ports;

/// A port nothing holds now, from below the ephemeral range, as text for `--port`.
fn free_port() -> String {
    ports::free_port().to_string()
}

fn daemon_file(repository: &TempRepo) -> std::path::PathBuf {
    repository.path.join(".catervas/local/daemon.json")
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

/// `catervas serve --port <port>` in `root` on a thread, on an engine with no transcripts.
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
    std::fs::create_dir_all(state.join("catervas")).expect("the folder is made");
    std::fs::set_permissions(
        state.join("catervas"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("the mode is set");
    let serving = serving(
        &repository.path,
        &free_port(),
        vec![("XDG_CONFIG_HOME", state.display().to_string())],
    );
    let file = state.join("catervas/state.json");
    until("state.json is written", || file.exists());

    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&state.join("catervas")), 0o700);
    assert_eq!(mode(&file), 0o600);
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("state.json reads"))
            .expect("state.json is JSON");
    let root = repository.path.canonicalize().expect("the root");
    assert_eq!(
        written,
        json!({
            "last_project": root.display().to_string(),
            "projects": [root.display().to_string()],
        })
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
    assert!(!state.join("catervas/state.json").exists(), "{}", ran.err);
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
            // The triage makes CTV-1 an epic, so serve refines it next; the refine asks the human
            // and leaves the board waiting on them, so serve goes back to its idle wait.
            io.engine = recorded(vec![triage_ctv_1_large(), refine_asks_ctv_1()]);
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

    // Back in its idle wait, with no permit left, so the stop lands before any other tick. The
    // sleeper is also entered while a session runs (the next tick waits for it, ADR 0064), so the
    // idle line itself says that the board is idle again.
    until("serve says it is idle again", || {
        out.idle_lines().len() == 2
    });
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

/// A campaign Catervas made for a plan, recorded in the project's log: the watch on the ad spend has
/// something to look at once it is there.
fn a_campaign_was_made(repository: &TempRepo) {
    record(
        repository,
        "",
        "marketing_campaign.created",
        &json!({
            "plan": "MP-1", "key": "search-launch", "account": "123-456-7890",
            "campaign": "customers/1234567890/campaigns/11",
            "budget": "customers/1234567890/campaignBudgets/12",
            "budget_kind": "total", "amount": "500.00"
        }),
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn watches_the_ad_spend_beside_its_ticks() {
    let repository = a_team("serve-ad-watch");
    a_campaign_was_made(&repository);
    let now = project::at() + chrono::Duration::days(365);
    let (entered, waiting) = channel();
    let gate = Arc::new(Semaphore::new(0));
    let sleeper = Arc::new(GatedSleeper {
        entered: Mutex::new(entered),
        gate: Arc::clone(&gate),
    });
    let root = repository.path.clone();
    let port = free_port();
    let serving = std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port], |io| {
            io.engine = recorded(Vec::new());
            io.clock = Arc::new(FixedClock::new(now));
            io.sleeper = Some(sleeper);
        })
    });

    // Two waits on the sleeper, both a minute from the clock serve was given: the board's idle
    // wait, and the watch's, which has looked at the campaign and waits for its next wake.
    for _ in 0..2 {
        let until = waiting
            .recv_timeout(Duration::from_secs(30))
            .expect("serve and its ad watch each wait on the sleeper");
        assert_eq!(until, now + chrono::Duration::seconds(60));
    }
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stops_on_catervas_stop() {
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

/// `catervas serve` on a thread, printing into `out`.
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
fn serve_status_is_not_signed_in_without_a_claude_code() {
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
            "signed_in": false,
            "port": port,
            "take_on_error": null,
            "setup_pending": false,
            "leaving": null,
            "keys_copied": null,
        }),
        "{status}"
    );
}

/// What `team.get` says `sandboxed` is while `catervas serve` runs in `repository`.
fn sandboxed_while_serving(repository: &TempRepo) -> Value {
    let out = SharedOut::default();
    let serving = serving_into(&repository.path, &out);
    until("the link is printed", || !links(&out.text()).is_empty());
    let (port, code) = links(&out.text()).remove(0);
    let cookie = connected(port, &code);
    let got = call(
        port,
        &cookie,
        "query",
        json!({ "name": "team.get", "params": {} }),
    );
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    got["result"]["sandboxed"].clone()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn tells_the_page_whether_it_runs_in_the_sandbox_by_the_setting() {
    // The setting, not Docker's answer: the Docker daemon may not be there at all, and the page
    // is not held up asking it.
    let unsandboxed = a_team("serve-sandboxed-none");
    assert_eq!(sandboxed_while_serving(&unsandboxed), json!(false));

    let sandboxed = a_team("serve-sandboxed-docker");
    std::fs::write(
        sandboxed.path.join(".catervas/local/settings.json"),
        r#"{"sandbox":"docker"}"#,
    )
    .expect("the settings are written");
    assert_eq!(sandboxed_while_serving(&sandboxed), json!(true));
}

/// What the governor's door says, asked by `ask` in a project whose sandbox setting is `docker`,
/// the moment the engine is made: once the daemon listens, which is as soon as a page that
/// reconnects can ask it anything.
fn told_at_listen<T: Send + 'static>(
    test: &str,
    ask: impl Fn(&catervas_runtime::tools::ToolContext) -> T + Send + Sync + 'static,
) -> T {
    use catervas::Engine;
    use catervas_core::budget::DEFAULT_SESSION_LIMITS;
    use catervas_runtime::SessionPurpose;
    use catervas_runtime::daemon::SessionRegistration;

    let repository = a_team(test);
    std::fs::write(
        repository.path.join(".catervas/local/settings.json"),
        r#"{"sandbox":"docker"}"#,
    )
    .expect("the settings are written");
    let seen: Arc<Mutex<Option<T>>> = Arc::default();
    let kept = Arc::clone(&seen);
    let port = free_port();
    let root = repository.path.clone();
    let serving = std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port], |io| {
            let Engine::Given(engine) = recorded(Vec::new()) else {
                unreachable!("`recorded` is a given engine");
            };
            io.engine = Engine::Given(Arc::new(move |daemon| {
                // The engine is made once the daemon listens; what the governor's door knows
                // then is what a page asked first would be told.
                daemon.register_session(SessionRegistration {
                    read_roots: Vec::new(),
                    session_id: "probe".to_string(),
                    agent_id: "probe".to_string(),
                    task_id: None,
                    purpose: SessionPurpose::Triage,
                    in_reply_to: None,
                    thread: None,
                    skills: Vec::new(),
                    skills_root: None,
                    cwd: std::path::PathBuf::new(),
                    executor: None,
                    limits: DEFAULT_SESSION_LIMITS,
                    catervas_tools: Vec::new(),
                    tiers: Vec::new(),
                    connectors: Vec::new(),
                    preview: None,
                });
                let told = daemon.tool_context("probe").map(|context| ask(&context));
                daemon.end_session("probe");
                *kept.lock().expect("the answer") = told;
                engine(daemon)
            }));
        })
    });
    until("the engine is made", || {
        seen.lock().expect("the answer").is_some()
    });
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving, "the serve");
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    seen.lock()
        .expect("the answer")
        .take()
        .expect("the probe was told")
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn knows_the_sandbox_setting_from_the_moment_it_listens() {
    // A tab that reconnects asks `team.get` the moment the daemon listens, and the engine is made
    // right after that: a setting told only later would let that tab read `false` in between.
    let sandboxed = told_at_listen("serve-sandbox-at-listen", |context| {
        context.deps.transitions.sandboxed()
    });
    assert!(sandboxed);
}

#[test]
#[ignore = "needs the git program and Docker: cargo xtask check --integration"]
fn knows_what_runs_previews_from_the_moment_it_listens() {
    // `task.get` of a task in `verifying` and `team.propose` ask whether the Designer can have a
    // browser, and read `NoSandbox` until the driver has said what runs previews: with Docker
    // answering and no preview in the starter team, the answer is `NoPreview`.
    let browser = told_at_listen("serve-previews-at-listen", |context| {
        let team = context.deps.files.read_team().expect("the team is read");
        context.deps.transitions.designer_browser(&team)
    });
    assert_eq!(browser, DesignerBrowser::NoPreview);
}

/// `catervas serve <extra>` on a thread whose opener records what it is asked to open, and answers
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
    // Its own settings folder and no keychain, so that the real ones are not written to.
    let (home, state) = (scratch("serve-e2e-home"), scratch("serve-e2e-state"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_catervas-e2e-serve"))
        .args(["--port", &port, "--no-keychain"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &state)
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
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn answers_the_setup_page_while_docker_info_hangs() {
    use std::io::{BufRead as _, BufReader};
    use std::process::{Command, Stdio};

    // A `docker` whose `info` never answers by itself: Catervas gives it 10 seconds.
    let directory = scratch("serve-docker-hangs-bin");
    let source = directory.join("docker.txt");
    std::fs::write(
        &source,
        "#!/bin/sh\n[ \"$1\" = info ] && exec sleep 60\nexit 1\n",
    )
    .expect("written");
    // Copied rather than written in place, as `a_claude_saying` says.
    let program = directory.join("docker");
    let copied = Command::new("cp")
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
    let repository = a_team("serve-docker-hangs");
    std::fs::write(
        repository.path.join(".catervas/local/settings.json"),
        r#"{"sandbox":"docker"}"#,
    )
    .expect("the settings are written");

    let port = free_port();
    // Its own settings folder and no keychain, so that the real ones are not written to.
    let (home, state) = (
        scratch("serve-docker-hangs-home"),
        scratch("serve-docker-hangs-state"),
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_catervas-e2e-serve"))
        .args([
            "--port",
            &port,
            "--no-keychain",
            "--sandbox-image",
            "catervas-sandbox-unused",
        ])
        .env("PATH", path)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &state)
        .current_dir(&repository.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary starts");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let (port, code) = loop {
        let line = lines.next().expect("the link is printed").expect("a line");
        if let Some(link) = link_of(&line) {
            break link;
        }
    };
    let cookie = connected(port, &code);
    let asked = Instant::now();
    let proposed = call(
        port,
        &cookie,
        "query",
        json!({ "name": "team.propose", "params": {} }),
    );
    let waited = asked.elapsed();
    let stopped = run(&repository.path, &["stop"]);
    let status = child.wait().expect("the binary ends");

    // `docker info` is asked before the daemon listens and never on a request's path: the page is
    // answered at once, with the Designer as it is without the Docker that did not answer.
    assert!(
        waited < Duration::from_secs(5),
        "team.propose took {waited:?}"
    );
    assert_eq!(
        proposed["result"]["unavailable"],
        json!([{ "agent_id": "iris", "reason": "designer_needs_sandbox" }]),
        "{proposed}"
    );
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    assert!(status.success());
}

#[cfg(feature = "e2e")]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn waits_in_the_wizard_for_a_missing_sandbox_image() {
    use std::io::{BufRead as _, BufReader};
    use std::process::{Command, Stdio};

    // A `docker` that answers, whose sandbox image is there only once `built` is.
    let directory = scratch("serve-image-missing-bin");
    let built = directory.join("built");
    let source = directory.join("docker.txt");
    std::fs::write(
        &source,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  version|rm|run) exit 0 ;;\n  image) [ -f '{}' ] ;;\n  \
             *) exit 1 ;;\nesac\n",
            built.display()
        ),
    )
    .expect("written");
    // Copied rather than written in place, as `a_claude_saying` says.
    let program = directory.join("docker");
    let copied = Command::new("cp")
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
    // A task in progress in a sandboxed project: the first tick makes its sandbox.
    let repository = a_team("serve-image-missing");
    std::fs::write(
        repository.path.join(".catervas/local/settings.json"),
        r#"{"sandbox":"docker"}"#,
    )
    .expect("the settings are written");
    let request = repository.path.join("request.yaml");
    std::fs::write(&request, project::a_request("Write done.txt")).expect("written");
    let filed = run(
        &repository.path,
        &["task", "create", request.to_str().expect("a path")],
    );
    assert_eq!(filed.code, 0, "{}", filed.err);
    std::fs::remove_file(&request).expect("removed");
    project::record(
        &repository,
        "CTV-1",
        "request.triaged",
        &json!({ "size": "small", "reason": "One file.", "triaged_by": "human" }),
    );
    let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
    for (from, to) in [
        ("draft", "refining"),
        ("refining", "ready"),
        ("ready", "assigned"),
        ("assigned", "in_progress"),
    ] {
        project::moved(&repository, "CTV-1", from, to, &people);
    }

    let (home, state) = (
        scratch("serve-image-missing-home"),
        scratch("serve-image-missing-state"),
    );
    let errors = state.join("stderr.txt");
    let mut child = Command::new(env!("CARGO_BIN_EXE_catervas-e2e-serve"))
        .args([
            "--port",
            &free_port(),
            "--no-keychain",
            "--sandbox-image",
            "catervas-sandbox-unused",
        ])
        .env("PATH", path)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &state)
        .current_dir(&repository.path)
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&errors).expect("the error file"))
        .spawn()
        .expect("the binary starts");
    let out = Arc::new(Mutex::new(String::new()));
    let kept = Arc::clone(&out);
    let (linked, link) = channel();
    let stdout = child.stdout.take().expect("stdout");
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(found) = link_of(&line) {
                let _ = linked.send(found);
            }
            let mut out = kept.lock().expect("the output");
            out.push_str(&line);
            out.push('\n');
        }
    });
    let (port, code) = link
        .recv_timeout(Duration::from_secs(60))
        .expect("the link is printed");
    let cookie = connected(port, &code);

    // The sandbox cannot be made: serve goes back to the wizard, waiting on the project, on the
    // same port, so the owner can prepare the image there.
    let mut status = Value::Null;
    let mut exited = None;
    until("serve is back in setup mode", || {
        exited = child.try_wait().expect("the binary is asked");
        status = serve_status_across_a_restart(port, &cookie).unwrap_or_default();
        exited.is_some() || status.is_object() && status["project_root"].is_null()
    });
    if let Some(exited) = exited {
        joined(reader, "the output");
        let said = std::fs::read_to_string(&errors).unwrap_or_default();
        panic!(
            "serve ended ({exited}):\n{}{said}",
            out.lock().expect("the output")
        );
    }
    let check = |cookie: &str| {
        call(
            port,
            cookie,
            "query",
            json!({ "name": "computer.check", "params": {} }),
        )
    };
    let missing = check(&cookie);
    assert_eq!(
        missing["result"]["sandbox_image"]["state"], "missing",
        "{missing}"
    );
    assert_eq!(missing["result"]["taking_on"], false, "{missing}");

    // Once the image is there, the check takes the project on again.
    std::fs::write(&built, "").expect("written");
    let ready = check(&cookie);
    assert_eq!(ready["result"]["taking_on"], true, "{ready}");
    until("the project is driven again", || {
        status = serve_status_across_a_restart(port, &cookie).unwrap_or_default();
        status["project_root"].is_string()
    });
    // Its task's session waits until it is aborted, which the second Ctrl-C does.
    let interrupt = || {
        let sent = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success());
    };
    interrupt();
    std::thread::sleep(Duration::from_millis(500));
    interrupt();
    let ended = child.wait().expect("the binary ends");
    joined(reader, "the output");
    let out = out.lock().expect("the output").clone();
    assert_eq!(ended.code(), Some(130), "{out}");
    assert!(
        out.contains(&format!(
            "the sandbox is not ready: prepare it in the browser, on 127.0.0.1:{port}"
        )),
        "{out}"
    );
    assert_eq!(links(&out).len(), 1, "{out}");
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
                pair.starts_with("catervas_session=") && pair.len() > "catervas_session=".len()
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_catervas-e2e-serve"))
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
            .join(format!("catervas-git-preview-{}-ThreadId1", child.id()))
            .join(".catervas/team.yaml"),
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
    assert!(!team.contains("plan_in_sprints"), "{team}");
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
    socket_at(port, "/rpc", cookie).await
}

/// `socket`, to the page `path` of the hub: `/rpc`, `/hub/rpc` or `/t/<team>/rpc`.
async fn socket_at(port: u16, path: &str, cookie: &str) -> Socket {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match try_socket_at(port, path, cookie).await {
            Ok(socket) => return socket,
            Err(error) => assert!(Instant::now() < deadline, "the socket opens: {error}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The browser's socket on `port`, with the session `cookie`, tried once. The handshake is given
/// up after 5 s: a port that is held but never served takes the connection and never answers.
async fn try_socket(port: u16, cookie: &str) -> Result<Socket, String> {
    try_socket_at(port, "/rpc", cookie).await
}

/// `try_socket`, to the page `path`.
async fn try_socket_at(port: u16, path: &str, cookie: &str) -> Result<Socket, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let mut request = format!("ws://127.0.0.1:{port}{path}")
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

/// `serve.status` of the page `path`, tried once: `None` when nothing answers there now.
fn status_at(port: u16, path: &str, cookie: &str) -> Option<Value> {
    a_runtime().block_on(async {
        let mut socket = try_socket_at(port, path, cookie).await.ok()?;
        let status = json!({ "name": "serve.status", "params": {} });
        let answer = try_ask(&mut socket, 1, "query", status).await.ok()?;
        Some(answer["result"].clone())
    })
}

/// `method` on the page `path`, answering the reply.
fn call_at(port: u16, path: &str, cookie: &str, method: &str, params: Value) -> Value {
    a_runtime().block_on(async {
        let mut socket = socket_at(port, path, cookie).await;
        ask(&mut socket, 1, method, params).await
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

/// `catervas serve` in setup mode or not, on a thread, with its link traded for a cookie.
struct Serving {
    out: SharedOut,
    err: SharedOut,
    interrupt: tokio::sync::mpsc::UnboundedSender<()>,
    thread: std::thread::JoinHandle<Ran>,
    port: u16,
    cookie: String,
}

/// `catervas serve` in `cwd`, with `HOME` at `home`, the state folder at `state`, a signed-in fake
/// Claude Code on `PATH`, and interrupts the test sends.
fn serving_setup(cwd: &Path, home: &Path, state: &Path) -> Serving {
    let mut env = setup_env(home, state);
    signed_in(&mut env, &format!("serving-setup-{}", name_of(home)));
    serving_in(cwd, env, true)
}

/// Puts a fake `claude` that is signed in first on `env`'s `PATH`: Claude Code's sign-in is what
/// serve asks (ADR 0084).
fn signed_in(env: &mut std::collections::BTreeMap<String, String>, name: &str) {
    let (_directory, path) = project::a_claude_saying(name, "2.1.300 (Claude Code)");
    env.insert("PATH".to_string(), path);
}

/// `PATH`, `HOME` at `home`, and the state folder at `state`: whatever `claude` the `PATH` finds.
fn setup_env(home: &Path, state: &Path) -> std::collections::BTreeMap<String, String> {
    let mut env = project::a_bare_env();
    env.insert("HOME".to_string(), home.display().to_string());
    env.insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
    env
}

/// `catervas serve` in `cwd` with `env` alone, on the recorded engine when `recorded_engine`, else on
/// Claude Code's, with interrupts the test sends.
fn serving_in(
    cwd: &Path,
    env: std::collections::BTreeMap<String, String>,
    recorded_engine: bool,
) -> Serving {
    serving_in_with(cwd, env, recorded_engine, None)
}

/// `serving_in`, trying a team driven elsewhere again after `retry` when it is given.
fn serving_in_with(
    cwd: &Path,
    env: std::collections::BTreeMap<String, String>,
    recorded_engine: bool,
    retry: Option<Duration>,
) -> Serving {
    serving_custom(cwd, env, recorded_engine, retry, Box::new(|_| {}))
}

/// `serving_in_with`, then `customize` of the harness before serve starts.
fn serving_custom(
    cwd: &Path,
    env: std::collections::BTreeMap<String, String>,
    recorded_engine: bool,
    retry: Option<Duration>,
    customize: Box<dyn FnOnce(&mut catervas::CliIo<'_>) + Send>,
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
            if let Some(retry) = retry {
                io.elsewhere_retry = retry;
            }
            io.stdout = Box::new(shared_out);
            io.stderr = Box::new(shared_err);
            io.interrupts = catervas::Interrupts::Channel(interrupts);
            customize(io);
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
    assert!(
        !cwd.join(".catervas").exists(),
        "no daemon.json, nor anything"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reopens_the_last_project() {
    let repository = a_team("setup-reopens");
    let (cwd, state) = setup_folders("setup-reopens");
    std::fs::create_dir_all(state.join("catervas")).expect("the folder");
    let root = repository.path.canonicalize().expect("the root");
    std::fs::write(
        state.join("catervas/state.json"),
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

    let padded = format!("  {description}\n");
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
        create(&padded),
    );
    let root = home.join("bakery").canonicalize().expect("bakery is made");
    assert_eq!(
        made["result"]["project_root"],
        json!(root.display().to_string()),
        "{made}"
    );
    until("the team is driven", || {
        root.join(".catervas/local/daemon.json").exists()
    });
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");

    assert!(root.join(".git").is_dir());
    let readme = std::fs::read_to_string(root.join("README.md")).expect("a README");
    assert!(readme.contains(description), "{readme}");
    assert!(root.join(".catervas/team.yaml").is_file());
    assert!(root.join(".catervas/local/setup-pending").is_file());
    let log =
        catervas_store::open_event_log(&root.join(".catervas/local/catervas.db"), project::at())
            .expect("the log opens");
    let paused = log
        .read(&catervas_store::EventQuery {
            kinds: vec![EventKind::TeamPaused],
            ..catervas_store::EventQuery::default()
        })
        .expect("the log reads");
    assert_eq!(paused.len(), 1);
    // The project is recorded as new, with the paragraph trimmed.
    assert_eq!(
        catervas_store::product_plan::new_project(&log).expect("the log reads"),
        Some(catervas_store::product_plan::NewProject {
            name: "bakery".to_string(),
            description: description.to_string(),
        })
    );
    let started = log
        .read(&catervas_store::EventQuery {
            kinds: vec![EventKind::ProjectStarted],
            ..catervas_store::EventQuery::default()
        })
        .expect("the log reads");
    assert_eq!(started.len(), 1);
    let contract = catervas_store::files::ProjectFiles::open(root.clone())
        .read_contract(&"CTV-1".parse().expect("an id"))
        .expect("the first request is filed");
    // The request keeps the paragraph as it was sent; only the event trims it.
    assert_eq!(contract.intent.as_str(), padded);
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
        root.join(".catervas/local/daemon.json").exists()
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
        &std::fs::read_to_string(state.join("catervas/state.json")).expect("state.json"),
    )
    .expect("JSON");
    assert_eq!(written["last_project"], json!(root.display().to_string()));
    // Taking on a repository does not make it a new project.
    let log =
        catervas_store::open_event_log(&root.join(".catervas/local/catervas.db"), project::at())
            .expect("the log opens");
    assert_eq!(
        catervas_store::product_plan::new_project(&log).expect("the log reads"),
        None
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn serves_a_project_folder() {
    let app = TempRepo::new("setup-folder-app");
    let infra = TempRepo::new("setup-folder-infra");
    let home = app.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-folder");
    let parent = format!("catervas-serving-folder-{}", std::process::id());
    let _ = std::fs::remove_dir_all(home.join(&parent));
    let serving = serving_setup(&cwd, &home, &state);

    let (made, closed) = call_then_closed(
        serving.port,
        &serving.cookie,
        "project.create_folder",
        json!({
            "parent": parent,
            "name": "shop",
            "repos": [
                { "name": "app", "path": name_of(&app.path) },
                { "name": "infra", "path": name_of(&infra.path), "integration_branch": "main" },
            ],
            "no_sandbox": false,
        }),
    );
    let root = home
        .join(&parent)
        .join("shop")
        .canonicalize()
        .expect("the project folder is made");
    assert_eq!(
        made["result"]["project_root"],
        json!(root.display().to_string()),
        "{made}"
    );
    assert!(closed, "the setup daemon closes the socket");
    until("the team is driven", || {
        root.join(".catervas/local/daemon.json").exists()
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
    assert!(root.join(".catervas/repos.yaml").is_file());
    let _ = std::fs::remove_dir_all(home.join(&parent));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn lists_the_members_of_a_served_project_folder() {
    let app = TempRepo::new("setup-members-app");
    let infra = TempRepo::new("setup-members-infra");
    let web = TempRepo::new("setup-members-web");
    let home = app.path.parent().expect("a parent").to_path_buf();
    let (cwd, state) = setup_folders("setup-members");
    let parent = format!("catervas-serving-members-{}", std::process::id());
    let _ = std::fs::remove_dir_all(home.join(&parent));
    let serving = serving_setup(&cwd, &home, &state);

    let (made, _) = call_then_closed(
        serving.port,
        &serving.cookie,
        "project.create_folder",
        json!({
            "parent": parent,
            "name": "shop",
            "repos": [
                { "name": "app", "path": name_of(&app.path) },
                { "name": "infra", "path": name_of(&infra.path), "integration_branch": "main" },
            ],
            "no_sandbox": false,
        }),
    );
    let root = home
        .join(&parent)
        .join("shop")
        .canonicalize()
        .expect("the project folder is made");
    assert!(made["error"].is_null(), "{made}");
    until("the team is driven", || {
        root.join(".catervas/local/daemon.json").exists()
    });
    let members = a_runtime().block_on(async {
        let mut socket = try_socket(serving.port, &serving.cookie).await.ok()?;
        try_ask(
            &mut socket,
            1,
            "query",
            json!({ "name": "repos.list", "params": {} }),
        )
        .await
        .ok()
    });
    let Some(members) = members else {
        panic!(
            "repos.list was not answered\n{}\n{}",
            serving.out.text(),
            serving.err.text()
        );
    };
    // The daemon that drives the folder knows home, so a path relative to it is found.
    let added = call(
        serving.port,
        &serving.cookie,
        "repo.add",
        json!({ "name": "web", "path": name_of(&web.path) }),
    );
    let grown = call(
        serving.port,
        &serving.cookie,
        "query",
        json!({ "name": "repos.list", "params": {} }),
    );
    let stopped = run(&root, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());

    assert_eq!(added["result"], json!({}), "{added}");
    assert_eq!(grown["result"]["repos"][2]["name"], "web", "{grown}");
    assert_eq!(
        members["result"],
        json!({ "repos": [
            {
                "name": "app",
                "path": app.path.canonicalize().expect("a path").display().to_string(),
                "integration_branch": null,
                "missing": false,
                "moved_out": null
            },
            {
                "name": "infra",
                "path": infra.path.canonicalize().expect("a path").display().to_string(),
                "integration_branch": "main",
                "missing": false,
                "moved_out": null
            },
        ] }),
        "{members}"
    );
    let _ = std::fs::remove_dir_all(home.join(&parent));
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
        "another catervas is already running this project"
    );
    assert_eq!(status["project_root"], Value::Null, "{status}");
    assert!(!state.join("catervas/state.json").exists());
    assert_eq!(ran.code, 130, "{out}\n{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn goes_back_to_setup_when_the_driver_cannot_start() {
    let repository = a_team("setup-cannot-start");
    std::fs::write(repository.path.join(".catervas/prices.json"), "not JSON").expect("written");
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
    assert!(!state.join("catervas/state.json").exists());
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
        &std::fs::read_to_string(repository.path.join(".catervas/local/settings.json"))
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
    catervas_store::git::fixtures::git_in(&shop, &["init", "-b", "main"]);
    // Home a git project itself, as some keep their dotfiles: Catervas's settings would be in it.
    catervas_store::git::fixtures::git_in(&home, &["init", "-b", "main"]);
    let (claude, path) = project::a_claude_saying("setup-guards-claude", "2.1.300 (Claude Code)");
    let mut env = setup_env(&home, &state);
    env.insert("PATH".to_string(), path);
    let serving = serving_in(&cwd, env, true);
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
        (
            "project.open",
            open(""),
            "your home folder itself cannot be a project; choose a folder inside it",
        ),
        ("project.create", create("", &escaped), named),
        ("project.create", create("", "Bad Name"), named),
        ("project.open", open("shop/src"), inside.as_str()),
        (
            "project.create",
            create("", "shop"),
            "a folder with that name is already there",
        ),
    ];
    let answers: Vec<Value> = asked
        .iter()
        .map(|(method, params, _)| call(serving.port, &serving.cookie, method, params.clone()))
        .collect();
    // Claude Code signed out: the project steps wait on the account.
    std::fs::write(claude.join("signed-out"), "").expect("a marker");
    let signed_out = [
        call(serving.port, &serving.cookie, "project.open", open("shop")),
        call(
            serving.port,
            &serving.cookie,
            "project.create",
            create("", "bakery"),
        ),
    ];
    let (ran, out, err) = serving.interrupted();
    for ((method, params, sentence), answer) in asked.iter().zip(&answers) {
        assert_eq!(
            answer["error"]["code"], -32005,
            "{method} {params}: {answer}"
        );
        assert_eq!(answer["error"]["message"], *sentence, "{method} {params}");
    }
    for answer in &signed_out {
        assert_eq!(answer["error"]["code"], -32005, "{answer}");
        assert_eq!(
            answer["error"]["message"], "connect your AI account first",
            "{answer}"
        );
    }
    assert!(!home.join("bakery").exists());
    assert!(
        !home
            .parent()
            .expect("a parent")
            .join(&escaped[3..])
            .exists()
    );
    assert!(!shop.join(".catervas/team.yaml").exists());
    assert!(!home.join(".catervas").exists());
    assert!(!state.join("catervas/state.json").exists());
    assert_eq!(ran.code, 130, "{out}\n{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn signing_in_takes_the_waiting_project_on() {
    let repository = a_team("setup-sign-in-takes-on");
    let (_home, state) = setup_folders("setup-sign-in-takes-on");
    let (claude, path) = project::a_claude_saying("setup-sign-in-claude", "2.1.300 (Claude Code)");
    // Not signed in: serve shows the wizard, with the project it found waiting on the account.
    std::fs::write(claude.join("signed-out"), "").expect("a marker");
    let mut env = setup_env(&repository.path, &state);
    env.insert("PATH".to_string(), path);
    let serving = serving_in(&repository.path, env, false);
    let before = serve_status(serving.port, &serving.cookie);
    assert_eq!(before["project_root"], Value::Null, "{before}");
    assert_eq!(before["signed_in"], false, "{before}");

    // Claude Code is signed in (as its login would leave it), and the check takes the project on.
    std::fs::remove_file(claude.join("signed-out")).expect("the marker goes");
    let checked = call(serving.port, &serving.cookie, "account.check", json!({}));
    assert_eq!(
        checked["result"],
        json!({ "outcome": "working", "taking_on": true }),
        "{checked}"
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
    std::fs::write(repository.path.join(".catervas/prices.json"), "not JSON").expect("written");
    let (_home, state) = setup_folders("setup-waits-again");
    let (claude, path) = project::a_claude_saying("setup-waits-claude", "2.1.300 (Claude Code)");
    std::fs::write(claude.join("signed-out"), "").expect("a marker");
    let mut env = setup_env(&repository.path, &state);
    env.insert("PATH".to_string(), path);
    let serving = serving_in(&repository.path, env, false);
    std::fs::remove_file(claude.join("signed-out")).expect("the marker goes");
    let prober = Prober::on(serving.port);

    let first = call(serving.port, &serving.cookie, "account.check", json!({}));
    assert_eq!(first["result"]["taking_on"], true, "{first}");
    let mut status = Value::Null;
    until("serve is back in setup mode with the reason", || {
        status = serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
        status["take_on_error"].is_string()
    });
    let took = prober.stopped();
    // The project is still the one setup waits on: checking again takes it on again.
    let again = call(serving.port, &serving.cookie, "account.check", json!({}));
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

/// The web app's Connect lists Catervas's own connector by starting the program the process was
/// found at (ADR 0038).
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn connector_tools_runs_catervas_s_own_connector() {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let repository = a_team("serve-own-connector");
    let state = scratch("serve-own-connector-state");
    let out = SharedOut::default();
    let root = repository.path.clone();
    let (port_to_use, shared) = (free_port(), out.clone());
    let serving = std::thread::spawn(move || {
        run_with(&root, &["serve", "--port", &port_to_use], |io| {
            io.engine = recorded(Vec::new());
            io.stdout = Box::new(shared);
            io.own_program = Some(std::path::PathBuf::from(env!("CARGO_BIN_EXE_catervas")));
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
        })
    });
    until("the link is printed", || !links(&out.text()).is_empty());
    let (port, code) = links(&out.text()).remove(0);
    let cookie = connected(port, &code);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let answer = runtime.block_on(async {
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
            "jsonrpc": "2.0", "id": 1, "method": "connector.tools",
            "params": { "agent": "dev-a", "server": {
                "name": "osv", "transport": "stdio", "command": "catervas",
                "args": ["connector", "osv"], "credential_keys": []
            } }
        });
        socket
            .send(Message::Text(asked.to_string().into()))
            .await
            .expect("sent");
        let frame = tokio::time::timeout(Duration::from_secs(60), socket.next())
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
    let names: Vec<&str> = answer["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"))
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(
        names,
        ["query_package", "query_packages", "get_vulnerability"]
    );
}

/// Serves `a_team(name)` from its own folder, with `HOME` a scratch folder that does not hold it.
fn serving_a_team(name: &str) -> (TempRepo, Serving) {
    let repository = a_team(name);
    let (home, state) = setup_folders(name);
    let mut env = setup_env(&home, &state);
    signed_in(&mut env, &format!("serving-a-team-{name}"));
    let serving = serving_in(&repository.path, env, true);
    (repository, serving)
}

/// Leaves the project and waits until the wizard answers on the same port.
fn left_for_the_wizard(serving: &Serving) -> Value {
    let (left, closed) =
        call_then_closed(serving.port, &serving.cookie, "project.leave", json!({}));
    assert_eq!(left["result"], json!({}), "{left}");
    assert!(closed, "the driver closes the socket");
    let mut status = None;
    until("the wizard answers", || {
        status = serve_status_across_a_restart(serving.port, &serving.cookie)
            .filter(|status| status["project_root"].is_null());
        status.is_some()
    });
    status.expect("a status")
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn leaves_the_project_for_the_wizard_on_the_same_port() {
    let (repository, serving) = serving_a_team("serve-leaves");
    let root = repository.path.canonicalize().expect("the root");

    let status = left_for_the_wizard(&serving);

    assert_eq!(
        status["leaving"],
        json!(root.display().to_string()),
        "{status}"
    );
    assert_eq!(status["port"], serving.port, "{status}");
    assert_eq!(
        links(&serving.out.text()).len(),
        1,
        "{}",
        serving.out.text()
    );
    let said = format!(
        "the team left {}: choose its next project in the browser",
        root.display()
    );
    assert!(serving.out.text().contains(&said), "{}", serving.out.text());
    the_run_lock_frees(&repository);
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn stays_on_the_project_it_left() {
    let (repository, serving) = serving_a_team("serve-stays");
    let root = repository.path.canonicalize().expect("the root");
    let team = repository.path.join(".catervas/team.yaml");
    let before = std::fs::read(&team).expect("the team");

    left_for_the_wizard(&serving);
    let opened = call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": root.display().to_string(), "no_sandbox": false }),
    );
    assert_eq!(
        opened["result"]["project_root"],
        json!(root.display().to_string()),
        "{opened}"
    );
    until("the team is driven again", || {
        daemon_file(&repository).exists()
    });

    assert_eq!(std::fs::read(&team).expect("the team"), before);
    assert!(
        !repository
            .path
            .join(".catervas/local/setup-pending")
            .exists()
    );
    assert!(
        !repository
            .path
            .join(".catervas/local/keys-copied.json")
            .exists()
    );
    let stopped = run(&root, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

/// `serving_a_team`, with `HOME` the temporary folder, where `TempRepo` makes the repositories a
/// change of project can go to.
fn serving_a_team_under_home(name: &str) -> (TempRepo, Serving, std::path::PathBuf) {
    let repository = a_team(name);
    let state = scratch(&format!("{name}-state"));
    let mut env = setup_env(&std::env::temp_dir(), &state);
    signed_in(&mut env, &format!("serving-under-home-{name}"));
    let serving = serving_in(&repository.path, env, true);
    (repository, serving, state)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn changes_project_from_the_browser() {
    let (repository, serving, state) = serving_a_team_under_home("serve-changes");
    let target = TempRepo::new("serve-changes-target");
    let root = target.path.canonicalize().expect("the root");
    left_for_the_wizard(&serving);

    let opened = call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&target.path), "no_sandbox": false }),
    );
    assert_eq!(
        opened["result"]["project_root"],
        json!(root.display().to_string()),
        "{opened}"
    );
    until("the target is driven", || daemon_file(&target).exists());
    let mut status = Value::Null;
    until("serve answers for the target", || {
        status = serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
        status["project_root"] == json!(root.display().to_string())
    });

    assert_eq!(status["paused"], false, "{status}");
    // The driver answers before it remembers the project, so wait for the write, not the answer.
    let mut written = Value::Null;
    until("the target is remembered", || {
        written = std::fs::read_to_string(state.join("catervas/state.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        written["last_project"] == json!(root.display().to_string())
    });
    assert_eq!(written["last_project"], json!(root.display().to_string()));
    assert!(!root.join(".catervas/local/setup-pending").exists());
    let stopped = run(&root, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
    drop(repository);
}

/// A one-repo team in `name`, served under home with a task filed, and the repository it is to
/// gain, with the place the project folder is asked for.
fn serving_a_conversion(name: &str) -> (TempRepo, Serving, TempRepo, String) {
    let (repository, serving, _state) = serving_a_team_under_home(name);
    let second = TempRepo::new(&format!("{name}-second"));
    let parent = format!("catervas-{name}-{}", std::process::id());
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(&parent));
    until("the team is driven", || daemon_file(&repository).exists());
    until("the team answers", || {
        serve_status_across_a_restart(serving.port, &serving.cookie)
            .is_some_and(|status| !status["project_root"].is_null())
    });
    // Catervas's files are tracked in the repository, so the move leaves a branch that removes them.
    repository.commit("Catervas's files");
    (repository, serving, second, parent)
}

/// What `project.convert` is asked with, naming `second` `web` and the repository `bakery`.
fn conversion_of(parent: &str, second: &TempRepo) -> Value {
    json!({
        "first_name": "bakery",
        "parent": parent,
        "name": "bakery",
        "second": { "name": "web", "path": name_of(&second.path) }
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn adds_a_second_repository_from_the_browser() {
    let (repository, serving, second, parent) = serving_a_conversion("serve-converts");
    // Waiting on the owner, so that no session starts for it on the recorded engine.
    let task = filed(&repository, "A task for the bakery");
    walked(&repository, &task, &["escalated"]);
    let folder = std::env::temp_dir()
        .canonicalize()
        .expect("home")
        .join(&parent)
        .join("bakery");

    let asked = call(
        serving.port,
        &serving.cookie,
        "project.convert",
        conversion_of(&parent, &second),
    );
    assert_eq!(asked["result"], json!({}), "{asked}");

    let mut status = Value::Null;
    let deadline = Instant::now() + Duration::from_secs(30);
    while status["project_root"] != json!(folder.display().to_string()) {
        assert!(
            Instant::now() < deadline,
            "serve did not answer for the project folder\n{status}\n{}\n{}",
            serving.out.text(),
            serving.err.text()
        );
        std::thread::sleep(Duration::from_millis(50));
        status = serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
    }
    assert_eq!(status["port"], serving.port, "{status}");
    assert!(status["take_on_error"].is_null(), "{status}");
    // The team keeps its address: the project folder answers where the repository did.
    let slug = slugs_of(&[&repository]).remove(0);
    let there = status_at(serving.port, &format!("/t/{slug}/rpc"), &serving.cookie)
        .expect("the team answers under its address");
    assert_eq!(there["project_root"], json!(folder.display().to_string()));
    // Both repositories are members now, and the team's task came along with its repository.
    let members = call(
        serving.port,
        &serving.cookie,
        "query",
        json!({ "name": "repos.list", "params": {} }),
    );
    let names: Vec<&str> = members["result"]["repos"]
        .as_array()
        .expect("a list of members")
        .iter()
        .filter_map(|member| member["name"].as_str())
        .collect();
    assert_eq!(names, ["bakery", "web"], "{members}");
    let tasks = a_runtime().block_on(async {
        let mut socket = try_socket(serving.port, &serving.cookie).await.ok()?;
        try_ask(
            &mut socket,
            1,
            "query",
            json!({ "name": "tasks.list", "params": {} }),
        )
        .await
        .ok()
    });
    let Some(tasks) = tasks else {
        panic!(
            "tasks.list was not answered\n{}\n{}",
            serving.out.text(),
            serving.err.text()
        );
    };
    assert!(tasks.to_string().contains(&task), "{tasks}");
    let contract = std::fs::read_to_string(folder.join(format!(".catervas/contracts/{task}.yaml")))
        .expect("the contract moved");
    assert!(contract.contains("repo: bakery"), "{contract}");
    // The repository the team left is not driven any more, says where its team went, and holds
    // the branch that removes what Catervas kept in it.
    assert!(!run_lock_is_held(&repository));
    assert!(
        std::process::Command::new("git")
            .args(["branch", "--list", "catervas/moved-out"])
            .current_dir(&repository.path)
            .output()
            .is_ok_and(|output| !output.stdout.is_empty()),
        "the removal branch"
    );
    assert_eq!(
        std::fs::read_to_string(repository.path.join(".catervas/local/moved-to")).expect("note"),
        folder.display().to_string()
    );

    assert!(
        !serving.thread.is_finished(),
        "serve ended on its own\n{}\n{}",
        serving.out.text(),
        serving.err.text()
    );
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(&parent));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn falls_back_to_the_repository_when_the_move_fails() {
    let (repository, serving, second, parent) = serving_a_conversion("serve-convert-fails");
    let root = repository.path.canonicalize().expect("the root");
    // A branch named `catervas` keeps git from making `catervas/moved-out`.
    repository.git(&["branch", "catervas"]);

    let asked = call(
        serving.port,
        &serving.cookie,
        "project.convert",
        conversion_of(&parent, &second),
    );
    assert_eq!(asked["result"], json!({}), "{asked}");

    let mut status = Value::Null;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !(status["project_root"] == json!(root.display().to_string())
        && status["take_on_error"].is_string())
    {
        assert!(
            Instant::now() < deadline,
            "the repository was not driven again with the sentence\n{status}\n{}\n{}",
            serving.out.text(),
            serving.err.text()
        );
        std::thread::sleep(Duration::from_millis(50));
        status = serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
    }
    assert!(
        status["take_on_error"]
            .as_str()
            .is_some_and(|said| said.contains("catervas/moved-out")),
        "{status}"
    );
    assert!(!std::env::temp_dir().join(&parent).join("bakery").exists());
    assert!(!repository.path.join(".catervas/local/moved-to").exists());

    // The sentence is shown until the next drive, which passes none.
    left_for_the_wizard(&serving);
    let opened = call(
        serving.port,
        &serving.cookie,
        "project.open",
        json!({ "path": root.display().to_string(), "no_sandbox": false }),
    );
    assert_eq!(
        opened["result"]["project_root"],
        json!(root.display().to_string()),
        "{opened}"
    );
    until("the repository is driven without it", || {
        let status =
            serve_status_across_a_restart(serving.port, &serving.cookie).unwrap_or_default();
        status["project_root"] == json!(root.display().to_string())
            && status["take_on_error"].is_null()
    });
    let (ran, out, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{out}\n{err}");
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(&parent));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_how_to_start_when_nothing_runs() {
    let repository = a_team("open-nothing-runs");

    let ran = run(&repository.path, &["open"]);

    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.err.contains("Catervas is not running here"),
        "{}",
        ran.err
    );
    assert!(ran.err.contains("catervas serve"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_a_new_link_from_the_running_driver() {
    let repository = a_team("open-new-link");
    let out = SharedOut::default();
    let serving = serving_into(&repository.path, &out);
    until("the link is printed", || !links(&out.text()).is_empty());
    let (port, first) = links(&out.text())[0].clone();

    let ran = run(&repository.path, &["open"]);
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    joined(serving, "the serve");

    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    let printed = links(&ran.out);
    assert_eq!(printed.len(), 1, "{}", ran.out);
    assert_eq!(printed[0].0, port);
    assert_ne!(printed[0].1, first);
}

/// The team id the log of `repository` says its events belong to.
fn team_id_of(repository: &TempRepo) -> String {
    let log = catervas_store::open_event_log(
        &repository.path.join(".catervas/local/catervas.db"),
        project::at(),
    )
    .expect("the log opens");
    log.read(&catervas_store::EventQuery {
        limit: Some(1),
        ..catervas_store::EventQuery::default()
    })
    .expect("the log reads")
    .first()
    .expect("the log has an event")
    .envelope
    .ids
    .team_id
    .clone()
}

/// The slugs the hub gives `repositories`' teams, in order.
fn slugs_of(repositories: &[&TempRepo]) -> Vec<String> {
    let ids: Vec<String> = repositories.iter().map(|each| team_id_of(each)).collect();
    catervas_runtime::daemon::hub::slugs_for(&ids)
        .iter()
        .map(|slug| slug.as_str().to_string())
        .collect()
}

/// Whether another process holds the run lock of `repository`.
fn run_lock_is_held(repository: &TempRepo) -> bool {
    let file = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(repository.path.join(".catervas/local/run.lock"))
        .expect("the lock file opens");
    matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// `catervas serve` outside any project, with `state.json` listing `repositories`, under a home
/// that holds them.
fn serving_teams(name: &str, repositories: &[&TempRepo]) -> (Serving, std::path::PathBuf) {
    let (cwd, state) = setup_folders(name);
    std::fs::create_dir_all(state.join("catervas")).expect("the state folder");
    let projects: Vec<String> = repositories
        .iter()
        .map(|each| {
            each.path
                .canonicalize()
                .expect("a root")
                .display()
                .to_string()
        })
        .collect();
    std::fs::write(
        state.join("catervas/state.json"),
        json!({ "last_project": projects[0], "projects": projects }).to_string(),
    )
    .expect("state.json");
    let mut env = setup_env(&std::env::temp_dir(), &state);
    signed_in(&mut env, &format!("serving-teams-{name}"));
    (serving_in(&cwd, env, true), state)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn drives_two_teams_at_once() {
    let (first, second) = (a_team("serve-two-a"), a_team("serve-two-b"));
    let (serving, _state) = serving_teams("serve-two", &[&first, &second]);
    until("both teams are driven", || {
        daemon_file(&first).exists() && daemon_file(&second).exists()
    });
    assert!(run_lock_is_held(&first) && run_lock_is_held(&second));

    let slugs = slugs_of(&[&first, &second]);
    for (slug, repository) in slugs.iter().zip([&first, &second]) {
        let status = status_at(serving.port, &format!("/t/{slug}/rpc"), &serving.cookie)
            .expect("the team answers");
        let root = repository.path.canonicalize().expect("the root");
        assert_eq!(status["project_root"], json!(root.display().to_string()));
        assert_eq!(status["port"], serving.port, "{status}");
    }
    // Stopping one team leaves the other driven, and serve running.
    let stopped = run(&first.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    the_run_lock_frees(&first);
    assert!(run_lock_is_held(&second));
    assert!(
        status_at(
            serving.port,
            &format!("/t/{}/rpc", slugs[1]),
            &serving.cookie
        )
        .is_some(),
        "the other team answers"
    );
    assert!(!serving.thread.is_finished());
    let stopped = run(&second.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn adds_a_team_from_the_wizard_beside_the_running_one() {
    let (repository, serving, state) = serving_a_team_under_home("serve-adds");
    let target = TempRepo::new("serve-adds-target");
    let root = target.path.canonicalize().expect("the root");

    let opened = call_at(
        serving.port,
        "/hub/rpc",
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&target.path), "no_sandbox": false }),
    );
    assert_eq!(
        opened["result"]["project_root"],
        json!(root.display().to_string()),
        "{opened}"
    );
    until("the new team is driven", || {
        daemon_file(&target).exists() && run_lock_is_held(&target)
    });
    assert!(run_lock_is_held(&repository), "the first team goes on");
    let mut written = Value::Null;
    until("both teams are listed", || {
        written = std::fs::read_to_string(state.join("catervas/state.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        written["projects"]
            .as_array()
            .is_some_and(|all| all.len() == 2)
    });
    assert_eq!(written["last_project"], json!(root.display().to_string()));

    let again = call_at(
        serving.port,
        "/hub/rpc",
        &serving.cookie,
        "project.open",
        json!({ "path": name_of(&target.path), "no_sandbox": false }),
    );
    let said = again["error"]["message"].as_str().unwrap_or_default();
    assert!(said.contains("team_already_here"), "{again}");
    assert!(said.contains("already one of your teams"), "{again}");

    let stopped = run(&root, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    the_run_lock_frees(&target);
    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn leaves_one_team_and_keeps_the_other() {
    let (first, second) = (a_team("serve-leave-a"), a_team("serve-leave-b"));
    let (serving, state) = serving_teams("serve-leave", &[&first, &second]);
    until("both teams are driven", || {
        daemon_file(&first).exists() && daemon_file(&second).exists()
    });
    let slugs = slugs_of(&[&first, &second]);

    let (left, closed) = a_runtime().block_on(async {
        use futures_util::StreamExt as _;
        let mut socket = socket_at(
            serving.port,
            &format!("/t/{}/rpc", slugs[0]),
            &serving.cookie,
        )
        .await;
        let left = ask(&mut socket, 1, "project.leave", json!({})).await;
        let closed = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(Ok(_)) = socket.next().await {}
        })
        .await
        .is_ok();
        (left, closed)
    });
    assert_eq!(left["result"], json!({}), "{left}");
    assert!(closed, "the team's daemon closes the socket");
    the_run_lock_frees(&first);
    assert!(run_lock_is_held(&second));
    let root = second.path.canonicalize().expect("the root");
    until("the other team still answers", || {
        status_at(
            serving.port,
            &format!("/t/{}/rpc", slugs[1]),
            &serving.cookie,
        )
        .is_some_and(|status| status["project_root"] == json!(root.display().to_string()))
    });
    // The team's daemon frees its lock before serve hears it ended and forgets it.
    until("state.json keeps only the other team", || {
        std::fs::read_to_string(state.join("catervas/state.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|written| written["projects"] == json!([root.display().to_string()]))
    });

    let stopped = run(&second.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn skips_a_team_driven_elsewhere() {
    let (held, free) = (a_team("serve-else-a"), a_team("serve-else-b"));
    let lock = hold_the_run_lock(&held);
    let (cwd, state) = setup_folders("serve-else");
    std::fs::create_dir_all(state.join("catervas")).expect("the state folder");
    let projects: Vec<String> = [&held, &free]
        .iter()
        .map(|each| {
            each.path
                .canonicalize()
                .expect("a root")
                .display()
                .to_string()
        })
        .collect();
    std::fs::write(
        state.join("catervas/state.json"),
        json!({ "last_project": projects[1], "projects": projects }).to_string(),
    )
    .expect("state.json");
    let mut env = setup_env(&std::env::temp_dir(), &state);
    signed_in(&mut env, "serving-teams-serve-else");
    let serving = serving_in_with(&cwd, env, true, Some(Duration::from_millis(200)));

    until("the free team is driven", || daemon_file(&free).exists());
    assert!(!daemon_file(&held).exists(), "the held team is not driven");
    let listed = call_at(
        serving.port,
        "/hub/rpc",
        &serving.cookie,
        "query",
        json!({ "name": "teams.list", "params": {} }),
    );
    let held_root = held
        .path
        .canonicalize()
        .expect("the root")
        .display()
        .to_string();
    let row = listed["result"]["teams"]
        .as_array()
        .and_then(|teams| teams.iter().find(|team| team["root"] == json!(held_root)))
        .unwrap_or_else(|| panic!("the held team is listed: {listed}"));
    assert_eq!(row["status"], "elsewhere", "{listed}");

    drop(lock);
    until("the team is driven once its lock is free", || {
        daemon_file(&held).exists()
    });
    for repository in [&held, &free] {
        let stopped = run(&repository.path, &["stop"]);
        assert_eq!(stopped.code, 0, "{}", stopped.err);
    }
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn sets_the_computer_s_limits() {
    let (repository, serving, state) = serving_a_team_under_home("serve-limits");
    let written = || -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(state.join("catervas/state.json")).expect("state.json"),
        )
        .expect("JSON")
    };
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    let query = |name: &str| hub("query", json!({ "name": name, "params": {} }))["result"].clone();

    let set = hub(
        "computer.set",
        json!({ "agents_at_once": 2, "daily_cap_usd": 25 }),
    );
    assert_eq!(set["result"], json!({}), "{set}");
    let kept = written();
    assert_eq!(kept["agents_at_once"], 2, "{kept}");
    assert_eq!(kept["daily_cap_usd"], 25.0, "{kept}");
    let computer = query("computer.get");
    assert_eq!(computer["agents_at_once"], 2, "{computer}");
    assert_eq!(computer["daily_cap_usd"], 25.0, "{computer}");

    // Out of range values are refused, and change nothing.
    for (params, said) in [
        (
            json!({ "agents_at_once": 17 }),
            "agents_at_once_out_of_range",
        ),
        (
            json!({ "agents_at_once": 0 }),
            "agents_at_once_out_of_range",
        ),
        (json!({ "daily_cap_usd": 0 }), "daily_cap_out_of_range"),
        (
            json!({ "daily_cap_usd": 100_001 }),
            "daily_cap_out_of_range",
        ),
    ] {
        let refused = hub("computer.set", params);
        let message = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains(said), "{refused}");
    }
    assert_eq!(written()["agents_at_once"], 2);

    // A field that is left out is kept, and null clears one.
    hub("computer.set", json!({ "daily_cap_usd": null }));
    let kept = written();
    assert_eq!(kept["agents_at_once"], 2, "{kept}");
    assert!(kept.get("daily_cap_usd").is_none(), "{kept}");
    hub("computer.set", json!({ "agents_at_once": null }));
    let computer = query("computer.get");
    assert_eq!(computer["agents_at_once"], Value::Null, "{computer}");
    assert_eq!(computer["daily_cap_usd"], Value::Null, "{computer}");

    let stopped = run(&repository.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn removes_a_team_and_keeps_its_files() {
    let (first, second) = (a_team("serve-remove-a"), a_team("serve-remove-b"));
    let (serving, state) = serving_teams("serve-remove", &[&first, &second]);
    until("both teams are driven", || {
        daemon_file(&first).exists() && daemon_file(&second).exists()
    });
    let slugs = slugs_of(&[&first, &second]);

    let asked = |params: Value| {
        call_at(
            serving.port,
            "/hub/rpc",
            &serving.cookie,
            "teams.remove",
            params,
        )
    };
    let refused = asked(json!({ "slug": slugs[0] }));
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("confirmation_needed")),
        "{refused}"
    );
    assert!(run_lock_is_held(&first), "nothing was stopped");
    let removed = asked(json!({ "slug": slugs[0], "confirmed": true }));
    assert_eq!(removed["result"], json!({ "removed": true }), "{removed}");
    the_run_lock_frees(&first);
    let root = second.path.canonicalize().expect("the root");
    until("the team is forgotten", || {
        std::fs::read_to_string(state.join("catervas/state.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|written| written["projects"] == json!([root.display().to_string()]))
    });
    assert!(
        first.path.join(".catervas/team.yaml").exists(),
        "no file is deleted"
    );
    assert!(run_lock_is_held(&second));
    let unknown = asked(json!({ "slug": slugs[0], "confirmed": true }));
    assert!(
        unknown["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("team_unknown")),
        "{unknown}"
    );

    let stopped = run(&second.path, &["stop"]);
    assert_eq!(stopped.code, 0, "{}", stopped.err);
    let ran = joined(serving.thread, "the serve");
    assert_eq!(ran.code, 0, "{}", serving.err.text());
}

/// A home folder with a `shop` folder under it and one session worked in `shop` a while ago, as
/// Claude Code's history holds it, and a `settings.json` of Claude Code's with two keys.
fn a_home_with_history(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use catervas_core::claude_history::fixtures::{a_reply_line, a_turn_line};
    use catervas_core::pricing::Usage;

    let (home, state) = setup_folders(name);
    let home = home.canonicalize().expect("a home");
    std::fs::create_dir_all(home.join("shop")).expect("a project folder");
    let history = home.join(".claude/projects/-shop");
    std::fs::create_dir_all(&history).expect("a history folder");
    let id = "11111111-1111-4111-8111-111111111111";
    let at = (Utc::now() - chrono::Duration::hours(2))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let cwd = home.join("shop").display().to_string();
    let usage = Usage {
        input_tokens: 10,
        output_tokens: 5,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
    };
    std::fs::write(
        history.join(format!("{id}.jsonl")),
        format!(
            "{}\n{}\n",
            a_turn_line(id, &cwd, &at),
            a_reply_line(id, &cwd, &at, "msg_1", "claude-opus-5-5", usage)
        ),
    )
    .expect("a transcript");
    let settings = home.join(".claude/settings.json");
    std::fs::write(&settings, r#"{"theme":"dark","model":"opus"}"#).expect("settings");
    std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o600)).expect("a mode");
    (home, state)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn finds_projects_through_serve() {
    let (home, state) = a_home_with_history("serve-finding");
    let serving = serving_setup(&home, &home, &state);
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    let written = || -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(state.join("catervas/state.json")).expect("state.json"),
        )
        .expect("JSON")
    };

    assert_eq!(
        hub("finding.offer", json!({}))["result"],
        json!({ "history": "claude_code", "sessions_dir_exists": true })
    );
    assert_eq!(hub("finding.start", json!({}))["result"]["started"], true);
    let mut status = Value::Null;
    until("the scan is done", || {
        status = hub("finding.status", json!({}))["result"].clone();
        status["phase"] == "done"
    });
    assert_eq!(status["files_read"], 1, "{status}");
    assert_eq!(status["sessions"], 1, "{status}");
    let listed = hub("finding.list", json!({}));
    let row = &listed["result"]["main"][0];
    assert_eq!(row["name"], "shop", "{listed}");
    assert_eq!(row["suggested"], "team", "{listed}");
    assert_eq!(
        row["folders"],
        json!([home.join("shop").display().to_string()])
    );

    // Pausing is kept for the next serve, and stops the next scan from starting.
    hub("finding.pause", json!({ "paused": true }));
    assert_eq!(written()["finding_paused"], true);
    let refused = hub("finding.start", json!({}));
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("finding_paused")),
        "{refused}"
    );
    hub("finding.pause", json!({ "paused": false }));
    assert!(written().get("finding_paused").is_none(), "{}", written());
    hub("finding.export_dismissed", json!({}));
    assert_eq!(written()["export_card"], "dismissed");

    // Keeping the history longer is the one write into Claude Code's settings.
    let settings = home.join(".claude/settings.json");
    assert_eq!(
        hub("finding.retention", json!({}))["result"],
        json!({ "cleanup_period_days": 30, "default": true })
    );
    let kept = hub("finding.keep_longer", json!({ "days": 365 }));
    assert_eq!(kept["result"]["cleanup_period_days"], 365, "{kept}");
    let text = std::fs::read_to_string(&settings).expect("settings");
    let after: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(
        after,
        json!({ "theme": "dark", "model": "opus", "cleanupPeriodDays": 365 })
    );
    let mode = std::fs::metadata(&settings)
        .expect("settings")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let refused = hub("finding.keep_longer", json!({ "days": 30 }));
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("retention_not_offered")),
        "{refused}"
    );

    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

/// `git` with `args` in `folder`, as a test's own author, answering its output.
fn git_in(folder: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(folder)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one test of a copy from its bare repository to the project it becomes"
)]
fn copies_a_repository_to_this_computer() {
    use catervas_runtime::claude::Secret;
    use catervas_runtime::connectors::MemoryConnectorSecrets;
    use catervas_runtime::github::{CloneCall, GithubPlaces, run_git, save_connection};

    let (home, state) = setup_folders("serve-github-copy");
    let home = home.canonicalize().expect("a home");
    // The "GitHub" the clone talks to is a bare repository on this computer, under owner/name.git.
    let remote = scratch("serve-github-copy-remote")
        .canonicalize()
        .expect("a remote folder");
    let made = remote.join("made");
    std::fs::create_dir_all(&made).expect("a folder");
    git_in(&made, &["init", "-b", "main"]);
    std::fs::write(made.join("README.md"), "# An invented shop\n").expect("a file");
    git_in(&made, &["add", "README.md"]);
    git_in(&made, &["commit", "-m", "Start"]);
    std::fs::create_dir_all(remote.join("octo")).expect("an owner folder");
    let bare = remote.join("octo/shop.git");
    git_in(
        &remote,
        &[
            "clone",
            "--bare",
            &made.display().to_string(),
            &bare.display().to_string(),
        ],
    );

    let token = "ghp_invented_for_the_copy_test";
    let secrets = Arc::new(MemoryConnectorSecrets::default());
    save_connection(secrets.as_ref(), &Secret::new(token.to_string()), "octo").expect("a token");
    let spawned: Arc<Mutex<Vec<CloneCall>>> = Arc::default();
    let places = GithubPlaces {
        // Nothing here asks GitHub's API: the token is already kept.
        api: "http://127.0.0.1:1".to_string(),
        clone_base: format!("file://{}", remote.display()),
        spawn: {
            let spawned = Arc::clone(&spawned);
            Arc::new(move |call| {
                spawned.lock().expect("calls").push(call.clone());
                run_git(call)
            })
        },
    };
    let mut env = setup_env(&home, &state);
    signed_in(&mut env, "serving-github-copy");
    let serving = serving_custom(
        &home,
        env,
        true,
        None,
        Box::new(move |io| {
            io.github = places;
            io.connector_secrets = secrets;
        }),
    );
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };

    let copied = hub(
        "github.copy",
        json!({ "full_name": "octo/shop", "parent": "work" }),
    );
    let folder = home.join("work/shop");
    assert_eq!(
        copied["result"]["folder"],
        json!(folder.display().to_string()),
        "{copied}"
    );
    // The folder is a git repository whose origin is the given address.
    assert_eq!(
        git_in(&folder, &["remote", "get-url", "origin"]),
        format!("file://{}/octo/shop.git", remote.display())
    );
    assert!(folder.join("README.md").exists());
    // The token went to git through its environment alone.
    let calls = spawned.lock().expect("calls").clone();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        calls[0]
            .args
            .iter()
            .all(|argument| !argument.contains(token)),
        "{:?}",
        calls[0].args
    );
    assert!(
        calls[0]
            .env
            .iter()
            .any(|(name, value)| name == "GIT_CONFIG_VALUE_0" && value.contains(token)),
        "{:?}",
        calls[0]
            .env
            .iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    );
    // The folder is a project now, and a name that is taken is refused.
    let listed = hub("finding.list", json!({}));
    assert!(
        listed["result"]["main"].as_array().is_some_and(|rows| rows
            .iter()
            .any(|row| row["folders"] == json!([folder.display().to_string()]))),
        "{listed}"
    );
    let again = hub(
        "github.copy",
        json!({ "full_name": "octo/shop", "parent": "work" }),
    );
    assert!(
        again["error"]["message"]
            .as_str()
            .is_some_and(|said| said.starts_with("folder_taken")),
        "{again}"
    );

    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

/// `POST /hub/export` with `body`, from the page's own origin and with `cookie` when it is given:
/// the status and the text of the answer.
fn post_export(port: u16, cookie: Option<&str>, body: &[u8]) -> (u16, String) {
    use std::io::Read as _;

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connects");
    let cookie = cookie.map_or_else(String::new, |cookie| format!("Cookie: {cookie}\r\n"));
    write!(
        stream,
        "POST /hub/export HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\n\
         {cookie}Content-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("sent");
    stream.write_all(body).expect("sent");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("read");
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let status = answer
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status");
    let text = answer.split_once("\r\n\r\n").map_or("", |(_, text)| text);
    (status, text.to_string())
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn imports_an_export_posted_to_the_hub() {
    use catervas_store::claude_export::fixtures::the_2026_10_export;

    let (home, state) = setup_folders("serve-export");
    let serving = serving_setup(&home, &home, &state);
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };

    // The export is read, and its Project is in the list.
    let (status, text) = post_export(serving.port, Some(&serving.cookie), &the_2026_10_export());
    assert_eq!(status, 200, "{text}");
    let summary: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(summary["conversations"], 3, "{summary}");
    assert_eq!(summary["new_projects"], 2, "{summary}");
    let listed = hub("finding.list", json!({}));
    let names: Vec<&str> = listed["result"]["main"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["kind"] == "claude_ai")
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert!(names.contains(&"Shop launch"), "{listed}");
    // The file it was read from is gone, whatever the result.
    let left: Vec<String> = std::fs::read_dir(state.join("catervas"))
        .expect("the state folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("export"))
        .collect();
    assert!(left.is_empty(), "{left:?}");

    // Without the session's cookie it is refused, and what is not a zip is said not to be one.
    let (status, _) = post_export(serving.port, None, &the_2026_10_export());
    assert_eq!(status, 401);
    let (status, text) = post_export(serving.port, Some(&serving.cookie), b"not a zip");
    let refused: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(
        (status, &refused["error"]),
        (422, &json!("not_an_export")),
        "{text}"
    );
    let left = std::fs::read_dir(state.join("catervas"))
        .expect("the state folder")
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy().contains("export"));
    assert!(!left, "no temporary file after a refusal");

    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

/// A home with these folders, each a git repository with one commit of its own and one recent
/// session of Claude Code's worked in it.
fn a_home_with_repos(name: &str, folders: &[&str]) -> (std::path::PathBuf, std::path::PathBuf) {
    use catervas_core::claude_history::fixtures::{a_reply_line, a_turn_line};
    use catervas_core::pricing::Usage;

    let (home, state) = setup_folders(name);
    let home = home.canonicalize().expect("a home");
    for (number, folder) in folders.iter().enumerate() {
        let root = home.join(folder);
        std::fs::create_dir_all(&root).expect("a project folder");
        git_in(&root, &["init", "-b", "main"]);
        // The repository is the owner's: its commits are by the person git knows there.
        git_in(&root, &["config", "user.email", "test@example.com"]);
        std::fs::write(
            root.join("README.md"),
            format!("# {folder}\nAn invented project.\n"),
        )
        .expect("a readme");
        git_in(&root, &["add", "README.md"]);
        git_in(&root, &["commit", "-m", "Start"]);
        let history = home.join(format!(".claude/projects/-{folder}"));
        std::fs::create_dir_all(&history).expect("a history folder");
        let id = format!("2222222{number}-2222-4222-8222-222222222222");
        let at = (Utc::now() - chrono::Duration::hours(2 + i64::try_from(number).expect("a few")))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let cwd = root.display().to_string();
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };
        std::fs::write(
            history.join(format!("{id}.jsonl")),
            format!(
                "{}\n{}\n",
                a_turn_line(&id, &cwd, &at),
                a_reply_line(&id, &cwd, &at, "msg_1", "claude-opus-5-5", usage)
            ),
        )
        .expect("a transcript");
    }
    (home, state)
}

/// The owner's Yes about `a` and `b`, kept as the hub keeps it, with no proof (a pair chosen from
/// the list of those with hints only).
fn said_yes(state: &Path, a: &Path, b: &Path) {
    let mut index = catervas_store::machine::open_machine_index(
        &state
            .join("catervas")
            .join(catervas_store::machine::MACHINE_DB),
        Utc::now(),
    )
    .expect("the machine index");
    index
        .put_link(
            &a.display().to_string(),
            &b.display().to_string(),
            catervas_store::linking::Answer::Yes,
            &[],
            Utc::now(),
        )
        .expect("a link");
}

/// Serve over a home of `folders`, scanned.
fn a_scanned_serve(
    name: &str,
    folders: &[&str],
) -> (Serving, std::path::PathBuf, std::path::PathBuf) {
    let (home, state) = a_home_with_repos(name, folders);
    let serving = serving_setup(&home, &home, &state);
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    assert_eq!(hub("finding.start", json!({}))["result"]["started"], true);
    until("the scan is done", || {
        hub("finding.status", json!({}))["result"]["phase"] == "done"
    });
    (serving, home, state)
}

/// Every team `teams.list` shows, by slug.
fn listed_teams(serving: &Serving) -> Vec<Value> {
    // The hub restarts, closing its sockets, when the wizard takes a project on; a query cut off
    // by that restart is asked again on a new socket.
    let listed = a_runtime().block_on(async {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let mut socket = socket_at(serving.port, "/hub/rpc", &serving.cookie).await;
            let params = json!({ "name": "teams.list", "params": {} });
            match try_ask(&mut socket, 1, "query", params).await {
                Ok(listed) => break listed,
                Err(why) if Instant::now() >= deadline => panic!("teams.list: {why}"),
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    });
    listed["result"]["teams"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The proposal of a home with `api` and `web` that work together, `solo` alone, and a bookkeeping
/// project the owner says they use Zoho Books for, created.
fn a_created_proposal(
    name: &str,
    keeper: bool,
) -> (Serving, std::path::PathBuf, std::path::PathBuf, Value) {
    let folders: &[&str] = if keeper {
        &["api", "web", "solo", "keeper"]
    } else {
        &["api", "web", "solo"]
    };
    let (serving, home, state) = a_scanned_serve(name, folders);
    said_yes(&state, &home.join("api"), &home.join("web"));
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    hub("finding.declare_services", json!({ "ids": ["zoho_books"] }));
    if keeper {
        // Another team that is driven all along: the last team removed would send serve back to
        // its wizard and close the socket the answer comes on.
        hub(
            "finding.choose",
            json!({ "key": home.join("keeper").display().to_string(), "choice": "watch" }),
        );
    }
    let proposed = hub("proposal.get", json!({}));
    let cards = proposed["result"]["cards"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        cards.len(),
        3,
        "{proposed} {}",
        hub("finding.list", json!({}))
    );
    assert!(
        cards.iter().all(|card| card["kind"] == "new_team"),
        "{proposed}"
    );
    let made = hub("proposal.create", json!({ "confirmed": true }));
    if keeper {
        let opened = hub(
            "project.open",
            json!({ "path": "keeper", "no_sandbox": true }),
        );
        assert!(opened.get("error").is_none(), "{opened}");
        until("the keeper is driven", || listed_teams(&serving).len() == 4);
    }
    (serving, home, state, made)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn creates_paused_teams_from_the_proposal() {
    let (serving, home, _state, made) = a_created_proposal("proposal-create", false);
    assert_eq!(
        made["result"]["made"].as_array().map(Vec::len),
        Some(3),
        "{made}"
    );
    assert_eq!(made["result"]["failed"], json!([]), "{made}");

    let teams = listed_teams(&serving);
    assert_eq!(teams.len(), 3, "{teams:?}");
    assert!(
        teams.iter().all(|team| team["status"] == "paused"),
        "{teams:?}"
    );
    assert!(
        teams.iter().all(|team| team["undoable"] == true),
        "{teams:?}"
    );

    // The group is a project folder that lists both members.
    let group = home.join("Catervas/api");
    let repos = std::fs::read_to_string(group.join(".catervas/repos.yaml")).expect("repos.yaml");
    assert!(repos.contains("api") && repos.contains("web"), "{repos}");
    // The lone repository has the template's roles in its own folder.
    let team = std::fs::read_to_string(home.join("solo/.catervas/team.yaml")).expect("team.yaml");
    assert!(
        team.contains("architect") && team.contains("software_developer"),
        "{team}"
    );
    // The bookkeeping project is a new git repository with Catervas's commit, on the Financial
    // analysis team.
    let books = home.join("Catervas/bookkeeping");
    assert_eq!(git_in(&books, &["log", "--format=%an"]), "catervas");
    let team = std::fs::read_to_string(books.join(".catervas/team.yaml")).expect("team.yaml");
    assert!(team.contains("finance_specialist"), "{team}");
    let found = std::fs::read_to_string(books.join(".catervas/local/found-services.json"))
        .expect("found-services.json");
    assert!(found.contains("zoho_books"), "{found}");
    // No team has started a session.
    for root in [&group, &home.join("solo"), &books] {
        let log = catervas_store::open_event_log(
            &root.join(".catervas/local/catervas.db"),
            project::at(),
        )
        .expect("the log opens");
        let started = log
            .read(&catervas_store::EventQuery {
                kinds: vec![EventKind::SessionStarted],
                ..catervas_store::EventQuery::default()
            })
            .expect("the log reads");
        assert!(started.is_empty(), "{root:?}");
    }
    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn undoes_a_team_until_it_starts() {
    let (serving, home, _state, made) = a_created_proposal("proposal-undo", true);
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    let slug_of = |name: &str| -> String {
        made["result"]["made"]
            .as_array()
            .and_then(|all| all.iter().find(|each| each["name"] == name))
            .and_then(|each| each["slug"].as_str())
            .unwrap_or_else(|| panic!("{name} in {made}"))
            .to_string()
    };

    // A team in the owner's own folder: only `.catervas/` goes.
    let solo = hub("teams.undo", json!({ "slug": slug_of("solo") }));
    assert_eq!(solo["result"]["removed"], true, "{solo}");
    assert!(!home.join("solo/.catervas").exists());
    assert!(home.join("solo/README.md").exists());

    // A folder Catervas made, with only Catervas's commits: all of it goes.
    let books = hub("teams.undo", json!({ "slug": slug_of("Bookkeeping") }));
    assert_eq!(books["result"]["removed"], true, "{books}");
    assert!(!home.join("Catervas/bookkeeping").exists());

    // A folder with a commit of the owner's is left, and the owner is told.
    let group = home.join("Catervas/api");
    std::fs::write(group.join("notes.md"), "mine\n").expect("a file");
    git_in(&group, &["add", "notes.md"]);
    git_in(&group, &["commit", "-m", "My notes"]);
    let left = hub("teams.undo", json!({ "slug": slug_of("api") }));
    assert_eq!(left["result"]["removed"], true, "{left}");
    assert_eq!(
        left["result"]["left"],
        json!(format!(
            "Catervas left {}: it has changes of yours.",
            group.display()
        )),
        "{left}"
    );
    assert!(group.join("notes.md").exists());
    until("the teams are gone", || listed_teams(&serving).len() == 1);

    // The projects are on the proposal again.
    let again = hub("proposal.get", json!({}));
    assert_eq!(
        again["result"]["cards"].as_array().map(Vec::len),
        Some(3),
        "{again}"
    );

    // A team that has started a session cannot be undone.
    let made = hub("proposal.create", json!({ "confirmed": true }));
    let slug = made["result"]["made"][0]["slug"]
        .as_str()
        .expect("a slug")
        .to_string();
    let root =
        std::path::PathBuf::from(made["result"]["made"][0]["root"].as_str().expect("a root"));
    let log =
        catervas_store::open_event_log(&root.join(".catervas/local/catervas.db"), project::at())
            .expect("the log opens");
    let first = log
        .read(&catervas_store::EventQuery {
            limit: Some(1),
            ..catervas_store::EventQuery::default()
        })
        .expect("the log reads");
    let ids = first[0].envelope.ids.clone();
    let wire = json!({
        "seq": 1,
        "recorded_at": project::at().to_rfc3339(),
        "team_id": ids.team_id,
        "project_id": ids.project_id,
        "agent_id": "pm",
        "session_id": "s-1",
        "kind": "session.started",
        "body": { "purpose": "refine", "model": "claude-opus-5", "effort": "high" },
    });
    let event = catervas_protocol::event::event_from_value(&wire).expect("a session.started");
    log.append(&catervas_protocol::event::NewEvent {
        recorded_at: event.envelope.recorded_at,
        ids: event.envelope.ids,
        body: event.body,
    })
    .expect("appends");
    let refused = hub("teams.undo", json!({ "slug": slug }));
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.starts_with("team_has_started")),
        "{refused}"
    );
    assert!(root.join(".catervas").exists());
    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn adds_a_linked_project_to_the_team_it_works_with() {
    let (serving, home, state) = a_scanned_serve(
        "proposal-add",
        &["shop", "shopfront", "extra", "single", "other"],
    );
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    // A team of two repositories, and a team of one, both driven now.
    let folder = hub(
        "project.create_folder",
        json!({
            "parent": "suite", "name": "suite", "no_sandbox": true,
            "repos": [{ "name": "shop", "path": "shop" }, { "name": "shopfront", "path": "shopfront" }]
        }),
    );
    assert!(folder.get("error").is_none(), "{folder}");
    until("the first team is driven", || {
        listed_teams(&serving).len() == 1
    });
    let one = hub(
        "project.open",
        json!({ "path": "single", "no_sandbox": true }),
    );
    assert!(one.get("error").is_none(), "{one}");
    until("both teams are listed", || {
        listed_teams(&serving).len() == 2
    });
    // A new scan finds the work of the folder and of nothing else; the projects keep their keys.
    said_yes(&state, &home.join("shop"), &home.join("extra"));
    said_yes(&state, &home.join("single"), &home.join("other"));

    let proposed = hub("proposal.get", json!({}));
    let cards = proposed["result"]["cards"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let adds: Vec<&Value> = cards
        .iter()
        .filter(|card| card["kind"] == "add_to_team")
        .collect();
    assert_eq!(adds.len(), 2, "{proposed}");
    assert!(
        cards.iter().all(|card| card["kind"] == "add_to_team"),
        "{proposed}"
    );

    let made = hub("proposal.create", json!({ "confirmed": true }));
    let added = made["result"]["added"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(added.len(), 1, "{made}");
    let repos =
        std::fs::read_to_string(home.join("suite/suite/.catervas/repos.yaml")).expect("repos.yaml");
    assert!(repos.contains("extra"), "{repos}");
    // A team of one repository takes a second only once the owner agrees to make a project folder
    // of it: the card is told so, with the team, and nothing is changed.
    let failed = made["result"]["failed"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(failed.len(), 1, "{made}");
    assert!(
        failed[0]["reason"]
            .as_str()
            .is_some_and(|said| said.starts_with("conversion_needed")),
        "{made}"
    );
    assert!(failed[0]["team"].is_string(), "{made}");
    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn asks_to_convert_a_one_repo_team_before_linking() {
    let (serving, home, state) = a_scanned_serve("proposal-convert", &["single", "other"]);
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    let one = hub(
        "project.open",
        json!({ "path": "single", "no_sandbox": true }),
    );
    assert!(one.get("error").is_none(), "{one}");
    until("the team is listed", || listed_teams(&serving).len() == 1);
    said_yes(&state, &home.join("single"), &home.join("other"));
    let before = std::fs::read(home.join("single/.catervas/team.yaml")).expect("the team");

    let proposed = hub("proposal.get", json!({}));
    let cards = proposed["result"]["cards"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(cards.len(), 1, "{proposed}");
    assert_eq!(cards[0]["kind"], "add_to_team", "{proposed}");
    let slug = cards[0]["team"].as_str().expect("the team").to_string();
    let made = hub("proposal.create", json!({ "confirmed": true }));

    // The card is told the team must be converted first, and names it.
    let failed = made["result"]["failed"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(failed.len(), 1, "{made}");
    assert!(
        failed[0]["reason"]
            .as_str()
            .is_some_and(|said| said.starts_with("conversion_needed")),
        "{made}"
    );
    assert_eq!(failed[0]["team"], json!(slug), "{made}");
    // Nothing was converted: no project folder lists members, and the team is as it was.
    assert!(!home.join("single/.catervas/repos.yaml").exists());
    assert_eq!(
        std::fs::read(home.join("single/.catervas/team.yaml")).expect("the team"),
        before
    );
    assert!(
        made["result"]["added"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "{made}"
    );
    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}

/// A directory holding a `claude` that is signed in, and for `-p` records its arguments in
/// `args.txt` (one line a run) and its standard input in `stdin.txt`, then prints `answer.json`
/// (or, once `fail` is made beside it, an error), and the `PATH` that finds it first.
fn a_recording_claude(name: &str) -> (std::path::PathBuf, String) {
    let directory = scratch(name);
    let source = directory.join("claude.txt");
    std::fs::write(
        &source,
        "#!/bin/sh\n\
         d=\"$(dirname \"$0\")\"\n\
         if [ \"$1\" = auth ] && [ \"$2\" = status ]; then\n\
           echo '{\"loggedIn\": true, \"authMethod\": \"oauth_token\"}'; exit 0\n\
         fi\n\
         if [ \"$1\" = -p ]; then\n\
           echo \"$@\" >> \"$d/args.txt\"\n\
           cat > \"$d/stdin.txt\"\n\
           if [ -f \"$d/fail\" ]; then\n\
             echo '{\"type\":\"result\",\"is_error\":true,\"result\":\"Credit balance is too low\"}'; exit 1\n\
           fi\n\
           cat \"$d/answer.json\"; exit 0\n\
         fi\n\
         echo '2.1.300 (Claude Code)'\n",
    )
    .expect("written");
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

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one test of the estimate, the one run, what it was shown and what it changed"
)]
fn asks_claude_for_names_with_nothing_secret() {
    let (home, state) = a_home_with_repos("proposal-names", &["api", "web", "solo"]);
    // A README with a key in it, committed.
    let api = home.join("api");
    std::fs::write(
        api.join("README.md"),
        "# api\nAn invented shop api.\nAPI_KEY=sk-test-123\n",
    )
    .expect("a readme");
    git_in(&api, &["add", "README.md"]);
    git_in(&api, &["commit", "-m", "Say what it is"]);
    let (claude, path) = a_recording_claude("proposal-names-claude");
    let mut env = setup_env(&home, &state);
    env.insert("PATH".to_string(), path);
    let serving = serving_in(&home, env, true);
    let hub = |method: &str, params: Value| {
        call_at(serving.port, "/hub/rpc", &serving.cookie, method, params)
    };
    assert_eq!(hub("finding.start", json!({}))["result"]["started"], true);
    until("the scan is done", || {
        hub("finding.status", json!({}))["result"]["phase"] == "done"
    });
    said_yes(&state, &home.join("api"), &home.join("web"));
    hub("finding.declare_services", json!({ "ids": ["zoho_books"] }));

    let estimate = hub("proposal.names_estimate", json!({}));
    let usd = estimate["result"]["usd"].as_f64().expect("a price");
    assert!(usd > 0.0 && usd < 0.10, "{estimate}");
    assert!(
        estimate["result"]["input_tokens"].as_u64().unwrap_or(0) > 0,
        "{estimate}"
    );

    let refused = hub("proposal.names", json!({ "confirmed": false }));
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.starts_with("confirmation_needed")),
        "{refused}"
    );
    assert!(
        !claude.join("args.txt").exists(),
        "nothing ran without the owner's yes"
    );

    let proposed = hub("proposal.get", json!({}));
    let cards = proposed["result"]["cards"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(cards.len(), 3, "{proposed}");
    let id_of = |name: &str| -> String {
        cards
            .iter()
            .find(|card| card["name"] == name)
            .and_then(|card| card["id"].as_str())
            .unwrap_or_else(|| panic!("{name} in {proposed}"))
            .to_string()
    };
    let (pair, solo, books) = (id_of("api"), id_of("solo"), id_of("Bookkeeping"));
    let answer = json!([
        { "id": pair, "name": "Shop", "purpose": "Sells things online", "template": "online-store",
          "link": ["a", "b"], "projects": ["x"] },
        { "id": solo, "name": "N".repeat(60), "purpose": "P".repeat(200), "template": "no-such-team" },
        { "id": "not-a-card", "name": "Ghost" },
    ]);
    let wrapped = json!({
        "type": "result", "subtype": "success", "is_error": false,
        "result": format!("Here you go:\n```json\n{answer}\n```"), "total_cost_usd": 0.02,
    });
    std::fs::write(claude.join("answer.json"), wrapped.to_string()).expect("an answer");

    let names = hub("proposal.names", json!({ "confirmed": true }));
    assert_eq!(names["result"]["cost_usd"], 0.02, "{names}");
    let calls = std::fs::read_to_string(claude.join("args.txt")).expect("one run");
    assert_eq!(calls.lines().count(), 1, "{calls}");
    assert!(calls.contains("--model haiku"), "{calls}");
    let seen = std::fs::read_to_string(claude.join("stdin.txt")).expect("its input");
    for wanted in ["api", "web", "solo", "An invented project"] {
        assert!(seen.contains(wanted), "{wanted} in {seen}");
    }
    assert!(
        !seen.contains("sk-test-123") && !seen.contains("API_KEY"),
        "{seen}"
    );

    let after = hub("proposal.get", json!({}));
    let cards = after["result"]["cards"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        cards.len(),
        3,
        "a link-like key in the answer changes nothing: {after}"
    );
    let card = |id: &str| {
        cards
            .iter()
            .find(|card| card["id"] == id)
            .expect("a card")
            .clone()
    };
    let shop = card(&pair);
    assert_eq!(
        (
            shop["name"].as_str(),
            shop["purpose"].as_str(),
            shop["template"].as_str()
        ),
        (
            Some("Shop"),
            Some("Sells things online"),
            Some("online-store")
        )
    );
    assert_eq!(shop["from_claude"], true);
    assert_eq!(shop["projects"].as_array().map(Vec::len), Some(2), "{shop}");
    let lone = card(&solo);
    assert_eq!(
        lone["name"].as_str().map(|name| name.chars().count()),
        Some(40),
        "{lone}"
    );
    assert_eq!(
        lone["purpose"].as_str().map(|text| text.chars().count()),
        Some(120),
        "{lone}"
    );
    assert_eq!(
        lone["template"], "software-product",
        "a template not in the list is ignored"
    );
    assert_eq!(card(&books)["from_claude"], false);

    // Keep the folder name reverts one card.
    hub("proposal.revert", json!({ "id": pair }));
    let reverted = hub("proposal.get", json!({}));
    let kept = reverted["result"]["cards"]
        .as_array()
        .and_then(|all| all.iter().find(|card| card["id"] == json!(pair)))
        .expect("the card");
    assert_eq!(
        (kept["name"].as_str(), kept["from_claude"].clone()),
        (Some("api"), json!(false))
    );

    // A run that fails says so in Claude Code's words, and changes nothing.
    std::fs::write(claude.join("fail"), "").expect("a failure");
    let failed = hub("proposal.names", json!({ "confirmed": true }));
    let said = failed["error"]["message"].as_str().unwrap_or_default();
    assert!(
        said.starts_with("names_failed") && said.contains("Credit balance is too low"),
        "{failed}"
    );
    let (ran, _, err) = serving.interrupted();
    assert_eq!(ran.code, 130, "{err}");
}
