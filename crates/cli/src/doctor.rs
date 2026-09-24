//! `farik doctor`: every way this project disagrees with itself.
//!
//! Drift between the files and the log is `farik_store::reconcile`'s answer (D5). The five checks
//! beyond it are the ones earlier steps of this phase recorded as being nobody's to report: a team
//! rule that does not compile, a setting Farik does not know, a criterion whose verification matches
//! no branch of its `oneOf`, a repository with no working tree, and a team file or criterion library
//! that is there and cannot be read. A sixth names each active agent's model that no price table
//! prices (ADR 0015).

use chrono::{DateTime, Utc};
use farik_core::governor::paths::{
    GlobError, PathRefusal, check_allowed_paths, check_protected_paths,
};
use farik_core::governor::permissions::{CommandRefusal, evaluate_command};
use farik_protocol::event::EventBody;
use farik_protocol::generated::event::{DriftDetectedBody, DriftDetectedBodyDrift};
use farik_runtime::cost::unpriced_models;
use farik_store::files::FilesError;
use farik_store::{Drift, reconcile};
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// Reads everything this project has and says where it disagrees with itself.
///
/// Every drift is recorded as a `drift.detected` event, because D5 says reconciliation reports each
/// difference as one and because the log is the record that somebody looked. The other findings are
/// about files rather than about a task, so they are printed and not recorded: no event kind in
/// `event.schema.json` is about them, and this step adds none.
///
/// # Errors
///
/// A sentence saying what could not be read at all — a project whose team file is unreadable cannot
/// even be opened, and that refusal reaches the person from `open_project`.
pub fn doctor(project: &Project, now: DateTime<Utc>) -> Result<Report, String> {
    let mut findings: Vec<String> = Vec::new();
    let mut recorded = Vec::new();

    let projections = project.projections()?;
    let drifts = reconcile(&project.files, &projections).map_err(|error| error.to_string())?;
    for drift in &drifts {
        findings.push(format!("{drift}"));
        let event = project.event(
            EventBody::DriftDetected(DriftDetectedBody {
                detail: drift.detail().to_string(),
                drift: named(drift),
            }),
            now,
            Some(drift.task_id().clone()),
        )?;
        recorded.push(project.append(&event)?);
    }

    findings.extend(rules_that_do_not_compile(project));
    findings.extend(settings_farik_does_not_know(project));
    findings.extend(criteria_that_match_no_branch(project));
    findings.extend(models_no_price_table_prices(project));

    let lines = if findings.is_empty() {
        vec!["nothing to report: the files and the log agree".to_string()]
    } else {
        findings.clone()
    };
    Ok(Report {
        lines,
        json: json!({
            "findings": findings,
            "drift": drifts.len(),
            "events": recorded,
        }),
        json_lines: None,
    })
}

/// Whether `doctor` found anything, which is what its exit code says.
#[must_use]
pub fn found_something(report: &Report) -> bool {
    report.json["findings"]
        .as_array()
        .is_some_and(|findings| !findings.is_empty())
}

/// The drift's kind as the event schema spells it.
fn named(drift: &Drift) -> DriftDetectedBodyDrift {
    match drift {
        Drift::ContractWithoutEvents { .. } => DriftDetectedBodyDrift::ContractWithoutEvents,
        Drift::EventsWithoutContract { .. } => DriftDetectedBodyDrift::EventsWithoutContract,
        Drift::StatusMismatch { .. } => DriftDetectedBodyDrift::StatusMismatch,
        Drift::LockMismatch { .. } => DriftDetectedBodyDrift::LockMismatch,
        Drift::ContractUnreadable { .. } => DriftDetectedBodyDrift::ContractUnreadable,
    }
}

/// A glob or a regular expression in the team's rules that does not compile.
///
/// `validate_team` accepts either, and 5.12 and 5.6 say what a rule that cannot be read does: it
/// refuses everything it is asked about. That is the right answer and the wrong way to find out, one
/// tool call at a time, so this is where it is said against the file.
fn rules_that_do_not_compile(project: &Project) -> Vec<String> {
    let rules = project.team.rules();
    let mut found = Vec::new();
    if let Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) =
        check_protected_paths(&[], &rules.protected_paths)
    {
        found.push(format!(
            ".farik/team.yaml: protected_paths has a glob that does not compile, {pattern:?}: \
             {detail}. Until it is fixed it refuses every path check, which is every tool call an \
             agent makes (5.12)"
        ));
    }
    if let Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) =
        check_allowed_paths(&[], &rules.allowed_paths_ceiling)
    {
        found.push(format!(
            ".farik/team.yaml: allowed_paths_ceiling has a glob that does not compile, \
             {pattern:?}: {detail}. Until it is fixed it refuses every contract's allowed paths \
             (5.12)"
        ));
    }
    if let Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) =
        check_allowed_paths(&[], &rules.document_paths)
    {
        found.push(format!(
            ".farik/team.yaml: document_paths has a glob that does not compile, {pattern:?}: \
             {detail}. Until it is fixed it refuses every task not assigned to the \
             software_developer (5.12)"
        ));
    }
    if let Err(CommandRefusal::InvalidPattern { pattern, detail }) =
        evaluate_command("true", &rules)
    {
        found.push(format!(
            ".farik/team.yaml: forbidden_commands has a pattern that is not a regular expression, \
             {pattern:?}: {detail}. Until it is fixed it refuses every command the team runs (5.12)"
        ));
    }
    found
}

/// Each active agent's model that no price table prices, whose usage is recorded at no cost and
/// counted by no dollar limit (ADR 0015); or, when the prices or a role cannot be read, that
/// sentence alone, since no model can be checked against them.
fn models_no_price_table_prices(project: &Project) -> Vec<String> {
    match unpriced(project) {
        Ok(sentences) => sentences
            .into_iter()
            .map(|sentence| format!(".farik/team.yaml: {sentence} (5.5)"))
            .collect(),
        Err(sentence) => vec![sentence],
    }
}

/// One sentence per model an active agent uses that the project's prices do not price, in the
/// words `farik doctor` and every driving start share: the model, the ids of the agents that use
/// it, what that means, and how to price it.
///
/// # Errors
///
/// The sentence of the price table or the role that cannot be read.
pub(crate) fn unpriced(project: &Project) -> Result<Vec<String>, String> {
    let prices = project
        .files
        .effective_prices()
        .map_err(|error| error.to_string())?;
    let models = unpriced_models(&project.team, &prices).map_err(|error| error.to_string())?;
    Ok(models
        .into_iter()
        .map(|(model, ids)| {
            format!(
                "no price table prices {model} (used by {}): its usage is recorded at no cost, \
                 and no dollar limit counts it. Add it to .farik/prices.json to price it",
                ids.join(", ")
            )
        })
        .collect())
}

/// A key in `.farik/local/settings.json` that Farik does not know.
///
/// That file is the one structured file with no schema behind it, so a typo is read as silence
/// rather than as a refusal, and the person goes on believing they set something.
fn settings_farik_does_not_know(project: &Project) -> Vec<String> {
    const KNOWN: [&str; 1] = ["sandbox"];
    let path = project.root.join(".farik/local/settings.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => map
            .keys()
            .filter(|key| !KNOWN.contains(&key.as_str()))
            .map(|key| {
                format!(
                    ".farik/local/settings.json: {key:?} is not a setting Farik knows, so it is \
                     read as nothing. The ones it knows are: {}",
                    KNOWN.join(", ")
                )
            })
            .collect(),
        Ok(_) => vec![
            ".farik/local/settings.json is not an object, so nothing in it is read".to_string(),
        ],
        Err(error) => vec![format!(
            ".farik/local/settings.json is not JSON: {error}. Every setting in it is read as its \
             default"
        )],
    }
}

/// A criterion whose `verification` matches no branch of its `oneOf`.
///
/// The schema refuses it with the whole object and the word `oneOf`, naming neither the property nor
/// the method. `.farik/team/criteria.yaml` is a file 5.13 expects people to hand-edit, so this says
/// which method was meant and what that method wants.
fn criteria_that_match_no_branch(project: &Project) -> Vec<String> {
    match project.files.read_criteria() {
        Ok(_) => Vec::new(),
        Err(FilesError::NotFound { .. }) => vec![
            ".farik/team/criteria.yaml is not there: farik init writes it from what the repository \
             says about itself"
                .to_string(),
        ],
        Err(FilesError::Invalid { path, detail }) => {
            let mut said = format!("{path} is not a criterion library: {detail}");
            if detail.contains("oneOf") {
                said.push_str(
                    ". A verification is one of five shapes, and each wants its own fields: \
                     `command` wants a command and an expect, `test` wants a command and \
                     new_tests_required, `artifact` wants a path and must_contain, `review` wants a \
                     rubric, `human` wants a question",
                );
            }
            vec![said]
        }
        Err(other) => vec![other.to_string()],
    }
}
