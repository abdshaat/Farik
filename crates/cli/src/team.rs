//! `catervas rules show` and `catervas criteria list`: the two team files a person hand-edits (F15, F16).

use catervas_core::criteria::CriterionTemplate;
use catervas_store::requests::{criteria_json, criterion_how, criterion_method, rules_json};

use crate::Report;
use crate::project::Project;

/// The rules every command and every path check is held to (`docs/SPEC.md` section 5.12).
///
/// These are the team's rules plus the ones `catervas-core` ships, which is what the governor actually
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
        listed("read only paths", &rules.read_only_paths),
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
        listed("git subcommands", &rules.git_subcommands),
        listed("document paths", &rules.document_paths),
    ];
    Ok(Report {
        lines,
        json: rules_json(&rules),
        json_lines: None,
    })
}

/// One criterion as `catervas criteria list` prints it: its name, method, source, the member it
/// checks (empty for any), and what it runs, reads or asks.
fn criterion_line(one: &CriterionTemplate) -> String {
    format!(
        "{:<22} {:<9} {:<12} {:<12} {}",
        one.name.as_str(),
        criterion_method(&one.verification),
        one.source
            .as_ref()
            .map_or("human".to_string(), ToString::to_string),
        one.repo.as_ref().map_or("", |repo| repo.as_str()),
        criterion_how(&one.verification)
    )
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
                "no criteria: catervas init seeds them from what the repository says about itself"
                    .to_string(),
            ],
            json: criteria_json(&library),
            json_lines: None,
        });
    }
    let lines = library.criteria.iter().map(criterion_line).collect();
    Ok(Report {
        lines,
        json: criteria_json(&library),
        json_lines: None,
    })
}

#[cfg(test)]
mod tests {
    use catervas_core::criteria::validate_criteria;
    use serde_json::json;

    use super::criterion_line;

    #[test]
    fn lists_a_criterions_member() {
        let library = validate_criteria(&json!({ "criteria": [
            { "name": "app-the-tests-pass", "text": "The tests pass in app.",
              "source": "project_scan", "repo": "app",
              "verification": { "method": "test", "command": "npm test" } },
            { "name": "my-check", "text": "A person wrote this one.",
              "verification": { "method": "human", "question": "Is it right?" } },
        ]}))
        .expect("a library");
        assert_eq!(
            criterion_line(&library.criteria[0]),
            "app-the-tests-pass     test      project_scan app          npm test"
        );
        assert_eq!(
            criterion_line(&library.criteria[1]),
            "my-check               human     human                     Is it right?"
        );
    }
}
