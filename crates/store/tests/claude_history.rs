//! Placing Claude Code sessions in projects, on real repositories.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use std::path::{Path, PathBuf};

use catervas_core::claude_history::HandProjectRefusal;
use catervas_core::claude_history::Source;
use catervas_core::claude_history::fixtures::{CLAUDE_CODE_2_1_296, a_reply_line, a_turn_line};
use catervas_core::pricing::Usage;
use catervas_protocol::event::EventKind;
use catervas_protocol::event::fixtures::a_new_event;
use catervas_store::claude_history::{
    HistoryError, add_project_by_hand, find_copies, place_sessions, read_transcripts,
};
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

    /// A session in `cwd` whose replies are `(message id, second, tokens)`, written under `file`
    /// (transcripts are read in the order of their names, which is the order sessions are indexed in).
    fn session_of(&self, file: &str, id: &str, cwd: &Path, replies: &[(&str, u32, u64)]) {
        let cwd = cwd.to_string_lossy();
        let mut lines = vec![a_turn_line(id, &cwd, "2026-10-12T09:00:00Z")];
        for (message, second, count) in replies {
            let at = format!("2026-10-12T09:{:02}:{:02}Z", second / 60, second % 60);
            lines.push(a_reply_line(id, &cwd, &at, message, MODEL, tokens(*count)));
        }
        std::fs::write(
            self.temp.root.join("claude/projects/-x").join(file),
            lines.join("\n") + "\n",
        )
        .expect("a transcript");
    }

    /// The rows of the two copy tables, as text.
    fn links(&self) -> Vec<String> {
        let connection = rusqlite::Connection::open(self.temp.root.join("state").join(MACHINE_DB))
            .expect("the index opens");
        let mut lines = Vec::new();
        for query in [
            "SELECT 'link ' || session_id || ' ' || other_id || ' ' || kind FROM session_links ORDER BY 1",
            "SELECT 'reply ' || reply_key || ' ' || same_as FROM reply_copies ORDER BY 1",
        ] {
            let mut statement = connection.prepare(query).expect("a statement");
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("a query");
            lines.extend(rows.map(|row| row.expect("a row")));
        }
        lines
    }

    fn scan(&self) -> Found {
        let mut index = self.index();
        read_transcripts(&mut index, &self.temp.root.join("claude"), now()).expect("reads");
        place_sessions(&mut index, &[self.home()], now()).expect("places");
        find_copies(&mut index).expect("finds copies");
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

const A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const B: &str = "aaaaaaaa-0000-4000-8000-000000000002";
const C: &str = "aaaaaaaa-0000-4000-8000-000000000003";

/// Four replies at four different seconds, each with its own usage.
const FOUR: [(&str, u32, u64); 4] = [("m1", 5, 1), ("m2", 65, 2), ("m3", 125, 3), ("m4", 185, 4)];

/// The same four, under new message ids.
fn copy_of_four(prefix: &str) -> Vec<(String, u32, u64)> {
    FOUR.iter()
        .map(|(id, second, count)| (format!("{prefix}{id}"), *second, *count))
        .collect()
}

fn as_borrowed(replies: &[(String, u32, u64)]) -> Vec<(&str, u32, u64)> {
    replies
        .iter()
        .map(|(id, second, count)| (id.as_str(), *second, *count))
        .collect()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn counts_a_teleported_copy_once() {
    let world = World::new("counts-teleported-copy-once");
    let shop = world.repository("shop", Some("me@example.invalid"));
    world.session_of("a.jsonl", A, &shop, &FOUR);
    let copy = copy_of_four("copy-");
    world.session_of("b.jsonl", B, &shop, &as_borrowed(&copy));

    let found = world.scan();

    assert_eq!(world.links()[0], format!("link {B} {A} copy_of"));
    let project = &found.projects[0];
    assert_eq!(project.own.sessions, 1);
    assert_eq!(project.own.tokens.get(MODEL), Some(&tokens(10)));
    assert_eq!(
        project.sources,
        [Source::Local, Source::Teleported].into_iter().collect()
    );
    assert_eq!(found.maybe_same_sessions, 0);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn does_not_link_sessions_sharing_one_reply_by_chance() {
    let world = World::new("no-link-by-chance");
    let shop = world.repository("shop", Some("me@example.invalid"));
    world.session_of("a.jsonl", A, &shop, &[("a1", 5, 1), ("a2", 65, 2)]);
    world.session_of("b.jsonl", B, &shop, &[("b1", 5, 1), ("b2", 66, 7)]);

    let found = world.scan();

    assert_eq!(world.links(), Vec::<String>::new());
    let project = &found.projects[0];
    assert_eq!(project.own.sessions, 2);
    assert_eq!(project.own.tokens.get(MODEL), Some(&tokens(1 + 2 + 1 + 7)));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_a_resumed_session_apart_from_a_copy() {
    let world = World::new("resumed-is-not-a-copy");
    let shop = world.repository("shop", Some("me@example.invalid"));
    world.session_of("a.jsonl", A, &shop, &FOUR);
    // A resumed session repeats the earlier replies under the same message ids.
    world.session_of("b.jsonl", B, &shop, &FOUR);

    let found = world.scan();

    assert_eq!(world.links(), Vec::<String>::new());
    assert_eq!(found.projects[0].own.sessions, 2);
    assert!(!found.projects[0].sources.contains(&Source::Teleported));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn marks_ambiguous_copies_maybe_the_same() {
    let world = World::new("marks-maybe-the-same");
    let shop = world.repository("shop", Some("me@example.invalid"));
    world.session_of("a.jsonl", A, &shop, &FOUR);
    let (one, two) = (copy_of_four("one-"), copy_of_four("two-"));
    world.session_of("b.jsonl", B, &shop, &as_borrowed(&one));
    let found = world.scan();
    assert_eq!(world.links()[0], format!("link {B} {A} copy_of"));
    assert_eq!(found.maybe_same_sessions, 0);

    world.session_of("c.jsonl", C, &shop, &as_borrowed(&two));
    let found = world.scan();

    let links: Vec<String> = world
        .links()
        .into_iter()
        .filter(|line| line.starts_with("link"))
        .collect();
    assert_eq!(
        links,
        [
            format!("link {A} {B} maybe_same"),
            format!("link {A} {C} maybe_same"),
            format!("link {B} {C} maybe_same"),
        ]
    );
    assert_eq!(found.maybe_same_sessions, 3);
    let project = &found.projects[0];
    assert_eq!(project.own.sessions, 3);
    assert_eq!(project.own.tokens.get(MODEL), Some(&tokens(10)));
    assert!(!project.sources.contains(&Source::Teleported));
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keys_a_project_by_its_remote() {
    let mut world = World::new("keys-by-remote");
    let shop = world.repository("shop", Some("me@example.invalid"));
    git_in(
        &shop,
        &["remote", "add", "origin", "git@github.com:Me/Shop.git"],
    );
    let plain = world.folder("plain");
    world.session(&shop);
    world.session(&plain);

    let found = world.scan();

    let of = |suffix: &str| {
        found
            .projects
            .iter()
            .find(|project| project.root.ends_with(suffix))
            .unwrap_or_else(|| panic!("no project {suffix}"))
    };
    assert_eq!(of("/shop").remote.as_deref(), Some("github.com/me/shop"));
    assert_eq!(of("/plain").remote, None);
}

/// Guard: checked by mutation (store the address as written), not watched to fail.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_no_token_from_a_remote() {
    let mut world = World::new("keeps-no-token");
    let shop = world.repository("shop", Some("me@example.invalid"));
    git_in(
        &shop,
        &[
            "remote",
            "add",
            "origin",
            "https://x-access-token:SECRET-REMOTE@github.com/me/shop.git",
        ],
    );
    world.session(&shop);
    let found = world.scan();
    assert_eq!(
        found.projects[0].remote.as_deref(),
        Some("github.com/me/shop")
    );
    let bytes = std::fs::read(world.temp.root.join("state").join(MACHINE_DB)).expect("the file");
    let secret = b"SECRET-REMOTE";
    assert!(!bytes.windows(secret.len()).any(|window| window == secret));
}

fn the_shop() -> World {
    let mut world = World::new("hand-projects");
    let shop = world.repository("shop", Some("me@example.invalid"));
    git_in(
        &shop,
        &["remote", "add", "origin", "git@github.com:me/shop.git"],
    );
    world.session(&shop);
    world
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn adds_a_project_by_hand_to_its_found_project() {
    let world = the_shop();
    let shop_root = world.home().join("shop").to_string_lossy().into_owned();
    world.scan();
    let mut index = world.index();
    let blocked = [world.home()];

    let by_address = add_project_by_hand(
        &mut index,
        "Shop",
        None,
        Some("https://github.com/me/shop"),
        &blocked,
        now(),
    )
    .expect("added");
    assert_eq!(by_address.same_as.as_deref(), Some(shop_root.as_str()));
    let found = world.scan();
    let shop = &found.projects[0];
    assert_eq!(shop.name_by_hand.as_deref(), Some("Shop"));
    assert_eq!(
        shop.sources,
        [Source::Local, Source::ByHand].into_iter().collect()
    );
    assert!(found.by_hand.is_empty());

    #[cfg(unix)]
    {
        let link = world.home().join("link-to-shop");
        std::os::unix::fs::symlink(world.home().join("shop"), &link).expect("a link");
        for (name, folder) in [("Linked", link), ("Inside", world.home().join("shop/sub"))] {
            std::fs::create_dir_all(&folder).ok();
            let added = add_project_by_hand(&mut index, name, Some(&folder), None, &blocked, now())
                .expect("added");
            assert_eq!(added.same_as.as_deref(), Some(shop_root.as_str()), "{name}");
        }
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_an_unmatched_project_by_hand() {
    let world = the_shop();
    let mut index = world.index();
    let blocked = [world.home()];
    let added = add_project_by_hand(
        &mut index,
        "Billing",
        None,
        Some("git@github.com:me/billing.git"),
        &blocked,
        now(),
    )
    .expect("added");
    assert_eq!(added.same_as, None);

    let found = world.scan();
    let listed: Vec<(&str, Option<&str>, Option<&str>)> = found
        .by_hand
        .iter()
        .map(|p| (p.name.as_str(), p.root.as_deref(), p.remote.as_deref()))
        .collect();
    assert_eq!(listed, [("Billing", None, Some("github.com/me/billing"))]);

    add_project_by_hand(
        &mut index,
        "Billing 2",
        None,
        Some("https://github.com/me/billing"),
        &blocked,
        now(),
    )
    .expect("renamed");
    let found = world.scan();
    let names: Vec<&str> = found.by_hand.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Billing 2"]);
}

/// Guard: a link to a folder is the folder.
#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn places_a_session_in_a_linked_folder_with_its_project() {
    let mut world = the_shop();
    let link = world.home().join("link-to-shop");
    std::os::unix::fs::symlink(world.home().join("shop"), &link).expect("a link");
    world.session(&link);
    let found = world.scan();
    assert_eq!(found.projects.len(), 1);
    assert_eq!(found.projects[0].own.sessions, 2);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_project_by_hand_with_nowhere() {
    let world = the_shop();
    let mut index = world.index();
    let blocked = [world.home()];
    let mut add = |folder: Option<&Path>, repository: Option<&str>| {
        add_project_by_hand(&mut index, "Shop", folder, repository, &blocked, now())
    };
    assert!(matches!(
        add(None, None),
        Err(HistoryError::HandProject(HandProjectRefusal::NoPlace))
    ));
    let missing = world.home().join("missing");
    assert!(matches!(
        add(Some(&missing), None),
        Err(HistoryError::NoSuchFolder(path)) if path == missing
    ));
    for folder in [world.home(), PathBuf::from("/"), PathBuf::from("/tmp")] {
        assert!(
            matches!(
                add(Some(&folder), None),
                Err(HistoryError::HandProject(
                    HandProjectRefusal::NotAProjectFolder(_)
                ))
            ),
            "{}",
            folder.display()
        );
    }
    let connection = rusqlite::Connection::open(world.temp.root.join("state").join(MACHINE_DB))
        .expect("the index opens");
    let rows: i64 = connection
        .query_row("SELECT count(*) FROM hand_projects", [], |row| row.get(0))
        .expect("the table exists");
    assert_eq!(rows, 0);
}
