//! `farik skill` (`docs/SPEC.md` 6.7, ADR 0034): a skill is shown whole before it is added, and
//! added, confirmed or removed through the command that the driving process handles, or here.
//!
//! Every test here needs the `git` program, and is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use farik_core::skill::skill_sha256;
use farik_protocol::command::{Command, SkillScope};
use farik_protocol::event::EventKind;
use farik_runtime::skills::read_skill_folder;
use farik_store::git::fixtures::TempRepo;

use project::{LiveDriver, a_team, events, files_of, run, run_with, scratch};

/// A skill folder `name` in a scratch folder: `SKILL.md` and `references/a.md`.
fn a_skill(test: &str, name: &str, body: &str) -> PathBuf {
    let folder = scratch(test).join(name);
    std::fs::create_dir_all(folder.join("references")).expect("a skill folder");
    std::fs::write(
        folder.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Use when {name}.\n---\n{body}"),
    )
    .expect("a SKILL.md");
    std::fs::write(folder.join("references/a.md"), "details").expect("a reference");
    folder
}

fn hash_of(folder: &Path) -> String {
    skill_sha256(&read_skill_folder(folder).expect("a readable folder"))
}

/// Runs `farik skill <args>` with `answer` on standard input, at a terminal when `terminal`.
fn skill(repository: &TempRepo, args: &[&str], answer: &str, terminal: bool) -> project::Ran {
    let answer = answer.to_string();
    let mut all = vec!["skill"];
    all.extend_from_slice(args);
    run_with(&repository.path, &all, move |io| {
        io.stdin = Box::new(Cursor::new(answer.into_bytes()));
        io.stdin_is_terminal = terminal;
    })
}

fn pinned(repository: &TempRepo, agent: &str) -> Vec<String> {
    let team = files_of(repository).read_team().expect("the team");
    team.agents
        .iter()
        .find(|held| held.id.as_str() == agent)
        .expect("the agent")
        .skills
        .iter()
        .map(|pin| pin.name.to_string())
        .collect()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_show_refuses_a_linked_skills_folder() {
    let repository = a_team("skill-show-linked");
    let outside = a_skill("skill-show-linked", "api-style", "OUTSIDE");
    let skills = repository.path.join(".farik/skills");
    std::fs::create_dir_all(&skills).expect("a folder");
    std::os::unix::fs::symlink(&outside, skills.join("api-style")).expect("a link");
    let ran = skill(&repository, &["show", "api-style", "--team"], "", false);
    assert_ne!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(!ran.out.contains("OUTSIDE"), "{}", ran.out);
    assert!(
        ran.err.contains("skill_path_invalid") || ran.err.contains(".farik/skills/api-style"),
        "{}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_add_escapes_a_skills_control_characters() {
    let repository = a_team("skill-add-escapes");
    let folder = a_skill("skill-add-escapes", "api-style", "\u{1b}[2Jhidden");
    let args = ["add", folder.to_str().expect("a path"), "--agent", "dev-a"];
    let ran = skill(&repository, &args, "n\n", true);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    let asked = ran
        .out
        .find("Add api-style for dev-a? [y/N]")
        .expect("it asks");
    let before = &ran.out[..asked];
    assert!(before.contains("\\u001b[2Jhidden"), "escaped: {before:?}");
    assert!(
        !before.contains('\u{1b}'),
        "no ESC byte reaches the terminal: {before:?}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_add_shows_then_asks() {
    let repository = a_team("skill-add-asks");
    let folder = a_skill("skill-add-asks", "api-style", "THE WHOLE BODY");
    let sha = hash_of(&folder);
    let args = ["add", folder.to_str().expect("a path"), "--agent", "dev-a"];

    // At a terminal, answering n: it showed the skill, asked, and nothing was done.
    let ran = skill(&repository, &args, "n\n", true);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    let (shown, asked) = (
        ran.out
            .find("THE WHOLE BODY")
            .expect("the SKILL.md is shown"),
        ran.out
            .find("Add api-style for dev-a? [y/N]")
            .expect("it asks"),
    );
    assert!(ran.out.contains(&sha), "the hash is shown: {}", ran.out);
    assert!(ran.out.contains("details"), "so is every file: {}", ran.out);
    assert!(
        shown < asked && ran.out.find(&sha) < Some(asked),
        "all of it before the question"
    );
    assert!(events(&repository, &[EventKind::SkillAdded]).is_empty());
    assert!(pinned(&repository, "dev-a").is_empty());
    assert!(!repository.path.join(".farik/agents/dev-a/skills").exists());

    // Answering y sends skill_save to the process driving the project.
    let driver = LiveDriver::new(&repository);
    let ran = skill(&repository, &args, "y\n", true);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    let commands = driver.commands();
    let [
        Command::SkillSave {
            scope,
            files,
            replace_shipped,
        },
    ] = commands.as_slice()
    else {
        panic!("one skill_save, not {commands:?}");
    };
    assert_eq!(scope, &SkillScope::Agent("dev-a".to_string()));
    assert!(!replace_shipped);
    let sent: BTreeMap<String, Vec<u8>> = files
        .iter()
        .map(|(path, text)| (path.clone(), text.clone().into_bytes()))
        .collect();
    assert_eq!(skill_sha256(&sent), sha, "what is sent is what was shown");
    drop(driver);

    // With nothing driving, it is handled here.
    let ran = skill(&repository, &args, "yes\n", true);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(
        ran.out
            .lines()
            .last()
            .is_some_and(|line| line == "Added api-style for dev-a."),
        "{}",
        ran.out
    );
    assert_eq!(pinned(&repository, "dev-a"), ["api-style"]);
    assert_eq!(events(&repository, &[EventKind::SkillAdded]).len(), 1);
    assert_eq!(
        hash_of(&repository.path.join(".farik/agents/dev-a/skills/api-style")),
        sha
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_add_needs_yes_without_a_terminal() {
    let repository = a_team("skill-add-yes");
    let folder = a_skill("skill-add-yes", "api-style", "body");
    let path = folder.to_str().expect("a path");

    let ran = skill(&repository, &["add", path, "--team"], "", false);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("--yes"), "{}", ran.err);
    assert!(events(&repository, &[EventKind::SkillAdded]).is_empty());

    let ran = skill(&repository, &["add", path, "--team", "--yes"], "", false);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(
        ran.out.contains("body"),
        "--yes still shows what it adds: {}",
        ran.out
    );
    assert_eq!(
        files_of(&repository)
            .read_team()
            .expect("team")
            .skills()
            .len(),
        1
    );

    // A shipped name is replaced only on the person's say so.
    let shipped = a_skill("skill-add-yes", "writing-task-contracts", "mine");
    let shipped = shipped.to_str().expect("a path");
    let ran = skill(&repository, &["add", shipped, "--team", "--yes"], "", false);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("--replace"), "{}", ran.err);
    assert!(ran.err.contains("writing-task-contracts"), "{}", ran.err);
    let ran = skill(
        &repository,
        &["add", shipped, "--team", "--yes", "--replace"],
        "",
        false,
    );
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert_eq!(
        files_of(&repository)
            .read_team()
            .expect("team")
            .skills()
            .len(),
        2
    );

    // A skill the checks refuse is refused before it is shown or asked about.
    let running = a_skill("skill-add-yes", "runner", "run !`ls`");
    let ran = skill(
        &repository,
        &["add", running.to_str().expect("a path"), "--team", "--yes"],
        "",
        false,
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("skill_runs_commands"), "{}", ran.err);
    // Neither --team nor --agent, or both, is the command line's to refuse.
    assert_eq!(skill(&repository, &["add", path], "", false).code, 2);
    assert_eq!(
        skill(
            &repository,
            &["add", path, "--team", "--agent", "dev-a"],
            "",
            false
        )
        .code,
        2
    );
    let ran = skill(
        &repository,
        &["add", path, "--agent", "ghost", "--yes"],
        "",
        false,
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("ghost"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_confirm_takes_a_prefix() {
    let repository = a_team("skill-confirm");
    let folder = a_skill("skill-confirm", "api-style", "first");
    let ran = skill(
        &repository,
        &["add", folder.to_str().expect("a path"), "--team", "--yes"],
        "",
        false,
    );
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    // A pull changes the folder and the pin together.
    let placed = repository.path.join(".farik/skills/api-style");
    std::fs::write(placed.join("references/a.md"), "changed by a pull").expect("an edit");
    let sha = hash_of(&placed);
    let confirm = |given: &str| {
        skill(
            &repository,
            &["confirm", "api-style", "--team", given],
            "",
            false,
        )
    };

    for refused in [&sha[..11], "0123456789ab", &"f".repeat(64)] {
        let ran = confirm(refused);
        assert_eq!(ran.code, 1, "{refused}: {}", ran.out);
        assert!(
            events(&repository, &[EventKind::SkillConfirmed]).is_empty(),
            "{refused}"
        );
    }
    let ran = confirm(&sha[..12]);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert_eq!(
        ran.out.lines().last(),
        Some("Confirmed api-style for the team.")
    );
    let team = files_of(&repository).read_team().expect("team");
    assert_eq!(
        team.skills()[0].sha256.as_str(),
        sha,
        "the pin is the hash the person read"
    );
    assert_eq!(events(&repository, &[EventKind::SkillConfirmed]).len(), 1);
    // The whole hash does too.
    assert_eq!(confirm(&sha).code, 0);
    // Confirming a shipped name needs --replace.
    let shipped = a_skill("skill-confirm", "writing-task-contracts", "mine");
    skill(
        &repository,
        &[
            "add",
            shipped.to_str().expect("a path"),
            "--team",
            "--yes",
            "--replace",
        ],
        "",
        false,
    );
    let placed = repository.path.join(".farik/skills/writing-task-contracts");
    let sha = hash_of(&placed);
    let ran = confirm_named(&repository, "writing-task-contracts", &sha[..12], &[]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("--replace"), "{}", ran.err);
    assert_eq!(
        confirm_named(
            &repository,
            "writing-task-contracts",
            &sha[..12],
            &["--replace"]
        )
        .code,
        0
    );
}

fn confirm_named(repository: &TempRepo, name: &str, given: &str, extra: &[&str]) -> project::Ran {
    let mut args = vec!["confirm", name, "--team", given];
    args.extend_from_slice(extra);
    skill(repository, &args, "", false)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_list_shows_levels_and_states() {
    let repository = a_team("skill-list");
    for (name, whom) in [
        ("api-style", ["--team"].as_slice()),
        ("notes", ["--agent", "dev-a"].as_slice()),
    ] {
        let folder = a_skill("skill-list", name, "x");
        let mut args = vec!["add", folder.to_str().expect("a path")];
        args.extend_from_slice(whom);
        args.push("--yes");
        let ran = skill(&repository, &args, "", false);
        assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    }
    // The agent's skill is edited after it was confirmed: review.
    std::fs::write(
        repository
            .path
            .join(".farik/agents/dev-a/skills/notes/references/a.md"),
        "edited",
    )
    .expect("an edit");
    let ran = skill(&repository, &["list", "--agent", "dev-a"], "", false);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        [
            "role  implementing-a-contract  in use",
            "role  test-driven-development  in use",
            "role  debugging  in use",
            "role  safe-migrations  in use",
            "role  testing-per-stack  in use",
            "role  answering-a-review  in use",
            "role  using-docs-and-the-browser  in use",
            "team  api-style  in use",
            "agent  notes  review",
        ]
    );
    // Without an agent, the team's rows alone.
    let ran = skill(&repository, &["list"], "", false);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        ["team  api-style  in use"]
    );
    let json = run(
        &repository.path,
        &["--json", "skill", "list", "--agent", "dev-a"],
    );
    let value: serde_json::Value = serde_json::from_str(&json.out).expect("JSON");
    assert_eq!(value["skills"][8]["state"], "review");
    // A missing and a replaced state read as the words say.
    std::fs::remove_dir_all(repository.path.join(".farik/skills/api-style")).expect("gone");
    let ran = skill(&repository, &["list"], "", false);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        ["team  api-style  missing"]
    );
    let own = a_skill("skill-list", "api-style", "mine");
    skill(
        &repository,
        &[
            "add",
            own.to_str().expect("a path"),
            "--agent",
            "dev-a",
            "--yes",
        ],
        "",
        false,
    );
    let ran = skill(&repository, &["list", "--agent", "dev-a"], "", false);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        [
            "role  implementing-a-contract  in use",
            "role  test-driven-development  in use",
            "role  debugging  in use",
            "role  safe-migrations  in use",
            "role  testing-per-stack  in use",
            "role  answering-a-review  in use",
            "role  using-docs-and-the-browser  in use",
            "team  api-style  replaced",
            "agent  notes  review",
            "agent  api-style  in use",
        ]
    );
    let ran = skill(&repository, &["list", "--agent", "ghost"], "", false);
    assert_eq!(ran.code, 1, "{}", ran.out);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_show_prints_every_file_escaped_and_remove_removes() {
    let repository = a_team("skill-show");
    let folder = a_skill("skill-show", "api-style", "line\u{1b}[31mred");
    skill(
        &repository,
        &["add", folder.to_str().expect("a path"), "--team", "--yes"],
        "",
        false,
    );
    let placed = repository.path.join(".farik/skills/api-style");
    let ran = skill(&repository, &["show", "api-style", "--team"], "", false);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(
        !ran.out.contains('\u{1b}'),
        "a control character never reaches the terminal"
    );
    assert!(ran.out.contains("\\u001b[31mred"), "{}", ran.out);
    assert!(ran.out.contains("references/a.md"), "{}", ran.out);
    assert!(ran.out.contains(&hash_of(&placed)), "{}", ran.out);
    let ran = skill(&repository, &["show", "nothing", "--team"], "", false);
    assert_eq!(ran.code, 1, "{}", ran.out);

    let ran = skill(&repository, &["remove", "api-style", "--team"], "", false);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(
        ran.out.contains("Removed api-style for the team."),
        "{}",
        ran.out
    );
    assert!(!placed.exists());
    assert!(
        files_of(&repository)
            .read_team()
            .expect("team")
            .skills()
            .is_empty()
    );
    assert_eq!(
        skill(&repository, &["remove", "api-style", "--team"], "", false).code,
        1
    );
}

/// The Software Developer's kit with one skill, `launch-plans`.
fn a_kit_with_launch_plans() -> farik_roles::Kit {
    let text = "---\nname: launch-plans\ndescription: Use when planning a launch.\n---\nSteps.\n";
    let file = serde_json::json!({ "role": "software_developer", "skills": ["launch-plans"], "connectors": [] });
    farik_roles::parse_kit(
        farik_core::contract::Role::SoftwareDeveloper,
        &file.to_string(),
        &[],
        &[("launch-plans", &[("SKILL.md", text)])],
    )
    .expect("the fixture kit loads")
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_skill_add_and_confirm_refuse_a_kit_skills_name_without_replace() {
    let repository = a_team("skill-kit-name");
    let mine = a_skill("skill-kit-name", "launch-plans", "mine");
    let mine = mine.to_str().expect("a path");
    let with_the_kit = |args: &[&str]| {
        let kit = a_kit_with_launch_plans();
        let mut all = vec!["skill"];
        all.extend_from_slice(args);
        run_with(&repository.path, &all, move |io| {
            io.kits = std::sync::Arc::new(move |role| {
                if role == farik_core::contract::Role::SoftwareDeveloper {
                    Ok(kit.clone())
                } else {
                    farik_roles::load_kit(role)
                }
            });
        })
    };
    let ran = with_the_kit(&["add", mine, "--team", "--yes"]);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("--replace") && ran.err.contains("launch-plans"),
        "{}",
        ran.err
    );
    // Without the kit, the name is free: it is the kit's presence that takes it.
    let free = skill(&repository, &["add", mine, "--team", "--yes"], "", false);
    assert_eq!(free.code, 0, "{}{}", free.out, free.err);
    let hash = hash_of(&repository.path.join(".farik/skills/launch-plans"));
    let ran = with_the_kit(&["confirm", "launch-plans", "--team", &hash]);
    assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
    assert!(ran.err.contains("--replace"), "{}", ran.err);
}
