//! Placing Claude Code sessions in projects, on real repositories.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use std::path::{Path, PathBuf};

use catervas_core::claude_history::fixtures::{CLAUDE_CODE_2_1_296, a_reply_line, a_turn_line};
use catervas_core::pricing::Usage;
use catervas_protocol::event::EventKind;
use catervas_protocol::event::fixtures::a_new_event;
use catervas_store::claude_history::{place_sessions, read_transcripts};
use catervas_store::files::fixtures::TempProject;
use catervas_store::git::fixtures::git_in;
use catervas_store::machine::{Found, MACHINE_DB, MachineIndex, Ownership, open_machine_index};
use catervas_store::open_event_log;
use chrono::{DateTime, TimeZone, Utc};

const MODEL: &str = "claude-opus-5-5";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
        .single()
        .expect("a valid time")
}

fn one_each() -> Usage {
    Usage {
        input_tokens: 1,
        output_tokens: 2,
        cache_read_tokens: 3,
        cache_write_tokens: 4,
    }
}

/// A home folder, a Claude folder and an index, in one temporary folder.
struct World {
    temp: TempProject,
    next: u32,
}

impl World {
    fn new(name: &str) -> Self {
        let temp = TempProject::new(name);
        std::fs::create_dir_all(temp.root.join("home")).expect("a home");
        std::fs::create_dir_all(temp.root.join("claude/projects/-x")).expect("a projects folder");
        Self { temp, next: 0 }
    }

    fn home(&self) -> PathBuf {
        self.temp.root.join("home").canonicalize().expect("a home")
    }

    fn folder(&self, relative: &str) -> PathBuf {
        let path = self.home().join(relative);
        std::fs::create_dir_all(&path).expect("a folder");
        path
    }

    /// A repository whose own `user.email` is the owner's, with one commit by `committer`.
    fn repository(&self, relative: &str, committer: Option<&str>) -> PathBuf {
        let path = self.folder(relative);
        git_in(&path, &["init", "-b", "main"]);
        for (key, value) in [
            ("user.email", "me@example.invalid"),
            ("user.name", "Me"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            git_in(&path, &["config", key, value]);
        }
        if let Some(email) = committer {
            std::fs::write(path.join("README.md"), "x\n").expect("a file");
            git_in(&path, &["add", "-A"]);
            let setting = format!("user.email={email}");
            git_in(&path, &["-c", &setting, "commit", "-m", "the first commit"]);
        }
        path
    }

    /// One session that worked in `cwd`, with one reply.
    fn session(&mut self, cwd: &Path) -> String {
        self.next += 1;
        let id = format!("00000000-0000-4000-8000-{:012}", self.next);
        let cwd = cwd.to_string_lossy();
        let at = format!("2026-10-12T09:{:02}:00Z", self.next);
        let lines = [
            a_turn_line(&id, &cwd, &at),
            a_reply_line(&id, &cwd, &at, &format!("msg_{id}"), MODEL, one_each()),
        ];
        let file = self
            .temp
            .root
            .join("claude/projects/-x")
            .join(format!("{id}.jsonl"));
        std::fs::write(file, lines.join("\n") + "\n").expect("a transcript");
        id
    }

    fn scan(&self) -> Found {
        let mut index = self.index();
        read_transcripts(&mut index, &self.temp.root.join("claude"), now()).expect("reads");
        place_sessions(&mut index, &[self.home()], now()).expect("places");
        index.found().expect("found")
    }

    fn index(&self) -> MachineIndex {
        open_machine_index(&self.temp.root.join("state").join(MACHINE_DB), now())
            .expect("the index opens")
    }
}

fn tokens(count: u64) -> Usage {
    Usage {
        input_tokens: count,
        output_tokens: 2 * count,
        cache_read_tokens: 3 * count,
        cache_write_tokens: 4 * count,
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn places_every_session() {
    let mut world = World::new("places-every-session");
    let shop = world.repository("shop", Some("me@example.invalid"));
    let worktree = shop.join(".claude/worktrees/w");
    git_in(
        &shop,
        &["worktree", "add", "-b", "w", &worktree.to_string_lossy()],
    );
    let notes = world.folder("notes");
    let sub = world.folder("notes/sub");
    let catervas = shop.join(".catervas/local/worktrees/CTV-1");
    for cwd in [
        shop.clone(),
        worktree,
        sub,
        notes,
        world.home(),
        shop.join(".claude/worktrees/old"),
        world.home().join("gone-app"),
        catervas,
    ] {
        world.session(&cwd);
    }

    let found = world.scan();

    let names: Vec<&str> = found
        .projects
        .iter()
        .map(|project| project.root.rsplit('/').next().expect("a name"))
        .collect();
    assert_eq!(found.projects.len(), 2, "{names:?}");
    let shop_found = found
        .projects
        .iter()
        .find(|project| project.root.ends_with("/shop"))
        .expect("shop");
    assert_eq!(shop_found.own.sessions, 3);
    assert_eq!(shop_found.catervas.sessions, 1);
    assert_eq!(shop_found.own.tokens.get(MODEL), Some(&tokens(3)));
    assert_eq!(shop_found.catervas.tokens.get(MODEL), Some(&tokens(1)));
    let notes_found = found
        .projects
        .iter()
        .find(|project| project.root.ends_with("/notes"))
        .expect("notes");
    assert_eq!(notes_found.own.sessions, 2);
    assert_eq!(notes_found.catervas.sessions, 0);
    assert_eq!(found.gone.len(), 1);
    assert!(found.gone[0].folder.ends_with("/gone-app"));
    assert_eq!(found.gone[0].activity.sessions, 1);
    assert_eq!(found.not_a_project.sessions, 1);
    assert_eq!(found.not_a_project.tokens.get(MODEL), Some(&tokens(1)));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn places_the_same_whatever_the_scans_saw_first() {
    let mut world = World::new("places-whatever-first");
    let sub = world.folder("notes/sub");
    world.session(&sub);
    let first = world.scan();
    assert!(first.projects[0].root.ends_with("/notes/sub"));

    let notes = world.folder("notes");
    world.session(&notes);
    let second = world.scan();
    assert_eq!(second.projects.len(), 1, "{:?}", second.projects.len());
    assert!(second.projects[0].root.ends_with("/notes"));
    assert_eq!(second.projects[0].own.sessions, 2);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_how_many_sessions_it_could_not_read() {
    let world = World::new("says-unreadable");
    let transcripts = world.temp.root.join("claude/projects/-x");
    std::fs::write(
        transcripts.join("a.jsonl"),
        "not json\nnot json\nnot json\n",
    )
    .expect("a file");
    std::fs::write(transcripts.join("b.jsonl"), CLAUDE_CODE_2_1_296).expect("a file");
    let found = world.scan();
    assert_eq!(found.unreadable_sessions, 1);
    assert_eq!(found.unreadable_lines, 5);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn counts_catervas_sessions_from_the_projects_log() {
    let world = World::new("counts-catervas-log");
    let shop = world.repository("shop", Some("me@example.invalid"));
    let log_path = shop.join(catervas_store::claude_history::PROJECT_LOG);
    {
        let log = open_event_log(&log_path, now()).expect("a log");
        let mut event = a_new_event(EventKind::ChatMessagePosted);
        event.ids.session_id = Some("cat-1".to_string());
        log.append(&event).expect("an event");
    }
    // A session named `cat-1` in the shop, written by hand because its id is the log's.
    let cwd = shop.to_string_lossy();
    let lines = [
        a_turn_line("cat-1", &cwd, "2026-10-12T09:30:00Z"),
        a_reply_line(
            "cat-1",
            &cwd,
            "2026-10-12T09:30:01Z",
            "msg_cat",
            MODEL,
            one_each(),
        ),
    ];
    std::fs::write(
        world.temp.root.join("claude/projects/-x/cat-1.jsonl"),
        lines.join("\n") + "\n",
    )
    .expect("a transcript");
    let before = std::fs::read(&log_path).expect("the log");

    let found = world.scan();

    let project = &found.projects[0];
    assert_eq!(project.catervas.sessions, 1);
    assert_eq!(project.own.sessions, 0);
    assert_eq!(std::fs::read(&log_path).expect("the log"), before);
    let beside = log_path.parent().expect("a folder");
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            !beside.join(format!("catervas.db{suffix}")).exists(),
            "{suffix}"
        );
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn tells_explored_from_yours() {
    let mut world = World::new("tells-explored");
    let shop = world.repository("shop", Some("me@example.invalid"));
    let clone = world.repository("clone", Some("other@example.invalid"));
    let fresh = world.repository("fresh", None);
    let plain = world.folder("plain");
    for cwd in [&shop, &clone, &fresh, &plain] {
        world.session(cwd);
    }

    let found = world.scan();

    let of = |suffix: &str| {
        found
            .projects
            .iter()
            .find(|project| project.root.ends_with(suffix))
            .unwrap_or_else(|| panic!("no project {suffix}"))
    };
    assert_eq!(of("/shop").ownership, Ownership::Yours);
    assert_eq!(of("/clone").ownership, Ownership::Explored);
    assert_eq!(of("/fresh").ownership, Ownership::Yours);
    assert_eq!(of("/plain").ownership, Ownership::Yours);
    assert!(of("/shop").is_git);
    assert!(!of("/plain").is_git);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn remembers_a_root_after_its_folder_is_gone() {
    let mut world = World::new("remembers-root");
    let shop = world.repository("shop", Some("me@example.invalid"));
    world.session(&shop);
    let found = world.scan();
    assert_eq!(found.projects.len(), 1);

    let old = shop.join(".claude/worktrees/old");
    world.session(&old);
    std::fs::remove_dir_all(&shop).expect("the folder goes");
    let found = world.scan();

    assert!(found.projects.is_empty());
    assert_eq!(found.gone.len(), 1);
    assert!(found.gone[0].folder.ends_with("/shop"));
    assert_eq!(found.gone[0].activity.sessions, 2);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn treats_a_file_where_a_folder_was_as_gone() {
    let mut world = World::new("file-for-folder");
    let app = world.home().join("app");
    std::fs::write(&app, "now a file\n").expect("a file");
    world.session(&app);
    let found = world.scan();
    assert!(found.projects.is_empty());
    assert_eq!(found.gone.len(), 1);
    assert!(found.gone[0].folder.ends_with("/app"));
    // The index says it too: the session is `gone`, and no project was made of the file.
    let connection = rusqlite::Connection::open(world.temp.root.join("state").join(MACHINE_DB))
        .expect("the index opens");
    let places: Vec<String> = connection
        .prepare("SELECT place FROM sessions UNION ALL SELECT root FROM projects")
        .expect("a statement")
        .query_map([], |row| row.get(0))
        .expect("a query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(places, vec!["gone".to_string()]);
}
