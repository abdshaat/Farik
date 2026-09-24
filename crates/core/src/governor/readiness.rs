//! The Definition of Ready (`docs/SPEC.md` section 5.3), the team rules it applies (5.12), and
//! the parent rules of an epic's tasks (5.16), as one function over a contract and a context.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::paths::{GlobError, PathRefusal, check_allowed_paths};
use super::team_rules::TeamRules;
use crate::contract::{
    Role, TaskContract, TaskStatus, Verification, VerificationWire, wire_method,
};
use crate::generated::task_contract::FarikTaskContractKind as Kind;
use crate::text::listed;

/// Builders for readiness contexts and typed contracts, usable by every crate's tests.
pub mod fixtures;

/// One rule of the Definition of Ready. Failures are reported in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReadinessRule {
    /// The intent has words in it, not only whitespace.
    IntentPresent,
    /// At least one exit criterion exists.
    CriteriaPresent,
    /// Every criterion's `method` value names the shape its fields have.
    CriteriaMethodsValid,
    /// Every `command` and `test` criterion has a command that is not blank.
    CommandCriteriaComplete,
    /// The budget does not exceed the remaining sprint budget.
    BudgetWithinSprint,
    /// Someone other than the assignee can review: the human, for an epic only; otherwise one
    /// active agent of the reviewer role, or two when it is the assignee's role.
    ReviewerAvailable,
    /// Scope names at least one `out_of_scope` item that is not blank.
    OutOfScopePresent,
    /// Every listed dependency exists and is at least `ready`.
    DependenciesReady,
    /// The contract has a criterion of every method the team rules require.
    RequiredCriteriaPresent,
    /// Every `test` criterion sets `new_tests_required` when the team rules require new tests.
    NewTestsRequiredByRule,
    /// Every allowed path falls within the team's ceiling.
    AllowedPathsWithinCeiling,
    /// A task not assigned to the Software Developer keeps every allowed path within the team's
    /// document paths: only the Developer changes code.
    DocumentPathsOnly,
    /// A task's budget does not exceed the team's cap on a task; an epic is bounded by the
    /// sprint budget instead.
    BudgetWithinTeamMax,
    /// An epic has no parent.
    NoParentForEpic,
    /// A task's parent is known and `in_progress`.
    ParentInProgress,
    /// A task's allowed paths fall within its parent's.
    PathsWithinParent,
    /// A task's budget fits in its parent's remaining budget.
    BudgetWithinParent,
    /// The Scrum Master's judgment review is recorded when the team has one.
    JudgmentRecorded,
    /// The Scrum Master judged that the task fits its budget.
    JudgmentFitsBudget,
    /// The Scrum Master judged that the criteria would detect the failure the intent worries
    /// about.
    JudgmentCriteriaDetectFailure,
}

/// The Scrum Master's judgment (`docs/SPEC.md` section 5.3), recorded by the runtime as a
/// `contract.judged` event and passed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentReview {
    /// The task is small enough to finish within its budget.
    pub fits_budget: bool,
    /// The criteria would detect the failure the intent worries about, not just that something
    /// ran.
    pub criteria_detect_failure: bool,
    /// The written reason for both answers.
    pub reason: String,
}

/// What the readiness check needs to know about a task's epic (`docs/SPEC.md` section 5.16).
#[derive(Debug, Clone, PartialEq)]
pub struct ParentState {
    /// The epic's status; a task is ready only while its epic is `in_progress`.
    pub status: TaskStatus,
    /// The epic's allowed paths, which the task's must fall within.
    pub allowed_paths: Vec<String>,
    /// What is left of the epic's budget for its tasks, in dollars.
    pub remaining_budget_usd: f64,
}

/// Everything the readiness check needs from the world, passed in by the runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadinessContext {
    /// What is left of the sprint's budget, in dollars.
    pub remaining_sprint_budget_usd: f64,
    /// The status of every task the contract may list as a dependency, by task id.
    pub dependency_statuses: BTreeMap<String, TaskStatus>,
    /// How many active agents the team has of each role.
    pub active_agents_by_role: BTreeMap<Role, u32>,
    /// The contract's epic, when it lists a parent the runtime knows.
    pub parent: Option<ParentState>,
    /// The team rules in force.
    pub rules: TeamRules,
    /// Whether the team has an active Scrum Master, whose judgment review is then required.
    pub requires_judgment_review: bool,
    /// The recorded judgment review, when there is one.
    pub judgment_review: Option<JudgmentReview>,
}

/// One rule the contract fails, with a message that says what to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessFailure {
    /// The rule that failed.
    pub rule: ReadinessRule,
    /// What is wrong and what would fix it.
    pub message: String,
}

type Check = fn(&TaskContract, &ReadinessContext) -> Option<ReadinessFailure>;

const CHECKS: [Check; 17] = [
    intent_present,
    criteria_present,
    criteria_methods_valid,
    command_criteria_complete,
    budget_within_sprint,
    reviewer_available,
    out_of_scope_present,
    dependencies_ready,
    required_criteria_present,
    new_tests_required_by_rule,
    allowed_paths_within_ceiling,
    document_paths_only,
    budget_within_team_max,
    no_parent_for_epic,
    parent_in_progress,
    paths_within_parent,
    budget_within_parent,
];

/// The Scrum Master's judgment, asked of a contract only once every other rule passes, so that a
/// contract going back for another rule is not also refused for a judgment nobody asked for yet.
const JUDGMENT_CHECKS: [Check; 3] = [
    judgment_recorded,
    judgment_fits_budget,
    judgment_criteria_detect_failure,
];

/// Checks a contract against the Definition of Ready: the structural rules of `docs/SPEC.md`
/// section 5.3, the team rules of 5.12, the parent rules of 5.16, and the Scrum Master's
/// recorded judgment when the team has one, evaluated only when every other rule passes. Refuses
/// with every rule the contract fails.
///
/// # Errors
///
/// Every failed rule, in the order of `ReadinessRule`, each with a message that says what to
/// change.
pub fn evaluate_readiness(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Result<(), Vec<ReadinessFailure>> {
    let failed = |checks: &[Check]| -> Vec<ReadinessFailure> {
        checks
            .iter()
            .filter_map(|check| check(contract, context))
            .collect()
    };
    let mut failures = failed(&CHECKS);
    if failures.is_empty() {
        failures = failed(&JUDGMENT_CHECKS);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn failure(rule: ReadinessRule, message: String) -> ReadinessFailure {
    ReadinessFailure { rule, message }
}

fn criterion_ids<'a>(
    contract: &'a TaskContract,
    mut keep: impl FnMut(&Verification, &VerificationWire) -> bool + 'a,
) -> Vec<String> {
    contract
        .exit_criteria
        .iter()
        .filter(|criterion| {
            keep(
                &Verification::from(&criterion.verification),
                &criterion.verification,
            )
        })
        .map(|criterion| criterion.id.to_string())
        .collect()
}

fn intent_present(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
    if contract.intent.trim().is_empty() {
        return Some(failure(
            ReadinessRule::IntentPresent,
            "the intent is blank; state the user-facing reason for the task".to_string(),
        ));
    }
    None
}

fn criteria_present(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
    if contract.exit_criteria.is_empty() {
        return Some(failure(
            ReadinessRule::CriteriaPresent,
            "there is no exit criterion; add at least one".to_string(),
        ));
    }
    None
}

fn criteria_methods_valid(
    contract: &TaskContract,
    _: &ReadinessContext,
) -> Option<ReadinessFailure> {
    let bad: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter_map(|criterion| {
            let implied = Verification::from(&criterion.verification).method();
            let named = wire_method(&criterion.verification).unwrap_or("nothing");
            (named != implied).then(|| {
                format!(
                    "criterion {} names the method {named} but has the fields of {implied}",
                    criterion.id.as_str()
                )
            })
        })
        .collect();
    if bad.is_empty() {
        return None;
    }
    Some(failure(ReadinessRule::CriteriaMethodsValid, bad.join("; ")))
}

fn command_criteria_complete(
    contract: &TaskContract,
    _: &ReadinessContext,
) -> Option<ReadinessFailure> {
    let bad = criterion_ids(contract, |verification, _| match verification {
        Verification::Command { command, .. } | Verification::Test { command, .. } => {
            command.trim().is_empty()
        }
        _ => false,
    });
    if bad.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::CommandCriteriaComplete,
        format!(
            "{} {} a blank command",
            listed("criterion", "criteria", &bad),
            if bad.len() == 1 { "has" } else { "have" }
        ),
    ))
}

/// Whether a budget is above a ceiling. A figure that cannot be compared is, because spec 5.5
/// counts a spend that is not a number as exhausted, and `budget` and the assignment gate both
/// read it that way: a contract that passed here with such a budget would be refused at
/// assignment instead, after the human had already approved it.
fn exceeds(cost: f64, ceiling: f64) -> bool {
    !matches!(
        cost.partial_cmp(&ceiling),
        Some(Ordering::Less | Ordering::Equal)
    )
}

fn budget_within_sprint(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if exceeds(
        contract.budget.max_cost_usd,
        context.remaining_sprint_budget_usd,
    ) {
        return Some(failure(
            ReadinessRule::BudgetWithinSprint,
            format!(
                "the budget of {} USD exceeds the {} USD left in the sprint",
                contract.budget.max_cost_usd, context.remaining_sprint_budget_usd
            ),
        ));
    }
    None
}

fn reviewer_available(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if contract.reviewer_role == Role::Human {
        if contract.kind == Kind::Epic {
            return None;
        }
        return Some(failure(
            ReadinessRule::ReviewerAvailable,
            "the human reviews only epics; name an agent role as the reviewer of a task"
                .to_string(),
        ));
    }
    let needed = if contract.reviewer_role == contract.assignee_role {
        2
    } else {
        1
    };
    let active = context
        .active_agents_by_role
        .get(&contract.reviewer_role)
        .copied()
        .unwrap_or(0);
    if active < needed {
        return Some(failure(
            ReadinessRule::ReviewerAvailable,
            format!(
                "no agent can review: the team needs {needed} active agent(s) with role {} and has {active}; add an agent with role {}",
                contract.reviewer_role, contract.reviewer_role
            ),
        ));
    }
    None
}

fn out_of_scope_present(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
    if !contract
        .scope
        .out_of_scope
        .iter()
        .any(|item| !item.trim().is_empty())
    {
        return Some(failure(
            ReadinessRule::OutOfScopePresent,
            "scope names no out_of_scope item; say where the task stops".to_string(),
        ));
    }
    None
}

fn is_at_least_ready(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Ready
            | TaskStatus::Assigned
            | TaskStatus::InProgress
            | TaskStatus::Blocked
            | TaskStatus::Verifying
            | TaskStatus::Rejected
            | TaskStatus::Accepted
    )
}

fn dependencies_ready(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    let not_ready: Vec<&str> = contract
        .dependencies
        .iter()
        .map(|dependency| dependency.as_str())
        .filter(|dependency| {
            !context
                .dependency_statuses
                .get(*dependency)
                .is_some_and(|status| is_at_least_ready(*status))
        })
        .collect();
    if not_ready.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::DependenciesReady,
        format!(
            "dependencies {} do not exist or are not yet ready",
            not_ready.join(", ")
        ),
    ))
}

fn required_criteria_present(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    let methods: BTreeSet<&str> = contract
        .exit_criteria
        .iter()
        .map(|criterion| Verification::from(&criterion.verification).method())
        .collect();
    let missing: Vec<&str> = context
        .rules
        .required_criteria
        .iter()
        .map(String::as_str)
        .filter(|method| !methods.contains(method))
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::RequiredCriteriaPresent,
        format!(
            "the team rules require a criterion with method {} and the contract has none",
            missing.join(", ")
        ),
    ))
}

fn new_tests_required_by_rule(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if !context.rules.require_new_tests {
        return None;
    }
    let bad = criterion_ids(contract, |verification, _| {
        matches!(
            verification,
            Verification::Test {
                new_tests_required: false,
                ..
            }
        )
    });
    if bad.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::NewTestsRequiredByRule,
        format!(
            "the team rules require new tests; test criteria {} do not set new_tests_required",
            bad.join(", ")
        ),
    ))
}

/// Splits a glob at its first `*`, `?`, `[`, or `{` into its literal prefix and the rest.
fn split_glob(pattern: &str) -> (&str, &str) {
    let end = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
    pattern.split_at(end)
}

/// Whether a path glob stays under one of the ceiling globs. `**` admits everything. A ceiling
/// that is a directory (`src`, `src/`, `src/**`) admits a path without a wildcard that is that
/// directory or below it, and a path whose literal prefix, cut at its first wildcard, is below
/// it and so ends at a `/`: `src/login/**` is within `src/**`; `src2/**` is not, nor is
/// `src*/**`, whose wildcard runs on into a sibling. Any other ceiling
/// (`docs/**/*.md`, `src/*.rs`, an empty entry, `/`) admits only a path written exactly like it,
/// so that a file filter is never widened and a stray entry never opens the ceiling. A path
/// with a `..` segment is never within a directory. Paths are compared as written: `./src/**`
/// is not `src/**`.
fn is_within_any(path: &str, ceilings: &[String]) -> bool {
    ceilings.iter().any(|ceiling| {
        if ceiling == "**" {
            return true;
        }
        let (prefix, rest) = split_glob(ceiling);
        let directory = prefix.trim_end_matches('/');
        if !matches!(rest, "" | "**") || directory.is_empty() {
            return path == ceiling;
        }
        let (prefix, wildcard) = split_glob(path);
        if prefix.split('/').any(|segment| segment == "..") {
            return false;
        }
        prefix.starts_with(&format!("{directory}/"))
            || (wildcard.is_empty() && prefix.trim_end_matches('/') == directory)
    })
}

fn paths_outside<'a>(paths: &'a [String], ceilings: &[String]) -> Vec<&'a str> {
    paths
        .iter()
        .map(String::as_str)
        .filter(|path| !is_within_any(path, ceilings))
        .collect()
}

fn allowed_paths_within_ceiling(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if context.rules.allowed_paths_ceiling.is_empty() {
        return None;
    }
    let outside = paths_outside(
        &contract.allowed_paths,
        &context.rules.allowed_paths_ceiling,
    );
    if outside.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::AllowedPathsWithinCeiling,
        format!(
            "allowed paths {} reach outside the team's ceiling {}",
            outside.join(", "),
            context.rules.allowed_paths_ceiling.join(", ")
        ),
    ))
}

/// Whether an allowed path stays inside the document globs: within one by the ceiling's
/// containment (`docs/adr/**` within `docs/**`), or, having no wildcard, matched by one as a path
/// (`README.md` by `**/*.md`). A wildcard path within no directory glob could name code.
fn is_a_document_path(path: &String, documents: &[String]) -> bool {
    is_within_any(path, documents)
        || (split_glob(path).1.is_empty()
            && check_allowed_paths(std::slice::from_ref(path), documents).is_ok())
}

fn document_paths_only(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if contract.kind != Kind::Task || contract.assignee_role == Role::SoftwareDeveloper {
        return None;
    }
    let documents = &context.rules.document_paths;
    // Fail closed (5.6): a document glob that does not compile puts every path outside.
    if let Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) =
        check_allowed_paths(&[], documents)
    {
        return Some(failure(
            ReadinessRule::DocumentPathsOnly,
            format!(
                "allowed paths {} cannot be checked: the document path {pattern} does not compile ({detail})",
                contract.allowed_paths.join(", ")
            ),
        ));
    }
    let outside: Vec<&str> = contract
        .allowed_paths
        .iter()
        .filter(|path| !is_a_document_path(path, documents))
        .map(String::as_str)
        .collect();
    if outside.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::DocumentPathsOnly,
        format!(
            "allowed paths {} reach outside the team's document paths {}",
            outside.join(", "),
            context.rules.document_paths.join(", ")
        ),
    ))
}

fn budget_within_team_max(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if contract.kind == Kind::Task
        && let Some(max) = context.rules.max_task_budget_usd
        && exceeds(contract.budget.max_cost_usd, max)
    {
        return Some(failure(
            ReadinessRule::BudgetWithinTeamMax,
            format!(
                "the budget of {} USD exceeds the team's cap of {max} USD on a task",
                contract.budget.max_cost_usd
            ),
        ));
    }
    None
}

fn no_parent_for_epic(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
    if contract.kind == Kind::Epic && contract.parent.is_some() {
        return Some(failure(
            ReadinessRule::NoParentForEpic,
            "an epic cannot have a parent".to_string(),
        ));
    }
    None
}

fn parent_in_progress(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    let Some(parent_id) = &contract.parent else {
        return None;
    };
    match &context.parent {
        None => Some(failure(
            ReadinessRule::ParentInProgress,
            format!("the parent {} is not a known task", parent_id.as_str()),
        )),
        Some(parent) if parent.status != TaskStatus::InProgress => Some(failure(
            ReadinessRule::ParentInProgress,
            format!(
                "the parent {} is {}, not in_progress",
                parent_id.as_str(),
                parent.status
            ),
        )),
        Some(_) => None,
    }
}

fn paths_within_parent(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    contract.parent.as_ref()?;
    let Some(parent) = &context.parent else {
        return None;
    };
    let outside = paths_outside(&contract.allowed_paths, &parent.allowed_paths);
    if outside.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::PathsWithinParent,
        format!(
            "allowed paths {} reach outside the parent's allowed paths {}",
            outside.join(", "),
            parent.allowed_paths.join(", ")
        ),
    ))
}

fn budget_within_parent(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    contract.parent.as_ref()?;
    let Some(parent) = &context.parent else {
        return None;
    };
    if exceeds(contract.budget.max_cost_usd, parent.remaining_budget_usd) {
        return Some(failure(
            ReadinessRule::BudgetWithinParent,
            format!(
                "the budget of {} USD exceeds the {} USD left in the parent's budget",
                contract.budget.max_cost_usd, parent.remaining_budget_usd
            ),
        ));
    }
    None
}

fn judgment_recorded(_: &TaskContract, context: &ReadinessContext) -> Option<ReadinessFailure> {
    if context.requires_judgment_review && context.judgment_review.is_none() {
        return Some(failure(
            ReadinessRule::JudgmentRecorded,
            "the Scrum Master's judgment review is not recorded".to_string(),
        ));
    }
    None
}

fn judgment_fits_budget(_: &TaskContract, context: &ReadinessContext) -> Option<ReadinessFailure> {
    if context.requires_judgment_review
        && let Some(review) = &context.judgment_review
        && !review.fits_budget
    {
        return Some(failure(
            ReadinessRule::JudgmentFitsBudget,
            format!(
                "the Scrum Master judged the task too large for its budget: {}",
                review.reason
            ),
        ));
    }
    None
}

fn judgment_criteria_detect_failure(
    _: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if context.requires_judgment_review
        && let Some(review) = &context.judgment_review
        && !review.criteria_detect_failure
    {
        return Some(failure(
            ReadinessRule::JudgmentCriteriaDetectFailure,
            format!(
                "the Scrum Master judged that the criteria would not detect the failure the intent worries about: {}",
                review.reason
            ),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{a_contract, a_ready_context};
    use super::{
        JudgmentReview, ParentState, ReadinessContext, ReadinessRule as R, TeamRules,
        evaluate_readiness,
    };
    use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire};
    use crate::generated::task_contract::FarikTaskContractKind as Kind;

    fn failed_rules(contract: &TaskContract, context: &ReadinessContext) -> Vec<R> {
        evaluate_readiness(contract, context)
            .expect_err("expected a refusal")
            .iter()
            .map(|failure| failure.rule)
            .collect()
    }

    fn message_of(contract: &TaskContract, context: &ReadinessContext, rule: R) -> String {
        evaluate_readiness(contract, context)
            .expect_err("expected a refusal")
            .into_iter()
            .find(|failure| failure.rule == rule)
            .map(|failure| failure.message)
            .expect("the rule failed")
    }

    fn a_review(fits_budget: bool, criteria_detect_failure: bool) -> JudgmentReview {
        JudgmentReview {
            fits_budget,
            criteria_detect_failure,
            reason: "Two files, one form.".to_string(),
        }
    }

    fn a_task_under(parent: ParentState) -> (TaskContract, ReadinessContext) {
        let mut contract = a_contract();
        contract.parent = Some("FRK-3".parse().expect("a task id"));
        let mut context = a_ready_context();
        context.parent = Some(parent);
        (contract, context)
    }

    fn an_in_progress_parent() -> ParentState {
        ParentState {
            status: TaskStatus::InProgress,
            allowed_paths: vec!["src/**".to_string()],
            remaining_budget_usd: 50.0,
        }
    }

    #[test]
    fn accepts_the_fixture_contract_in_the_fixture_context() {
        assert_eq!(
            evaluate_readiness(&a_contract(), &a_ready_context()),
            Ok(())
        );
    }

    #[test]
    fn refuses_a_blank_intent() {
        let mut contract = a_contract();
        contract.intent = " "
            .repeat(24)
            .parse()
            .expect("twenty-four spaces pass the schema");
        assert_eq!(
            failed_rules(&contract, &a_ready_context()),
            [R::IntentPresent]
        );
    }

    #[test]
    fn refuses_a_contract_without_exit_criteria() {
        let mut contract = a_contract();
        contract.exit_criteria.clear();
        assert_eq!(
            failed_rules(&contract, &a_ready_context()),
            [R::CriteriaPresent]
        );
    }

    #[test]
    fn refuses_a_criterion_whose_method_does_not_match_its_fields() {
        let mut contract = a_contract();
        contract.exit_criteria[0].verification = VerificationWire::Variant4 {
            method: json!("review"),
            question: "Did you sign in?".to_string(),
        };
        assert_eq!(
            failed_rules(&contract, &a_ready_context()),
            [R::CriteriaMethodsValid]
        );
        assert_eq!(
            message_of(&contract, &a_ready_context(), R::CriteriaMethodsValid),
            "criterion C1 names the method review but has the fields of human"
        );
    }

    #[test]
    fn refuses_a_test_criterion_with_a_blank_command() {
        let mut contract = a_contract();
        contract.exit_criteria[0].verification = VerificationWire::Variant1 {
            command: "   ".to_string(),
            method: json!("test"),
            new_tests_required: false,
        };
        assert_eq!(
            failed_rules(&contract, &a_ready_context()),
            [R::CommandCriteriaComplete]
        );
    }

    #[test]
    fn refuses_a_budget_above_the_remaining_sprint_budget() {
        let mut context = a_ready_context();
        context.remaining_sprint_budget_usd = 4.0;
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::BudgetWithinSprint]
        );
    }

    #[test]
    fn fits_any_budget_in_a_sprint_with_no_limit() {
        // ADR 0015: a team with no sprint budget has an infinite one left, however much it spent.
        let mut context = a_ready_context();
        context.remaining_sprint_budget_usd = f64::INFINITY;
        let mut contract = a_contract();
        contract.budget.max_cost_usd = 1000.0;
        assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
    }

    #[test]
    fn refuses_a_budget_that_cannot_be_compared_at_every_ceiling() {
        // Spec 5.5: a figure that cannot be compared counts as exhausted, which `budget` and step
        // 08's assignment gate both read that way. A contract that was ready with such a budget
        // would be refused at assignment instead, after the human had already approved it.
        let (mut contract, mut context) = a_task_under(ParentState {
            allowed_paths: a_contract().allowed_paths.clone(),
            ..an_in_progress_parent()
        });
        contract.budget.max_cost_usd = f64::NAN;
        context.rules.max_task_budget_usd = Some(5.0);
        assert_eq!(
            failed_rules(&contract, &context),
            [
                R::BudgetWithinSprint,
                R::BudgetWithinTeamMax,
                R::BudgetWithinParent
            ]
        );
    }

    #[test]
    fn refuses_a_contract_nobody_can_review_and_names_the_role_to_add() {
        let mut context = a_ready_context();
        context.active_agents_by_role.remove(&Role::Architect);
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::ReviewerAvailable]
        );
        assert!(
            message_of(&a_contract(), &context, R::ReviewerAvailable)
                .ends_with("add an agent with role architect")
        );
    }

    #[test]
    fn lets_the_human_review_an_epic_but_not_a_task() {
        let mut epic = a_contract();
        epic.kind = Kind::Epic;
        epic.reviewer_role = Role::Human;
        let mut context = a_ready_context();
        context.active_agents_by_role.clear();
        assert_eq!(evaluate_readiness(&epic, &context), Ok(()));
        let mut task = a_contract();
        task.reviewer_role = Role::Human;
        assert_eq!(
            failed_rules(&task, &a_ready_context()),
            [R::ReviewerAvailable]
        );
        assert!(
            message_of(&task, &a_ready_context(), R::ReviewerAvailable)
                .starts_with("the human reviews only epics")
        );
    }

    #[test]
    fn needs_two_agents_when_the_reviewer_role_is_the_assignee_role() {
        let mut contract = a_contract();
        contract.reviewer_role = Role::SoftwareDeveloper;
        let mut context = a_ready_context();
        assert_eq!(failed_rules(&contract, &context), [R::ReviewerAvailable]);
        context
            .active_agents_by_role
            .insert(Role::SoftwareDeveloper, 2);
        assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
    }

    #[test]
    fn refuses_a_scope_whose_out_of_scope_items_are_blank() {
        let mut contract = a_contract();
        contract.scope.out_of_scope = vec![" ".to_string()];
        assert_eq!(
            failed_rules(&contract, &a_ready_context()),
            [R::OutOfScopePresent]
        );
    }

    #[test]
    fn refuses_a_dependency_that_is_unknown_or_not_yet_ready() {
        let mut contract = a_contract();
        contract.dependencies = vec!["FRK-2".parse().expect("a task id")];
        let mut context = a_ready_context();
        assert_eq!(failed_rules(&contract, &context), [R::DependenciesReady]);
        context
            .dependency_statuses
            .insert("FRK-2".to_string(), TaskStatus::Refining);
        assert_eq!(failed_rules(&contract, &context), [R::DependenciesReady]);
        context
            .dependency_statuses
            .insert("FRK-2".to_string(), TaskStatus::Ready);
        assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
    }

    #[test]
    fn refuses_a_contract_missing_a_criterion_method_the_team_requires() {
        let mut context = a_ready_context();
        context.rules.required_criteria = vec!["review".to_string()];
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::RequiredCriteriaPresent]
        );
    }

    #[test]
    fn refuses_a_test_criterion_without_new_tests_when_the_team_requires_them() {
        let mut context = a_ready_context();
        context.rules.require_new_tests = true;
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::NewTestsRequiredByRule]
        );
    }

    #[test]
    fn refuses_allowed_paths_outside_the_team_ceiling() {
        let mut context = a_ready_context();
        context.rules.allowed_paths_ceiling = vec!["docs/**".to_string()];
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::AllowedPathsWithinCeiling]
        );
        context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
        assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
        context.rules.allowed_paths_ceiling = vec!["src2/**".to_string()];
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::AllowedPathsWithinCeiling]
        );
        assert!(
            message_of(&a_contract(), &context, R::AllowedPathsWithinCeiling)
                .ends_with("reach outside the team's ceiling src2/**")
        );
        context.rules.allowed_paths_ceiling = vec!["src2/**".to_string(), "**".to_string()];
        assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
    }

    #[test]
    fn treats_an_empty_ceiling_entry_as_exact_rather_than_open() {
        for entry in ["", "/"] {
            let mut context = a_ready_context();
            context.rules.allowed_paths_ceiling = vec![entry.to_string()];
            assert_eq!(
                failed_rules(&a_contract(), &context),
                [R::AllowedPathsWithinCeiling],
                "{entry:?}"
            );
        }
    }

    #[test]
    fn refuses_an_allowed_path_that_climbs_out_with_a_parent_segment() {
        let mut contract = a_contract();
        contract.allowed_paths = vec!["src/../.env".to_string()];
        let mut context = a_ready_context();
        context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
        assert_eq!(
            failed_rules(&contract, &context),
            [R::AllowedPathsWithinCeiling]
        );
    }

    #[test]
    fn keeps_a_ceiling_with_a_file_filter_exact() {
        let mut contract = a_contract();
        contract.allowed_paths = vec!["docs/**".to_string()];
        let mut context = a_ready_context();
        context.rules.allowed_paths_ceiling = vec!["docs/**/*.md".to_string()];
        assert_eq!(
            failed_rules(&contract, &context),
            [R::AllowedPathsWithinCeiling]
        );
        contract.allowed_paths = vec!["docs/**/*.md".to_string()];
        assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
    }

    #[test]
    fn refuses_a_wildcard_that_runs_past_the_directory_name() {
        // Each of these reaches `docsrc/` or `docs-site/`, siblings of `docs`, not inside it.
        for path in ["docs*/**", "docs?/x", "docs{,rc}/**", "docs[x]/**"] {
            let mut contract = a_contract();
            contract.allowed_paths = vec![path.to_string()];
            let mut context = a_ready_context();
            context.rules.allowed_paths_ceiling = vec!["docs/**".to_string()];
            assert_eq!(
                failed_rules(&contract, &context),
                [R::AllowedPathsWithinCeiling],
                "{path} against the ceiling"
            );
            let task = a_task_for(Role::Architect, &[path]);
            assert_eq!(
                failed_rules(&task, &a_ready_context()),
                [R::DocumentPathsOnly],
                "{path} against the document paths"
            );
        }
        for path in ["docs", "docs/**", "docs/*.md"] {
            let task = a_task_for(Role::Architect, &[path]);
            assert_eq!(
                evaluate_readiness(&task, &a_ready_context()),
                Ok(()),
                "{path}"
            );
        }
    }

    /// A task for `role`, reviewed by the Product Manager so that any role may be the assignee,
    /// allowed `paths`.
    fn a_task_for(role: Role, paths: &[&str]) -> TaskContract {
        let mut contract = a_contract();
        contract.assignee_role = role;
        contract.reviewer_role = Role::ProductManager;
        contract.allowed_paths = paths.iter().map(|path| (*path).to_string()).collect();
        contract
    }

    #[test]
    fn keeps_an_architects_task_to_the_document_paths() {
        let task = a_task_for(Role::Architect, &["src/**"]);
        assert_eq!(
            failed_rules(&task, &a_ready_context()),
            [R::DocumentPathsOnly]
        );
        assert!(
            message_of(&task, &a_ready_context(), R::DocumentPathsOnly).contains("src/**"),
            "the message names the path outside"
        );
    }

    #[test]
    fn passes_an_architects_task_inside_them() {
        // `docs/adr/**` is within `docs/**`; `README.md` and `notes/plan.md` have no wildcard and
        // `**/*.md` matches each as a path.
        let task = a_task_for(
            Role::Architect,
            &["docs/adr/**", "README.md", "notes/plan.md"],
        );
        assert_eq!(evaluate_readiness(&task, &a_ready_context()), Ok(()));
    }

    #[test]
    fn refuses_a_wildcard_outside_a_document_directory() {
        // `**/*.md` matches no wildcard path as a path, and `notes/*.md` is within no directory
        // glob: a wildcard could name code.
        let task = a_task_for(Role::Architect, &["notes/*.md"]);
        assert_eq!(
            failed_rules(&task, &a_ready_context()),
            [R::DocumentPathsOnly]
        );
    }

    #[test]
    fn leaves_a_developers_task_to_the_ceiling_alone() {
        let task = a_task_for(Role::SoftwareDeveloper, &["src/**"]);
        assert_eq!(evaluate_readiness(&task, &a_ready_context()), Ok(()));
    }

    #[test]
    fn does_not_hold_an_epic_to_the_document_paths() {
        // An epic's tasks are held, each against its own assignee role.
        let mut epic = a_task_for(Role::Architect, &["src/**"]);
        epic.kind = Kind::Epic;
        assert_eq!(evaluate_readiness(&epic, &a_ready_context()), Ok(()));
    }

    #[test]
    fn refuses_a_document_task_when_a_document_glob_does_not_compile() {
        // Fail closed (5.6): a glob that does not compile makes every path outside, even one a
        // sound glob beside it would contain.
        let task = a_task_for(Role::Architect, &["docs/adr/**"]);
        for globs in [vec!["docs/[**"], vec!["docs/**", "docs/[**"]] {
            let mut context = a_ready_context();
            context.rules.document_paths = globs.iter().map(|glob| (*glob).to_string()).collect();
            assert_eq!(
                failed_rules(&task, &context),
                [R::DocumentPathsOnly],
                "{globs:?}"
            );
            let message = message_of(&task, &context, R::DocumentPathsOnly);
            assert!(message.contains("docs/[** does not compile"), "{message}");
        }
    }

    #[test]
    fn refuses_every_document_task_with_an_empty_list() {
        let task = a_task_for(Role::MarketingSpecialist, &["docs/marketing/**"]);
        let mut context = a_ready_context();
        context.rules.document_paths.clear();
        assert_eq!(failed_rules(&task, &context), [R::DocumentPathsOnly]);
    }

    #[test]
    fn refuses_a_budget_above_the_team_cap_and_accepts_any_budget_without_one() {
        let mut context = a_ready_context();
        context.rules.max_task_budget_usd = Some(4.0);
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::BudgetWithinTeamMax]
        );
        context.rules.max_task_budget_usd = None;
        assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
    }

    #[test]
    fn accepts_any_budget_under_the_default_rules_which_set_no_cap() {
        let mut contract = a_contract();
        contract.budget.max_cost_usd = 12.0;
        let context = a_ready_context();
        assert_eq!(context.rules, TeamRules::default());
        assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
    }

    #[test]
    fn does_not_cap_an_epic_at_the_team_maximum_for_a_task() {
        let mut epic = a_contract();
        epic.kind = Kind::Epic;
        epic.budget.max_cost_usd = 40.0;
        assert_eq!(evaluate_readiness(&epic, &a_ready_context()), Ok(()));
    }

    #[test]
    fn refuses_an_epic_with_a_parent() {
        let (mut contract, context) = a_task_under(an_in_progress_parent());
        contract.kind = Kind::Epic;
        assert_eq!(failed_rules(&contract, &context), [R::NoParentForEpic]);
    }

    #[test]
    fn refuses_a_task_whose_parent_is_unknown_or_not_in_progress() {
        let (contract, mut context) = a_task_under(an_in_progress_parent());
        assert_eq!(evaluate_readiness(&contract, &context), Ok(()));
        context.parent = None;
        assert_eq!(failed_rules(&contract, &context), [R::ParentInProgress]);
        context.parent = Some(ParentState {
            status: TaskStatus::Ready,
            ..an_in_progress_parent()
        });
        assert_eq!(failed_rules(&contract, &context), [R::ParentInProgress]);
    }

    #[test]
    fn refuses_a_task_whose_paths_reach_outside_its_parent() {
        let (contract, context) = a_task_under(ParentState {
            allowed_paths: vec!["docs/**".to_string()],
            ..an_in_progress_parent()
        });
        assert_eq!(failed_rules(&contract, &context), [R::PathsWithinParent]);
    }

    #[test]
    fn refuses_a_task_whose_budget_exceeds_what_its_parent_has_left() {
        let (contract, context) = a_task_under(ParentState {
            remaining_budget_usd: 4.0,
            ..an_in_progress_parent()
        });
        assert_eq!(failed_rules(&contract, &context), [R::BudgetWithinParent]);
    }

    #[test]
    fn requires_the_judgment_review_only_when_the_team_has_a_scrum_master() {
        let mut context = a_ready_context();
        context.judgment_review = None;
        assert_eq!(failed_rules(&a_contract(), &context), [R::JudgmentRecorded]);
        context.requires_judgment_review = false;
        context.active_agents_by_role.remove(&Role::ScrumMaster);
        assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
        context.judgment_review = Some(a_review(false, false));
        assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
    }

    #[test]
    fn refuses_a_judgment_that_the_task_does_not_fit_its_budget() {
        let mut context = a_ready_context();
        context.judgment_review = Some(a_review(false, true));
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::JudgmentFitsBudget]
        );
    }

    #[test]
    fn refuses_a_judgment_that_the_criteria_would_not_detect_the_failure() {
        let mut context = a_ready_context();
        context.judgment_review = Some(a_review(true, false));
        assert_eq!(
            failed_rules(&a_contract(), &context),
            [R::JudgmentCriteriaDetectFailure]
        );
    }

    #[test]
    fn asks_no_judgment_of_a_contract_that_fails_another_rule() {
        let mut contract = a_contract();
        contract.scope.out_of_scope.clear();
        let mut context = a_ready_context();
        context.requires_judgment_review = true;
        context.judgment_review = None;
        assert_eq!(failed_rules(&contract, &context), [R::OutOfScopePresent]);
    }

    #[test]
    fn asks_the_judgment_of_an_otherwise_ready_contract() {
        let mut context = a_ready_context();
        context.requires_judgment_review = true;
        context.judgment_review = None;
        assert_eq!(failed_rules(&a_contract(), &context), [R::JudgmentRecorded]);
    }

    #[test]
    fn reports_every_failure_in_rule_order() {
        let mut contract = a_contract();
        contract.exit_criteria.clear();
        let mut context = a_ready_context();
        context.judgment_review = None;
        context.remaining_sprint_budget_usd = 1.0;
        context.rules.required_criteria = vec!["human".to_string()];
        assert_eq!(
            failed_rules(&contract, &context),
            [
                R::CriteriaPresent,
                R::BudgetWithinSprint,
                R::RequiredCriteriaPresent,
            ]
        );
    }
}
