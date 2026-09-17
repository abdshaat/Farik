//! The files under `.farik/`, against a real directory.
//!
//! Every test here needs somewhere to write and nothing else, so they run in the default
//! `cargo xtask check` as `event_log_file.rs` does, rather than behind `--integration`.

use farik_store::files::FilesError;
use farik_store::files::fixtures::{TempProject, a_team};

/// Holds a refusal to `docs/standards/code.md`: it is read by the person who edited the file, so it
/// names the file, quotes the line, and says nothing about the API that would have accepted it.
fn said_to_a_person(detail: &str) {
    assert!(
        detail.contains(".farik/team.yaml"),
        "names the file: {detail}"
    );
    for programmer in ["Options", "DuplicateKeyPolicy", "from_multiple", "<input>"] {
        assert!(
            !detail.contains(programmer),
            "says {programmer} to a person editing a team file: {detail}"
        );
    }
}

#[test]
fn makes_the_layout_a_project_starts_with() {
    let project = TempProject::new("init");
    let files = project.files();
    assert_eq!(
        files.root(),
        project.root,
        "the project root is where it was opened"
    );
    files.init(&a_team()).expect("a project is made");

    for directory in [
        "team",
        "contracts",
        "agents",
        "decisions",
        "product",
        "local",
    ] {
        assert!(
            project.root.join(".farik").join(directory).is_dir(),
            "{directory} is where a person will look for it"
        );
    }
    assert_eq!(
        files
            .read_team()
            .expect("the team reads back")
            .name
            .as_str(),
        "Farik"
    );
    // Without this, every repository Farik touches is dirty for good: a task's worktree and the
    // event log both live under .farik/local/ (5.14, 8.4).
    assert_eq!(
        std::fs::read_to_string(project.root.join(".farik/local/.gitignore"))
            .expect("the ignore file"),
        "*\n"
    );
}

#[test]
fn a_second_init_takes_nothing_away() {
    let project = TempProject::new("init-twice");
    let files = project.files();
    files.init(&a_team()).expect("a project is made");

    let mut changed = a_team();
    changed.name = "Renamed by hand".parse().expect("a name");
    files.write_team(&changed).expect("the team is written");
    files.init(&a_team()).expect("init runs again");

    assert_eq!(
        files
            .read_team()
            .expect("the team reads back")
            .name
            .as_str(),
        "Renamed by hand",
        "a second init is a command with nothing to do, not one that throws the team away"
    );
}

#[test]
fn reads_back_the_team_it_wrote() {
    let project = TempProject::new("round-trip-team");
    let files = project.files();
    files.write_team(&a_team()).expect("the team is written");

    assert_eq!(files.read_team().expect("the team"), a_team());
    // YAML, because a person edits this by hand and a person does not edit JSON by hand.
    let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
    assert!(text.starts_with("agents:"), "{text}");
}

#[test]
fn says_there_is_no_team_when_there_is_none() {
    let project = TempProject::new("no-team");
    assert_eq!(
        project.files().read_team(),
        Err(FilesError::NotFound {
            path: ".farik/team.yaml".to_string(),
        })
    );
}

#[test]
fn refuses_a_team_file_a_person_broke() {
    let project = TempProject::new("broken-team");
    let files = project.files();
    files.init(&a_team()).expect("a project is made");

    // A file read back is held to what a team on the wire is held to: the same rule, the same
    // words, whichever door it came through.
    std::fs::write(
        project.root.join(".farik/team.yaml"),
        "name: Farik\nagents: []\nbudgets:\n  daily_usd: 20\npolicy:\n  human_accepts_contracts: high_risk\n  wip_limit_per_agent: 2\n  blocked_limit_hours: 24\n  max_iterations: 3\n  integration: manual\nrules: {}\n",
    )
    .expect("the file is written");
    let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
        panic!("a team of nobody is not a team");
    };
    assert_eq!(path, ".farik/team.yaml");
    assert!(detail.contains("/agents"), "{detail}");

    std::fs::write(project.root.join(".farik/team.yaml"), "name: [\n").expect("the file");
    let Err(FilesError::Invalid { detail, .. }) = files.read_team() else {
        panic!("this is not YAML at all");
    };
    assert!(!detail.is_empty(), "the parser's own words");
}

#[test]
fn refuses_a_yaml_file_that_says_one_thing_twice() {
    // serde-saphyr refuses a duplicate key rather than taking the last quietly, which is the whole
    // reason ADR 0007 takes it: a person who wrote `name:` twice is told so.
    let project = TempProject::new("duplicate-key");
    let files = project.files();
    files.write_team(&a_team()).expect("written");
    let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
    std::fs::write(
        project.root.join(".farik/team.yaml"),
        format!("{text}name: Hijacked\n"),
    )
    .expect("a person writes it twice");
    let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
        panic!("one key, one value");
    };
    assert_eq!(path, ".farik/team.yaml");
    assert!(detail.contains("duplicate mapping key: name"), "{detail}");
    said_to_a_person(&detail);
}

#[test]
fn refuses_a_yaml_file_that_holds_two_documents() {
    // A `---` in the middle of a team file makes everything after it a second document, and taking
    // the first quietly would hide half of what the person wrote.
    let project = TempProject::new("two-documents");
    let files = project.files();
    files.write_team(&a_team()).expect("written");
    let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
    std::fs::write(
        project.root.join(".farik/team.yaml"),
        format!("{text}---\nname: The Second Team\n"),
    )
    .expect("a person leaves a second document behind");
    let Err(FilesError::Invalid { path, detail }) = files.read_team() else {
        panic!("one file, one team");
    };
    assert_eq!(path, ".farik/team.yaml");
    assert!(detail.contains("single YAML document"), "{detail}");
    said_to_a_person(&detail);
}

#[test]
fn keeps_a_word_yaml_would_otherwise_have_an_opinion_about() {
    // YAML 1.1 resolves an unquoted `no`, `y` or `off` to a boolean, so an agent whose id is `no`
    // would reach `validate_team` as `false` and be refused for not being a string. ADR 0007 reads
    // these files with that resolution off, so the person gets the agent they wrote.
    let project = TempProject::new("norway");
    let files = project.files();
    files.write_team(&a_team()).expect("written");
    let path = project.root.join(".farik/team.yaml");
    let text = std::fs::read_to_string(&path).expect("the file");
    let first = a_team().agents[0].id.as_str().to_string();
    std::fs::write(&path, text.replace(&format!("id: {first}"), "id: no"))
        .expect("a person writes an id yaml has an opinion about");
    let team = files.read_team().expect("an id is a word, not a boolean");
    assert_eq!(team.agents[0].id.as_str(), "no");
}

#[test]
fn reads_a_file_a_windows_editor_saved() {
    // A byte-order mark is not content, and a parser that meets one says the file holds two
    // documents -- which is not a thing a person can act on.
    let project = TempProject::new("bom");
    let files = project.files();
    files.write_team(&a_team()).expect("written");
    let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
    std::fs::write(
        project.root.join(".farik/team.yaml"),
        format!("\u{feff}{text}"),
    )
    .expect("saved again with a mark at the front");
    assert_eq!(files.read_team().expect("it still reads"), a_team());
}

#[test]
fn leaves_nothing_half_written() {
    // A write goes to a file beside the one being written and is renamed over it, because a rename
    // within a directory is the one file operation that is all or nothing. A crash in the middle of
    // team.yaml would otherwise leave the team unreadable, and the team is what every session is
    // assembled from.
    let project = TempProject::new("atomic");
    let files = project.files();
    files.init(&a_team()).expect("a project is made");
    files.write_team(&a_team()).expect("and written again");

    let left_behind: Vec<String> = std::fs::read_dir(project.root.join(".farik"))
        .expect("the directory reads")
        .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
        .filter_map(|name| name.to_str().map(std::string::ToString::to_string))
        .filter(|name| name.contains("writing"))
        .collect();
    assert!(left_behind.is_empty(), "{left_behind:?}");

    // And the file is a new one each time rather than one truncated in place, which is what makes
    // a crash mid-write leave the old team readable rather than half of a new one.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let written = project.root.join(".farik/team.yaml");
        let before = std::fs::metadata(&written).expect("the file").ino();
        files.write_team(&a_team()).expect("and once more");
        let after = std::fs::metadata(&written).expect("the file").ino();
        assert_ne!(
            before, after,
            "a rename replaces the file; a write truncates it"
        );
    }
}

#[test]
fn refuses_to_write_a_team_that_could_not_be_read_back() {
    // The round trip is the promise: what write_team accepts, read_team returns. A value built in
    // memory that breaks a rule of the file is refused here rather than becoming a file nothing
    // can open.
    let project = TempProject::new("write-invalid");
    let files = project.files();
    let mut team = a_team();
    team.agents.truncate(1);
    let refused = files.write_team(&team);
    assert!(
        matches!(refused, Err(FilesError::Invalid { .. })),
        "{refused:?}"
    );
    assert!(!project.root.join(".farik/team.yaml").exists());
}
