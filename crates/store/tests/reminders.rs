//! What the daily reminder reads from the index, from git and from Claude Code's folder, on real
//! repositories (phase 10 step 07).
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use std::path::{Path, PathBuf};

use catervas_core::claude_history::fixtures::a_turn_line;
use catervas_core::reminders::MarkKind;
use catervas_store::claude_history::{place_sessions, read_transcripts};
use catervas_store::files::fixtures::TempProject;
use catervas_store::git::fixtures::git_in;
use catervas_store::machine::{MACHINE_DB, MachineIndex, open_machine_index};
use catervas_store::reminders::ReminderSources;
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::json;

const MODEL: &str = "claude-opus-5-5";
const SESSION: &str = "00000000-0000-4000-8000-000000000001";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 9, 0, 0)
        .single()
        .expect("a valid time")
}

/// The transcript line of a reply that wrote a to-do list of three items, one of them completed.
fn a_todo_write_line(cwd: &str, at: &str) -> String {
    json!({
        "type": "assistant", "sessionId": SESSION, "cwd": cwd, "timestamp": at,
        "gitBranch": "fix/login-redirect", "version": "2.1.296", "entrypoint": "cli",
        "message": {
            "id": "msg_todos", "model": MODEL,
            "content": [{
                "type": "tool_use", "id": "toolu_todos", "name": "TodoWrite",
                "input": { "todos": [
                    {"content": "NEVER-KEPT-TODO one", "status": "completed", "activeForm": "a"},
                    {"content": "NEVER-KEPT-TODO two", "status": "in_progress", "activeForm": "b"},
                    {"content": "NEVER-KEPT-TODO three", "status": "pending", "activeForm": "c"},
                ]},
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1,
                      "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        },
    })
    .to_string()
}

fn configure(repository: &Path) {
    for (key, value) in [
        ("user.email", "me@example.invalid"),
        ("user.name", "Me"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git_in(repository, &["config", key, value]);
    }
}

struct World {
    temp: TempProject,
}

impl World {
    fn new(name: &str) -> Self {
        let temp = TempProject::new(name);
        std::fs::create_dir_all(temp.root.join("home")).expect("a home");
        std::fs::create_dir_all(temp.root.join("claude/projects/-x")).expect("a projects folder");
        Self { temp }
    }

    fn home(&self) -> PathBuf {
        self.temp.root.join("home").canonicalize().expect("a home")
    }

    fn claude(&self) -> PathBuf {
        self.temp.root.join("claude")
    }

    fn index(&self) -> MachineIndex {
        open_machine_index(&self.temp.root.join("state").join(MACHINE_DB), now())
            .expect("the index opens")
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
#[allow(
    clippy::too_many_lines,
    reason = "one world, built once, and every thing the input reads from it"
)]
fn gathers_the_reminders_input() {
    let world = World::new("gathers-reminders-input");
    // A repository with a remote that has its first commit; then one commit that is not pushed and
    // one file that is not committed.
    let remote = world.temp.root.join("remote.git");
    std::fs::create_dir_all(&remote).expect("a remote");
    git_in(&remote, &["init", "--bare", "-b", "main"]);
    let repository = world.home().join("recipe-cards");
    std::fs::create_dir_all(&repository).expect("a repository folder");
    git_in(&repository, &["init", "-b", "main"]);
    configure(&repository);
    std::fs::write(repository.join("README.md"), "x\n").expect("a file");
    git_in(&repository, &["add", "-A"]);
    git_in(&repository, &["commit", "-m", "the first commit"]);
    git_in(
        &repository,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    git_in(&repository, &["push", "-u", "origin", "main"]);
    std::fs::write(repository.join("export.rs"), "fn main() {}\n").expect("a file");
    git_in(&repository, &["add", "-A"]);
    git_in(&repository, &["commit", "-m", "an unpushed commit"]);
    std::fs::write(repository.join("notes.txt"), "not committed\n").expect("a file");

    let cwd = repository.to_string_lossy().into_owned();
    let at = (now() - Duration::days(4)).to_rfc3339();
    let lines = [
        a_turn_line(SESSION, &cwd, &at),
        a_todo_write_line(&cwd, &at),
    ];
    std::fs::write(
        world
            .claude()
            .join("projects/-x")
            .join(format!("{SESSION}.jsonl")),
        lines.join("\n") + "\n",
    )
    .expect("a transcript");
    std::fs::write(
        world.claude().join("mcp-needs-auth-cache.json"),
        json!({"zoho-books": {"timestamp": 1_792_000_000_000_u64}, "slack": {"timestamp": 1}})
            .to_string(),
    )
    .expect("a cache file");

    let mut index = world.index();
    read_transcripts(&mut index, &world.claude(), now()).expect("reads");
    place_sessions(&mut index, &[world.home()], now()).expect("places");

    let sources = ReminderSources {
        claude_dir: &world.claude(),
        home: &world.home(),
        github_connected: false,
        utc_offset_secs: 0,
    };
    let input = index.reminder_input(&sources, now()).expect("the input");
    assert_eq!(input.sessions.len(), 1, "{:?}", input.sessions);
    let session = &input.sessions[0];
    assert_eq!(session.id, SESSION);
    assert_eq!(session.project, "recipe-cards");
    assert_eq!(session.branch.as_deref(), Some("main"));
    assert!(session.uncommitted, "one file is not committed");
    assert_eq!(session.unpushed, 1, "one commit is not pushed");
    assert_eq!(session.open_todos, 2, "3 items, 1 completed");
    assert_eq!(input.services, ["slack", "zoho-books"]);

    // No to-do text reaches the index.
    drop(index);
    let bytes = std::fs::read(world.temp.root.join("state").join(MACHINE_DB)).expect("the index");
    let marker = b"NEVER-KEPT-TODO";
    assert!(!bytes.windows(marker.len()).any(|window| window == marker));

    // A session in a folder outside home is not read by git.
    let outside = world.temp.root.join("outside");
    std::fs::create_dir_all(&outside).expect("a folder");
    let sources = ReminderSources {
        home: &outside,
        ..sources
    };
    let narrowed = world
        .index()
        .reminder_input(&sources, now())
        .expect("the input");
    assert_eq!(
        (
            narrowed.sessions[0].uncommitted,
            narrowed.sessions[0].unpushed
        ),
        (false, 0),
        "the folder is outside the home it was given"
    );

    // Marks round-trip, and the migrations end at 11.
    let mut index = world.index();
    let until = now() + Duration::days(7);
    index
        .put_reminder_mark("forgotten:a", &MarkKind::Snooze { until }, now())
        .expect("a snooze");
    index
        .put_reminder_mark(
            "schedules:b",
            &MarkKind::Dismiss {
                fingerprint: "abc".to_string(),
            },
            now(),
        )
        .expect("a dismissal");
    let marks: Vec<(String, MarkKind)> = index
        .reminder_marks()
        .expect("marks")
        .into_iter()
        .map(|mark| (mark.key, mark.kind))
        .collect();
    assert_eq!(
        marks,
        vec![
            ("forgotten:a".to_string(), MarkKind::Snooze { until }),
            (
                "schedules:b".to_string(),
                MarkKind::Dismiss {
                    fingerprint: "abc".to_string()
                }
            ),
        ]
    );
    assert_eq!(
        index.applied_migrations().expect("versions"),
        (1..=11).collect::<Vec<i64>>()
    );
}
