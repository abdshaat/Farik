//! The team's documents kept on this computer, against a real repository (ADR 0074). Every test
//! needs the `git` program, so every one is `#[ignore]`d and run by `cargo xtask check
//! --integration`.

use std::os::unix::fs::symlink;
use std::path::Path;

use catervas_core::contract::TaskId;
use catervas_store::document_source::DocumentSource;
use catervas_store::git::fixtures::{TempRepo, git_in, git_output_in};
use catervas_store::local_documents::{
    copy_in, document_changes, exclude_documents, include_documents, land, restore_taken_out,
};

fn task() -> TaskId {
    "CTV-3".parse().expect("a task id")
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn a_task_copy_lands_what_changed_and_sets_aside_what_moved() {
    let repository = TempRepo::new("local-documents-copy");
    let git = repository.adapter();
    let root = repository.path.clone();
    exclude_documents(&git).expect("the folder is ignored");
    repository.write("docs/catervas/product/spec.html", "spec v1");
    repository.write("docs/catervas/product/roadmap.html", "roadmap v1");
    symlink("spec.html", root.join("docs/catervas/product/evil.html")).expect("a link");

    // The copy in holds the two files and not the link, and keeps the start copy once.
    let worktree = root.join(".catervas/local/worktrees/CTV-3");
    git.create_detached_worktree(&worktree, "main")
        .expect("a worktree");
    copy_in(&git, &root, &worktree, Some(&task())).expect("copied in");
    let there = worktree.join("docs/catervas/product");
    assert_eq!(read(&there.join("spec.html")).as_deref(), Some("spec v1"));
    assert_eq!(
        read(&there.join("roadmap.html")).as_deref(),
        Some("roadmap v1")
    );
    assert!(!there.join("evil.html").exists());
    let start = root.join(".catervas/local/documents/start/CTV-3/product");
    assert_eq!(read(&start.join("spec.html")).as_deref(), Some("spec v1"));
    assert!(!start.join("evil.html").exists());
    // A task sent back copies in again and keeps its first start copy.
    std::fs::write(root.join("docs/catervas/product/spec.html"), "spec v1b").expect("owner");
    copy_in(&git, &root, &worktree, Some(&task())).expect("copied in again");
    assert_eq!(read(&start.join("spec.html")).as_deref(), Some("spec v1"));
    std::fs::write(root.join("docs/catervas/product/spec.html"), "spec v1").expect("owner");

    // The disk is read as a tree: the two files, hashed as git would, the link left out.
    let listed = DocumentSource::Disk(root.clone())
        .entries(&git, "docs/catervas")
        .expect("listed");
    let paths: Vec<&str> = listed.iter().map(|entry| entry.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "docs/catervas/product/roadmap.html",
            "docs/catervas/product/spec.html"
        ]
    );
    let hashed = git_output_in(&root, &["hash-object", "docs/catervas/product/spec.html"]);
    assert_eq!(listed[1].blob, hashed.trim());
    assert_eq!(listed[1].mode, "100644");

    // The task changes both documents and adds one beside a link; the owner changes the roadmap
    // meanwhile.
    std::fs::write(there.join("spec.html"), "spec v2").expect("the task");
    std::fs::write(there.join("roadmap.html"), "roadmap v2").expect("the task");
    std::fs::create_dir_all(worktree.join("docs/catervas/design")).expect("a folder");
    std::fs::write(worktree.join("docs/catervas/design/new.html"), "design v1").expect("the task");
    let outside = repository.path.join("outside");
    std::fs::create_dir_all(&outside).expect("somewhere else");
    std::fs::remove_dir_all(root.join("docs/catervas/design")).ok();
    symlink(&outside, root.join("docs/catervas/design")).expect("a link to elsewhere");
    std::fs::write(
        root.join("docs/catervas/product/roadmap.html"),
        "roadmap owner",
    )
    .expect("the owner");
    assert_eq!(
        document_changes(&root, &worktree, &task()).expect("changes"),
        [
            "docs/catervas/design/new.html",
            "docs/catervas/product/roadmap.html",
            "docs/catervas/product/spec.html"
        ]
    );

    let landed = land(&root, &worktree, &task()).expect("landed");
    assert_eq!(landed.written, ["docs/catervas/product/spec.html"]);
    assert!(landed.removed.is_empty());
    assert_eq!(
        landed.set_aside,
        [
            "docs/catervas/design/new.html",
            "docs/catervas/product/roadmap.html"
        ]
    );
    let root_product = root.join("docs/catervas/product");
    assert_eq!(
        read(&root_product.join("spec.html")).as_deref(),
        Some("spec v2")
    );
    assert_eq!(
        read(&root_product.join("roadmap.html")).as_deref(),
        Some("roadmap owner")
    );
    let aside = root.join(".catervas/local/documents/set-aside/CTV-3");
    assert_eq!(
        read(&aside.join("product/roadmap.html")).as_deref(),
        Some("roadmap v2")
    );
    // A link on the way is never written through: the task's file is set aside whole.
    assert_eq!(
        read(&aside.join("design/new.html")).as_deref(),
        Some("design v1")
    );
    assert_eq!(std::fs::read_dir(&outside).expect("elsewhere").count(), 0);
    // The start copy went with the landing, and a second landing writes nothing.
    assert!(!root.join(".catervas/local/documents/start/CTV-3").exists());
    let again = land(&root, &worktree, &task()).expect("landed twice");
    assert!(again.written.is_empty() && again.removed.is_empty() && again.set_aside.is_empty());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn documents_taken_out_and_put_back_stay_on_disk() {
    let repository = TempRepo::new("local-documents-switch");
    let git = repository.adapter();
    let root = repository.path.clone();
    repository.write("docs/catervas/s.md", "hello\n");
    repository.commit("a document");
    let scratch = root.join(".catervas/local/worktrees/documents-1");

    exclude_documents(&git).expect("ignored");
    exclude_documents(&git).expect("ignored once");
    let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).expect("the file");
    assert_eq!(exclude.matches("/docs/catervas").count(), 1);

    // Taking out: the branch removes the folder from the tree, the checkout loses the file when it
    // fast-forwards, and the restore brings it back from the parent commit.
    let out = git
        .commit_untracking(
            &scratch,
            "docs/keep-documents-on-this-computer",
            "main",
            "docs/catervas",
            "docs: keep the team's documents on this computer",
        )
        .expect("committed")
        .expect("something was tracked");
    assert!(!scratch.exists(), "the scratch worktree is gone");
    git_in(
        &root,
        &["merge", "--ff-only", "docs/keep-documents-on-this-computer"],
    );
    assert!(!root.join("docs/catervas/s.md").exists(), "git removed it");
    let restored = restore_taken_out(&git, &out).expect("restored");
    assert_eq!(restored, ["docs/catervas/s.md"]);
    assert_eq!(
        read(&root.join("docs/catervas/s.md")).as_deref(),
        Some("hello\n")
    );
    assert_eq!(git_output_in(&root, &["status", "--porcelain"]).trim(), "");
    assert_eq!(
        git_output_in(&root, &["ls-files", "docs/catervas"]).trim(),
        "",
        "untracked now"
    );
    // A file the owner already has is never written over.
    std::fs::write(root.join("docs/catervas/s.md"), "owner's\n").expect("owner");
    assert!(restore_taken_out(&git, &out).expect("restored").is_empty());
    assert_eq!(
        read(&root.join("docs/catervas/s.md")).as_deref(),
        Some("owner's\n")
    );
    // With nothing tracked there is nothing to commit.
    assert_eq!(
        git.commit_untracking(&scratch, "docs/again", "main", "docs/catervas", "again")
            .expect("nothing"),
        None
    );
    assert!(!git.has_local_branch("docs/again").expect("asked"));

    // Putting back: what is on disk goes in as one commit, forced past the ignore.
    let back = git
        .commit_folder_from(
            &scratch,
            "docs/put-documents-back-in-the-repository",
            "main",
            "docs/catervas",
            &root.join("docs/catervas"),
            "docs: put the team's documents back in the repository",
        )
        .expect("committed")
        .expect("something to add");
    assert!(!scratch.exists() && !back.is_empty());
    git_in(
        &root,
        &[
            "merge",
            "--ff-only",
            "docs/put-documents-back-in-the-repository",
        ],
    );
    assert!(git_output_in(&root, &["ls-files", "docs/catervas"]).contains("docs/catervas/s.md"));
    assert_eq!(
        read(&root.join("docs/catervas/s.md")).as_deref(),
        Some("owner's\n")
    );
    assert_eq!(
        git_output_in(&root, &["show", "HEAD:docs/catervas/s.md"]),
        "owner's"
    );
    include_documents(&git).expect("no longer ignored");
    let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).expect("the file");
    assert!(!exclude.contains("Catervas") && !exclude.contains("/docs/catervas"));
}
