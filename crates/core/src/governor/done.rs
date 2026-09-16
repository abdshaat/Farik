//! The Definition of Done (`docs/SPEC.md` section 5.4): what a reviewer must have run, what the
//! diff may touch, what was written down, and when the human must accept, as one function over a
//! contract and the evidence gathered for it.

use crate::contract::{ExitCriterion, TaskContract, wire_method};
use crate::generated::task_contract::FarikTaskContractKind as Kind;
use crate::generated::task_contract::FarikTaskContractRisk as Risk;
use crate::governor::paths::{GlobError, PathRefusal, check_allowed_paths};
use crate::text::{distinct, listed};

/// Who ran a criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunBy {
    /// The agent assigned to the task.
    Assignee,
    /// The agent reviewing the task, in its own session.
    Reviewer,
    /// The user.
    Human,
}

/// One criterion's result, as the runtime recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionResult {
    /// The criterion's id within the contract.
    pub criterion_id: String,
    /// Whether it passed.
    pub passed: bool,
    /// What shows it: a command's output, a file path, a test name.
    pub evidence: String,
    /// Who ran it.
    pub run_by: RunBy,
}

/// Everything the Definition of Done is judged on, gathered by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoneEvidence {
    /// The results recorded in this verification round: the reviewer's own runs and the
    /// human's acceptances. The assignee's earlier run is step 08's gate, not this one's evidence.
    pub results: Vec<CriterionResult>,
    /// The paths the task's diff touches.
    pub changed_paths: Vec<String>,
    /// The assignee's completion note.
    pub completion_note: Option<String>,
    /// The reviewer's note mapping each criterion to its evidence.
    pub review_note: Option<String>,
    /// Whether the human has accepted the task.
    pub human_accepted: bool,
}

/// One rule of the Definition of Done. Failures are reported in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DoneRule {
    /// Every criterion has a result from the reviewer's own run, with evidence.
    CriterionRunByReviewer,
    /// Every criterion passed.
    CriterionPassed,
    /// Every `human` criterion was accepted by the human, not by an agent.
    HumanCriterionAccepted,
    /// The diff touches nothing outside the contract's allowed paths.
    PathsWithinAllowed,
    /// The assignee wrote a completion note.
    CompletionNotePresent,
    /// The reviewer wrote a review note.
    ReviewNotePresent,
    /// The human accepted, which a `high` risk task and every epic require.
    HumanAccepted,
}

/// One rule the task fails, with a message that says what is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneFailure {
    /// The rule that failed.
    pub rule: DoneRule,
    /// What is wrong and what would fix it.
    pub message: String,
}

/// Whether the human must accept the finished result before the task is accepted: the contract's
/// risk is `high`, or it is an epic (`docs/SPEC.md` section 5.4 item 5 and section 5.16 item 4).
/// No team policy touches this.
///
/// This is not the human's approval of the contract before work starts, which spec 5.2's
/// `refining -> escalated` gate asks and the team policy `human_accepts_contracts` widens to every
/// task; step 09 takes that answer from its context rather than from here.
#[must_use]
pub fn requires_human_acceptance(contract: &TaskContract) -> bool {
    contract.risk == Risk::High || contract.kind == Kind::Epic
}

type Check = fn(&TaskContract, &DoneEvidence) -> Option<DoneFailure>;

const CHECKS: [Check; 7] = [
    criterion_run_by_reviewer,
    criterion_passed,
    human_criterion_accepted,
    paths_within_allowed,
    completion_note_present,
    review_note_present,
    human_accepted,
];

/// Checks a task against the Definition of Done (`docs/SPEC.md` section 5.4): every exit
/// criterion run by the reviewer independently and passed, every `human` criterion accepted by
/// the human, no change outside the contract's allowed paths, a completion note, a review note,
/// and the human's acceptance where the risk or the kind requires it. Refuses with every rule the
/// task fails.
///
/// # Errors
///
/// Every failed rule, in the order of `DoneRule`, each with a message that says what is missing.
pub fn evaluate_done(
    contract: &TaskContract,
    evidence: &DoneEvidence,
) -> Result<(), Vec<DoneFailure>> {
    let failures: Vec<DoneFailure> = CHECKS
        .iter()
        .filter_map(|check| check(contract, evidence))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn failure(rule: DoneRule, message: String) -> DoneFailure {
    DoneFailure { rule, message }
}

/// The result the reviewer recorded for a criterion, if any.
/// Whether only the human can answer this criterion (`docs/SPEC.md` section 5.4 item 1).
fn is_answered_by_the_human(criterion: &ExitCriterion) -> bool {
    wire_method(&criterion.verification) == Some("human")
}

fn reviewer_result<'a>(
    evidence: &'a DoneEvidence,
    criterion_id: &str,
) -> Option<&'a CriterionResult> {
    evidence.results.iter().find(|result| {
        result.criterion_id.trim() == criterion_id && result.run_by == RunBy::Reviewer
    })
}

fn criterion_run_by_reviewer(
    contract: &TaskContract,
    evidence: &DoneEvidence,
) -> Option<DoneFailure> {
    let missing: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| !is_answered_by_the_human(criterion))
        .map(|criterion| criterion.id.to_string())
        .filter(|id| {
            reviewer_result(evidence, id).is_none_or(|result| result.evidence.trim().is_empty())
        })
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(failure(
        DoneRule::CriterionRunByReviewer,
        format!(
            "the reviewer's own run recorded no evidence for {}",
            listed("criterion", "criteria", &missing)
        ),
    ))
}

fn criterion_passed(contract: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
    let failed: Vec<String> = contract
        .exit_criteria
        .iter()
        .map(|criterion| criterion.id.to_string())
        .filter(|id| {
            evidence.results.iter().any(|result| {
                result.criterion_id.trim() == id
                    && result.run_by != RunBy::Assignee
                    && !result.passed
            })
        })
        .collect();
    if failed.is_empty() {
        return None;
    }
    Some(failure(
        DoneRule::CriterionPassed,
        format!("{} did not pass", listed("criterion", "criteria", &failed)),
    ))
}

fn human_criterion_accepted(
    contract: &TaskContract,
    evidence: &DoneEvidence,
) -> Option<DoneFailure> {
    let missing: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| is_answered_by_the_human(criterion))
        .map(|criterion| criterion.id.to_string())
        .filter(|id| {
            !evidence.results.iter().any(|result| {
                result.criterion_id.trim() == id && result.passed && result.run_by == RunBy::Human
            })
        })
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(failure(
        DoneRule::HumanCriterionAccepted,
        format!(
            "the human has not answered {}, and only the human can",
            listed("criterion", "criteria", &missing)
        ),
    ))
}

fn paths_within_allowed(contract: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
    match check_allowed_paths(&evidence.changed_paths, &contract.allowed_paths) {
        Ok(()) => None,
        Err(PathRefusal::Violations(violations)) => {
            let changed = distinct(
                &violations
                    .iter()
                    .map(|violation| violation.path.clone())
                    .collect::<Vec<String>>(),
            );
            Some(failure(
                DoneRule::PathsWithinAllowed,
                if contract.allowed_paths.is_empty() {
                    format!("the diff changes {changed}, and the contract allows no path at all")
                } else {
                    format!(
                        "the diff changes {changed} outside the contract's allowed paths {}",
                        distinct(&contract.allowed_paths)
                    )
                },
            ))
        }
        Err(PathRefusal::Glob(GlobError::Invalid { pattern, detail })) => Some(failure(
            DoneRule::PathsWithinAllowed,
            format!("the contract's allowed path {pattern} is not a valid glob: {detail}"),
        )),
    }
}

fn is_written(note: Option<&String>) -> bool {
    note.is_some_and(|text| !text.trim().is_empty())
}

fn completion_note_present(_: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
    if is_written(evidence.completion_note.as_ref()) {
        return None;
    }
    Some(failure(
        DoneRule::CompletionNotePresent,
        "the assignee wrote no completion note: what changed, what was not done, and what the reviewer should look at first".to_string(),
    ))
}

fn review_note_present(_: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
    if is_written(evidence.review_note.as_ref()) {
        return None;
    }
    Some(failure(
        DoneRule::ReviewNotePresent,
        "the reviewer wrote no review note mapping each criterion to its evidence".to_string(),
    ))
}

fn human_accepted(contract: &TaskContract, evidence: &DoneEvidence) -> Option<DoneFailure> {
    if !requires_human_acceptance(contract) || evidence.human_accepted {
        return None;
    }
    let why = if contract.kind == Kind::Epic {
        "an epic"
    } else {
        "a high risk task"
    };
    Some(failure(
        DoneRule::HumanAccepted,
        format!("{why} is accepted by the human, and the human has not accepted"),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CriterionResult, DoneEvidence, DoneRule as R, RunBy, evaluate_done,
        requires_human_acceptance,
    };
    use crate::contract::{ExitCriterion, TaskContract, VerificationWire};
    use crate::generated::task_contract::FarikTaskContractKind as Kind;
    use crate::generated::task_contract::FarikTaskContractRisk as Risk;
    use crate::governor::readiness::fixtures::a_contract;

    fn named_criterion(from: &ExitCriterion, id: &str) -> ExitCriterion {
        let mut criterion = from.clone();
        criterion.id = id.parse().expect("a criterion id");
        criterion
    }

    fn a_result(criterion_id: &str, run_by: RunBy) -> CriterionResult {
        CriterionResult {
            criterion_id: criterion_id.to_string(),
            passed: true,
            evidence: "cargo test: 11 passed".to_string(),
            run_by,
        }
    }

    fn an_evidence() -> DoneEvidence {
        DoneEvidence {
            results: vec![a_result("C1", RunBy::Reviewer)],
            changed_paths: vec!["src/login/form.rs".to_string()],
            completion_note: Some("The form takes an email and a password.".to_string()),
            review_note: Some("C1: cargo test, 11 passed.".to_string()),
            human_accepted: false,
        }
    }

    #[test]
    fn reads_a_recorded_criterion_id_the_same_however_it_is_padded() {
        // Step 08's `check_criteria_recorded` trims a recorded result's criterion id, so this must
        // too: a padded id would otherwise open `in_progress -> verifying` and then block
        // acceptance for good, with nothing to do about it but edit a frozen contract.
        let mut evidence = an_evidence();
        evidence.results = vec![a_result(" C1 ", RunBy::Reviewer)];
        assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
        // A padded id on a failed run is still that criterion failing.
        let mut failed = a_result(" C1 ", RunBy::Reviewer);
        failed.passed = false;
        evidence.results = vec![failed];
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            vec![R::CriterionPassed]
        );
        // And a padded id on the human's acceptance is still that criterion answered.
        let mut contract = a_contract();
        contract.exit_criteria[0].verification = VerificationWire::Variant4 {
            method: serde_json::json!("human"),
            question: "Did you sign in successfully?".to_string(),
        };
        let mut asked = an_evidence();
        asked.results = vec![a_result(" C1 ", RunBy::Human)];
        assert_eq!(evaluate_done(&contract, &asked), Ok(()));
    }

    fn failed_rules(contract: &TaskContract, evidence: &DoneEvidence) -> Vec<R> {
        evaluate_done(contract, evidence)
            .expect_err("expected a refusal")
            .iter()
            .map(|failure| failure.rule)
            .collect()
    }

    fn message_of(contract: &TaskContract, evidence: &DoneEvidence, rule: R) -> String {
        evaluate_done(contract, evidence)
            .expect_err("expected a refusal")
            .into_iter()
            .find(|failure| failure.rule == rule)
            .map(|failure| failure.message)
            .expect("the rule failed")
    }

    #[test]
    fn accepts_a_task_the_reviewer_ran_and_wrote_up() {
        assert_eq!(evaluate_done(&a_contract(), &an_evidence()), Ok(()));
    }

    #[test]
    fn refuses_a_criterion_the_reviewer_did_not_run_itself() {
        let mut evidence = an_evidence();
        evidence.results = vec![a_result("C1", RunBy::Assignee)];
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::CriterionRunByReviewer]
        );
        evidence.results = Vec::new();
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::CriterionRunByReviewer]
        );
        assert_eq!(
            message_of(&a_contract(), &evidence, R::CriterionRunByReviewer),
            "the reviewer's own run recorded no evidence for criterion C1"
        );
    }

    #[test]
    fn refuses_a_reviewer_result_without_evidence() {
        let mut evidence = an_evidence();
        evidence.results[0].evidence = "  ".to_string();
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::CriterionRunByReviewer]
        );
        // A result that is blank and failed breaks the first two rules at once, which is what
        // pins their order against each other.
        evidence.results[0].passed = false;
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::CriterionRunByReviewer, R::CriterionPassed]
        );
    }

    #[test]
    fn refuses_a_criterion_that_did_not_pass() {
        let mut evidence = an_evidence();
        evidence.results[0].passed = false;
        assert_eq!(failed_rules(&a_contract(), &evidence), [R::CriterionPassed]);
        assert_eq!(
            message_of(&a_contract(), &evidence, R::CriterionPassed),
            "criterion C1 did not pass"
        );
    }

    #[test]
    fn accepts_what_the_reviewer_passed_although_the_assignee_had_failed_it() {
        // The independent run is the point: the assignee's earlier failure is step 08's gate, and
        // a reviewer that ran the criterion itself and watched it pass is what 5.4 item 1 asks.
        let mut evidence = an_evidence();
        let mut failed = a_result("C1", RunBy::Assignee);
        failed.passed = false;
        evidence.results.push(failed);
        assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
    }

    #[test]
    fn refuses_a_criterion_whose_id_is_shared_with_a_human_one() {
        // Two criteria called C1, the human one first. Reading a verification back up by id would
        // hand the human exemption to the second and accept a criterion nobody ran.
        let mut contract = a_contract();
        let mut human = contract.exit_criteria[0].clone();
        human.verification = VerificationWire::Variant4 {
            method: json!("human"),
            question: "Did you sign in successfully?".to_string(),
        };
        contract.exit_criteria.insert(0, human);
        let mut evidence = an_evidence();
        evidence.results = vec![a_result("C1", RunBy::Human)];
        assert_eq!(
            failed_rules(&contract, &evidence),
            [R::CriterionRunByReviewer]
        );
    }

    #[test]
    fn refuses_a_diff_when_the_contract_allows_no_path_at_all() {
        // `validate_contract` refuses an empty `allowed_paths` (minItems 1), so this is defence in
        // depth against a runtime that builds a contract itself: every field of TaskContract is
        // public, and the same argument keeps the assignee's results from being trusted.
        let mut contract = a_contract();
        contract.allowed_paths = Vec::new();
        assert_eq!(
            failed_rules(&contract, &an_evidence()),
            [R::PathsWithinAllowed]
        );
        assert_eq!(
            message_of(&contract, &an_evidence(), R::PathsWithinAllowed),
            "the diff changes src/login/form.rs, and the contract allows no path at all"
        );
    }

    #[test]
    fn ignores_a_result_that_names_no_criterion_of_this_contract() {
        // The contract's criteria are the list; a result for anything else is noise from the
        // runtime and decides nothing, including when it failed. The criteria that are in the
        // contract still have to be run and pass, which the other tests pin.
        let mut evidence = an_evidence();
        let mut stray = a_result("C9", RunBy::Reviewer);
        stray.passed = false;
        evidence.results.push(stray);
        assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
    }

    #[test]
    fn refuses_allowed_paths_that_do_not_compile() {
        let mut contract = a_contract();
        contract.allowed_paths = vec!["src/[".to_string()];
        assert_eq!(
            failed_rules(&contract, &an_evidence()),
            [R::PathsWithinAllowed]
        );
        assert_eq!(
            message_of(&contract, &an_evidence(), R::PathsWithinAllowed),
            "the contract's allowed path src/[ is not a valid glob: unclosed character class; missing ']'"
        );
    }

    #[test]
    fn lets_only_the_human_satisfy_a_human_criterion() {
        let mut contract = a_contract();
        contract.exit_criteria[0].verification = VerificationWire::Variant4 {
            method: json!("human"),
            question: "Did you sign in successfully?".to_string(),
        };
        let mut evidence = an_evidence();
        evidence.results = vec![a_result("C1", RunBy::Reviewer)];
        assert_eq!(
            failed_rules(&contract, &evidence),
            [R::HumanCriterionAccepted]
        );
        assert_eq!(
            message_of(&contract, &evidence, R::HumanCriterionAccepted),
            "the human has not answered criterion C1, and only the human can"
        );
        evidence.results = vec![a_result("C1", RunBy::Human)];
        assert_eq!(evaluate_done(&contract, &evidence), Ok(()));
        // The acceptance event is the evidence, so a human answer needs no write-up of its own
        // while a reviewer's run does.
        evidence.results[0].evidence = String::new();
        assert_eq!(evaluate_done(&contract, &evidence), Ok(()));
        // A human answer recorded as a failure refuses too, which is why the passed check reads
        // every result that is not the assignee's rather than only the reviewer's.
        let mut refused = a_result("C1", RunBy::Human);
        refused.passed = false;
        evidence.results = vec![refused];
        assert_eq!(
            failed_rules(&contract, &evidence),
            [R::CriterionPassed, R::HumanCriterionAccepted]
        );
    }

    #[test]
    fn refuses_a_diff_that_reaches_outside_the_allowed_paths() {
        let mut evidence = an_evidence();
        evidence
            .changed_paths
            .push("src/billing/invoice.rs".to_string());
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::PathsWithinAllowed]
        );
        assert_eq!(
            message_of(&a_contract(), &evidence, R::PathsWithinAllowed),
            "the diff changes src/billing/invoice.rs outside the contract's allowed paths src/login/**"
        );
    }

    #[test]
    fn accepts_a_task_that_changed_nothing_outside_its_paths_including_nothing_at_all() {
        let mut evidence = an_evidence();
        evidence.changed_paths = Vec::new();
        assert_eq!(evaluate_done(&a_contract(), &evidence), Ok(()));
    }

    #[test]
    fn refuses_a_missing_or_blank_completion_note() {
        let mut evidence = an_evidence();
        evidence.completion_note = None;
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::CompletionNotePresent]
        );
        evidence.completion_note = Some(" ".to_string());
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::CompletionNotePresent]
        );
        assert_eq!(
            message_of(&a_contract(), &evidence, R::CompletionNotePresent),
            "the assignee wrote no completion note: what changed, what was not done, and what the reviewer should look at first"
        );
    }

    #[test]
    fn refuses_a_missing_or_blank_review_note() {
        let mut evidence = an_evidence();
        evidence.review_note = None;
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::ReviewNotePresent]
        );
        evidence.review_note = Some("\n".to_string());
        assert_eq!(
            failed_rules(&a_contract(), &evidence),
            [R::ReviewNotePresent]
        );
        assert_eq!(
            message_of(&a_contract(), &evidence, R::ReviewNotePresent),
            "the reviewer wrote no review note mapping each criterion to its evidence"
        );
    }

    #[test]
    fn needs_the_human_for_a_high_risk_task_and_for_every_epic() {
        let contract = a_contract();
        assert!(!requires_human_acceptance(&contract));
        let mut risky = a_contract();
        risky.risk = Risk::High;
        assert!(requires_human_acceptance(&risky));
        assert_eq!(failed_rules(&risky, &an_evidence()), [R::HumanAccepted]);
        assert_eq!(
            message_of(&risky, &an_evidence(), R::HumanAccepted),
            "a high risk task is accepted by the human, and the human has not accepted"
        );
        let mut epic = a_contract();
        epic.kind = Kind::Epic;
        assert!(requires_human_acceptance(&epic));
        assert_eq!(failed_rules(&epic, &an_evidence()), [R::HumanAccepted]);
        assert_eq!(
            message_of(&epic, &an_evidence(), R::HumanAccepted),
            "an epic is accepted by the human, and the human has not accepted"
        );
        let mut evidence = an_evidence();
        evidence.human_accepted = true;
        assert_eq!(evaluate_done(&risky, &evidence), Ok(()));
        assert_eq!(evaluate_done(&epic, &evidence), Ok(()));
    }

    #[test]
    fn reports_every_failure_in_rule_order() {
        // All seven at once, so that the order is one assertion rather than a chain of pairs, and
        // every list is plural, so that the singular and the plural wording are both pinned.
        let mut contract = a_contract();
        contract.risk = Risk::High;
        let ran = contract.exit_criteria[0].clone();
        let mut asked = ran.clone();
        asked.verification = VerificationWire::Variant4 {
            method: json!("human"),
            question: "Did you sign in successfully?".to_string(),
        };
        contract.exit_criteria = vec![
            named_criterion(&ran, "C1"),
            named_criterion(&ran, "C2"),
            named_criterion(&asked, "C3"),
            named_criterion(&asked, "C4"),
        ];
        let blank_and_failed = |id: &str| CriterionResult {
            criterion_id: id.to_string(),
            passed: false,
            evidence: "  ".to_string(),
            run_by: RunBy::Reviewer,
        };
        let mut evidence = an_evidence();
        evidence.results = vec![blank_and_failed("C1"), blank_and_failed("C2")];
        evidence.changed_paths = vec!["README.md".to_string(), "Cargo.toml".to_string()];
        evidence.completion_note = None;
        evidence.review_note = None;
        assert_eq!(
            failed_rules(&contract, &evidence),
            [
                R::CriterionRunByReviewer,
                R::CriterionPassed,
                R::HumanCriterionAccepted,
                R::PathsWithinAllowed,
                R::CompletionNotePresent,
                R::ReviewNotePresent,
                R::HumanAccepted
            ]
        );
        assert_eq!(
            message_of(&contract, &evidence, R::CriterionRunByReviewer),
            "the reviewer's own run recorded no evidence for criteria C1, C2"
        );
        assert_eq!(
            message_of(&contract, &evidence, R::CriterionPassed),
            "criteria C1, C2 did not pass"
        );
        assert_eq!(
            message_of(&contract, &evidence, R::HumanCriterionAccepted),
            "the human has not answered criteria C3, C4, and only the human can"
        );
        assert_eq!(
            message_of(&contract, &evidence, R::PathsWithinAllowed),
            "the diff changes README.md, Cargo.toml outside the contract's allowed paths src/login/**"
        );
    }

    #[test]
    fn names_a_repeated_path_or_criterion_once_in_a_message() {
        // A contract built by hand can repeat an id, and a diff can list one path twice; a message
        // that repeats itself reads as two problems where there is one.
        let mut contract = a_contract();
        let one = contract.exit_criteria[0].clone();
        contract.exit_criteria = vec![named_criterion(&one, "C1"), named_criterion(&one, "C1")];
        let mut evidence = an_evidence();
        evidence.results = Vec::new();
        evidence.changed_paths = vec!["README.md".to_string(), "README.md".to_string()];
        assert_eq!(
            message_of(&contract, &evidence, R::CriterionRunByReviewer),
            "the reviewer's own run recorded no evidence for criterion C1"
        );
        assert_eq!(
            message_of(&contract, &evidence, R::PathsWithinAllowed),
            "the diff changes README.md outside the contract's allowed paths src/login/**"
        );
    }
}
