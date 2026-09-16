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
