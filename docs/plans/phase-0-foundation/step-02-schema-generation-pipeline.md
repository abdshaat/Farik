# Phase 0, step 02: Schema generation pipeline

Status: draft
Branch: `phase/0-foundation` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` section 3 (Contract), F4 (validation against the JSON schema); `docs/standards/code.md`, "Schema validation"; ADR 0005
Depends on: step 01 of this phase (not yet committed; record the sha here when it lands)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: pending

## Goal

`docs/schemas/task-contract.schema.json` is the single source of truth for what a contract is, and `farik-core` carries Rust types generated from it, so that no type is ever hand-written twice and a schema change is a regenerate away. When this step is done, `cargo xtask generate` writes the generated module and a copy of the schema into `crates/core/src/generated/`, `cargo xtask check` fails when either is stale, and step 03 can build a validator on the generated types.

## Decisions

- JSON Schema is the source of truth; Rust types are generated, committed, and checked for staleness by `cargo xtask check`: `docs/plans/project-plan.md`, every-phase decisions; ADR 0005.
- The generator is the `typify` 0.8.0 library called from `xtask` (not the `cargo-typify` binary), so that nothing has to be installed beyond the workspace. `schemars` 0.8.22 parses the schema for it, `syn` 3.0.5 and `prettyplease` 0.3.0 render the code, and the result is piped through `rustfmt` so that `cargo fmt --check` accepts it. Rejected: `#[rustfmt::skip]` or a rustfmt ignore list, because the latter is a nightly-only option.
- Struct builders are off (`with_struct_builder(false)`); the schema's patterns become newtypes with `FromStr` validation, its defaults become `#[serde(default)]`, and its `oneOf` on `verification` becomes an untagged enum with positional variants, which step 03 wraps in a named enum.
- The generated module carries `#![allow(clippy::all, clippy::pedantic, missing_docs)]`; generated code is not held to the lint rules hand-written code is.
- The schema is also copied verbatim next to the module so that `farik-core` can embed it with `include_str!` (step 03) without reaching outside the crate.
- `cargo xtask generate --check` regenerates in memory and compares; `cargo xtask check` runs it after the tests.
- `farik-core` gains the dependencies the generated code uses: `serde` 1.0.229 (derive), `serde_json` 1.0.151, `chrono` 0.4.45 (serde; for `date-time`), `regress` 0.12.0 (typify's pattern engine).

## Design

`xtask::generate` holds `GENERATED_SCHEMAS` (one entry: the contract schema, its Rust module, its schema copy) and `generate_types`. `cargo xtask generate` writes the files; `--check` compares. `farik-core` gets `src/generated/mod.rs` declaring the generated module and `pub mod generated;` in `lib.rs`.

Out of scope: validation and the domain `Verification` enum (step 03); any second schema (each is added by the step that owns it).

## Architecture notes

Touches `xtask` (new module, new dependencies, two new commands), `crates/core` (new dependencies, new `generated/` directory), and the workspace `Cargo.toml`. Reads `docs/schemas/task-contract.schema.json` at generation time only.

## Global constraints

- `farik-core` does no I/O; `include_str!` is compile-time and is what step 03 uses.
- Generated files are written only by `cargo xtask generate` and never edited.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
Cargo.toml                                              modifies: adds the generator and core dependencies to the workspace table
xtask/Cargo.toml                                        modifies: adds prettyplease, schemars, serde_json, syn, typify
xtask/src/lib.rs                                        modifies: declares the generate module
xtask/src/generate.rs                                   creates: GENERATED_SCHEMAS, generate_types, rustfmt
xtask/src/main.rs                                       modifies: the generate command and its place in check
crates/core/Cargo.toml                                  modifies: adds chrono, regress, serde, serde_json
crates/core/src/lib.rs                                  modifies: declares the generated module
crates/core/src/generated/mod.rs                        creates: declares task_contract
crates/core/src/generated/task_contract.rs              creates (generated): the contract types
crates/core/src/generated/task_contract.schema.json     creates (generated): the schema copy
docs/plans/phase-0-foundation/step-02-schema-generation-pipeline.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: The generator and the generated module

Files: created `xtask/src/generate.rs`, `crates/core/src/generated/mod.rs`, `crates/core/src/generated/task_contract.rs`, `crates/core/src/generated/task_contract.schema.json`; modified `Cargo.toml`, `xtask/Cargo.toml`, `xtask/src/lib.rs`, `xtask/src/main.rs`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`

Consumes: `docs/schemas/task-contract.schema.json` on `main`; the `xtask` commands from step 01
Produces: `cargo xtask generate [--check]`; `xtask::generate::{GeneratedSchema, GENERATED_SCHEMAS, generate_types}`; `farik_core::generated::task_contract::*`

- [ ] Add the dependencies. Make the workspace `Cargo.toml` exactly:

  ```toml
  [workspace]
  resolver = "3"
  members = ["crates/*", "xtask"]

  [workspace.package]
  version = "0.0.0"
  edition = "2024"
  license = "Apache-2.0"
  repository = "https://github.com/abdshaat/Farik"
  rust-version = "1.98.1"

  [workspace.dependencies]
  anyhow = "=1.0.104"
  chrono = { version = "=0.4.45", features = ["serde"] }
  prettyplease = "=0.3.0"
  regress = "=0.12.0"
  schemars = "=0.8.22"
  serde = { version = "=1.0.229", features = ["derive"] }
  serde_json = "=1.0.151"
  syn = { version = "=3.0.5", features = ["full"] }
  typify = "=0.8.0"

  [workspace.lints.rust]
  unsafe_code = "forbid"
  missing_docs = "warn"

  [workspace.lints.clippy]
  all = { level = "warn", priority = -1 }
  pedantic = { level = "warn", priority = -1 }
  ```

  `xtask/Cargo.toml`:

  ```toml
  [package]
  name = "xtask"
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  publish = false

  [dependencies]
  anyhow.workspace = true
  prettyplease.workspace = true
  schemars.workspace = true
  serde_json.workspace = true
  syn.workspace = true
  typify.workspace = true

  [lints]
  workspace = true
  ```

  `crates/core/Cargo.toml`:

  ```toml
  [package]
  name = "farik-core"
  description = "Schemas, task state machine, governor, and cost model. Performs no I/O."
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  repository.workspace = true
  rust-version.workspace = true

  [dependencies]
  chrono.workspace = true
  regress.workspace = true
  serde.workspace = true
  serde_json.workspace = true

  [lints]
  workspace = true
  ```

- [ ] Declare the modules before they exist, so that the check fails for the right reason. `crates/core/src/generated/mod.rs`:

  ```rust
  //! Rust types generated from the JSON Schemas in `docs/schemas/`. Regenerate with `cargo xtask generate`.

  pub mod task_contract;
  ```

  `crates/core/src/lib.rs`:

  ```rust
  //! Farik's harness: schemas, the task state machine, the governor, and the cost model.
  //! This crate performs no I/O.

  /// Types generated from `docs/schemas/`.
  pub mod generated;

  /// The crate's package name, as published.
  pub const CORE_CRATE_NAME: &str = "farik-core";

  #[cfg(test)]
  mod tests {
      use super::CORE_CRATE_NAME;

      #[test]
      fn exposes_its_crate_name() {
          assert_eq!(CORE_CRATE_NAME, "farik-core");
      }
  }
  ```

- [ ] Confirm the crate fails to build because the generated module is missing:

  ```
  cargo check --package farik-core
  # expected, among the output:
  # error[E0583]: file not found for module `task_contract`
  # error: could not compile `farik-core` (lib) due to 1 previous error
  ```

- [ ] Write the generator. `xtask/src/lib.rs`:

  ```rust
  //! Repository tasks: the check command, hooks, and code generation. Run with `cargo xtask`.

  /// Commit message rules from `docs/standards/code.md`.
  pub mod commit_message;
  /// Rust types generated from the JSON Schemas in `docs/schemas/`.
  pub mod generate;
  /// The bare `TODO` rule from `docs/standards/code.md`.
  pub mod todos;
  ```

  `xtask/src/generate.rs`:

  ```rust
  use std::io::Write;
  use std::process::{Command, Stdio};

  use anyhow::{Context, bail};

  /// One schema and the two files generated from it, as paths relative to the workspace root.
  pub struct GeneratedSchema {
      /// The JSON Schema, the source of truth.
      pub schema: &'static str,
      /// The Rust module `typify` writes.
      pub types: &'static str,
      /// A verbatim copy of the schema next to the types, for the crate to embed at compile time.
      pub schema_copy: &'static str,
  }

  /// Every schema that has generated code.
  pub const GENERATED_SCHEMAS: [GeneratedSchema; 1] = [GeneratedSchema {
      schema: "docs/schemas/task-contract.schema.json",
      types: "crates/core/src/generated/task_contract.rs",
      schema_copy: "crates/core/src/generated/task_contract.schema.json",
  }];

  /// Generates the Rust module for one schema's JSON text, formatted by `rustfmt`.
  ///
  /// # Errors
  ///
  /// Returns an error when the schema does not parse, `typify` cannot express it, or `rustfmt`
  /// is not installed.
  pub fn generate_types(entry: &GeneratedSchema, schema_json: &str) -> anyhow::Result<String> {
      let schema: schemars::schema::RootSchema =
          serde_json::from_str(schema_json).with_context(|| format!("parsing {}", entry.schema))?;
      let mut settings = typify::TypeSpaceSettings::default();
      settings.with_struct_builder(false);
      let mut type_space = typify::TypeSpace::new(&settings);
      type_space
          .add_root_schema(schema)
          .with_context(|| format!("converting {}", entry.schema))?;
      let file =
          syn::parse2::<syn::File>(type_space.to_stream()).context("parsing generated code")?;
      let source = format!(
          "// Generated from {} by `cargo xtask generate`. Do not edit.\n#![allow(clippy::all, clippy::pedantic, missing_docs)]\n\n{}",
          entry.schema,
          prettyplease::unparse(&file)
      );
      rustfmt(&source)
  }

  fn rustfmt(source: &str) -> anyhow::Result<String> {
      let mut child = Command::new("rustfmt")
          .args(["--edition", "2024", "--config", "style_edition=2024"])
          .stdin(Stdio::piped())
          .stdout(Stdio::piped())
          .spawn()
          .context("running rustfmt")?;
      child
          .stdin
          .take()
          .context("rustfmt stdin")?
          .write_all(source.as_bytes())?;
      let output = child.wait_with_output()?;
      if !output.status.success() {
          bail!("rustfmt failed on generated code");
      }
      Ok(String::from_utf8(output.stdout)?)
  }
  ```

  `xtask/src/main.rs` in full (the `generate` command and its call inside `check` are the additions):

  ```rust
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
  ```

- [ ] Run the freshness check and confirm it fails because the generated files do not exist:

  ```
  cargo xtask generate --check
  # expected:
  # xtask: crates/core/src/generated/task_contract.rs is out of date with docs/schemas/task-contract.schema.json; run cargo xtask generate
  ```

- [ ] Generate:

  ```
  cargo xtask generate
  # expected:
  # generated crates/core/src/generated/task_contract.rs
  # generated crates/core/src/generated/task_contract.schema.json
  sha256sum crates/core/src/generated/task_contract.rs crates/core/src/generated/task_contract.schema.json
  # expected:
  # 7ce68a45c733f6942397ee2cff0a89f711d50f39bf834bb34aadf0259f5bf205  crates/core/src/generated/task_contract.rs
  # 546dff15c3d2d76208e5e334dabf5d2179cb55fede30e42dc9cd50e07f95f983  crates/core/src/generated/task_contract.schema.json
  wc -l crates/core/src/generated/task_contract.rs
  # expected: 980
  head -3 crates/core/src/generated/task_contract.rs
  # expected:
  # // Generated from docs/schemas/task-contract.schema.json by `cargo xtask generate`. Do not edit.
  # #![allow(clippy::all, clippy::pedantic, missing_docs)]
  # 
  ```

  The module exports `Role`, `ExitCriterion`, `ExitCriterionVerification` (five positional variants), `FarikTaskContract`, and a newtype for every `pattern` and length constraint (`FarikTaskContractId`, `ExitCriterionId`, and so forth). If the hashes differ, the schema on `main` or a pinned crate version has changed since this plan was written; stop and update the plan.

- [ ] Run the full check; confirm green:

  ```
  cargo fmt --all
  cargo xtask check
  # expected, among the output:
  # test result: ok. 1 passed; ...    (farik-core)
  # test result: ok. 13 passed; ...   (xtask)
  # xtask check: ok
  # exit code 0
  ```

- [ ] Commit: `build(repo): generate contract types from the json schema`

## Verification

```
cargo xtask check
# expected: exit code 0 ending with "xtask check: ok"; the generate --check inside it passes silently.
```

```
cargo xtask generate && git status --porcelain crates/core/src/generated
# expected: no "generated ..." lines (nothing changed), and no output from git status.
```

```
git log --oneline -1
# expected: build(repo): generate contract types from the json schema
```

## Open questions

none
