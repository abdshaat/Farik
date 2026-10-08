//! The open-source code never holds a credential of Farik's, and no source reads a variable at
//! build time but those Cargo sets (ADR 0044): a build with the variable set would carry it into
//! every binary, where it protects nothing. Farik Cloud holds Farik's apps and their secrets from
//! phase 8.

use std::path::{Path, PathBuf};

/// The start of both macros that read a variable at build time (`option_env!` ends the same
/// way), put together from parts so that this file does not hold it and find itself.
const NEEDLE: &str = concat!("en", "v!(");

/// Whether `name` is a variable Cargo itself sets for every build: the package's own fields, its
/// folder and its crate's and binaries' names. Any other name, one starting `CARGO_` included
/// (a registry token, say), is something the environment of the build would carry in.
fn is_cargo_s_own(name: &str) -> bool {
    name.starts_with("CARGO_PKG_")
        || name.starts_with("CARGO_BIN_EXE_")
        || ["CARGO_MANIFEST_DIR", "CARGO_CRATE_NAME", "CARGO_BIN_NAME"].contains(&name)
}

/// The line numbers in `text` where a macro reads a variable at build time, `env!` or
/// `option_env!`, of any name but Cargo's own. A name that is not a string literal counts.
fn build_time_reads(text: &str) -> Vec<usize> {
    text.match_indices(NEEDLE)
        .filter(|(at, _)| {
            let after = text[at + NEEDLE.len()..].trim_start();
            let name = after
                .strip_prefix('"')
                .and_then(|quoted| quoted.split_once('"'))
                .map(|(name, _)| name);
            !name.is_some_and(is_cargo_s_own)
        })
        .map(|(at, _)| text[..at].matches('\n').count() + 1)
        .collect()
}

/// Every `.rs` file under `dir`, `target` folders skipped.
fn sources_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("{} is read: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("a folder entry is read").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            sources_under(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_source_reads_a_build_time_variable() {
    // The workspace root is the grandparent of this crate's folder: `crates/runtime`.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two folders below the workspace root");
    let mut sources = Vec::new();
    for folder in ["crates", "xtask"] {
        sources_under(&root.join(folder), &mut sources);
    }
    assert!(
        !sources.is_empty(),
        "no source was found under {}",
        root.display()
    );
    sources.sort();

    let mut reading = Vec::new();
    for source in &sources {
        let text = std::fs::read_to_string(source)
            .unwrap_or_else(|error| panic!("{} is read: {error}", source.display()));
        for line in build_time_reads(&text) {
            let shown = source.strip_prefix(root).unwrap_or(source);
            reading.push(format!("{}:{line}", shown.display()));
        }
    }
    assert!(
        reading.is_empty(),
        "these lines read a variable at build time: {reading:?}"
    );
}

#[test]
fn finds_a_variable_read_by_either_macro() {
    // Put together from parts, as `NEEDLE` is, so that this file does not hold them.
    let plain = concat!("en", "v!(\"SOME_SECRET\")");
    let optional = concat!("option_", "en", "v!(\"SOME_SECRET\")");
    let wrapped = concat!("en", "v!(\n        \"SOME_SECRET\",\n    )");
    assert_eq!(
        build_time_reads(&format!("fn a() {{}}\nconst A: &str = {plain};")),
        [2]
    );
    assert_eq!(
        build_time_reads(&format!("const A: Option<&str> = {optional};")),
        [1]
    );
    assert_eq!(
        build_time_reads(&format!("const A: &str = {wrapped};")),
        [1]
    );
}

#[test]
fn lets_the_names_cargo_sets_through() {
    for name in [
        "CARGO_PKG_NAME",
        "CARGO_PKG_VERSION",
        "CARGO_MANIFEST_DIR",
        "CARGO_BIN_EXE_farik",
    ] {
        let read = format!("{}\"{name}\")", concat!("en", "v!("));
        assert_eq!(build_time_reads(&format!("const A: &str = {read};")), []);
    }
    // A name that only starts like Cargo's is a variable of someone's, such as a token in the
    // environment of the build.
    let token = concat!("en", "v!(\"CARGO_REGISTRY_TOKEN\")");
    assert_eq!(build_time_reads(&format!("const A: &str = {token};")), [1]);
}
