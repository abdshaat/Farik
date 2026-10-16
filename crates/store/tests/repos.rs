//! A member repository's folder against the file system and git.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use std::path::{Path, PathBuf};

use catervas_core::repos::validate_repos;
use catervas_core::team::{Team, validate_team};
use catervas_store::files::ProjectFiles;
use catervas_store::git::fixtures::{TempRepo, git_in};
use catervas_store::repos::{
    MemberPathRefusal, TaskRepositoryError, member_root, repo_present,
    task_branch_numbers_everywhere, task_repository,
};

/// A home folder of its own, removed when the value is dropped however the test ends.
struct Home {
    path: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let path = std::fs::canonicalize(std::env::temp_dir())
            .expect("the temporary directory")
            .join(format!("catervas-repos-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a home folder");
        Self { path }
    }

    fn repository(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::create_dir_all(&path).expect("a folder");
        git_in(&path, &["init", "-b", "main"]);
        path
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn finds_a_members_root() {
    let home = Home::new("root");
    let app = home.repository("app");
    std::fs::create_dir_all(app.join("src")).expect("a folder inside the repository");
    std::fs::create_dir_all(home.path.join("notes")).expect("a plain folder");
    let team = home.repository("team");
    std::fs::create_dir_all(team.join(".catervas")).expect("a state folder");
    std::fs::write(team.join(".catervas/team.yaml"), "agents: []\n").expect("a team file");
    std::os::unix::fs::symlink(&app, home.path.join("ln")).expect("a link");

    let root = |path: &str| member_root(&home.path, path);
    assert_eq!(root("app"), Ok(app.clone()));
    assert_eq!(root("ln"), Ok(app.clone()));
    assert_eq!(root("nowhere"), Err(MemberPathRefusal::NotThere));
    assert_eq!(root("../"), Err(MemberPathRefusal::OutsideHome));
    assert_eq!(root(""), Err(MemberPathRefusal::HomeItself));
    assert_eq!(root("."), Err(MemberPathRefusal::HomeItself));
    assert_eq!(root("notes"), Err(MemberPathRefusal::NotARepository));
    assert_eq!(
        root("app/src"),
        Err(MemberPathRefusal::NotTheTop { top: app })
    );
    assert_eq!(root("team"), Err(MemberPathRefusal::HasTeam));

    // A home folder kept in git: a plain folder under it is not a repository, since the one it is
    // inside has home itself as its top and no refusal may point at home. A repository inside it
    // still answers with its own top folder.
    git_in(&home.path, &["init", "-b", "main"]);
    assert_eq!(
        member_root(&home.path, "notes"),
        Err(MemberPathRefusal::NotARepository)
    );
    assert_eq!(
        member_root(&home.path, "app/src"),
        Err(MemberPathRefusal::NotTheTop {
            top: home.path.join("app")
        })
    );

    let top = home.path.join("app");
    let rows = [
        (
            MemberPathRefusal::NotThere,
            "not_there",
            "/path",
            "that folder is not there".to_string(),
        ),
        (
            MemberPathRefusal::OutsideHome,
            "outside_home",
            "/path",
            "that folder is outside your home folder".to_string(),
        ),
        (
            MemberPathRefusal::HomeItself,
            "home_itself",
            "/path",
            "your home folder itself cannot be one of the project's repositories".to_string(),
        ),
        (
            MemberPathRefusal::NotARepository,
            "not_a_repository",
            "/path",
            "that folder is not a git repository".to_string(),
        ),
        (
            MemberPathRefusal::NotTheTop { top: top.clone() },
            "not_the_top",
            "/path",
            format!(
                "that folder is inside a git repository; choose {} instead",
                top.display()
            ),
        ),
        (
            MemberPathRefusal::HasTeam,
            "has_team",
            "/path",
            "that repository has a Catervas team of its own".to_string(),
        ),
    ];
    for (refusal, code, pointer, message) in rows {
        assert_eq!(refusal.code(), code);
        assert_eq!(refusal.pointer(), pointer, "{code}");
        assert_eq!(refusal.message(), message, "{code}");
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_whether_a_member_is_present() {
    let home = Home::new("present");
    let app = home.repository("app");
    assert!(repo_present(&app));

    let moved = home.path.join("apps-moved");
    std::fs::rename(&app, &moved).expect("the repository moved away");
    assert!(!repo_present(&app));
    assert!(repo_present(&moved));

    std::fs::create_dir_all(&app).expect("a plain folder at its path");
    assert!(!repo_present(&app));
    assert!(!repo_present(Path::new("/nowhere/at/all")));

    // A plain folder inside another repository is not a repository's top folder.
    let outer = home.repository("outer");
    std::fs::create_dir_all(outer.join("inner")).expect("a plain folder inside it");
    assert!(repo_present(&outer));
    assert!(!repo_present(&outer.join("inner")));
}

/// A team whose policy names `trunk` as the project folder's integration branch.
fn a_team_on_trunk() -> Team {
    let mut wire = catervas_core::team::fixtures::a_team_wire();
    wire["policy"]["integration_branch"] = serde_json::json!("trunk");
    validate_team(&wire).expect("a team")
}

/// A project folder that is a git repository of its own, with `repos.yaml` naming `members`, each
/// `(name, repository, integration branch)`.
fn project_folder(
    name: &str,
    members: &[(&str, &TempRepo, Option<&str>)],
) -> (TempRepo, ProjectFiles) {
    let project = TempRepo::new(name);
    let files = ProjectFiles::open(project.path.clone());
    let repos: Vec<_> = members
        .iter()
        .map(|(name, repo, branch)| {
            serde_json::json!({
                "name": name,
                "path": repo.path.display().to_string(),
                "integration_branch": branch,
            })
        })
        .collect();
    let repos = repos
        .into_iter()
        .map(|mut repo| {
            if repo["integration_branch"].is_null() {
                repo.as_object_mut()
                    .expect("an object")
                    .remove("integration_branch");
            }
            repo
        })
        .collect::<Vec<_>>();
    files
        .write_repos(&validate_repos(&serde_json::json!({ "repos": repos })).expect("repos"))
        .expect("repos.yaml is written");
    (project, files)
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn resolves_a_tasks_repository() {
    let app = TempRepo::new("task-repo-app");
    let infra = TempRepo::new("task-repo-infra");
    infra.git(&["branch", "release"]);
    let (project, files) = project_folder(
        "task-repo-project",
        &[("app", &app, None), ("infra", &infra, Some("release"))],
    );
    let team = a_team_on_trunk();
    let project_git = project.adapter();

    let own = task_repository(&files, &team, &project_git, None).expect("the project's own");
    assert_eq!(own.repo, None);
    assert_eq!(own.git.root(), project.path.as_path());
    assert_eq!(own.integration_branch, "trunk");

    let in_app = task_repository(&files, &team, &project_git, Some("app")).expect("app");
    assert_eq!(in_app.repo.as_deref(), Some("app"));
    assert_eq!(in_app.git.root(), app.path.as_path());
    assert_eq!(in_app.integration_branch, "main");

    let in_infra = task_repository(&files, &team, &project_git, Some("infra")).expect("infra");
    assert_eq!(in_infra.git.root(), infra.path.as_path());
    assert_eq!(in_infra.integration_branch, "release");

    assert!(matches!(
        task_repository(&files, &team, &project_git, Some("web")),
        Err(TaskRepositoryError::UnknownRepo { repo }) if repo == "web"
    ));

    let moved = PathBuf::from(format!("{}-moved", app.path.display()));
    std::fs::rename(&app.path, &moved).expect("the folder is moved");
    let error = task_repository(&files, &team, &project_git, Some("app"))
        .err()
        .expect("a member that is gone");
    std::fs::rename(&moved, &app.path).expect("the folder is moved back");
    assert!(
        matches!(&error, TaskRepositoryError::Missing { repo, path }
            if repo == "app" && path == &app.path),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "app is not at {}: locate it in Settings",
            app.path.display()
        )
    );

    let alone = TempRepo::new("task-repo-alone");
    let error = task_repository(
        &ProjectFiles::open(alone.path.clone()),
        &team,
        &alone.adapter(),
        Some("app"),
    )
    .err()
    .expect("a one-repo project");
    assert!(matches!(&error, TaskRepositoryError::NotAProjectFolder { repo } if repo == "app"));
    assert_eq!(
        error.to_string(),
        "this task names repository app, but this project is one repository"
    );
    let unknown = task_repository(&files, &team, &project_git, Some("web"))
        .err()
        .expect("a name that is not a member");
    assert_eq!(
        unknown.to_string(),
        "this task names repository web, which this project does not have"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn counts_task_branches_in_every_repository() {
    let app = TempRepo::new("branches-app");
    let infra = TempRepo::new("branches-infra");
    app.git(&["branch", "feature/CTV-4"]);
    infra.git(&["branch", "fix/CTV-9"]);
    let (project, files) = project_folder(
        "branches-project",
        &[("app", &app, None), ("infra", &infra, None)],
    );
    project.git(&["branch", "docs/CTV-2"]);
    assert_eq!(
        task_branch_numbers_everywhere(&files).into_iter().max(),
        Some(9)
    );

    let moved = PathBuf::from(format!("{}-moved", infra.path.display()));
    std::fs::rename(&infra.path, &moved).expect("the folder is moved");
    let highest = task_branch_numbers_everywhere(&files).into_iter().max();
    std::fs::rename(&moved, &infra.path).expect("the folder is moved back");
    assert_eq!(highest, Some(4));
}
