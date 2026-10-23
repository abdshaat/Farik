//! `catervas discover`, through the command line.
//!
//! The tests that reach the `git` program are `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas::{CliIo, run_cli};
use catervas_core::claude_export::fixtures::{a_conversation_json, a_project_json};
use catervas_core::claude_history::fixtures::a_reply_line;
use catervas_core::claude_history::fixtures::{
    CLAUDE_CODE_2_1_296, CLAUDE_CODE_2_1_296_CONNECTORS, a_turn_line,
};
use catervas_core::claude_services::fixtures::{
    a_claude_json_full_of_keys, a_desktop_config_full_of_keys, a_plugin_mcp_json_full_of_keys,
};
use catervas_core::pricing::Usage;
use catervas_protocol::clock::FixedClock;
use catervas_store::claude_export::fixtures::{an_export_zip, the_2026_10_export};
use catervas_store::files::fixtures::TempProject;
use catervas_store::git::fixtures::git_in;
use catervas_store::machine::{MACHINE_DB, open_machine_index};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

const NOW: &str = "2026-10-12T12:00:00Z";
const SESSION: &str = "11111111-1111-4111-8111-111111111111";

struct Ran {
    code: i32,
    out: String,
    err: String,
}

fn run(env: &[(&str, &Path)], args: &[&str]) -> Ran {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = {
        let now = DateTime::parse_from_rfc3339(NOW)
            .expect("a moment")
            .with_timezone(&Utc);
        let mut io = CliIo::new(
            std::env::temp_dir(),
            Box::new(&mut out),
            Box::new(&mut err),
            Arc::new(FixedClock::new(now)),
        );
        io.env = env
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.to_string_lossy().into_owned()))
            .collect::<BTreeMap<_, _>>();
        let arguments: Vec<String> = std::iter::once("catervas")
            .chain(args.iter().copied())
            .map(ToString::to_string)
            .collect();
        run_cli(&arguments, &mut io)
    };
    Ran {
        code,
        out: String::from_utf8(out).expect("text"),
        err: String::from_utf8(err).expect("text"),
    }
}

/// A home folder with Claude Code's history in it.
struct World {
    temp: TempProject,
}

impl World {
    fn new(name: &str) -> Self {
        let temp = TempProject::new(name);
        std::fs::create_dir_all(temp.root.join("home/.claude/projects/-shop"))
            .expect("a projects folder");
        Self { temp }
    }

    fn home(&self) -> PathBuf {
        self.temp.root.join("home").canonicalize().expect("a home")
    }

    fn config(&self) -> PathBuf {
        self.home().join(".config")
    }

    fn transcript(&self) -> PathBuf {
        self.home()
            .join(format!(".claude/projects/-shop/{SESSION}.jsonl"))
    }

    /// The fixture, worked in the home's `shop`.
    fn write_fixture(&self) {
        let text = CLAUDE_CODE_2_1_296.replace("/home/me", &self.home().to_string_lossy());
        std::fs::write(self.transcript(), text).expect("a transcript");
    }

    fn discover(&self, args: &[&str]) -> Ran {
        let home = self.home();
        let config = self.config();
        let mut arguments = vec!["discover"];
        arguments.extend_from_slice(args);
        run(&[("HOME", &home), ("XDG_CONFIG_HOME", &config)], &arguments)
    }

    /// `shop`, a repository with one commit by its owner.
    fn make_shop(&self) {
        self.make_repository("shop", "me@example.invalid");
    }

    /// A repository whose own `user.email` is the owner's, with one commit by `committer`.
    fn make_repository(&self, relative: &str, committer: &str) {
        let path = self.home().join(relative);
        std::fs::create_dir_all(&path).expect("a folder");
        git_in(&path, &["init", "-b", "main"]);
        for (key, value) in [
            ("user.email", "me@example.invalid"),
            ("user.name", "Me"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            git_in(&path, &["config", key, value]);
        }
        std::fs::write(path.join("README.md"), "x\n").expect("a file");
        git_in(&path, &["add", "-A"]);
        let setting = format!("user.email={committer}");
        git_in(&path, &["-c", &setting, "commit", "-m", "the first commit"]);
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn discovers_a_project_from_the_history() {
    let world = World::new("discovers-project");
    world.make_shop();
    world.write_fixture();

    let ran = world.discover(&[]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        "reading your Claude Code history on this computer; nothing leaves it\n\
         read 1 transcript file: 1 session, 1 new\n\
         skipped 2 lines it couldn't read\n\
         projects: 1\n  \
         shop  ~/shop  last worked 2026-10-12  1 session  1.2k in  47 out\n"
    );
    assert!(world.config().join("catervas").join(MACHINE_DB).is_file());

    let again = world.discover(&[]);
    assert!(
        again
            .out
            .contains("read 1 transcript file: 1 session, 0 new\n"),
        "{}",
        again.out
    );

    let as_json = world.discover(&["--json"]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    assert_eq!(
        value["projects"][0],
        json!({
            "root": world.home().join("shop").to_string_lossy(),
            "name": "shop",
            "is_git": true,
            "ownership": "yours",
            "last_worked_at": "2026-10-12T09:01:10Z",
            "own": {
                "sessions": 1,
                "tokens": { "claude-opus-5-5": {
                    "input_tokens": 15, "output_tokens": 47,
                    "cache_read_tokens": 1000, "cache_write_tokens": 200,
                } },
            },
            "catervas": { "sessions": 0, "tokens": {} },
            "jobs": [],
            "work": null,
            "services": [],
            "repository": null,
            "sources": ["local"],
        })
    );
}

#[test]
fn forgets_without_reading() {
    let world = World::new("forgets-without-reading");
    world.write_fixture();
    let database = world.config().join("catervas").join(MACHINE_DB);
    let now = DateTime::parse_from_rfc3339(NOW)
        .expect("a moment")
        .with_timezone(&Utc);
    drop(open_machine_index(&database, now).expect("an index"));
    let before = std::fs::metadata(world.transcript()).expect("a transcript");
    let bytes = std::fs::read(world.transcript()).expect("a transcript");

    let ran = world.discover(&["--forget"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        "forgot everything Catervas found: ~/.config/catervas/machine.db deleted\n"
    );
    assert!(!database.exists());
    let again = world.discover(&["--forget"]);
    assert_eq!(again.out, "nothing to forget\n");
    drop(open_machine_index(&database, now).expect("an index"));
    let as_json = world.discover(&["--forget", "--json"]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    assert_eq!(
        value,
        json!({ "forgotten": true, "path": database.to_string_lossy() })
    );
    let after = std::fs::metadata(world.transcript()).expect("a transcript");
    assert_eq!(
        before.modified().expect("a time"),
        after.modified().expect("a time")
    );
    assert_eq!(
        bytes,
        std::fs::read(world.transcript()).expect("a transcript")
    );
}

#[test]
fn says_when_there_is_no_history() {
    let temp = TempProject::new("says-no-history");
    let home = temp.root.join("home");
    std::fs::create_dir_all(&home).expect("a home");
    let config = home.join(".config");

    let none = run(
        &[("HOME", &home), ("XDG_CONFIG_HOME", &config)],
        &["discover"],
    );
    assert_eq!(none.code, 0, "{}", none.err);
    assert_eq!(
        none.out,
        "reading your Claude Code history on this computer; nothing leaves it\n\
         no Claude Code history on this computer\n"
    );
    assert!(!config.join("catervas").join(MACHINE_DB).exists());

    let nothing = run(&[], &["discover"]);
    assert_eq!(nothing.code, 1);
    assert_eq!(
        nothing.err,
        "catervas: Catervas has no state folder; set HOME\n"
    );
    let only_xdg = run(&[("XDG_CONFIG_HOME", &config)], &["discover"]);
    assert_eq!(only_xdg.code, 1);
    assert_eq!(
        only_xdg.err,
        "catervas: Catervas can't tell where Claude Code keeps its history; set HOME\n"
    );
}

/// Guard: checked by mutation (print a path raw), not watched to fail.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_paths_as_text() {
    let world = World::new("prints-paths-as-text");
    let cwd = format!("{}/a\u{1b}[2Jb", world.home().to_string_lossy());
    let line = a_turn_line(SESSION, &cwd, "2026-10-12T09:00:00Z");
    std::fs::write(world.transcript(), format!("{line}\n")).expect("a transcript");

    let ran = world.discover(&[]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.out.contains('\u{1b}'), "{:?}", ran.out);
    assert!(
        ran.out
            .contains("no longer on this computer: 1\n  ~/a\\u001b[2Jb  last worked 2026-10-12"),
        "{}",
        ran.out
    );
}

/// Every section of the report at once, with home given as a link to the real home, and every
/// path the report must show as it is or as `~/`.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_every_section() {
    use std::os::unix::fs::symlink;

    let world = World::new("prints-every-section");
    let home = world.home();
    let root = home.parent().expect("a parent").to_path_buf();
    let linked_home = root.join("home-link");
    symlink(&home, &linked_home).expect("a link to home");
    let tmpdir = root.join("tmpdir");
    std::fs::create_dir_all(&tmpdir).expect("a temporary folder");
    let tmpdir = tmpdir.canonicalize().expect("a temporary folder");
    world.make_shop();
    let shop = home.join("shop");
    let worktree = shop.join(".catervas/local/worktrees/CTV-1");
    std::fs::create_dir_all(&worktree).expect("a Catervas worktree");
    world.make_repository("clone", "other@example.invalid");
    let clone = home.join("clone");
    let sibling = PathBuf::from(format!("{}2/x", home.to_string_lossy()));
    std::fs::create_dir_all(&sibling).expect("a sibling of home");

    let sessions = [
        (&home, "2026-10-12T08:00:00Z"),
        (&tmpdir, "2026-10-12T08:01:00Z"),
        (&shop, "2026-10-12T09:00:00Z"),
        (&worktree, "2026-10-12T09:01:00Z"),
        (&clone, "2026-10-10T09:00:00Z"),
        (&home.join("gone-app"), "2026-10-09T09:00:00Z"),
        (&sibling, "2026-10-11T09:00:00Z"),
    ];
    let transcripts = home.join(".claude/projects/-shop");
    for (number, (cwd, at)) in sessions.iter().enumerate() {
        let id = format!("00000000-0000-4000-8000-{number:012}");
        let line = a_turn_line(&id, &cwd.to_string_lossy(), at);
        std::fs::write(transcripts.join(format!("{id}.jsonl")), format!("{line}\n"))
            .expect("a transcript");
    }
    std::fs::write(
        transcripts.join("bad.jsonl"),
        "not json\nnot json\nnot json\n",
    )
    .expect("an unreadable file");
    let outside = root.join("outside.jsonl");
    std::fs::write(&outside, "NEVER-READ\n").expect("an outside file");
    symlink(&outside, transcripts.join("escape.jsonl")).expect("a link out of projects");

    let config = world.config();
    let env = [
        ("HOME", linked_home.as_path()),
        ("TMPDIR", tmpdir.as_path()),
        ("XDG_CONFIG_HOME", config.as_path()),
    ];
    let ran = run(&env, &["discover"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    let outside_home = sibling.to_string_lossy();
    assert_eq!(
        ran.out,
        format!(
            "reading your Claude Code history on this computer; nothing leaves it\n\
             read 8 transcript files: 7 sessions, 7 new\n\
             refused 1 file outside Claude Code's transcripts\n\
             couldn't read 1 session from this version of Claude Code\n\
             skipped 3 lines it couldn't read\n\
             projects: 2\n  \
             shop  ~/shop  last worked 2026-10-12  1 session  0 in  0 out\n    \
             Catervas's agents  1 session  0 in  0 out\n  \
             x  {outside_home}  last worked 2026-10-11  1 session  0 in  0 out\n\
             explored: 1\n  \
             clone  ~/clone  last worked 2026-10-10  1 session  0 in  0 out\n\
             no longer on this computer: 1\n  \
             ~/gone-app  last worked 2026-10-09  1 session  0 in  0 out\n\
             not in a project: 2 sessions\n"
        )
    );

    let as_json = run(&env, &["discover", "--json"]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    assert_eq!(value["refused"], 1);
    assert_eq!(value["unreadable_sessions"], 1);
    assert_eq!(value["unreadable_lines"], 3);
    assert_eq!(value["gone"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        value["gone"][0]["folder"],
        home.join("gone-app").to_string_lossy().as_ref()
    );
    assert_eq!(value["not_a_project"]["sessions"], 2);
}

/// A folder holding a `crontab` program that prints these lines, as the whole `PATH`.
fn crontab_path(world: &World, lines: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    let bin = world.temp.root.join("bin");
    std::fs::create_dir_all(&bin).expect("a folder");
    let script = bin.join("crontab");
    // Only shell built-ins: the `PATH` of this test holds nothing but this script.
    let quoted: Vec<String> = lines.lines().map(|line| format!("'{line}'")).collect();
    let body = format!("#!/bin/sh\nprintf '%s\\n' {}\n", quoted.join(" "));
    std::fs::write(&script, body).expect("a script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("executable");
    bin
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn discovers_the_jobs_of_a_project() {
    let world = World::new("discovers-jobs");
    world.make_shop();
    world.write_fixture();
    let home = world.home();
    let shop = home.join("shop");
    std::fs::create_dir_all(shop.join(".github/workflows")).expect("workflows");
    std::fs::write(
        shop.join(".github/workflows/nightly.yml"),
        catervas_core::jobs::fixtures::GITHUB_WORKFLOW,
    )
    .expect("a workflow");
    git_in(&shop, &["add", "-A"]);
    git_in(&shop, &["commit", "-m", "a workflow"]);
    std::fs::create_dir_all(shop.join("bin")).expect("bin");
    std::fs::write(shop.join("bin/sync.sh"), "x").expect("a script");
    let job = home.join(".claude/jobs/abc12345");
    std::fs::create_dir_all(&job).expect("a job folder");
    std::fs::write(
        job.join("state.json"),
        catervas_core::jobs::fixtures::BACKGROUND_JOB.replace("/home/me", &home.to_string_lossy()),
    )
    .expect("a state");
    let lines = format!(
        "0 2 * * * {}/shop/bin/sync.sh --token SECRET-CLI\n0 1 * * * /usr/bin/backup-all",
        home.to_string_lossy()
    );
    let bin = crontab_path(&world, &lines);
    let config = world.config();
    let env = [
        ("HOME", home.as_path()),
        ("XDG_CONFIG_HOME", config.as_path()),
        ("PATH", bin.as_path()),
    ];

    let ran = run(&env, &["discover"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        "reading your Claude Code history on this computer; nothing leaves it\n\
         read 1 transcript file: 1 session, 1 new\n\
         skipped 2 lines it couldn't read\n\
         found 4 jobs\n\
         projects: 1\n  \
         shop  ~/shop  last worked 2026-10-12  1 session  1.2k in  47 out\n    \
         jobs: 3\n      \
         nightly-sync  GitHub Actions  .github/workflows/nightly.yml  0 3 * * *; 30 4 * * 1\n      \
         sync.sh  crontab  0 2 * * *\n      \
         shop-cleanup  background job  blocked  2026-10-11\n\
         this computer: 1 job\n  \
         backup-all  crontab  0 1 * * *\n"
    );
    let as_json = run(&env, &["discover", "--json"]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    assert_eq!(
        value["projects"][0]["jobs"][0],
        json!({
            "kind": "github_actions", "name": "nightly-sync",
            "file": ".github/workflows/nightly.yml",
            "schedules": ["0 3 * * *", "30 4 * * 1"],
            "expires_at": null, "state": null, "state_at": null,
            "last_seen": "2026-10-12T12:00:00Z",
        })
    );
    assert_eq!(value["this_computer"][0]["kind"], "crontab");
    assert_eq!(value["this_computer"][0]["name"], "backup-all");
    assert_eq!(value["this_computer"][0]["file"], Value::Null);
    assert_eq!(value["jobs_found"], 4);
    let bytes = std::fs::read(config.join("catervas").join(MACHINE_DB)).expect("the index");
    assert!(!bytes.windows(6).any(|window| window == b"SECRET"));
}

/// Guard: checked by mutation (print a name raw), not watched to fail.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_jobs_as_text() {
    let world = World::new("prints-jobs-as-text");
    world.make_shop();
    world.write_fixture();
    let bin = crontab_path(&world, "0 1 * * * /usr/bin/a\u{1b}[2Jb");
    let home = world.home();
    let config = world.config();
    let env = [
        ("HOME", home.as_path()),
        ("XDG_CONFIG_HOME", config.as_path()),
        ("PATH", bin.as_path()),
    ];

    let ran = run(&env, &["discover"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.out.contains('\u{1b}'), "{:?}", ran.out);
    assert!(ran.out.contains("a\\u001b[2Jb  crontab"), "{}", ran.out);
}

const CONNECTORS_SESSION: &str = "33333333-3333-4333-8333-333333333333";

impl World {
    /// The connectors fixture, worked at home, in a transcript folder of its own.
    fn write_connectors(&self) {
        let folder = self.home().join(".claude/projects/-home");
        std::fs::create_dir_all(&folder).expect("a projects folder");
        let text =
            CLAUDE_CODE_2_1_296_CONNECTORS.replace("/home/me", &self.home().to_string_lossy());
        std::fs::write(folder.join(format!("{CONNECTORS_SESSION}.jsonl")), text)
            .expect("a transcript");
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn discovers_services_used_through_claude() {
    let world = World::new("discovers-services");
    world.make_shop();
    world.write_fixture();
    world.write_connectors();
    std::fs::write(
        world.home().join(".claude.json"),
        r#"{"mcpServers":{"stripe":{"type":"http","url":"https://mcp.stripe.com","headers":{"Authorization":"Bearer KEY-MARKER"}}}}"#,
    )
    .expect("a settings file");

    let ran = world.discover(&[]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        "reading your Claude Code history on this computer; nothing leaves it\n\
         read 2 transcript files: 2 sessions, 2 new\n\
         skipped 2 lines it couldn't read\n\
         projects: 2\n  \
         Bookkeeping (Zoho Books)  last worked 2026-10-12  1 session  6 in  6 out\n    \
         services: Zoho Books 3 calls, last 2026-10-12; slack 1 call, last 2026-10-12\n  \
         shop  ~/shop  last worked 2026-10-12  1 session  1.2k in  47 out\n\
         services you use with Claude: Zoho Books, Stripe, slack\n"
    );

    let as_json = world.discover(&["--json"]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    let formed = &value["projects"][0];
    assert_eq!(formed["root"], Value::Null);
    assert_eq!(formed["name"], "Bookkeeping (Zoho Books)");
    assert_eq!(formed["work"], "bookkeeping");
    assert_eq!(formed["jobs"], json!([]));
    assert_eq!(
        formed["services"][0],
        json!({
            "id": "zoho_books", "name": "Zoho Books", "kind": "bookkeeping",
            "calls": 3, "last_used_at": "2026-10-12T09:10:20Z",
        })
    );
    assert_eq!(value["projects"][1]["work"], Value::Null);
    assert_eq!(
        value["services"],
        json!([
            {"id": "zoho_books", "name": "Zoho Books", "kind": "bookkeeping", "host": null,
             "sources": ["claude_ai", "transcripts"]},
            {"id": "stripe", "name": "Stripe", "kind": "payments", "host": "mcp.stripe.com",
             "sources": ["claude_code"]},
            {"id": "slack@", "name": "slack", "kind": "other", "host": null,
             "sources": ["transcripts"]},
        ])
    );
    assert_eq!(value["not_a_project"]["sessions"], 0);
    assert_eq!(value["desktop"], Value::Null);
    let bytes = std::fs::read(world.config().join("catervas").join(MACHINE_DB)).expect("the index");
    assert!(!bytes.windows(10).any(|window| window == b"KEY-MARKER"));
}

/// Guard: checked by mutation (print a header, keep a key), not watched to fail.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn prints_no_key() {
    let world = World::new("prints-no-key");
    world.make_shop();
    world.write_fixture();
    world.write_connectors();
    let home = world.home();
    std::fs::write(home.join(".claude.json"), a_claude_json_full_of_keys()).expect("settings");
    let desktop = world.config().join("Claude");
    std::fs::create_dir_all(&desktop).expect("a desktop folder");
    std::fs::write(
        desktop.join("claude_desktop_config.json"),
        a_desktop_config_full_of_keys(),
    )
    .expect("a desktop config");
    let plugin = home.join(".claude/plugins/cache/kw/pm/1.0");
    std::fs::create_dir_all(&plugin).expect("a plugin folder");
    std::fs::write(plugin.join(".mcp.json"), a_plugin_mcp_json_full_of_keys()).expect("a plugin");
    std::fs::write(
        home.join(".claude/settings.json"),
        json!({"enabledPlugins": {"pm@kw": true}}).to_string(),
    )
    .expect("settings");
    std::fs::write(
        home.join(".claude/plugins/installed_plugins.json"),
        json!({"plugins": {"pm@kw": [{"scope": "user", "installPath": plugin}]}}).to_string(),
    )
    .expect("installed plugins");

    let text = world.discover(&[]);
    let as_json = world.discover(&["--json"]);

    assert_eq!(text.code, 0, "{}", text.err);
    for out in [&text.out, &as_json.out] {
        assert!(!out.contains("KEY-MARKER"), "{out}");
        for name in ["Stripe", "slack"] {
            assert!(out.contains(name), "{name} in {out}");
        }
    }
    let bytes = std::fs::read(world.config().join("catervas").join(MACHINE_DB)).expect("the index");
    assert!(!bytes.windows(10).any(|window| window == b"KEY-MARKER"));
}

/// A home with no Claude Code history in it.
fn bare_home(name: &str) -> (TempProject, PathBuf, PathBuf) {
    let temp = TempProject::new(name);
    let home = temp.root.join("home");
    std::fs::create_dir_all(&home).expect("a home");
    let config = home.join(".config");
    (temp, home, config)
}

fn discover_bare(home: &Path, config: &Path, args: &[&str]) -> Ran {
    let mut arguments = vec!["discover"];
    arguments.extend_from_slice(args);
    run(&[("HOME", home), ("XDG_CONFIG_HOME", config)], &arguments)
}

const EXPORT_REPORT: &str = "claude.ai: 3 conversations, 2 Projects, latest export up to 2026-10-05\n  \
      2026-10  1 conversation\n  \
      2026-09  1 conversation\n  \
      2026-08  0 conversations\n  \
      2026-07  0 conversations\n  \
      2026-06  0 conversations\n  \
      2026-05  0 conversations\n  \
      2026-04  0 conversations\n  \
      2026-03  0 conversations\n  \
      2026-02  0 conversations\n  \
      2026-01  0 conversations\n  \
      2025-12  0 conversations\n  \
      2025-11  0 conversations\n  \
      undated  1 conversation\n  \
      Projects:\n    \
      Shop launch  2 files  last updated 2026-10-01\n    \
      Taxes  no files  last updated 2026-09-12\n";

#[test]
fn reads_a_claude_export() {
    let (_temp, home, config) = bare_home("reads-claude-export");
    let zip = home.join("export.zip");
    std::fs::write(&zip, the_2026_10_export()).expect("an export");
    let zip_arg = zip.to_string_lossy().into_owned();

    let ran = discover_bare(&home, &config, &["--claude-export", &zip_arg]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        format!(
            "reading your claude.ai export on this computer; nothing leaves it\n\
             read claude.ai's export: 3 conversations (3 new), 2 Projects (2 new)\n\
             skipped 1 conversation it couldn't read\n\
             reading your Claude Code history on this computer; nothing leaves it\n\
             no Claude Code history on this computer\n{EXPORT_REPORT}"
        )
    );

    let again = discover_bare(&home, &config, &["--claude-export", &zip_arg]);
    assert!(
        again
            .out
            .contains("read claude.ai's export: 3 conversations (0 new), 2 Projects (0 new)\n"),
        "{}",
        again.out
    );

    let as_json = discover_bare(&home, &config, &["--claude-export", &zip_arg, "--json"]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    let months = value["claude_ai"]["by_month"].as_array().expect("months");
    assert_eq!(months.len(), 12);
    assert_eq!(months[0], json!({"month": "2026-10", "conversations": 1}));
    assert_eq!(months[1], json!({"month": "2026-09", "conversations": 1}));
    assert!(months[2..].iter().all(|m| m["conversations"] == 0));
    assert_eq!(value["claude_ai"]["undated"], 1);
    assert_eq!(value["export"]["skipped"], 1);
    assert_eq!(value["export"]["new_conversations"], 0);
    assert_eq!(value["claude_ai"]["projects"][0]["name"], "Shop launch");
    assert_eq!(
        value["claude_ai"]["projects"][0]["doc_names"],
        json!(["brief.md", "prices.csv"])
    );
    assert_eq!(
        value["claude_ai"]["not_in_latest_export"],
        json!({"conversations": 0, "projects": 0})
    );

    let plain = discover_bare(&home, &config, &[]);
    assert_eq!(plain.code, 0, "{}", plain.err);
    assert_eq!(
        plain.out,
        format!(
            "reading your Claude Code history on this computer; nothing leaves it\n\
             no Claude Code history on this computer\n{EXPORT_REPORT}"
        )
    );
}

#[test]
fn refuses_what_is_not_a_claude_export() {
    let (_temp, home, config) = bare_home("refuses-not-an-export");
    let text = home.join("notes.txt");
    std::fs::write(&text, "hello").expect("a file");

    let ran = discover_bare(
        &home,
        &config,
        &["--claude-export", &text.to_string_lossy()],
    );

    assert_eq!(ran.code, 1);
    assert_eq!(
        ran.err,
        "catervas: this isn't claude.ai's export: it isn't a zip file; give Catervas the .zip claude.ai emailed you\n"
    );
    assert_eq!(ran.out, "");
}

/// Guard: checked by mutation (print a name raw), not watched to fail.
#[test]
fn prints_claude_ai_names_as_text() {
    let (_temp, home, config) = bare_home("prints-claude-ai-names");
    let project = a_project_json(
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "a\u{1b}[2Jb",
        "2026-10-01T09:00:00Z",
        &[],
    );
    let conversation = a_conversation_json(
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "c\u{1b}[2Jd",
        "2026-10-01T09:00:00Z",
        "2026-10-01T09:00:00Z",
        1,
    );
    let (conversations, projects) = (format!("[{conversation}]"), format!("[{project}]"));
    let zip = home.join("export.zip");
    std::fs::write(
        &zip,
        an_export_zip(&[
            ("conversations.json", conversations.as_bytes()),
            ("projects.json", projects.as_bytes()),
        ]),
    )
    .expect("an export");

    let ran = discover_bare(&home, &config, &["--claude-export", &zip.to_string_lossy()]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.out.contains('\u{1b}'), "{:?}", ran.out);
    assert!(
        ran.out
            .contains("    a\\u001b[2Jb  no files  last updated 2026-10-01\n"),
        "{}",
        ran.out
    );
}

/// Three sessions in `shop` that replied the same four times under different ids.
fn write_three_copies(world: &World) {
    let cwd = world.home().join("shop").to_string_lossy().into_owned();
    for (n, id) in ["a", "b", "c"].iter().enumerate() {
        let session = format!("0000000{n}-0000-4000-8000-000000000000");
        let lines: Vec<String> = (1..=4)
            .map(|second| {
                a_reply_line(
                    &session,
                    &cwd,
                    &format!("2026-10-12T09:00:0{second}Z"),
                    &format!("msg_{id}_{second}"),
                    "claude-opus-5-5",
                    Usage {
                        input_tokens: second,
                        output_tokens: 1,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                    },
                )
            })
            .collect();
        std::fs::write(
            world
                .home()
                .join(format!(".claude/projects/-shop/copy-{id}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("a transcript");
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn discovers_copies_and_projects_by_hand() {
    let world = World::new("discovers-copies-and-hand");
    world.make_shop();
    git_in(
        &world.home().join("shop"),
        &["remote", "add", "origin", "git@github.com:me/shop.git"],
    );
    world.write_fixture();
    let plain = world.discover(&[]);
    assert_eq!(plain.code, 0, "{}", plain.err);

    let added = world.discover(&[
        "--add-project",
        "Shop",
        "--repo",
        "https://github.com/Me/Shop",
    ]);
    assert_eq!(added.code, 0, "{}", added.err);
    assert_eq!(
        added.out,
        "added Shop by hand: the same project as ~/shop\n"
    );
    let as_json = world.discover(&[
        "--add-project",
        "Shop",
        "--repo",
        "https://github.com/Me/Shop",
        "--json",
    ]);
    let value: Value = serde_json::from_str(&as_json.out).expect("one JSON object");
    assert_eq!(
        value,
        json!({
            "name": "Shop", "root": null, "repository": "github.com/me/shop",
            "same_as": world.home().join("shop").to_string_lossy(),
        })
    );

    let shown = world.discover(&[]);
    assert!(
        shown.out.contains("  Shop  ~/shop  last worked 2026-10-12"),
        "{}",
        shown.out
    );
    assert!(!shown.out.contains("added by hand"), "{}", shown.out);
    let shown_json: Value =
        serde_json::from_str(&world.discover(&["--json"]).out).expect("one JSON object");
    let shop = &shown_json["projects"][0];
    assert_eq!(shop["name"], "Shop");
    assert_eq!(shop["sources"], json!(["by_hand", "local"]));
    assert_eq!(shop["repository"], "github.com/me/shop");

    let billing = world.discover(&[
        "--add-project",
        "Billing",
        "--repo",
        "git@github.com:me/billing.git",
    ]);
    assert_eq!(billing.out, "added Billing by hand\n");
    let listed = world.discover(&[]);
    assert!(
        listed
            .out
            .contains("added by hand: 1\n  Billing  github.com/me/billing\n"),
        "{}",
        listed.out
    );
    let listed_json: Value =
        serde_json::from_str(&world.discover(&["--json"]).out).expect("one JSON object");
    assert_eq!(
        listed_json["by_hand"],
        json!([{"name": "Billing", "root": null, "repository": "github.com/me/billing"}])
    );

    let nowhere = world.discover(&["--add-project", "X"]);
    assert_eq!(nowhere.code, 1);
    assert_eq!(
        nowhere.err,
        "catervas: say where the project is: --folder <path> or --repo <address>\n"
    );

    write_three_copies(&world);
    let copies = world.discover(&[]);
    assert!(
        copies
            .out
            .contains("3 sessions may be copies of others; their usage is counted once\n"),
        "{}",
        copies.out
    );
}
