//! The Definition of Ready (`docs/SPEC.md` section 5.3), the team rules it applies (5.12), and
//! the parent rules of an epic's tasks (5.16), as one function over a contract and a context.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::paths::{
    GlobError, PathRefusal, check_allowed_paths, reaches_the_farik_directory,
    reaches_the_marketing_directory,
};
use super::team_rules::TeamRules;
use crate::contract::{
    Role, TaskContract, TaskStatus, Verification, VerificationWire, wire_method,
};
use crate::generated::task_contract::FarikTaskContractKind as Kind;
use crate::team::{
    changes_code, plain_role, private_file_fault, private_folder, task_private_folder,
};
use crate::text::listed;

/// Builders for readiness contexts and typed contracts, usable by every crate's tests.
pub mod fixtures;

/// One rule of the Definition of Ready. Failures are reported in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReadinessRule {
    /// The intent has words in it, not only whitespace.
    IntentPresent,
    /// A plan the human approves has the summary a human gate leads with: every epic, and every
    /// task the team's `human_accepts_contracts` policy asks the human to approve.
    SummaryPresent,
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
    /// While the team has an active Marketing Specialist, no other role's task names a path that
    /// could reach `docs/marketing/`, which the Marketing Specialist owns.
    MarketingPathsOwned,
    /// No allowed path reaches under `.farik/`, whose files change only through Farik's tools.
    NoFarikPaths,
    /// A task for a role with a private folder works only there: every allowed path lies within
    /// the folder, no criterion is a `command` or a `test`, an `artifact` criterion names a
    /// workbook in the folder and searches no text, and the task has no parent epic.
    PrivateFolderTask,
    /// A task for a role with a private folder is reviewed by the Product Manager alone: the
    /// folder is its role's and the Product Manager's, as the reviewer, and no other role's.
    PrivateFolderReviewer,
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
    /// The judge's review is recorded when the team's policy checks plans.
    JudgmentRecorded,
    /// The judge answered yes to every question it was asked.
    JudgmentAnswers,
}

/// The judge's answer to one question of the plan check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentAnswer {
    /// The question, as it was asked.
    pub question: String,
    /// Whether the plan passes it.
    pub pass: bool,
    /// Why.
    pub reason: String,
}

/// The judge's check of a plan (`docs/SPEC.md` section 5.3), recorded by the runtime as a
/// `contract.judged` event and passed in. It counts under whatever questions it answered: a
/// change to the team's questions does not reopen a judged contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentReview {
    /// One answer per question, in the order asked.
    pub answers: Vec<JudgmentAnswer>,
    /// The judge's overall reason.
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
    /// Whether the team's policy checks plans, and the judge's review is then required.
    pub requires_judgment_review: bool,
    /// The recorded judgment review, when there is one.
    pub judgment_review: Option<JudgmentReview>,
    /// Whether the human approves this plan before work starts, and so reads its summary first:
    /// the team's policy is `all`, or the contract is an epic or a `high` risk task.
    pub human_approves: bool,
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

const CHECKS: [Check; 22] = [
    intent_present,
    summary_present,
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
    marketing_paths_owned,
    no_farik_paths,
    private_folder_task,
    private_folder_reviewer,
    budget_within_team_max,
    no_parent_for_epic,
    parent_in_progress,
    paths_within_parent,
    budget_within_parent,
];

/// The judge's review, asked of a contract only once every other rule passes, so that a
/// contract going back for another rule is not also refused for a judgment nobody asked for yet.
const JUDGMENT_CHECKS: [Check; 2] = [judgment_recorded, judgment_answers];

/// Checks a contract against the Definition of Ready: the structural rules of `docs/SPEC.md`
/// section 5.3, the team rules of 5.12, the parent rules of 5.16, and the judge's
/// recorded judgment when the team checks plans, evaluated only when every other rule passes. Refuses
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

/// How many rules `evaluate_readiness` ran to give `result`: every rule but the judge's, and the
/// judge's two as well once every other passed.
#[must_use]
pub fn rules_evaluated(result: &Result<(), Vec<ReadinessFailure>>) -> usize {
    let judged = result.as_ref().err().is_none_or(|failures| {
        failures.iter().all(|failure| {
            matches!(
                failure.rule,
                ReadinessRule::JudgmentRecorded | ReadinessRule::JudgmentAnswers
            )
        })
    });
    CHECKS.len() + if judged { JUDGMENT_CHECKS.len() } else { 0 }
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

fn summary_present(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    (context.human_approves && contract.summary.is_none()).then(|| {
        failure(
            ReadinessRule::SummaryPresent,
            "the plan has no summary for the user; write two or three plain sentences they can \
             decide on"
                .to_string(),
        )
    })
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
    // A task in a private folder has no worktree to run a command or a test in.
    let in_a_folder = task_private_folder(contract).is_some();
    let missing: Vec<&str> = context
        .rules
        .required_criteria
        .iter()
        .map(String::as_str)
        .filter(|method| !(in_a_folder && matches!(*method, "command" | "test")))
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
/// with a `..` segment anywhere, before or after a wildcard, is within nothing. Paths are compared as written: `./src/**`
/// is not `src/**`.
fn is_within_any(path: &str, ceilings: &[String]) -> bool {
    if path.split('/').any(|segment| segment == "..") {
        return false;
    }
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
    // A task in a private folder is held to its folder by `private_folder_task` instead.
    if context.rules.allowed_paths_ceiling.is_empty() || task_private_folder(contract).is_some() {
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
    if contract.kind != Kind::Task
        || changes_code(contract.assignee_role)
        || task_private_folder(contract).is_some()
    {
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

/// While the team has an active Marketing Specialist, another role's task may not name a path that
/// could reach `docs/marketing/` (5.3, ADR 0042): the brand kit and the marketing plans are the
/// Marketing Specialist's, and everyone else reads them. An epic names a ceiling and is not held.
fn marketing_paths_owned(
    contract: &TaskContract,
    context: &ReadinessContext,
) -> Option<ReadinessFailure> {
    if contract.kind != Kind::Task
        || contract.assignee_role == Role::MarketingSpecialist
        || context
            .active_agents_by_role
            .get(&Role::MarketingSpecialist)
            .is_none_or(|count| *count == 0)
    {
        return None;
    }
    let reaching: Vec<&str> = contract
        .allowed_paths
        .iter()
        .map(String::as_str)
        .filter(|path| reaches_the_marketing_directory(path))
        .collect();
    if reaching.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::MarketingPathsOwned,
        format!(
            "allowed paths {} could reach docs/marketing/, which the Marketing Specialist owns; \
             name narrower paths or give the task to the Marketing Specialist",
            reaching.join(", ")
        ),
    ))
}

/// No allowed path reaches under `.farik/` (5.3), whatever the role or kind: a contract, a
/// decision, a notebook, or the retro changes only through Farik's tools, never through a commit.
/// A path with a backslash is refused outright: the glob engine reads `\` as an escape, so a
/// glob such as `.f\arik/**` reads its first segment as `.f`, missing the directory it in fact
/// matches once escaped. The one exception is a task in a private folder (6.6), which may name
/// paths within its own folder.
fn no_farik_paths(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
    let backslashed: Vec<&str> = contract
        .allowed_paths
        .iter()
        .map(String::as_str)
        .filter(|path| path.contains('\\'))
        .collect();
    // A task in a private folder may name the folder, which lies under `.farik/local/`.
    let folder = task_private_folder(contract);
    let reaching: Vec<&str> = contract
        .allowed_paths
        .iter()
        .map(String::as_str)
        .filter(|path| !path.contains('\\') && reaches_the_farik_directory(path))
        .filter(|path| !folder.is_some_and(|folder| is_within_the_folder(path, folder)))
        .collect();
    if backslashed.is_empty() && reaching.is_empty() {
        return None;
    }
    let mut reasons = Vec::new();
    if !backslashed.is_empty() {
        reasons.push(format!(
            "allowed paths {} contain a backslash: backslashes are not allowed in allowed paths",
            backslashed.join(", ")
        ));
    }
    if !reaching.is_empty() {
        reasons.push(format!(
            "allowed paths {} reach under .farik/, whose files change only through Farik's tools",
            reaching.join(", ")
        ));
    }
    Some(failure(ReadinessRule::NoFarikPaths, reasons.join("; ")))
}

/// Whether an allowed path stays within a private folder, by the ceiling's containment: the folder,
/// a path below it, or a glob whose literal prefix is below it.
fn is_within_the_folder(path: &str, folder: &str) -> bool {
    is_within_any(path, &[folder.to_string()])
}

/// A task for a role with a private folder works only there (5.3, 6.6, 6.10): its allowed paths
/// lie within the folder, it has no `command` or `test` criterion, since it has no worktree to run
/// one in, its `artifact` criteria name files as the folder holds them (workbooks, and notes in the
/// procurement folder) and search no text, since a workbook is not text and a note is checked for
/// existence alone, and it has no parent epic, whose paths could not name `.farik/`.
fn private_folder_task(contract: &TaskContract, _: &ReadinessContext) -> Option<ReadinessFailure> {
    let folder = task_private_folder(contract)?;
    let mut reasons = Vec::new();
    let outside: Vec<&str> = contract
        .allowed_paths
        .iter()
        .map(String::as_str)
        .filter(|path| !is_within_the_folder(path, folder))
        .collect();
    if !outside.is_empty() {
        reasons.push(format!(
            "allowed paths {} lie outside {folder}; name {folder}/** or a file in it",
            outside.join(", ")
        ));
    }
    let runs = criterion_ids(contract, |verification, _| {
        matches!(
            verification,
            Verification::Command { .. } | Verification::Test { .. }
        )
    });
    if !runs.is_empty() {
        reasons.push(format!(
            "{} {} a command or a test, and there is no worktree to run one in; use artifact, \
             review and human criteria",
            listed("criterion", "criteria", &runs),
            if runs.len() == 1 { "is" } else { "are" }
        ));
    }
    let searches = criterion_ids(
        contract,
        |verification, _| matches!(verification, Verification::Artifact { must_contain, .. } if !must_contain.is_empty()),
    );
    if !searches.is_empty() {
        reasons.push(format!(
            "{} {} a workbook for text, and a workbook is not text; drop must_contain",
            listed("criterion", "criteria", &searches),
            if searches.len() == 1 {
                "searches"
            } else {
                "search"
            }
        ));
    }
    for criterion in &contract.exit_criteria {
        let Verification::Artifact { path, .. } = Verification::from(&criterion.verification)
        else {
            continue;
        };
        let why = if path.starts_with(".farik/") {
            let example = if private_folder(Role::ProcurementSpecialist) == Some(folder) {
                "vendors.xlsx"
            } else {
                "books.xlsx"
            };
            Some(format!(
                "is relative to the project, and a path here is relative to {folder}; write \
                 {example}"
            ))
        } else {
            private_file_fault(folder, &path)
        };
        if let Some(why) = why {
            reasons.push(format!(
                "criterion {} names {path}, which {why}",
                criterion.id.as_str()
            ));
        }
    }
    if let Some(parent) = &contract.parent {
        reasons.push(format!(
            "it has a parent epic, {}, whose paths cannot name .farik/, so no task under one is \
             within the folder",
            parent.as_str()
        ));
    }
    if reasons.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::PrivateFolderTask,
        format!(
            "a task for the {} works only in its private folder {folder}: {}",
            plain_role(contract.assignee_role),
            reasons.join("; ")
        ),
    ))
}

/// A task that works in a role's private folder is reviewed by the Product Manager alone (5.3, 6.6,
/// 6.10; the founder's decision of 2026-10-07), so that each folder stays its own role's and, as
/// the reviewer, the Product Manager's: the reviewer's session reads the folder's files, and no
/// other role's does.
fn private_folder_reviewer(
    contract: &TaskContract,
    _: &ReadinessContext,
) -> Option<ReadinessFailure> {
    let folder = task_private_folder(contract)?;
    (contract.reviewer_role != Role::ProductManager).then(|| {
        failure(
            ReadinessRule::PrivateFolderReviewer,
            format!(
                "a task in the private folder {folder} is reviewed by the Product Manager alone, \
                 and this one names the {} as its reviewer; name product_manager as its \
                 reviewer_role",
                plain_role(contract.reviewer_role)
            ),
        )
    })
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
            "the judge's judgment review is not recorded".to_string(),
        ));
    }
    None
}

fn judgment_answers(_: &TaskContract, context: &ReadinessContext) -> Option<ReadinessFailure> {
    let review = context
        .judgment_review
        .as_ref()
        .filter(|_| context.requires_judgment_review)?;
    let failed: Vec<String> = review
        .answers
        .iter()
        .filter(|answer| !answer.pass)
        .map(|answer| format!("{} ({})", answer.question, answer.reason))
        .collect();
    if failed.is_empty() {
        return None;
    }
    Some(failure(
        ReadinessRule::JudgmentAnswers,
        format!(
            "the judge answered no: {}. {}",
            failed.join("; "),
            review.reason
        ),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{a_contract, a_ready_context};
    use super::{
        JudgmentAnswer, JudgmentReview, ParentState, ReadinessContext, ReadinessRule as R,
        TeamRules, evaluate_readiness,
    };
    use crate::contract::fixtures::a_contract_wire;
    use crate::contract::{Role, TaskContract, TaskStatus, VerificationWire, validate_contract};
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
        let answer = |question: &str, pass: bool| JudgmentAnswer {
            question: question.to_string(),
            pass,
            reason: format!("{question} {pass}"),
        };
        JudgmentReview {
            answers: vec![
                answer("Fits?", fits_budget),
                answer("Caught?", criteria_detect_failure),
            ],
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
    fn refuses_a_parent_segment_after_the_first_wildcard() {
        let mut contract = a_contract();
        contract.allowed_paths = vec!["src/**/../../.env".to_string()];
        let mut context = a_ready_context();
        context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
        assert_eq!(
            failed_rules(&contract, &context),
            [R::AllowedPathsWithinCeiling]
        );
    }

    #[test]
    fn refuses_allowed_paths_that_reach_under_the_farik_directory() {
        // Each names a path under `.farik/` or could match one; every role and kind is held.
        let reaching = [
            ".farik/decisions/0001-x.md",
            ".farik/**",
            ".farik",
            "./.farik/team.yaml",
            ".FARIK/team/retro.md",
            "**",
            "**/*.md",
            "*/memory.md",
            ".f*/x",
            "[.]farik/x",
            "{src,.farik}/**",
            ".f\\arik/**",
            "a\\b",
        ];
        for path in reaching {
            for role in [Role::SoftwareDeveloper, Role::Architect] {
                let mut task = a_task_for(role, &["docs/x.md", path]);
                assert!(
                    failed_rules(&task, &a_context_without_marketing()).contains(&R::NoFarikPaths),
                    "{path} for {role:?}"
                );
                task.kind = Kind::Epic;
                assert!(
                    failed_rules(&task, &a_context_without_marketing()).contains(&R::NoFarikPaths),
                    "{path} for an epic"
                );
            }
        }
        let task = a_task_for(Role::Architect, &["**/*.md"]);
        assert_eq!(
            failed_rules(&task, &a_context_without_marketing()),
            [R::NoFarikPaths]
        );
        assert!(
            message_of(&task, &a_context_without_marketing(), R::NoFarikPaths).contains("**/*.md"),
            "the message names the path"
        );
        for path in [
            "src/**",
            "*.md",
            "*",
            "docs/**",
            ".farikx/**",
            ".github/**",
            "src/.farik/x",
        ] {
            let task = a_task_for(Role::SoftwareDeveloper, &[path]);
            assert_eq!(
                evaluate_readiness(&task, &a_context_without_marketing()),
                Ok(()),
                "{path}"
            );
        }
    }

    #[test]
    fn keeps_a_ceiling_with_a_file_filter_exact() {
        let mut contract = a_contract();
        contract.allowed_paths = vec!["docs/**".to_string()];
        let mut context = a_context_without_marketing();
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
            let mut context = a_context_without_marketing();
            context.rules.allowed_paths_ceiling = vec!["docs/**".to_string()];
            assert_eq!(
                failed_rules(&contract, &context),
                [R::AllowedPathsWithinCeiling],
                "{path} against the ceiling"
            );
            let task = a_task_for(Role::Architect, &[path]);
            assert_eq!(
                failed_rules(&task, &a_context_without_marketing()),
                [R::DocumentPathsOnly],
                "{path} against the document paths"
            );
        }
        for path in ["docs", "docs/**", "docs/*.md"] {
            let task = a_task_for(Role::Architect, &[path]);
            assert_eq!(
                evaluate_readiness(&task, &a_context_without_marketing()),
                Ok(()),
                "{path}"
            );
        }
    }

    /// The ready context with no Marketing Specialist, for the tests of the rules about paths
    /// that name `docs/` or everything: `marketing_paths_owned` would hold those paths too.
    fn a_context_without_marketing() -> ReadinessContext {
        let mut context = a_ready_context();
        context
            .active_agents_by_role
            .remove(&Role::MarketingSpecialist);
        context
    }

    #[test]
    fn another_role_may_not_name_the_marketing_folder() {
        let failed = |task: &TaskContract, context: &ReadinessContext| -> Vec<R> {
            match evaluate_readiness(task, context) {
                Ok(()) => Vec::new(),
                Err(failures) => failures.iter().map(|failure| failure.rule).collect(),
            }
        };
        // `a_ready_context()` has one active Marketing Specialist.
        let mut with_marketing = a_task_for(Role::ProductManager, &["docs/adr/**", "docs/**"]);
        with_marketing.reviewer_role = Role::Architect;
        assert_eq!(
            failed(&with_marketing, &a_ready_context()),
            [R::MarketingPathsOwned]
        );
        let message = message_of(&with_marketing, &a_ready_context(), R::MarketingPathsOwned);
        assert_eq!(
            message,
            "allowed paths docs/** could reach docs/marketing/, which the Marketing Specialist \
             owns; name narrower paths or give the task to the Marketing Specialist"
        );
        // The same task passes once nobody on the team is a Marketing Specialist, or one is paused.
        for count in [None, Some(0)] {
            let mut context = a_ready_context();
            context
                .active_agents_by_role
                .remove(&Role::MarketingSpecialist);
            if let Some(count) = count {
                context
                    .active_agents_by_role
                    .insert(Role::MarketingSpecialist, count);
            }
            assert_eq!(
                evaluate_readiness(&with_marketing, &context),
                Ok(()),
                "{count:?}"
            );
        }
        // Any other role is held, and a narrower path passes.
        for role in [Role::Architect, Role::SoftwareDeveloper] {
            let task = a_task_for(role, &["docs/**"]);
            assert_eq!(
                failed(&task, &a_ready_context()),
                [R::MarketingPathsOwned],
                "{role:?}"
            );
            let task = a_task_for(role, &["docs/adr/**"]);
            assert_eq!(failed(&task, &a_ready_context()), [], "{role:?}");
        }
        // The Marketing Specialist's own task may name its folder.
        let own = a_task_for(
            Role::MarketingSpecialist,
            &["docs/marketing/**", "CHANGELOG.md"],
        );
        assert_eq!(evaluate_readiness(&own, &a_ready_context()), Ok(()));
        // An epic is not held: it names the ceiling its tasks fall within.
        let mut epic = a_task_for(Role::Architect, &["docs/**"]);
        epic.kind = Kind::Epic;
        assert!(!failed(&epic, &a_ready_context()).contains(&R::MarketingPathsOwned));
    }

    /// A task for the Finance Specialist, reviewed by the Product Manager, allowed `paths`, with
    /// exactly `criteria` (wire values).
    fn a_finance_task(paths: &[&str], criteria: serde_json::Value) -> TaskContract {
        let mut wire = a_contract_wire();
        wire["assignee_role"] = json!("finance_specialist");
        wire["reviewer_role"] = json!("product_manager");
        wire["allowed_paths"] = json!(paths);
        wire["exit_criteria"] = criteria;
        validate_contract(&wire).expect("a schema-valid contract")
    }

    /// The criteria of a finance task that is ready: its books exist, a reviewer reads them, and
    /// the user agrees.
    fn the_books_criteria() -> serde_json::Value {
        json!([
            { "id": "C1", "text": "The books exist.",
              "verification": { "method": "artifact", "path": "books.xlsx" } },
            { "id": "C2", "text": "Every number names its source.",
              "verification": { "method": "review", "rubric": ["Does every number name its source?"] } },
            { "id": "C3", "text": "The books look right to the user.",
              "verification": { "method": "human", "question": "Do the books look right?" } }
        ])
    }

    #[test]
    fn a_finance_task_is_ready_in_its_folder() {
        let task = a_finance_task(&[".farik/local/finance/**"], the_books_criteria());
        assert_eq!(evaluate_readiness(&task, &a_ready_context()), Ok(()));
        // A path to one file in the folder, and the folder itself, are within it too.
        let task = a_finance_task(
            &[".farik/local/finance/books.xlsx", ".farik/local/finance"],
            the_books_criteria(),
        );
        assert_eq!(evaluate_readiness(&task, &a_ready_context()), Ok(()));
    }

    /// A task for the Procurement Specialist, reviewed by the Product Manager, allowed `paths`,
    /// with exactly `criteria` (wire values).
    fn a_procurement_task(paths: &[&str], criteria: serde_json::Value) -> TaskContract {
        let mut wire = a_contract_wire();
        wire["assignee_role"] = json!("procurement_specialist");
        wire["reviewer_role"] = json!("product_manager");
        wire["allowed_paths"] = json!(paths);
        wire["exit_criteria"] = criteria;
        validate_contract(&wire).expect("a schema-valid contract")
    }

    #[test]
    fn a_procurement_task_is_ready_without_a_branch() {
        let evaluation = json!([
            { "id": "C1", "text": "The comparison is written.",
              "verification": { "method": "artifact", "path": "evaluations/email-sending.md" } },
            { "id": "C2", "text": "Every price names its source.",
              "verification": { "method": "review", "rubric": ["Does every price name its source?"] } },
            { "id": "C3", "text": "The recommendation is clear to the founder.",
              "verification": { "method": "human", "question": "Is the recommendation clear?" } }
        ]);
        let folder = &[".farik/local/procurement/**"];
        let task = a_procurement_task(folder, evaluation.clone());
        assert_eq!(evaluate_readiness(&task, &a_ready_context()), Ok(()));
        // The register is a workbook in the folder, and a note may sit a folder down.
        let both = a_procurement_task(
            folder,
            json!([
                evaluation[0].clone(), evaluation[1].clone(), evaluation[2].clone(),
                { "id": "C4", "text": "The register is kept.",
                  "verification": { "method": "artifact", "path": "vendors.xlsx" } }
            ]),
        );
        assert_eq!(evaluate_readiness(&both, &a_ready_context()), Ok(()));
        // A command criterion has no worktree to run in.
        let runs = a_procurement_task(
            folder,
            json!([
                evaluation[0].clone(), evaluation[1].clone(), evaluation[2].clone(),
                { "id": "C4", "text": "It builds.",
                  "verification": { "method": "command", "command": "make", "expect": { "exit_code": 0 } } }
            ]),
        );
        let context = a_context_without_marketing();
        assert_eq!(failed_rules(&runs, &context), [R::PrivateFolderTask]);
        assert!(
            message_of(&runs, &context, R::PrivateFolderTask)
                .contains("criterion C4 is a command or a test")
        );
        // A note is not searched for text, as a workbook is not.
        let searched = a_procurement_task(
            folder,
            json!([
                { "id": "C1", "text": "The comparison names a seller.",
                  "verification": { "method": "artifact", "path": "evaluations/email-sending.md",
                                    "must_contain": ["seller"] } },
                evaluation[1].clone(), evaluation[2].clone()
            ]),
        );
        assert_eq!(failed_rules(&searched, &context), [R::PrivateFolderTask]);
        // The finance folder is not its own.
        let finance = a_procurement_task(&[".farik/local/finance/**"], evaluation.clone());
        assert_eq!(
            failed_rules(&finance, &context),
            [R::NoFarikPaths, R::PrivateFolderTask]
        );
        assert!(
            message_of(&finance, &context, R::PrivateFolderTask).contains(
                "allowed paths .farik/local/finance/** lie outside .farik/local/procurement"
            ),
        );
        // A path that is neither a workbook nor a note, and one that starts at the project.
        for (path, said) in [
            ("evaluations/x.txt", "is not a workbook or a note"),
            (
                ".farik/local/procurement/vendors.xlsx",
                "is relative to the project",
            ),
        ] {
            let task = a_procurement_task(
                folder,
                json!([
                    { "id": "C1", "text": "It exists.",
                      "verification": { "method": "artifact", "path": path } },
                    evaluation[1].clone(), evaluation[2].clone()
                ]),
            );
            assert_eq!(
                failed_rules(&task, &context),
                [R::PrivateFolderTask],
                "{path}"
            );
            let message = message_of(&task, &context, R::PrivateFolderTask);
            assert!(message.contains(said), "{path}: {message}");
        }
    }

    /// The founder's decision of 2026-10-07 ("Only the Product Manager"): a task that works in a
    /// role's private folder is reviewed by the Product Manager, whatever the role, so that each
    /// folder stays its own role's and, as the reviewer, the Product Manager's.
    #[test]
    fn a_private_folder_task_is_reviewed_by_the_product_manager() {
        // Both folders' roles are on the team, so that a reviewer of either is available and the
        // new rule is the only one that can refuse.
        let mut context = a_ready_context();
        for role in [Role::FinanceSpecialist, Role::ProcurementSpecialist] {
            context.active_agents_by_role.insert(role, 1);
        }
        let books = a_finance_task(&[".farik/local/finance/**"], the_books_criteria());
        let comparison = a_procurement_task(
            &[".farik/local/procurement/**"],
            json!([
                { "id": "C1", "text": "The comparison is written.",
                  "verification": { "method": "artifact", "path": "evaluations/email-sending.md" } },
                { "id": "C2", "text": "Every price names its source.",
                  "verification": { "method": "review", "rubric": ["Does every price name its source?"] } },
                { "id": "C3", "text": "The recommendation is clear to the founder.",
                  "verification": { "method": "human", "question": "Is the recommendation clear?" } }
            ]),
        );
        // The Product Manager is accepted, for each folder.
        assert_eq!(evaluate_readiness(&books, &context), Ok(()));
        assert_eq!(evaluate_readiness(&comparison, &context), Ok(()));
        // The other folder's role is refused, and so is any other role that could be available.
        for (task, reviewer, folder) in [
            (&books, Role::ProcurementSpecialist, ".farik/local/finance"),
            (
                &comparison,
                Role::FinanceSpecialist,
                ".farik/local/procurement",
            ),
            (&books, Role::Architect, ".farik/local/finance"),
            (
                &comparison,
                Role::SoftwareDeveloper,
                ".farik/local/procurement",
            ),
        ] {
            let mut task = task.clone();
            task.reviewer_role = reviewer;
            assert_eq!(
                failed_rules(&task, &context),
                [R::PrivateFolderReviewer],
                "{reviewer}"
            );
            let message = message_of(&task, &context, R::PrivateFolderReviewer);
            assert!(
                message.contains(folder) && message.contains("product_manager"),
                "{reviewer}: {message}"
            );
        }
        // An epic assigned to such a role works in no folder, and is not held to the rule.
        let mut epic = books.clone();
        epic.kind = Kind::Epic;
        epic.reviewer_role = Role::Human;
        assert!(!failed_rules(&epic, &context).contains(&R::PrivateFolderReviewer));
    }

    #[test]
    fn a_finance_task_may_not_run_or_test() {
        let books = || the_books_criteria()[0].clone();
        let with =
            |extra: serde_json::Value| json!([books(), the_books_criteria()[1].clone(), extra]);
        let folder = &[".farik/local/finance/**"];
        // Each case: the task, and what its message says. Each fails this rule alone.
        let cases = [
            (
                a_finance_task(
                    folder,
                    with(json!({ "id": "C4", "text": "The books build.",
                        "verification": { "method": "command", "command": "make books",
                                          "expect": { "exit_code": 0 } } })),
                ),
                "criterion C4 is a command or a test",
            ),
            (
                a_finance_task(
                    folder,
                    with(json!({ "id": "C4", "text": "The checks pass.",
                        "verification": { "method": "test", "command": "make check" } })),
                ),
                "criterion C4 is a command or a test",
            ),
            (
                a_finance_task(
                    folder,
                    json!([
                        { "id": "C1", "text": "The books say total.",
                          "verification": { "method": "artifact", "path": "books.xlsx",
                                            "must_contain": ["total"] } },
                        the_books_criteria()[1].clone()
                    ]),
                ),
                "criterion C1 searches a workbook for text",
            ),
            (
                a_finance_task(
                    folder,
                    json!([
                        { "id": "C1", "text": "The books exist.",
                          "verification": { "method": "artifact",
                                            "path": ".farik/local/finance/books.xlsx" } },
                        the_books_criteria()[1].clone()
                    ]),
                ),
                "criterion C1 names .farik/local/finance/books.xlsx",
            ),
            (
                a_finance_task(
                    folder,
                    json!([
                        { "id": "C1", "text": "The notes exist.",
                          "verification": { "method": "artifact", "path": "notes.txt" } },
                        the_books_criteria()[1].clone()
                    ]),
                ),
                "criterion C1 names notes.txt",
            ),
            (
                a_finance_task(&["src/**"], the_books_criteria()),
                "allowed paths src/** lie outside .farik/local/finance",
            ),
            (
                a_finance_task(
                    &[
                        ".farik/local/finance/**",
                        ".farik/local/financeX/**",
                        "docs/**",
                    ],
                    the_books_criteria(),
                ),
                "allowed paths .farik/local/financeX/**, docs/** lie outside .farik/local/finance",
            ),
        ];
        for (task, said) in cases {
            let context = a_context_without_marketing();
            let failed = failed_rules(&task, &context);
            // `.farik/local/financeX/**` is also under `.farik/`.
            let wanted: &[R] = if said.contains("financeX") {
                &[R::NoFarikPaths, R::PrivateFolderTask]
            } else {
                &[R::PrivateFolderTask]
            };
            assert_eq!(failed, wanted, "{said}");
            let message = message_of(&task, &context, R::PrivateFolderTask);
            assert!(message.contains(said), "{said}: {message}");
        }
        // A parent epic is refused: an epic's paths cannot name `.farik/`, so no task under one is
        // within the folder.
        let mut under_an_epic = a_finance_task(folder, the_books_criteria());
        under_an_epic.parent = Some("FRK-3".parse().expect("a task id"));
        let mut context = a_context_without_marketing();
        context.parent = Some(ParentState {
            status: TaskStatus::InProgress,
            allowed_paths: vec!["**".to_string()],
            remaining_budget_usd: 50.0,
        });
        assert_eq!(
            failed_rules(&under_an_epic, &context),
            [R::PrivateFolderTask]
        );
        assert!(
            message_of(&under_an_epic, &context, R::PrivateFolderTask)
                .contains("it has a parent epic, FRK-3"),
        );
    }

    #[test]
    fn a_team_requiring_tests_still_readies_a_finance_task() {
        let task = a_finance_task(&[".farik/local/finance/**"], the_books_criteria());
        let mut context = a_ready_context();
        context.rules.required_criteria = vec!["command".to_string(), "test".to_string()];
        context.rules.require_new_tests = true;
        assert_eq!(evaluate_readiness(&task, &context), Ok(()));
        // The exemption is for those two methods alone: a team that asks for a human or a review
        // criterion, or any other, still gets that.
        context.rules.required_criteria = vec!["command".to_string(), "review".to_string()];
        assert_eq!(evaluate_readiness(&task, &context), Ok(()));
        let artifact_only = a_finance_task(
            &[".farik/local/finance/**"],
            json!([the_books_criteria()[0].clone()]),
        );
        assert_eq!(
            failed_rules(&artifact_only, &context),
            [R::RequiredCriteriaPresent]
        );
        assert_eq!(
            message_of(&artifact_only, &context, R::RequiredCriteriaPresent),
            "the team rules require a criterion with method review and the contract has none"
        );
    }

    #[test]
    fn the_ceiling_does_not_hold_the_folder() {
        let task = a_finance_task(&[".farik/local/finance/**"], the_books_criteria());
        let mut context = a_ready_context();
        context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
        assert_eq!(evaluate_readiness(&task, &context), Ok(()));
        // Nor do the document paths: a finance task's paths are not documents.
        context.rules.document_paths = vec!["docs/**".to_string()];
        assert_eq!(evaluate_readiness(&task, &context), Ok(()));
        // Another role's task is held to both.
        let developers = a_task_for(Role::SoftwareDeveloper, &["docs/**"]);
        assert_eq!(
            failed_rules(&developers, &a_context_without_marketing_under_a_ceiling()),
            [R::AllowedPathsWithinCeiling]
        );
    }

    fn a_context_without_marketing_under_a_ceiling() -> ReadinessContext {
        let mut context = a_context_without_marketing();
        context.rules.allowed_paths_ceiling = vec!["src/**".to_string()];
        context
    }

    #[test]
    fn another_roles_task_still_may_not_name_farik() {
        // The folder is a finance task's own: a Developer's, an Architect's and a Marketing
        // Specialist's task naming it, or an epic whose assignee role is finance, is refused.
        for role in [
            Role::SoftwareDeveloper,
            Role::Architect,
            Role::MarketingSpecialist,
        ] {
            let task = a_task_for(role, &[".farik/local/finance/**"]);
            assert!(
                failed_rules(&task, &a_context_without_marketing()).contains(&R::NoFarikPaths),
                "{role:?}"
            );
        }
        let mut epic = a_finance_task(&[".farik/local/finance/**"], the_books_criteria());
        epic.kind = Kind::Epic;
        assert!(failed_rules(&epic, &a_ready_context()).contains(&R::NoFarikPaths));
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
    fn holds_neither_the_developer_nor_the_designer_to_the_document_paths() {
        let mut context = a_ready_context();
        context.active_agents_by_role.insert(Role::UiUxDesigner, 1);
        for role in [Role::SoftwareDeveloper, Role::UiUxDesigner] {
            let task = a_task_for(role, &["src/**"]);
            assert_eq!(evaluate_readiness(&task, &context), Ok(()), "{role}");
        }
        let task = a_task_for(Role::MarketingSpecialist, &["src/**"]);
        assert_eq!(failed_rules(&task, &context), [R::DocumentPathsOnly]);
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
    fn requires_the_judgment_review_only_when_the_policy_asks_for_it() {
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
    fn refuses_a_judgment_with_a_failed_answer_naming_each_one() {
        let mut context = a_ready_context();
        context.judgment_review = Some(a_review(false, true));
        assert_eq!(
            message_of(&a_contract(), &context, R::JudgmentAnswers),
            "the judge answered no: Fits? (Fits? false). Two files, one form."
        );
        context.judgment_review = Some(a_review(false, false));
        assert_eq!(
            message_of(&a_contract(), &context, R::JudgmentAnswers),
            "the judge answered no: Fits? (Fits? false); Caught? (Caught? false). Two files, \
             one form."
        );
        context.judgment_review = Some(a_review(true, true));
        assert_eq!(evaluate_readiness(&a_contract(), &context), Ok(()));
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
