//! Reading the committed files that proofs of linking come from, on real repositories.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use catervas_store::git::fixtures::TempRepo;
use catervas_store::linking::committed_files;

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_only_committed_manifests_at_head() {
    let repository = TempRepo::new("linking-files");
    repository.write("package.json", "{\"name\":\"at-head\"}\n");
    repository.write(".github/workflows/ci.yml", "name: ci\n");
    repository.write("notes/todo.txt", "not a manifest\n");
    repository.write("api/client/Cargo.toml", "[package]\nname = \"client\"\n");
    repository.write("a/b/c/d/package.json", "{\"name\":\"too-deep\"}\n");
    // A manifest over 256 KiB is skipped, and nothing under node_modules, vendor or target is read.
    repository.write(
        "big/package.json",
        &format!("{{\"pad\":\"{}\"}}\n", "x".repeat(300 * 1024)),
    );
    repository.write("node_modules/dep/package.json", "{\"name\":\"dep\"}\n");
    repository.write("vendor/lib/go.mod", "module lib\n");
    repository.write("target/debug/Cargo.toml", "[package]\nname = \"built\"\n");
    repository.commit("the manifests");

    // What is not committed is not read: a new file, and a change to a committed one.
    repository.write("compose.yaml", "services: {}\n");
    repository.write("package.json", "{\"name\":\"changed-after-the-commit\"}\n");

    let files = committed_files(&repository.adapter()).expect("the files are read");
    let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "package.json",
            ".github/workflows/ci.yml",
            "api/client/Cargo.toml"
        ]
    );
    assert_eq!(files[0].1, "{\"name\":\"at-head\"}\n", "the text at HEAD");

    // At most 300 files are read, and a folder that is no repository gives none.
    let many = TempRepo::new("linking-files-cap");
    for number in 0..320 {
        many.write(&format!("pkg{number:03}/package.json"), "{}\n");
    }
    many.commit("many manifests");
    assert_eq!(committed_files(&many.adapter()).expect("read").len(), 300);
    let plain = std::env::temp_dir().join(format!("catervas-no-repo-{}", std::process::id()));
    std::fs::create_dir_all(&plain).expect("a folder");
    std::fs::write(plain.join("package.json"), "{}\n").expect("a file");
    let none = committed_files(&catervas_store::Git::open(plain.clone())).expect("no error");
    let _ = std::fs::remove_dir_all(&plain);
    assert!(none.is_empty(), "{none:?}");
}
