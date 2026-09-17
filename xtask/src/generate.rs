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
pub const GENERATED_SCHEMAS: [GeneratedSchema; 6] = [
    GeneratedSchema {
        schema: "docs/schemas/task-contract.schema.json",
        types: "crates/core/src/generated/task_contract.rs",
        schema_copy: "crates/core/src/generated/task_contract.schema.json",
    },
    GeneratedSchema {
        schema: "docs/schemas/team.schema.json",
        types: "crates/core/src/generated/team.rs",
        schema_copy: "crates/core/src/generated/team.schema.json",
    },
    GeneratedSchema {
        schema: "docs/schemas/criteria.schema.json",
        types: "crates/core/src/generated/criteria.rs",
        schema_copy: "crates/core/src/generated/criteria.schema.json",
    },
    GeneratedSchema {
        schema: "docs/schemas/prices.schema.json",
        types: "crates/core/src/generated/prices.rs",
        schema_copy: "crates/core/src/generated/prices.schema.json",
    },
    GeneratedSchema {
        schema: "docs/schemas/event.schema.json",
        types: "crates/protocol/src/generated/event.rs",
        schema_copy: "crates/protocol/src/generated/event.schema.json",
    },
    GeneratedSchema {
        schema: "docs/schemas/command.schema.json",
        types: "crates/protocol/src/generated/command.rs",
        schema_copy: "crates/protocol/src/generated/command.schema.json",
    },
];

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
    settings.with_derive("PartialEq".to_string());
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

#[cfg(test)]
mod tests {
    use super::{GENERATED_SCHEMAS, generate_types};

    #[test]
    fn rejects_a_schema_that_is_not_json_and_names_it() {
        let error = generate_types(&GENERATED_SCHEMAS[0], "{ not json").expect_err("rejected");
        assert!(
            format!("{error:#}").starts_with("parsing docs/schemas/task-contract.schema.json"),
            "{error:#}"
        );
    }

    #[test]
    fn derives_partial_eq_so_that_a_generated_value_can_be_compared_to_an_expected_one() {
        // Without it, a test that builds an event can only assert on its wire form, which is the
        // thing the writer is supposed to be checked against.
        let schema = r#"{"title": "Thing", "type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}"#;
        let module = generate_types(&GENERATED_SCHEMAS[0], schema).expect("generated");
        assert!(
            module.contains(
                "#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]"
            ),
            "{module}"
        );
    }

    #[test]
    fn generates_a_formatted_module_with_the_header_and_the_type() {
        let schema =
            r#"{"title": "Thing", "type": "object", "properties": {"name": {"type": "string"}}}"#;
        let module = generate_types(&GENERATED_SCHEMAS[0], schema).expect("generated");
        assert!(module.starts_with(
            "// Generated from docs/schemas/task-contract.schema.json by `cargo xtask generate`. Do not edit.\n#![allow(clippy::all, clippy::pedantic, missing_docs)]\n\n"
        ));
        assert!(module.contains("pub struct Thing {"), "{module}");
        assert!(
            module.contains("pub name: ::std::option::Option<::std::string::String>,"),
            "{module}"
        );
    }
}
