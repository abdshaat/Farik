//! `cargo xtask <command>`: the repository's own commands.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, bail};

const CORE_FORBIDDEN: [&str; 7] = [
    "std::fs",
    "std::net",
    "std::process",
    "std::env",
    "std::time::SystemTime",
    "tokio",
    "rand",
];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> anyhow::Result<()> {
    let root = workspace_root();
    match args.first().map(String::as_str) {
        Some("check") => check(&root),
        Some("generate") => generate(&root, args.get(1).is_some_and(|flag| flag == "--check")),
        Some("pre-commit") => pre_commit(&root),
        Some("commit-msg") => commit_msg(
            args.get(1)
                .context("usage: cargo xtask commit-msg <file>")?,
        ),
        Some("todos") => todos(&root),
        Some("core-io") => core_io(&root),
        Some("install-hooks") => install_hooks(&root),
        _ => bail!(
            "usage: cargo xtask <check|pre-commit|commit-msg <file>|todos|core-io|install-hooks>"
        ),
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the workspace root")
        .to_path_buf()
}

fn cargo(root: &Path, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .args(args)
        .current_dir(root)
        .status()
        .with_context(|| format!("running cargo {}", args.join(" ")))?;
    if !status.success() {
        bail!("cargo {} failed", args.join(" "));
    }
    Ok(())
}

fn check(root: &Path) -> anyhow::Result<()> {
    cargo(root, &["fmt", "--all", "--check"])?;
    cargo(
        root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    cargo(root, &["test", "--workspace"])?;
    generate(root, true)?;
    todos(root)?;
    core_io(root)?;
    println!("xtask check: ok");
    Ok(())
}

fn pre_commit(root: &Path) -> anyhow::Result<()> {
    cargo(root, &["fmt", "--all", "--check"])?;
    todos(root)
}

fn commit_msg(file: &str) -> anyhow::Result<()> {
    let message = fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
    xtask::commit_message::check_commit_message(&message)
        .map_err(|reason| anyhow::anyhow!("commit message rejected: {reason}"))
}

fn tracked_files(root: &Path, patterns: &[&str]) -> anyhow::Result<Vec<(String, String)>> {
    let output = Command::new("git")
        .arg("ls-files")
        .arg("-z")
        .arg("--")
        .args(patterns)
        .current_dir(root)
        .output()
        .context("running git ls-files")?;
    if !output.status.success() {
        bail!("git ls-files failed");
    }
    String::from_utf8(output.stdout)?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| {
            let text =
                fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))?;
            Ok((path.to_string(), text))
        })
        .collect()
}

fn todos(root: &Path) -> anyhow::Result<()> {
    let files = tracked_files(root, &["*.rs", "*.ts", "*.tsx", "*.css", "*.toml"])?;
    let findings = xtask::todos::find_bare_todos(&files);
    if findings.is_empty() {
        return Ok(());
    }
    bail!(
        "bare TODO or FIXME without a task id or issue link:\n{}",
        findings.join("\n")
    );
}

fn core_io(root: &Path) -> anyhow::Result<()> {
    let files = tracked_files(root, &["crates/core/src/*.rs", "crates/core/src/**/*.rs"])?;
    let mut findings = Vec::new();
    for (path, text) in &files {
        for (index, line) in text.lines().enumerate() {
            if let Some(token) = CORE_FORBIDDEN.iter().find(|token| line.contains(*token)) {
                findings.push(format!("{path}:{}: uses {token}", index + 1));
            }
        }
    }
    if findings.is_empty() {
        return Ok(());
    }
    bail!(
        "farik-core performs no I/O (hard rule 5):\n{}",
        findings.join("\n")
    );
}

fn generate(root: &Path, check_only: bool) -> anyhow::Result<()> {
    for entry in &xtask::generate::GENERATED_SCHEMAS {
        let schema_json = fs::read_to_string(root.join(entry.schema))
            .with_context(|| format!("reading {}", entry.schema))?;
        let types = xtask::generate::generate_types(entry, &schema_json)?;
        let outputs = [(entry.types, types), (entry.schema_copy, schema_json)];
        for (path, wanted) in outputs {
            let current = fs::read_to_string(root.join(path)).unwrap_or_default();
            if check_only {
                if current != wanted {
                    bail!(
                        "{path} is out of date with {}; run cargo xtask generate",
                        entry.schema
                    );
                }
            } else if current != wanted {
                fs::write(root.join(path), &wanted).with_context(|| format!("writing {path}"))?;
                println!("generated {path}");
            }
        }
    }
    Ok(())
}

fn install_hooks(root: &Path) -> anyhow::Result<()> {
    let hooks = root.join(".git").join("hooks");
    fs::create_dir_all(&hooks)?;
    write_hook(
        &hooks.join("pre-commit"),
        "#!/bin/sh\nexec cargo xtask pre-commit\n",
    )?;
    write_hook(
        &hooks.join("commit-msg"),
        "#!/bin/sh\nexec cargo xtask commit-msg \"$1\"\n",
    )?;
    println!("installed pre-commit and commit-msg hooks");
    Ok(())
}

fn write_hook(path: &Path, body: &str) -> anyhow::Result<()> {
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
