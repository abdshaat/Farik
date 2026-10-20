//! The governor's rules in plain words, for the person a page shows them to (`docs/SPEC.md`
//! section 5.4). The rule's own message says what to change in the contract; these say what is
//! missing in words a non-technical user reads.

use super::readiness::ReadinessRule;

/// One plain sentence for a rule of the Definition of Ready that a plan fails.
#[must_use]
pub const fn plain_readiness(rule: ReadinessRule) -> &'static str {
    match rule {
        ReadinessRule::IntentPresent => "The plan does not say why the work matters to you.",
        ReadinessRule::CriteriaPresent => "The plan does not say how anyone will know it is done.",
        ReadinessRule::CriteriaMethodsValid => {
            "One of the checks for done is written in a way that cannot be run."
        }
        ReadinessRule::NoReviewOutsideCode => {
            "This task asks for a review, and only code is reviewed. Check it with a command, a test or a file instead."
        }
        ReadinessRule::CommandCriteriaComplete => {
            "One of the checks for done is missing what to run."
        }
        ReadinessRule::ReviewerAvailable => {
            "Nobody on the team is free to review the work but the one doing it."
        }
        ReadinessRule::AssigneeAvailable => {
            "Nobody on the team has the role this task is for. Add one on the Team page."
        }
        ReadinessRule::RepoNotAllowed => {
            "The plan names a repository where the work does not belong in one."
        }
        ReadinessRule::RepoRequired => {
            "The plan changes code in a project with several repositories and does not say which one."
        }
        ReadinessRule::RepoUnknown => "The plan names a repository this project does not have.",
        ReadinessRule::EpicCommandInProjectFolder => {
            "The epic asks for a command to run, and with several repositories its tasks run them. Move it to the tasks."
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
        ReadinessRule::FolderOwned => {
            "Only a folder's owner changes the documents in it, and this plan lets someone else."
        }
        ReadinessRule::NoCatervasPaths => "The plan reaches into Catervas's own files.",
        ReadinessRule::ProtectedPathAllowed => {
            "The plan asks to change a file your team keeps secret, which no agent may touch."
        }
        ReadinessRule::PrivateFolderTask => {
            "A task in a private folder works only there: no commands, no tests, no text searched in a workbook or a note, and no parent epic."
        }
        ReadinessRule::PrivateFolderReviewer => {
            "A task in a private folder is reviewed by the Product Manager alone, and this plan names someone else."
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
        ReadinessRule::LaneBeyondBuilders => {
            "The plan splits the work into more parallel parts than your team has builders."
        }
        ReadinessRule::LanesOverlap => {
            "Two parts of this work that run side by side could change the same files."
        }
        ReadinessRule::ProductPlanFirst => {
            "A new project plans nothing until the owner approves its product plan."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::readiness::ReadinessRule::{self, *};
    use super::plain_readiness;

    const EVERY_RULE: [ReadinessRule; 30] = [
        IntentPresent,
        CriteriaPresent,
        CriteriaMethodsValid,
        NoReviewOutsideCode,
        CommandCriteriaComplete,
        ReviewerAvailable,
        AssigneeAvailable,
        RepoNotAllowed,
        RepoRequired,
        RepoUnknown,
        EpicCommandInProjectFolder,
        OutOfScopePresent,
        DependenciesReady,
        RequiredCriteriaPresent,
        NewTestsRequiredByRule,
        AllowedPathsWithinCeiling,
        DocumentPathsOnly,
        FolderOwned,
        NoCatervasPaths,
        ProtectedPathAllowed,
        PrivateFolderTask,
        PrivateFolderReviewer,
        BudgetWithinTeamMax,
        NoParentForEpic,
        ParentInProgress,
        PathsWithinParent,
        BudgetWithinParent,
        LaneBeyondBuilders,
        LanesOverlap,
        ProductPlanFirst,
    ];

    /// Fails to compile when a rule is added and `EVERY_RULE` does not list it.
    const fn listed(rule: ReadinessRule) {
        match rule {
            IntentPresent
            | CriteriaPresent
            | CriteriaMethodsValid
            | NoReviewOutsideCode
            | CommandCriteriaComplete
            | ReviewerAvailable
            | AssigneeAvailable
            | RepoNotAllowed
            | RepoRequired
            | RepoUnknown
            | EpicCommandInProjectFolder
            | OutOfScopePresent
            | DependenciesReady
            | RequiredCriteriaPresent
            | NewTestsRequiredByRule
            | AllowedPathsWithinCeiling
            | DocumentPathsOnly
            | FolderOwned
            | NoCatervasPaths
            | ProtectedPathAllowed
            | PrivateFolderTask
            | PrivateFolderReviewer
            | BudgetWithinTeamMax
            | NoParentForEpic
            | ParentInProgress
            | PathsWithinParent
            | BudgetWithinParent
            | LaneBeyondBuilders
            | LanesOverlap
            | ProductPlanFirst => {}
        }
    }

    /// The rule holds the Finance Specialist's tasks and the Procurement Specialist's alike, so its
    /// plain words name neither.
    #[test]
    fn the_private_folder_sentence_names_no_role() {
        let plain = plain_readiness(PrivateFolderTask).to_lowercase();
        for role in ["finance", "procurement"] {
            assert!(!plain.contains(role), "{plain}");
        }
        assert!(plain.contains("private folder"), "{plain}");
    }

    /// The rule holds every role's folder, so its plain words name no role.
    #[test]
    fn the_folder_sentence_names_no_role() {
        let plain = plain_readiness(FolderOwned).to_lowercase();
        for role in ["marketing", "product manager", "architect"] {
            assert!(!plain.contains(role), "{plain}");
        }
        assert!(plain.contains("folder"), "{plain}");
    }

    #[test]
    fn the_product_plan_sentence_is_plain() {
        assert_eq!(
            plain_readiness(ProductPlanFirst),
            "A new project plans nothing until the owner approves its product plan."
        );
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
