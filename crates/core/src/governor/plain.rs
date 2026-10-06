//! The governor's rules in plain words, for the person a page shows them to (`docs/SPEC.md`
//! section 5.4). The rule's own message says what to change in the contract; these say what is
//! missing in words a non-technical user reads.

use super::readiness::ReadinessRule;

/// One plain sentence for a rule of the Definition of Ready that a plan fails.
#[must_use]
pub const fn plain_readiness(rule: ReadinessRule) -> &'static str {
    match rule {
        ReadinessRule::IntentPresent => "The plan does not say why the work matters to you.",
        ReadinessRule::SummaryPresent => "The plan has no short summary for you to decide on.",
        ReadinessRule::CriteriaPresent => "The plan does not say how anyone will know it is done.",
        ReadinessRule::CriteriaMethodsValid => {
            "One of the checks for done is written in a way that cannot be run."
        }
        ReadinessRule::CommandCriteriaComplete => {
            "One of the checks for done is missing what to run."
        }
        ReadinessRule::BudgetWithinSprint => {
            "The plan costs more than is left of this sprint's budget."
        }
        ReadinessRule::ReviewerAvailable => {
            "Nobody on the team is free to review the work but the one doing it."
        }
        ReadinessRule::OutOfScopePresent => "The plan does not say what it leaves out.",
        ReadinessRule::DependenciesReady => "Work this plan depends on is not ready yet.",
        ReadinessRule::RequiredCriteriaPresent => {
            "The plan is missing a kind of check your team's rules ask for."
        }
        ReadinessRule::NewTestsRequiredByRule => {
            "Your team's rules ask for new tests, and the plan does not ask for them."
        }
        ReadinessRule::AllowedPathsWithinCeiling => {
            "The plan reaches parts of the project your team keeps it out of."
        }
        ReadinessRule::DocumentPathsOnly => {
            "Only the developer changes code, and this plan lets someone else change it."
        }
        ReadinessRule::MarketingPathsOwned => {
            "Only the Marketing Specialist changes the brand kit and the marketing plans, and this plan lets someone else."
        }
        ReadinessRule::NoFarikPaths => "The plan reaches into Farik's own files.",
        ReadinessRule::PrivateFolderTask => {
            "A finance task works only in the private folder: no commands, no tests, no text searched in a workbook, and no parent epic."
        }
        ReadinessRule::BudgetWithinTeamMax => {
            "The plan costs more than your team allows for one task."
        }
        ReadinessRule::NoParentForEpic => "A large piece of work cannot sit inside another one.",
        ReadinessRule::ParentInProgress => {
            "The larger piece of work this belongs to is not under way."
        }
        ReadinessRule::PathsWithinParent => {
            "The plan reaches beyond the larger piece of work it belongs to."
        }
        ReadinessRule::BudgetWithinParent => {
            "The plan costs more than is left of the larger piece of work's budget."
        }
        ReadinessRule::JudgmentRecorded => "The plan has not been checked yet.",
        ReadinessRule::JudgmentAnswers => "The check found the plan not ready.",
    }
}

#[cfg(test)]
mod tests {
    use super::super::readiness::ReadinessRule::{self, *};
    use super::plain_readiness;

    const EVERY_RULE: [ReadinessRule; 23] = [
        IntentPresent,
        SummaryPresent,
        CriteriaPresent,
        CriteriaMethodsValid,
        CommandCriteriaComplete,
        BudgetWithinSprint,
        ReviewerAvailable,
        OutOfScopePresent,
        DependenciesReady,
        RequiredCriteriaPresent,
        NewTestsRequiredByRule,
        AllowedPathsWithinCeiling,
        DocumentPathsOnly,
        MarketingPathsOwned,
        NoFarikPaths,
        PrivateFolderTask,
        BudgetWithinTeamMax,
        NoParentForEpic,
        ParentInProgress,
        PathsWithinParent,
        BudgetWithinParent,
        JudgmentRecorded,
        JudgmentAnswers,
    ];

    /// Fails to compile when a rule is added and `EVERY_RULE` does not list it.
    const fn listed(rule: ReadinessRule) {
        match rule {
            IntentPresent
            | SummaryPresent
            | CriteriaPresent
            | CriteriaMethodsValid
            | CommandCriteriaComplete
            | BudgetWithinSprint
            | ReviewerAvailable
            | OutOfScopePresent
            | DependenciesReady
            | RequiredCriteriaPresent
            | NewTestsRequiredByRule
            | AllowedPathsWithinCeiling
            | DocumentPathsOnly
            | MarketingPathsOwned
            | NoFarikPaths
            | PrivateFolderTask
            | BudgetWithinTeamMax
            | NoParentForEpic
            | ParentInProgress
            | PathsWithinParent
            | BudgetWithinParent
            | JudgmentRecorded
            | JudgmentAnswers => {}
        }
    }

    #[test]
    fn plain_readiness_covers_every_rule() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in EVERY_RULE {
            listed(rule);
            let plain = plain_readiness(rule);
            assert!(!plain.trim().is_empty(), "{rule:?} has no plain sentence");
            assert!(plain.ends_with('.'), "{rule:?}: {plain}");
            assert!(
                seen.insert(plain),
                "{rule:?} repeats another rule's sentence"
            );
        }
    }
}
