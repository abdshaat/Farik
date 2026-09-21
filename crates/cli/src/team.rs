//! `farik rules show` and `farik criteria list`: the two team files a person hand-edits (F15, F16).

use farik_core::criteria::{CriterionTemplate, TemplateVerification};
use serde_json::json;

use crate::Report;
use crate::project::Project;

/// The rules every command and every path check is held to (`docs/SPEC.md` section 5.12).
///
/// These are the team's rules plus the ones `farik-core` ships, which is what the governor actually
/// applies: a rule only ever narrows what a tier allows, so the shipped protected paths are kept
/// whatever the team wrote.
///
/// # Errors
///
/// Nothing today; the team was read when the project was opened.
pub fn rules(project: &Project) -> Result<Report, String> {
    let rules = project.team.rules();
    let listed = |label: &str, values: &[String]| {
        if values.is_empty() {
            format!("{label}: none")
        } else {
            format!("{label}: {}", values.join(", "))
        }
    };
    let lines = vec![
        listed("protected paths", &rules.protected_paths),
        listed("allowed paths ceiling", &rules.allowed_paths_ceiling),
        listed("required criteria", &rules.required_criteria),
        format!(
            "new tests required of every test criterion: {}",
            if rules.require_new_tests { "yes" } else { "no" }
        ),
        match rules.max_task_budget_usd {
            Some(cap) => format!("most a contract may cost: ${cap}"),
            None => "most a contract may cost: no cap".to_string(),
        },
        listed("forbidden commands", &rules.forbidden_commands),
    ];
    Ok(Report {
        lines,
        json: json!({
            "protected_paths": rules.protected_paths,
            "allowed_paths_ceiling": rules.allowed_paths_ceiling,
            "required_criteria": rules.required_criteria,
            "require_new_tests": rules.require_new_tests,
            "max_task_budget_usd": rules.max_task_budget_usd,
            "forbidden_commands": rules.forbidden_commands,
        }),
        json_lines: None,
    })
}

/// The criterion library a contract refers to by name (`docs/SPEC.md` section 5.13).
///
/// # Errors
///
/// A sentence saying what the library file got wrong.
pub fn criteria(project: &Project) -> Result<Report, String> {
    let library = project
        .files
        .read_criteria()
        .map_err(|error| error.to_string())?;
    if library.criteria.is_empty() {
        return Ok(Report {
            lines: vec![
                "no criteria: farik init seeds them from what the repository says about itself"
                    .to_string(),
            ],
            json: json!({ "criteria": [] }),
            json_lines: None,
        });
    }
    let lines = library
        .criteria
        .iter()
        .map(|one| {
            format!(
                "{:<22} {:<9} {:<12} {}",
                one.name.as_str(),
                method_of(&one.verification),
                one.source
                    .as_ref()
                    .map_or("human".to_string(), ToString::to_string),
                how_of(&one.verification)
            )
        })
        .collect();
    Ok(Report {
        lines,
        json: json!({
            "criteria": library
                .criteria
                .iter()
                .map(|one| json!({
                    "name": one.name.as_str(),
                    "text": one.text.as_str(),
                    "source": one.source.as_ref().map(ToString::to_string),
                    "method": method_of(&one.verification),
                    "how": how_of(&one.verification),
                }))
                .collect::<Vec<_>>(),
        }),
        json_lines: None,
    })
}

/// How a criterion is verified, in the one word the schema's `method` carries.
fn method_of(verification: &TemplateVerification) -> &'static str {
    match verification {
        TemplateVerification::Variant0 { .. } => "command",
        TemplateVerification::Variant1 { .. } => "test",
        TemplateVerification::Variant2 { .. } => "artifact",
        TemplateVerification::Variant3 { .. } => "review",
        TemplateVerification::Variant4 { .. } => "human",
    }
}

/// What is actually run, read or asked, which is the part a person checks.
fn how_of(verification: &TemplateVerification) -> String {
    match verification {
        TemplateVerification::Variant0 { command, .. }
        | TemplateVerification::Variant1 { command, .. } => command.clone(),
        TemplateVerification::Variant2 { path, .. } => path.clone(),
        TemplateVerification::Variant3 { rubric, .. } => rubric.join("; "),
        TemplateVerification::Variant4 { question, .. } => question.clone(),
    }
}

/// Every criterion in the library, for a caller that wants the values rather than the words.
#[must_use]
pub fn named(criteria: &[CriterionTemplate]) -> Vec<String> {
    criteria.iter().map(|one| one.name.to_string()).collect()
}
