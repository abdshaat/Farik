//! The project scan against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`, as `git.rs` is and for the same reasons.

use chrono::{DateTime, Utc};
use farik_store::git::fixtures::{TempRepo, git_in};
use farik_store::{Git, ScanError, scan_project};

/// A moment to scan at, so that "last commit today" is an answer rather than a guess.
fn now() -> DateTime<Utc> {
    Utc::now()
}

fn read_back(repository: &TempRepo) -> String {
    scan_project(&repository.adapter(), now())
        .expect("it scans")
        .read_back
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_back_a_typescript_monorepo_the_way_section_4_does() {
    let repository = TempRepo::new("scan-monorepo");
    repository.write("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
    repository.write("pnpm-workspace.yaml", "packages:\n  - packages/*\n");
    repository.write(
        "package.json",
        r#"{"name":"root","devDependencies":{"vitest":"^3.0.0"},
            "scripts":{"test":"vitest run","build":"tsc -b","lint":"eslint ."}}"#,
    );
    repository.write("packages/ui/package.json", "{\"name\":\"ui\"}\n");
    repository.write("packages/ui/index.ts", "export const one = 1;\n");
    repository.write("packages/web/package.json", "{\"name\":\"web\"}\n");
    repository.write("packages/web/app.tsx", "export const App = () => null;\n");
    repository.write("apps/desktop/package.json", "{\"name\":\"desktop\"}\n");
    repository.write("apps/desktop/main.ts", "export const main = 1;\n");
    repository.commit("a monorepo");

    assert_eq!(
        read_back(&repository),
        "TypeScript monorepo, pnpm, 3 packages, tests in vitest, last commit today"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_the_project_own_scripts_rather_than_guessing_them() {
    let repository = TempRepo::new("scan-scripts");
    repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
    repository.write(
        "package.json",
        r#"{"name":"app","scripts":{"test":"node --test","typecheck":"tsc --noEmit",
            "release":"./ship.sh"}}"#,
    );
    repository.write("src/index.js", "module.exports = 1;\n");
    repository.commit("an application");

    let scan = scan_project(&repository.adapter(), now()).expect("it scans");
    assert_eq!(
        scan.detected_criteria
            .iter()
            .map(|one| (one.name.as_str(), command_of(&one.verification)))
            .collect::<Vec<_>>(),
        [
            ("the-tests-pass", "npm run test".to_string()),
            ("the-types-check", "npm run typecheck".to_string()),
        ],
        "a script the project has becomes a criterion; one it does not have does not, and neither \
         does one Farik has no name for"
    );
    assert_eq!(scan.read_back, "JavaScript, npm, last commit today");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_back_a_rust_workspace_and_the_commands_cargo_always_has() {
    let repository = TempRepo::new("scan-rust");
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    repository.write("crates/core/Cargo.toml", "[package]\nname = \"core\"\n");
    repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repository.write("crates/store/Cargo.toml", "[package]\nname = \"store\"\n");
    repository.write("crates/store/src/lib.rs", "pub fn two() -> u8 { 2 }\n");
    repository.commit("a workspace");

    let scan = scan_project(&repository.adapter(), now()).expect("it scans");
    assert_eq!(
        scan.read_back,
        "Rust monorepo, cargo, 2 packages, last commit today"
    );
    assert_eq!(
        scan.detected_criteria
            .iter()
            .map(|one| one.name.as_str())
            .collect::<Vec<_>>(),
        [
            "the-tests-pass",
            "the-build-succeeds",
            "clippy-is-clean",
            "formatting-is-clean"
        ],
        "F2 asks for the test and the build commands, and cargo has both"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_nothing_out_of_what_git_ignores() {
    // The scan reads the tree through git, so a vendored dependency directory is not the project.
    let repository = TempRepo::new("scan-ignored");
    repository.write(".gitignore", "node_modules/\n");
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
    repository.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    for index in 0..20 {
        repository.write(
            &format!("node_modules/dep{index}/index.ts"),
            "export const x = 1;\n",
        );
    }
    repository.commit("a rust project with a node_modules nobody asked for");

    assert_eq!(
        read_back(&repository),
        "Rust, cargo, last commit today",
        "twenty TypeScript files git ignores do not make this a TypeScript project"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_back_a_repository_with_nothing_in_it() {
    let repository = TempRepo::new("scan-empty");
    repository.git(&["rm", "--cached", "-q", "README.md"]);
    repository.git(&["update-ref", "-d", "HEAD"]);
    assert_eq!(
        read_back(&repository),
        "nothing tracked yet, no commits yet"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_back_a_repository_it_recognises_nothing_in() {
    let repository = TempRepo::new("scan-unknown");
    repository.write("notes.txt", "a folder of notes\n");
    repository.commit("notes");
    assert_eq!(read_back(&repository), "last commit today");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_directory_that_is_not_a_repository() {
    let plain = std::env::temp_dir().join(format!("farik-scan-plain-{}", std::process::id()));
    std::fs::create_dir_all(&plain).expect("a plain directory");
    let refused = scan_project(&Git::open(plain.clone()), now());
    let _ = std::fs::remove_dir_all(&plain);
    assert_eq!(
        refused,
        Err(ScanError::NotARepository {
            path: plain.display().to_string()
        }),
        "a project is a git repository plus .farik/, and this is neither"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_tree_that_is_missing_a_file_it_tracks() {
    // The other half of the decision below: a manifest that is not JSON is a project mid-edit, and a
    // manifest git tracks that is not there at all is a tree half-checked-out. The second is a fact
    // about the checkout rather than about the project, so the scan refuses instead of guessing.
    let repository = TempRepo::new("scan-missing-manifest");
    repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
    repository.write(
        "package.json",
        "{\"name\":\"app\",\"scripts\":{\"test\":\"vitest\"}}\n",
    );
    repository.write("src/index.ts", "export const one = 1;\n");
    repository.commit("a project");
    std::fs::remove_file(repository.path.join("package.json"))
        .expect("and a half-checked-out tree");

    let Err(ScanError::Io { path, detail }) = scan_project(&repository.adapter(), now()) else {
        panic!("a file git tracks that is not there is a tree half-written");
    };
    assert_eq!(path, "package.json");
    assert!(detail.contains("No such file or directory"), "{detail}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_what_git_said_about_a_repository_with_no_working_tree() {
    // A bare repository is a repository, so `is_repository` says yes, and it has nothing to scan.
    // Saying that better needs a refusal of its own, which step 08's `farik doctor` row records; what
    // this step promises is that git's own sentence reaches the person rather than being swallowed.
    let bare = std::env::temp_dir().join(format!("farik-scan-bare-{}.git", std::process::id()));
    let _ = std::fs::remove_dir_all(&bare);
    std::fs::create_dir_all(&bare).expect("a directory");
    git_in(&bare, &["init", "--bare", "-q"]);
    let refused = scan_project(&Git::open(bare.clone()), now());
    let _ = std::fs::remove_dir_all(&bare);

    let Err(ScanError::Git { detail }) = refused else {
        panic!("there is no working tree to scan: {refused:?}");
    };
    assert!(detail.contains("must be run in a work tree"), "{detail}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn a_manifest_that_is_not_json_is_a_fact_about_the_project() {
    // Half a `package.json` is something to say in the read-back, not a reason to refuse the scan.
    let repository = TempRepo::new("scan-broken-manifest");
    repository.write("package-lock.json", "{\"lockfileVersion\":3}\n");
    repository.write("package.json", "{\"name\": \"half a manifest\"\n");
    repository.write("src/index.ts", "export const one = 1;\n");
    repository.commit("a manifest a person was editing");

    let scan = scan_project(&repository.adapter(), now()).expect("it still scans");
    assert_eq!(scan.read_back, "TypeScript, npm, last commit today");
    assert!(
        scan.detected_criteria.is_empty(),
        "no scripts could be read, so no criterion was found"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn takes_the_toolchain_the_language_names_when_a_project_has_two() {
    // A Rust workspace with a front end, which is the shape this repository itself takes. One
    // toolchain is chosen, because a criterion called `the-tests-pass` can only mean one command,
    // and the tree says this is mostly Rust.
    let repository = TempRepo::new("scan-polyglot");
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    repository.write("crates/core/Cargo.toml", "[package]\nname = \"core\"\n");
    repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repository.write("crates/store/Cargo.toml", "[package]\nname = \"store\"\n");
    repository.write("crates/store/src/lib.rs", "pub fn two() -> u8 { 2 }\n");
    repository.write("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
    repository.write("package.json", "{\"scripts\":{\"build\":\"tsc -b\"}}\n");
    repository.write("web/app.ts", "export const one = 1;\n");
    repository.commit("a rust workspace with a front end");

    let scan = scan_project(&repository.adapter(), now()).expect("it scans");
    assert_eq!(
        scan.read_back,
        "Rust monorepo, cargo, 2 packages, last commit today"
    );
    assert_eq!(
        scan.detected_criteria
            .iter()
            .map(|one| one.name.as_str())
            .collect::<Vec<_>>(),
        [
            "the-tests-pass",
            "the-build-succeeds",
            "clippy-is-clean",
            "formatting-is-clean"
        ],
        "the library gets the commands of the language the project is, not of the one it also has"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_directory_inside_a_repository_rather_than_scanning_a_subtree() {
    // Onboarding asks a person to pick the project's folder (spec 4). A subtree would answer
    // confidently about a project it had only seen part of.
    let repository = TempRepo::new("scan-subtree");
    repository.write("Cargo.lock", "version = 4\n");
    repository.write("Cargo.toml", "[package]\nname = \"one\"\n");
    repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repository.commit("a project with a subdirectory");

    let inside = Git::open(repository.path.join("crates/core"));
    assert!(inside.is_repository(), "it is inside a repository");
    let Err(ScanError::NotTheRepositoryRoot { path, root }) = scan_project(&inside, now()) else {
        panic!("a subtree is not a project");
    };
    assert!(path.ends_with("crates/core"), "{path}");
    assert!(!root.ends_with("crates/core"), "{root}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_list_what_a_directory_that_is_not_a_repository_tracks() {
    // `tracked_paths` promises this, and `scan_project` asks `is_repository` before it, so nothing
    // else would notice if the adapter stopped checking.
    let plain = std::env::temp_dir().join(format!("farik-tracked-plain-{}", std::process::id()));
    std::fs::create_dir_all(&plain).expect("a plain directory");
    let refused = Git::open(plain.clone()).tracked_paths();
    let _ = std::fs::remove_dir_all(&plain);
    assert_eq!(refused, Err(farik_store::GitError::NotARepository));
}

/// The command a template's verification runs, whichever method it is.
fn command_of(verification: &farik_core::criteria::TemplateVerification) -> String {
    match verification {
        farik_core::criteria::TemplateVerification::Variant0 { command, .. }
        | farik_core::criteria::TemplateVerification::Variant1 { command, .. } => command.clone(),
        other => panic!("a criterion the scan found runs a command: {other:?}"),
    }
}
