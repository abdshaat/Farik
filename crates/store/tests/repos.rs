//! A member repository's folder against the file system and git.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use std::path::{Path, PathBuf};

use catervas_store::git::fixtures::git_in;
use catervas_store::repos::{MemberPathRefusal, member_root, repo_present};

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
}
