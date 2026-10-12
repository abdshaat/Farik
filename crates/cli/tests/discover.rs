//! `catervas discover`, through the command line.
//!
//! The tests that reach the `git` program are `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas::{CliIo, run_cli};
use catervas_core::claude_history::fixtures::{CLAUDE_CODE_2_1_296, a_turn_line};
use catervas_protocol::clock::FixedClock;
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
        let shop = self.home().join("shop");
        std::fs::create_dir_all(&shop).expect("a shop");
        git_in(&shop, &["init", "-b", "main"]);
        for (key, value) in [
            ("user.email", "me@example.invalid"),
            ("user.name", "Me"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            git_in(&shop, &["config", key, value]);
        }
        std::fs::write(shop.join("README.md"), "x\n").expect("a file");
        git_in(&shop, &["add", "-A"]);
        git_in(&shop, &["commit", "-m", "the first commit"]);
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
