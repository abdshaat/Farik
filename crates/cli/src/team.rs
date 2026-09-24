//! `farik rules show` and `farik criteria list`: the two team files a person hand-edits (F15, F16).

use farik_store::requests::{criteria_json, criterion_how, criterion_method, rules_json};

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
        listed("document paths", &rules.document_paths),
    ];
    Ok(Report {
        lines,
        json: rules_json(&rules),
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
            json: criteria_json(&library),
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
                criterion_method(&one.verification),
                one.source
                    .as_ref()
                    .map_or("human".to_string(), ToString::to_string),
                criterion_how(&one.verification)
            )
        })
        .collect();
    Ok(Report {
        lines,
        json: criteria_json(&library),
        json_lines: None,
    })
}
