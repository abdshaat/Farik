//! The open-source code never holds a credential of Farik's, and no source reads a variable at
//! build time (ADR 0044): a build with the variable set would carry it into every binary, where
//! it protects nothing. Farik Cloud holds Farik's apps and their secrets from phase 11.

use std::path::{Path, PathBuf};

/// The macro that reads a variable at build time, put together from parts so that this file does
/// not hold it and find itself.
const NEEDLE: &str = concat!("option", "_env!");

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
        for (index, line) in text.lines().enumerate() {
            if line.contains(NEEDLE) {
                let shown = source.strip_prefix(root).unwrap_or(source);
                reading.push(format!("{}:{}", shown.display(), index + 1));
            }
        }
    }
    assert!(
        reading.is_empty(),
        "these lines read a variable at build time: {reading:?}"
    );
}
