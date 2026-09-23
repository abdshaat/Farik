//! The Claude Code adapter against a fake `claude`: a shell script each test writes into its own
//! directory, which answers `--version`, records its arguments, environment and first stdin line
//! beside itself, and then does what the test asked of it.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use farik_runtime::claude::{ClaudeAdapter, ClaudeConfig, ClaudeCredential, Secret};
use farik_runtime::daemon::DaemonInfo;
use farik_runtime::recorded::fixtures::{a_session_spec, reads_a_file};
use farik_runtime::{
    EndReason, RuntimeAdapter, RuntimeError, SessionEvent, SessionHandle, SessionSpec, StreamParser,
};
use farik_store::files::fixtures::{TempProject, a_team};

const API_KEY: &str = "sk-the-test-key";

/// A project with a team file, and a fake `claude` in it that runs `body` after reading one line.
struct Fake {
    project: TempProject,
    dir: PathBuf,
}

impl Fake {
    fn new(name: &str, body: &str) -> Fake {
        Fake::with_version(name, "2.1.280", body)
    }

    fn with_version(name: &str, version: &str, body: &str) -> Fake {
        Fake::with_version_command(name, &format!("echo '{version} (Claude Code)'"), body)
    }

    /// A fake whose answer to `--version` is the shell line `version_command`.
    fn with_version_command(name: &str, version_command: &str, body: &str) -> Fake {
        let project = TempProject::new(&format!("claude-process-{name}"));
        project
            .files()
            .write_team(&a_team())
            .expect("the team is written");
        let dir = project.root.join("fake");
        std::fs::create_dir_all(&dir).expect("the fake's directory");
        let script = format!(
            "#!/bin/sh\n\
             dir='{dir}'\n\
             if [ \"$1\" = \"--version\" ]; then {version_command}; exit 0; fi\n\
             printf '%s\\n' \"$@\" > \"$dir/args\"\n\
             env > \"$dir/env\"\n\
             echo $$ > \"$dir/pid\"\n\
             IFS= read -r line\n\
             printf '%s\\n' \"$line\" > \"$dir/stdin\"\n\
             {body}\n",
            dir = dir.display()
        );
        // Written by `cp` rather than by this process: a file this process holds open for writing
        // is inherited by whatever another test thread forks meanwhile, and running it then fails
        // with "text file busy".
        let source = dir.join("claude.txt");
        std::fs::write(&source, script).expect("the script is written");
        let path = dir.join("claude");
        let copied = std::process::Command::new("cp")
            .arg(&source)
            .arg(&path)
            .status()
            .expect("cp runs");
        assert!(copied.success(), "the script is copied");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("the script is executable");
        Fake { project, dir }
    }

    fn config(&self) -> ClaudeConfig {
        let farik = self.project.root.join(".farik");
        ClaudeConfig {
            claude_path: self.dir.join("claude"),
            hook_command: PathBuf::from("/usr/local/bin/farik"),
            daemon_file: farik.join("local/daemon.json"),
            daemon: DaemonInfo {
                port: 47_123,
                token: "a-token".to_string(),
                pid: 1,
            },
            sessions_dir: farik.join("local/sessions"),
            team_file: farik.join("team.yaml"),
            env: BTreeMap::from([
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                (
                    "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                    "the-other-credential".to_string(),
                ),
            ]),
        }
    }

    fn adapter(&self) -> ClaudeAdapter {
        ClaudeAdapter::new(
            ClaudeCredential::ApiKey(Secret::new(API_KEY.to_string())),
            self.config(),
        )
        .expect("the fake is new enough")
    }

    fn spec(&self) -> SessionSpec {
        SessionSpec {
            cwd: self.project.root.clone(),
            ..a_session_spec()
        }
    }

    fn written(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.join(name)).unwrap_or_default()
    }

    /// Waits for the script to have written `name`, which it does once it is running.
    async fn wait_for(&self, name: &str) -> String {
        for _ in 0..200 {
            let text = self.written(name);
            if !text.is_empty() {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the fake never wrote {name}");
    }
}

fn transcript_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("src/recorded/transcripts/{name}.jsonl"))
}

async fn drain(handle: &mut dyn SessionHandle) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    while let Some(event) = handle.events().recv().await {
        events.push(event);
    }
    events
}

async fn next_within(handle: &mut dyn SessionHandle, limit: Duration) -> Option<SessionEvent> {
    tokio::time::timeout(limit, handle.events().recv())
        .await
        .expect("an event within the limit")
}

fn is_alive(pid: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("kill -0 {} 2>/dev/null", pid.trim()))
        .status()
        .is_ok_and(|status| status.success())
}

async fn gone_within(pid: &str, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        if !is_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn plays_a_session_through_the_process() {
    let fake = Fake::new(
        "plays",
        &format!("cat '{}'", transcript_path("reads_a_file").display()),
    );
    let mut parser = StreamParser::default();
    let expected: Vec<SessionEvent> = reads_a_file()
        .lines()
        .flat_map(|line| parser.parse_line(line).expect("a recorded line parses"))
        .collect();
    let spec = fake.spec();
    let mut handle = fake
        .adapter()
        .start_session(spec.clone())
        .expect("the session starts");
    let events = tokio::time::timeout(Duration::from_secs(10), drain(handle.as_mut()))
        .await
        .expect("the session ends");
    assert_eq!(events, expected);
    let env = fake.written("env");
    assert!(
        env.contains(&format!("ANTHROPIC_API_KEY={API_KEY}")),
        "{env}"
    );
    assert!(!env.contains("CLAUDE_CODE_OAUTH_TOKEN"), "{env}");
    assert!(!env.contains("CARGO_MANIFEST_DIR"), "{env}");
    assert!(
        env.lines().any(|line| line == "DISABLE_AUTOUPDATER=1"),
        "{env}"
    );
    assert!(
        env.lines()
            .any(|line| line == "CLAUDE_CODE_DISABLE_AUTO_MEMORY=1"),
        "{env}"
    );
    let prompt = fake
        .project
        .root
        .join(".farik/local/sessions")
        .join(&spec.session_id)
        .join("system-prompt.md");
    assert_eq!(
        std::fs::read_to_string(prompt).expect("the prompt file is kept"),
        spec.system_prompt
    );
    assert!(
        fake.written("args")
            .lines()
            .any(|arg| arg == spec.session_id)
    );
}

#[tokio::test]
async fn sends_the_first_prompt_as_a_stream_json_user_line() {
    let fake = Fake::new(
        "first-prompt",
        &format!("cat '{}'", transcript_path("reads_a_file").display()),
    );
    let spec = SessionSpec {
        initial_prompt: "Read \"note.txt\".".to_string(),
        ..fake.spec()
    };
    let mut handle = fake
        .adapter()
        .start_session(spec)
        .expect("the session starts");
    drain(handle.as_mut()).await;
    assert_eq!(
        fake.written("stdin"),
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"Read \\\"note.txt\\\".\"}}\n"
    );
}

#[tokio::test]
async fn ends_a_session_at_its_wall_clock_limit() {
    let fake = Fake::new("wall-clock", "sleep 30");
    let mut spec = fake.spec();
    spec.limits.max_wall_clock = Duration::from_secs(1);
    let mut handle = fake
        .adapter()
        .start_session(spec)
        .expect("the session starts");
    let pid = fake.wait_for("pid").await;
    match next_within(handle.as_mut(), Duration::from_secs(5)).await {
        Some(SessionEvent::Ended {
            reason: EndReason::Limit,
            ..
        }) => {}
        other => panic!("expected the limit's end, got {other:?}"),
    }
    assert!(gone_within(&pid, Duration::from_secs(5)).await);
}

#[tokio::test]
async fn ends_an_aborted_session() {
    let fake = Fake::new("aborted", "sleep 60 &\necho $! > \"$dir/sleep\"\nwait");
    let mut handle = fake
        .adapter()
        .start_session(fake.spec())
        .expect("the session starts");
    let sleep = fake.wait_for("sleep").await;
    handle.abort().expect("the session is stopped");
    match next_within(handle.as_mut(), Duration::from_secs(2)).await {
        Some(SessionEvent::Ended {
            reason: EndReason::Aborted,
            ..
        }) => {}
        other => panic!("expected the abort's end, got {other:?}"),
    }
    assert!(gone_within(&sleep, Duration::from_secs(5)).await);
    assert!(matches!(handle.send("more"), Err(RuntimeError::Aborted)));
}

#[tokio::test]
async fn refuses_a_second_message() {
    let fake = Fake::new("second-message", "sleep 30");
    let handle = fake
        .adapter()
        .start_session(fake.spec())
        .expect("the session starts");
    fake.wait_for("stdin").await;
    match handle.send("and another thing") {
        Err(RuntimeError::Spawn { detail }) => assert!(detail.contains("resume"), "{detail}"),
        other => panic!("expected a refusal naming resume, got {other:?}"),
    }
    handle.abort().expect("the session is stopped");
}

#[tokio::test]
async fn ends_with_the_stderr_tail_when_the_process_dies_without_a_result() {
    let fake = Fake::new("stderr", "echo boom >&2\nexit 1");
    let mut handle = fake
        .adapter()
        .start_session(fake.spec())
        .expect("the session starts");
    let events = tokio::time::timeout(Duration::from_secs(10), drain(handle.as_mut()))
        .await
        .expect("the session ends");
    match events.last() {
        Some(SessionEvent::Ended {
            reason: EndReason::Error,
            detail,
        }) => assert!(detail.contains("boom"), "{detail}"),
        other => panic!("expected an error end, got {other:?}"),
    }
}

#[tokio::test]
async fn refuses_to_resume_a_session_it_did_not_start() {
    let fake = Fake::new("resume-unknown", "exit 0");
    assert!(matches!(
        fake.adapter().resume("some-other-session", "go on"),
        Err(RuntimeError::Spawn { .. })
    ));
}

#[tokio::test]
async fn resumes_a_session_it_started_with_the_resume_flag() {
    let fake = Fake::new(
        "resume",
        &format!("cat '{}'", transcript_path("reads_a_file").display()),
    );
    let adapter = fake.adapter();
    let spec = fake.spec();
    let mut first = adapter
        .start_session(spec.clone())
        .expect("the session starts");
    drain(first.as_mut()).await;
    let mut second = adapter
        .resume(&spec.session_id, "go on")
        .expect("the session resumes");
    assert_eq!(second.session_id(), spec.session_id);
    drain(second.as_mut()).await;
    let args = fake.written("args");
    assert!(args.lines().any(|arg| arg == "--resume"), "{args}");
    assert!(fake.written("stdin").contains("\"content\":\"go on\""));
}

#[test]
fn refuses_a_claude_code_that_is_too_old_or_missing() {
    let fake = Fake::with_version("too-old", "2.1.200", "exit 0");
    let credential = || ClaudeCredential::ApiKey(Secret::new(API_KEY.to_string()));
    assert!(matches!(
        ClaudeAdapter::new(credential(), fake.config()),
        Err(RuntimeError::VersionTooOld { .. })
    ));
    let missing = ClaudeConfig {
        claude_path: fake.dir.join("no-such-claude"),
        ..fake.config()
    };
    assert!(matches!(
        ClaudeAdapter::new(credential(), missing),
        Err(RuntimeError::Spawn { .. })
    ));
}

/// A script body that prints `count` text lines, starts a background `sleep` whose pid it writes
/// to `sleep`, and waits for it.
fn floods_then_sleeps(count: u32) -> String {
    format!(
        "i=0\n\
         while [ $i -lt {count} ]; do\n\
         echo '{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"x\"}}]}}}}'\n\
         i=$((i+1))\n\
         done\n\
         sleep 30 &\n\
         echo $! > \"$dir/sleep\"\n\
         wait"
    )
}

#[tokio::test]
async fn ends_at_the_wall_clock_even_when_nobody_reads_the_events() {
    let fake = Fake::new("wall-clock-unread", &floods_then_sleeps(200));
    let mut spec = fake.spec();
    spec.limits.max_wall_clock = Duration::from_secs(1);
    let mut handle = fake
        .adapter()
        .start_session(spec)
        .expect("the session starts");
    let pid = fake.wait_for("pid").await;
    let sleep = fake.wait_for("sleep").await;
    assert!(
        gone_within(&pid, Duration::from_secs(2)).await,
        "the program outlived its wall clock"
    );
    assert!(
        gone_within(&sleep, Duration::from_secs(1)).await,
        "the program's child outlived its wall clock"
    );
    let events = tokio::time::timeout(Duration::from_secs(5), drain(handle.as_mut()))
        .await
        .expect("the session ends");
    assert!(
        matches!(
            events.last(),
            Some(SessionEvent::Ended {
                reason: EndReason::Limit,
                ..
            })
        ),
        "{:?}",
        events.last()
    );
    assert!(matches!(handle.send("more"), Err(RuntimeError::Limit)));
}

#[tokio::test]
async fn aborts_even_when_nobody_reads_the_events() {
    let fake = Fake::new("aborted-unread", &floods_then_sleeps(200));
    let mut handle = fake
        .adapter()
        .start_session(fake.spec())
        .expect("the session starts");
    let pid = fake.wait_for("pid").await;
    let sleep = fake.wait_for("sleep").await;
    handle.abort().expect("the session is stopped");
    assert!(
        gone_within(&pid, Duration::from_secs(2)).await,
        "the program outlived its abort"
    );
    assert!(
        gone_within(&sleep, Duration::from_secs(1)).await,
        "the program's child outlived its abort"
    );
    let events = tokio::time::timeout(Duration::from_secs(5), drain(handle.as_mut()))
        .await
        .expect("the session ends");
    assert!(
        matches!(
            events.last(),
            Some(SessionEvent::Ended {
                reason: EndReason::Aborted,
                ..
            })
        ),
        "{:?}",
        events.last()
    );
}

#[tokio::test]
async fn ends_at_once_when_the_program_exits_leaving_a_child_on_its_stderr() {
    let fake = Fake::new(
        "exits-leaving-a-child",
        "sleep 20 >/dev/null &\necho $! > \"$dir/sleep\"\necho boom >&2\nexit 1",
    );
    let mut handle = fake
        .adapter()
        .start_session(fake.spec())
        .expect("the session starts");
    match next_within(handle.as_mut(), Duration::from_secs(2)).await {
        Some(SessionEvent::Ended {
            reason: EndReason::Error,
            detail,
        }) => assert!(detail.contains("boom"), "{detail}"),
        other => panic!("expected an error end, got {other:?}"),
    }
    let sleep = fake.wait_for("sleep").await;
    assert!(
        gone_within(&sleep, Duration::from_secs(2)).await,
        "the program's child outlived it"
    );
}

#[test]
fn kills_the_group_when_the_runtime_shuts_down() {
    let fake = Fake::new(
        "runtime-shutdown",
        "sleep 60 &\necho $! > \"$dir/sleep\"\nwait",
    );
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let sleep = runtime.block_on(async {
        let handle = fake
            .adapter()
            .start_session(fake.spec())
            .expect("the session starts");
        let sleep = fake.wait_for("sleep").await;
        // The handle outlives the runtime, so only the runtime's end stops the session.
        std::mem::forget(handle);
        sleep
    });
    drop(runtime);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while is_alive(&sleep) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !is_alive(&sleep),
        "the program's child outlived the runtime"
    );
}

#[test]
fn asks_the_version_with_only_the_base_environment() {
    let fake = Fake::with_version_command(
        "version-env",
        "env > \"$dir/version-env\"; echo '2.1.280 (Claude Code)'",
        "exit 0",
    );
    fake.adapter();
    let env = fake.written("version-env");
    assert!(env.contains("PATH=/usr/bin:/bin"), "{env}");
    assert!(!env.contains("CARGO_MANIFEST_DIR"), "{env}");
}

#[test]
fn refuses_a_claude_code_that_does_not_say_its_version_in_time() {
    let fake = Fake::with_version_command("version-hangs", "exec sleep 30", "exit 0");
    let started = std::time::Instant::now();
    match ClaudeAdapter::new(
        ClaudeCredential::ApiKey(Secret::new(API_KEY.to_string())),
        fake.config(),
    ) {
        Err(RuntimeError::Spawn { detail }) => assert!(detail.contains("--version"), "{detail}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn refuses_to_resume_a_session_that_is_still_running() {
    let fake = Fake::new("resume-running", "sleep 30");
    let adapter = fake.adapter();
    let spec = fake.spec();
    let handle = adapter
        .start_session(spec.clone())
        .expect("the session starts");
    fake.wait_for("stdin").await;
    match adapter.resume(&spec.session_id, "go on") {
        Err(RuntimeError::Spawn { detail }) => assert!(detail.contains("running"), "{detail}"),
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
    handle.abort().expect("the session is stopped");
}

#[tokio::test]
async fn gives_a_subscription_session_no_api_key_from_its_base() {
    let fake = Fake::new(
        "oauth",
        &format!("cat '{}'", transcript_path("reads_a_file").display()),
    );
    let mut config = fake.config();
    config.env.insert(
        "ANTHROPIC_API_KEY".to_string(),
        "sk-from-the-base".to_string(),
    );
    let adapter = ClaudeAdapter::new(
        ClaudeCredential::OauthToken(Secret::new("the-token".to_string())),
        config,
    )
    .expect("the fake is new enough");
    let mut handle = adapter
        .start_session(fake.spec())
        .expect("the session starts");
    drain(handle.as_mut()).await;
    let env = fake.written("env");
    assert!(env.contains("CLAUDE_CODE_OAUTH_TOKEN=the-token"), "{env}");
    assert!(!env.contains("ANTHROPIC_API_KEY"), "{env}");
}
