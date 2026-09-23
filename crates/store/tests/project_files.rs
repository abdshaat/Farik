//! The files under `.farik/`, against a real directory.
//!
//! Every test here needs somewhere to write and nothing else, so they run in the default
//! `cargo xtask check` as `event_log_file.rs` does, rather than behind `--integration`.

use farik_core::contract::{TaskId, validate_contract};
use farik_core::criteria::{fixtures::a_criteria_library_wire, validate_criteria};
use farik_core::team::AgentId;
use farik_store::files::fixtures::{TempProject, a_team};
use farik_store::files::{FilesError, LocalSettings, Sandbox};

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

/// The contract `farik-core`'s own fixture describes, with the id a test asks for.
fn a_contract(id: &str) -> farik_core::contract::TaskContract {
    let mut wire = farik_core::contract::fixtures::a_contract_wire();
    wire["id"] = serde_json::json!(id);
    validate_contract(&wire).expect("the fixture is a contract")
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
fn a_second_init_keeps_what_a_person_added_to_the_ignore_file() {
    // `write_if_absent` is what makes init safe to run twice, and the ignore file is the one thing
    // init writes that is not the team.
    let project = TempProject::new("init-gitignore");
    let files = project.files();
    files.init(&a_team()).expect("a project is made");
    let ignore = project.root.join(".farik/local/.gitignore");
    std::fs::write(&ignore, "*\n!notes.md\n").expect("a person keeps one file");
    files.init(&a_team()).expect("and runs init again");
    assert_eq!(
        std::fs::read_to_string(&ignore).expect("it is still there"),
        "*\n!notes.md\n"
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
fn reads_back_the_criterion_library_it_wrote() {
    let project = TempProject::new("round-trip-criteria");
    let files = project.files();
    let library = validate_criteria(&a_criteria_library_wire()).expect("the fixture is a library");
    files
        .write_criteria(&library)
        .expect("the library is written");

    assert_eq!(files.read_criteria().expect("the library"), library);
    assert!(
        project.root.join(".farik/team/criteria.yaml").is_file(),
        "beside the team's other files, which is where 5.13 puts it"
    );
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
fn refuses_a_library_and_a_contract_a_person_broke() {
    // The same promise the team has: a file read back is held to what the same thing is held to
    // arriving on the wire, and every writer runs that validator before it writes.
    let project = TempProject::new("broken-others");
    let files = project.files();
    let library = validate_criteria(&a_criteria_library_wire()).expect("the fixture is a library");
    files.write_criteria(&library).expect("written");
    std::fs::write(
        project.root.join(".farik/team/criteria.yaml"),
        "criteria:\n  - name: one\n    text: A criterion of ten characters.\n    verification:\n      method: vibes\n",
    )
    .expect("a person edits it");
    let Err(FilesError::Invalid { path, detail }) = files.read_criteria() else {
        panic!("vibes is not a verification method");
    };
    assert_eq!(path, ".farik/team/criteria.yaml");
    assert!(detail.contains("/criteria/0/verification"), "{detail}");

    files.write_contract(&a_contract("FRK-1")).expect("written");
    std::fs::write(
        project.root.join(".farik/contracts/FRK-1.yaml"),
        "id: FRK-1\ntitle: Too little to be a contract\n",
    )
    .expect("a person edits it too");
    let id = TaskId::try_from("FRK-1").expect("an id");
    assert!(
        matches!(files.read_contract(&id), Err(FilesError::Invalid { .. })),
        "half a contract is not one"
    );
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
    files.init(&a_team()).expect("a project is made");
    let text = std::fs::read_to_string(project.root.join(".farik/team.yaml")).expect("the file");
    std::fs::write(
        project.root.join(".farik/team.yaml"),
        format!("\u{feff}{text}"),
    )
    .expect("saved again with a mark at the front");
    assert_eq!(files.read_team().expect("it still reads"), a_team());

    // And the JSON files, which is where it matters: serde_json refuses a mark at column 1 and
    // says only that it expected a value, while the YAML parser tolerates one.
    std::fs::write(
        project.root.join(".farik/prices.json"),
        format!("\u{feff}{}", farik_core::pricing::prices::PRICES_JSON),
    )
    .expect("an override a windows editor saved");
    assert!(
        files.read_prices().expect("it still reads").is_some(),
        "a mark is not content"
    );
    std::fs::write(
        project.root.join(".farik/local/settings.json"),
        "\u{feff}{\"sandbox\": \"none\"}\n",
    )
    .expect("settings a windows editor saved");
    assert_eq!(
        files.read_settings().expect("they still read"),
        LocalSettings {
            sandbox: Sandbox::None,
        }
    );
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
fn two_writers_on_one_project_never_publish_a_file_holding_both() {
    // The file beside the one being written is named after the target, so every writer on one root
    // -- in this process or another -- wrote the same one. The rename is atomic; what was in the
    // shared file was not, so a reader could see a file holding bytes from both writes, or none.
    let project = TempProject::new("two-writers");
    project.files().init(&a_team()).expect("a project is made");
    let first = "a".repeat(64 * 1024);
    let second = "b".repeat(48 * 1024);
    // One of them is published before the reader starts, so that "there is no file yet" is not a
    // thing this test can see. Every read after this is of a file some writer has published, which
    // is what the assertion below is about.
    project
        .files()
        .write_project_scan(&first)
        .expect("one of them is there before anything reads");
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writers: Vec<_> = [&first, &second]
        .into_iter()
        .map(|text| {
            let files = project.files();
            let text = text.clone();
            std::thread::spawn(move || {
                (0..200)
                    .filter(|_| files.write_project_scan(&text).is_err())
                    .count()
            })
        })
        .collect();
    let reader = {
        let files = project.files();
        let (first, second) = (first.clone(), second.clone());
        let done = std::sync::Arc::clone(&done);
        std::thread::spawn(move || {
            let mut mixed = 0_usize;
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                match files.read_project_scan() {
                    Ok(text) if text == first || text == second => {}
                    Ok(_) | Err(_) => mixed += 1,
                }
            }
            mixed
        })
    };
    let refused: usize = writers
        .into_iter()
        .map(|writer| writer.join().expect("a writer finished"))
        .sum();
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let mixed = reader.join().expect("the reader finished");
    assert_eq!(
        (refused, mixed),
        (0, 0),
        "every write lands and every read is one whole write or the other"
    );
}

#[test]
fn a_document_is_not_destroyed_by_another_ones_writing() {
    // The two names are both a tool call's to choose, and the one being written used the other as
    // the file it writes beside -- so writing `notes.md` renamed `notes.md.writing` away.
    let project = TempProject::new("writing-collision");
    let files = project.files();
    files
        .write_product_doc("notes.md.writing", "mine\n")
        .expect("a document a person named oddly");
    files
        .write_product_doc("notes.md", "another\n")
        .expect("and another beside it");
    assert_eq!(
        files.read_product_doc("notes.md.writing").as_deref(),
        Ok("mine\n"),
        "the first document is still there"
    );
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

#[test]
fn refuses_to_write_a_library_or_a_contract_that_could_not_be_read_back() {
    let project = TempProject::new("write-invalid-others");
    let files = project.files();
    let mut library = validate_criteria(&a_criteria_library_wire()).expect("a library");
    library.criteria[1].name = library.criteria[0].name.clone();
    assert!(
        matches!(
            files.write_criteria(&library),
            Err(FilesError::Invalid { .. })
        ),
        "a name names one criterion"
    );
    assert!(!project.root.join(".farik/team/criteria.yaml").exists());

    let mut contract = a_contract("FRK-1");
    contract.exit_criteria.clear();
    assert!(
        matches!(
            files.write_contract(&contract),
            Err(FilesError::Invalid { .. })
        ),
        "a contract with no exit criteria is not one"
    );
    assert!(!project.root.join(".farik/contracts/FRK-1.yaml").exists());
}

#[test]
fn writes_a_contract_to_the_file_its_own_id_names() {
    let project = TempProject::new("contracts");
    let files = project.files();
    let contract = a_contract("FRK-7");
    files.write_contract(&contract).expect("it is written");

    assert!(project.root.join(".farik/contracts/FRK-7.yaml").is_file());
    let id = TaskId::try_from("FRK-7").expect("an id");
    assert_eq!(files.read_contract(&id).expect("it reads back"), contract);
}

#[test]
fn creates_a_contract_only_where_there_is_none() {
    // An update overwrites the file it names; a new contract must not, because the file already
    // there is somebody's committed work and nothing else holds a copy of it.
    let project = TempProject::new("contract-create");
    let files = project.files();
    let contract = a_contract("FRK-7");
    files.create_contract(&contract).expect("it is created");
    let mut other = a_contract("FRK-7");
    other.title = "Another one".parse().expect("a title");

    assert!(
        matches!(
            files.create_contract(&other),
            Err(FilesError::Invalid { ref path, .. }) if path == ".farik/contracts/FRK-7.yaml"
        ),
        "a second create of one id is refused"
    );
    let id = TaskId::try_from("FRK-7").expect("an id");
    assert_eq!(
        files.read_contract(&id).expect("it reads back"),
        contract,
        "and the first is untouched"
    );
}

#[test]
fn refuses_a_contract_that_says_it_is_another() {
    // The file name and the id inside it are two claims about the same thing, and a board that
    // believed the file name would show a task that does not exist.
    let project = TempProject::new("contract-id");
    let files = project.files();
    files.write_contract(&a_contract("FRK-7")).expect("written");
    std::fs::rename(
        project.root.join(".farik/contracts/FRK-7.yaml"),
        project.root.join(".farik/contracts/FRK-8.yaml"),
    )
    .expect("a person moves it");

    let id = TaskId::try_from("FRK-8").expect("an id");
    let Err(FilesError::Invalid { path, detail }) = files.read_contract(&id) else {
        panic!("the contract inside says FRK-7");
    };
    assert_eq!(path, ".farik/contracts/FRK-8.yaml");
    assert!(detail.contains("says it is FRK-7"), "{detail}");
}

#[test]
fn refuses_a_contract_and_a_library_that_break_a_rule_only_the_schema_knows() {
    // Structurally these are what serde builds happily. The rules they break are the schema's, and
    // "held to exactly the rules it would be held to arriving on the wire" is the whole promise: a
    // contract with nothing to meet is not a contract, whatever serde makes of it.
    let project = TempProject::new("schema-only");
    let files = project.files();
    files.init(&a_team()).expect("a project is made");
    let mut wire = farik_core::contract::fixtures::a_contract_wire();
    wire["id"] = serde_json::json!("FRK-1");
    wire["exit_criteria"] = serde_json::json!([]);
    std::fs::write(
        project.root.join(".farik/contracts/FRK-1.yaml"),
        serde_json::to_string(&wire).expect("a value writes as JSON, which is YAML"),
    )
    .expect("a contract with nothing to meet");
    let Err(FilesError::Invalid { path, detail }) =
        files.read_contract(&TaskId::try_from("FRK-1").expect("an id"))
    else {
        panic!("a contract with no exit criteria is not one");
    };
    assert_eq!(path, ".farik/contracts/FRK-1.yaml");
    assert!(detail.contains("/exit_criteria"), "{detail}");
}

#[test]
fn lists_contracts_in_the_order_a_board_shows_them() {
    let project = TempProject::new("list");
    let files = project.files();
    for id in ["FRK-10", "FRK-2", "FRK-1"] {
        files.write_contract(&a_contract(id)).expect("written");
    }
    // A person's own notes in the same directory are not contracts and are not a problem either.
    std::fs::write(project.root.join(".farik/contracts/notes.md"), "mine\n").expect("a note");
    std::fs::write(project.root.join(".farik/contracts/FRK-3.txt"), "?\n").expect("not a contract");
    // Nor is a directory a person named that way: a board that listed it would show a task that
    // cannot be read.
    std::fs::create_dir_all(project.root.join(".farik/contracts/FRK-9.yaml"))
        .expect("notes kept in a directory");

    assert_eq!(
        files
            .list_contracts()
            .expect("they list")
            .iter()
            .map(|id| id.as_str().to_string())
            .collect::<Vec<_>>(),
        ["FRK-1", "FRK-2", "FRK-10"],
        "by the number in the id, so the tenth does not come before the ninth"
    );
}

#[test]
fn a_project_with_nothing_in_it_has_no_contracts() {
    assert!(
        TempProject::new("list-empty")
            .files()
            .list_contracts()
            .expect("an empty list, not a refusal")
            .is_empty()
    );
}

#[test]
fn an_agent_that_never_wrote_a_notebook_has_an_empty_one() {
    // Every session for an agent includes its notebook (5.8). A missing file is not something to
    // tell an agent about; it is an agent that has not written anything down yet.
    let project = TempProject::new("memory");
    let files = project.files();
    let ada = AgentId::try_from("ada").expect("an id");
    assert_eq!(files.read_memory(&ada).expect("an empty notebook"), "");

    files
        .write_memory(&ada, "The login form is in src/login.\n")
        .expect("it is written");
    assert_eq!(
        files.read_memory(&ada).expect("it reads back"),
        "The login form is in src/login.\n"
    );
}

#[test]
fn reads_back_the_project_scan_and_says_when_there_is_none() {
    let project = TempProject::new("scan");
    let files = project.files();
    assert_eq!(
        files.read_project_scan(),
        Err(FilesError::NotFound {
            path: ".farik/project.md".to_string(),
        })
    );
    files
        .write_project_scan("# Farik\n\nA Rust workspace.\n")
        .expect("it is written");
    assert!(
        files
            .read_project_scan()
            .expect("it reads back")
            .contains("A Rust workspace")
    );
}

#[test]
fn writes_a_product_document_and_refuses_one_that_climbs_out() {
    let project = TempProject::new("product");
    let files = project.files();
    files
        .write_product_doc("areas/login.md", "# Login\n")
        .expect("it is written");
    assert!(project.root.join(".farik/product/areas/login.md").is_file());
    assert_eq!(
        files
            .read_product_doc("areas/login.md")
            .expect("it reads back"),
        "# Login\n"
    );

    let refused = files.write_product_doc("../team.yaml", "not here\n");
    assert!(
        matches!(refused, Err(FilesError::Invalid { .. })),
        "{refused:?}"
    );
    assert!(
        !project.root.join(".farik/team.yaml").exists(),
        "and nothing was written where it pointed"
    );
}

#[test]
fn refuses_a_product_document_that_leaves_product_through_a_link() {
    // The string rule cannot see this one: there is no `..` in `out/loot.md`. A directory under
    // product/ may be a link pointing anywhere, and the path comes from a tool call.
    let project = TempProject::new("product-link");
    let files = project.files();
    files
        .write_product_doc("kept.md", "# Kept\n")
        .expect("a document is written, which makes product/");

    #[cfg(unix)]
    {
        let outside = project.root.join("secret");
        std::fs::create_dir_all(&outside).expect("somewhere outside .farik/");
        std::os::unix::fs::symlink(&outside, project.root.join(".farik/product/out"))
            .expect("a link out of product/");

        let refused = files.write_product_doc("out/loot.md", "taken\n");
        let Err(FilesError::Invalid { path, detail }) = refused else {
            panic!("it leads out of product/: {refused:?}");
        };
        assert_eq!(path, ".farik/product/out/loot.md");
        assert!(detail.contains("leads out of product/"), "{detail}");
        assert!(
            !outside.join("loot.md").exists(),
            "and nothing was written where it pointed"
        );
        assert!(
            files.read_product_doc("out/loot.md").is_err(),
            "and reading through it is refused too"
        );
    }
}

#[test]
fn refuses_a_product_document_that_leaves_product_for_a_name_beginning_the_same() {
    // `Path::starts_with` is component-wise, and has to be: a textual prefix would let any sibling
    // of product/ whose name begins with `product` be written to through a link inside it.
    let project = TempProject::new("product-sibling");
    let files = project.files();
    files
        .write_product_doc("kept.md", "# Kept\n")
        .expect("a document is written, which makes product/");

    #[cfg(unix)]
    {
        let sibling = project.root.join(".farik/product-secrets");
        std::fs::create_dir_all(&sibling).expect("a sibling sharing the prefix");
        std::os::unix::fs::symlink(&sibling, project.root.join(".farik/product/out"))
            .expect("a link to it from inside product/");
        let refused = files.write_product_doc("out/loot.md", "taken\n");
        assert!(
            matches!(refused, Err(FilesError::Invalid { .. })),
            "product-secrets/ is not product/: {refused:?}"
        );
        assert!(!sibling.join("loot.md").exists(), "and nothing was written");
    }
}

#[test]
fn refuses_a_product_document_when_it_cannot_tell_where_product_is() {
    // The boundary fails closed. A root that cannot be resolved means the answer to "is this
    // inside product/" is unknown, and an unknown boundary is not a boundary.
    let files = farik_store::files::ProjectFiles::open(std::path::PathBuf::new());
    let Err(FilesError::Invalid { path, detail }) = files.read_product_doc("roadmap.md") else {
        panic!("an unresolvable root is not a missing document");
    };
    assert_eq!(path, ".farik/product/roadmap.md");
    assert!(detail.contains("project root"), "{detail}");
}

#[test]
fn reading_a_product_document_makes_nothing() {
    // `.farik/` existing is what makes a directory a Farik project (spec 3). A read that made it,
    // to answer where a path lands, would make a project of whatever it was pointed at.
    let project = TempProject::new("product-read-makes-nothing");
    let files = project.files();
    assert!(matches!(
        files.read_product_doc("roadmap.md"),
        Err(FilesError::NotFound { .. })
    ));
    assert!(
        !project.root.join(".farik").exists(),
        "a read is not an init"
    );
    assert!(matches!(
        files.read_product_doc("../team.yaml"),
        Err(FilesError::Invalid { .. })
    ));
    assert!(!project.root.join(".farik").exists(), "nor is a refusal");
}

#[test]
fn reads_no_prices_when_a_project_does_not_override_them() {
    let project = TempProject::new("prices");
    let files = project.files();
    files.init(&a_team()).expect("a project is made");
    assert_eq!(
        files.read_prices().expect("no override is not a refusal"),
        None
    );

    let shipped = farik_core::pricing::prices::PRICES_JSON;
    std::fs::write(project.root.join(".farik/prices.json"), shipped).expect("an override");
    assert!(
        files
            .read_prices()
            .expect("it reads back")
            .is_some_and(|table| table.version.get() == 1)
    );

    std::fs::write(
        project.root.join(".farik/prices.json"),
        "{\"version\": 1}\n",
    )
    .expect("broken");
    let Err(FilesError::Invalid { path, .. }) = files.read_prices() else {
        panic!("half a price table is not one");
    };
    assert_eq!(path, ".farik/prices.json");

    // And a whole table of a format this program does not read, which serde alone would accept:
    // pricing a session against a later format would be guesswork.
    std::fs::write(
        project.root.join(".farik/prices.json"),
        shipped.replace("\"version\": 1", "\"version\": 2"),
    )
    .expect("a table from a later Farik");
    let Err(FilesError::Invalid { path, detail }) = files.read_prices() else {
        panic!("a version this program does not know is not a table it can use");
    };
    assert_eq!(path, ".farik/prices.json");
    assert!(detail.contains("/version"), "{detail}");
}

#[test]
fn a_machine_that_was_never_asked_runs_the_sandbox_the_spec_asks_for() {
    let project = TempProject::new("settings");
    let files = project.files();
    assert_eq!(
        files.read_settings().expect("the defaults"),
        LocalSettings {
            sandbox: Sandbox::Docker,
        }
    );

    files
        .write_settings(&LocalSettings {
            sandbox: Sandbox::None,
        })
        .expect("it is written");
    assert_eq!(
        files.read_settings().expect("it reads back").sandbox,
        Sandbox::None
    );
    assert_eq!(
        std::fs::read_to_string(project.root.join(".farik/local/settings.json")).expect("the file"),
        "{\n  \"sandbox\": \"none\"\n}\n",
        "snake_case on the wire, and a file a person can read"
    );
}

#[test]
fn reads_one_yaml_dialect_whatever_directory_the_file_came_from() {
    // `farik task create` is handed a contract from anywhere in the repository, and it goes through
    // the same reader as the files under `.farik/`: one duplicate key is one refusal, wherever the
    // file sits (ADR 0007).
    let refused = farik_store::files::yaml_value("name: a\nname: b\n", "request.yaml")
        .expect_err("a duplicate mapping key is not a document Farik reads");
    let text = refused.to_string();
    assert!(
        text.contains("request.yaml") && text.contains("name"),
        "the refusal names the file it is about and the key that is repeated: {text}"
    );
    assert_eq!(
        farik_store::files::yaml_value("on: true\n", "request.yaml"),
        Ok(serde_json::json!({ "on": true })),
        "and `on` is a key rather than a boolean, which is the same dialect the store reads"
    );
}

#[test]
fn lists_two_spellings_of_one_number_in_an_order_that_is_not_the_filesystem_s() {
    // The schema allows a leading zero, so `FRK-01` and `FRK-1` are two spellings of one number. The
    // number alone is not a key: without the id behind it the order falls to `read_dir`, which is
    // stable on one filesystem and not across a fresh clone or a restore. `projections` broke this
    // tie in step 03 and `reconcile` in step 07.
    let project = TempProject::new("list-two-spellings");
    let files = project.files();
    for id in [
        "FRK-1",
        "FRK-01",
        "FRK-001",
        "FRK-0001",
        "FRK-00001",
        "FRK-000001",
    ] {
        let mut contract = farik_core::contract::fixtures::a_contract_wire();
        contract["id"] = serde_json::json!(id);
        files
            .write_contract(&validate_contract(&contract).expect("a contract"))
            .expect("written");
    }

    assert_eq!(
        files
            .list_contracts()
            .expect("a list")
            .iter()
            .map(|id| id.as_str().to_string())
            .collect::<Vec<_>>(),
        [
            "FRK-000001",
            "FRK-00001",
            "FRK-0001",
            "FRK-001",
            "FRK-01",
            "FRK-1"
        ],
        "by the number, then by the id itself"
    );
}
