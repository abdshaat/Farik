use std::sync::LazyLock;

use crate::contract::{Role, TaskContract};
use crate::governor::paths::check_protected_paths;

/// The protected paths every team starts with (`docs/SPEC.md` section 5.6): secrets that no
/// tool may read or write.
pub const DEFAULT_PROTECTED_PATHS: [&str; 5] =
    [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"];

/// The document paths every team starts with (`docs/SPEC.md` section 5.12): where a task for any
/// role but the Software Developer may make changes. `team.schema.json` holds the same three as
/// its default; `team.rs` reads that one, and this one serves `TeamRules::default()` alone.
pub const DEFAULT_DOCUMENT_PATHS: [&str; 3] = ["docs/**", "**/*.md", "CHANGELOG.md"];

/// The UI paths every team starts with (`docs/SPEC.md` section 5.12, ADR 0026): a Software
/// Developer's change that touches one is design-reviewed. `team.schema.json` holds the same seven
/// as its default.
pub const DEFAULT_UI_PATHS: [&str; 7] = [
    "**/*.tsx",
    "**/*.jsx",
    "**/*.vue",
    "**/*.svelte",
    "**/*.css",
    "**/*.scss",
    "**/*.html",
];

/// Constraints the human writes once in `.farik/team.yaml` under `rules`, applied by the governor
/// to every contract and every tool call (`docs/SPEC.md` section 5.12). Rules never loosen a
/// permission tier; they only narrow what a granted tier allows.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamRules {
    /// Globs no tool may read or write, whatever its tier.
    pub protected_paths: Vec<String>,
    /// Globs a contract's `allowed_paths` must fall within; empty means no ceiling.
    pub allowed_paths_ceiling: Vec<String>,
    /// Verification methods every contract must have at least one criterion of.
    pub required_criteria: Vec<String>,
    /// Whether every `test` criterion must set `new_tests_required`.
    pub require_new_tests: bool,
    /// The most a contract's `max_cost_usd` may be; `None`, the default, means no cap (ADR 0015).
    pub max_task_budget_usd: Option<f64>,
    /// Regular expressions a command must not match.
    pub forbidden_commands: Vec<String>,
    /// Globs every allowed path of a task not assigned to the Software Developer must fall
    /// within: only the Developer changes code. Empty means no such task can be ready.
    pub document_paths: Vec<String>,
    /// Globs a Software Developer's diff is matched against to decide whether its change touches
    /// the interface (ADR 0026). Empty means only the contract's `ui_change` says so.
    pub ui_paths: Vec<String>,
}

impl Default for TeamRules {
    fn default() -> Self {
        Self {
            protected_paths: DEFAULT_PROTECTED_PATHS
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
            allowed_paths_ceiling: Vec::new(),
            required_criteria: Vec::new(),
            require_new_tests: false,
            max_task_budget_usd: None,
            forbidden_commands: Vec::new(),
            document_paths: DEFAULT_DOCUMENT_PATHS
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
            ui_paths: DEFAULT_UI_PATHS
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
        }
    }
}

/// Whether a task's change touches the interface (ADR 0026): its assignee is a Software Developer,
/// and its contract sets `ui_change` or one of its changed paths matches a UI glob. The paths are
/// the task branch's committed changes against its merge base, never the worktree.
///
/// Matched as the protected paths are (`check_protected_paths`: case-insensitive, normalised), and
/// it fails closed: a glob that does not compile, or a path that climbs, counts as a UI change, so
/// the Designer looks rather than nobody.
#[must_use]
pub fn is_ui_change(
    contract: &TaskContract,
    assignee_role: Role,
    changed_paths: &[String],
    ui_paths: &[String],
) -> bool {
    assignee_role == Role::SoftwareDeveloper
        && (contract.ui_change == Some(true)
            || check_protected_paths(changed_paths, ui_paths).is_err())
}

/// The default team rules, built once.
pub static DEFAULT_TEAM_RULES: LazyLock<TeamRules> = LazyLock::new(TeamRules::default);

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TEAM_RULES, TeamRules, is_ui_change};
    use crate::contract::Role;
    use crate::governor::readiness::fixtures::a_contract;

    #[test]
    fn judges_a_ui_change_by_the_diff_or_the_field() {
        let defaults = TeamRules::default().ui_paths;
        let changed = |path: &str| vec![path.to_string()];
        let plain = a_contract();
        let mut flagged = a_contract();
        flagged.ui_change = Some(true);
        let developer = Role::SoftwareDeveloper;
        assert!(is_ui_change(
            &plain,
            developer,
            &changed("app/Button.tsx"),
            &defaults
        ));
        assert!(!is_ui_change(
            &plain,
            developer,
            &changed("README.md"),
            &defaults
        ));
        assert!(
            is_ui_change(&flagged, developer, &changed("README.md"), &defaults),
            "the contract's field covers what no glob names"
        );
        assert!(
            !is_ui_change(&plain, developer, &changed("app/Button.tsx"), &[]),
            "an empty ui_paths and no field is no UI change"
        );
        // The rule covers only a Developer's task: the Designer's own work and the Marketing
        // Specialist's pages are reviewed as before.
        for role in [Role::UiUxDesigner, Role::MarketingSpecialist] {
            assert!(
                !is_ui_change(&plain, role, &changed("site/index.html"), &defaults),
                "{role}"
            );
            assert!(
                !is_ui_change(&flagged, role, &changed("site/index.html"), &defaults),
                "{role}"
            );
        }
    }

    #[test]
    fn protects_the_secret_paths_and_caps_no_task_by_default() {
        let rules = TeamRules::default();
        assert_eq!(
            rules.protected_paths,
            [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"]
        );
        assert_eq!(rules.max_task_budget_usd, None);
    }

    #[test]
    fn leaves_every_other_rule_empty_or_off_by_default() {
        let rules = TeamRules::default();
        assert!(rules.allowed_paths_ceiling.is_empty());
        assert!(rules.required_criteria.is_empty());
        assert!(!rules.require_new_tests);
        assert!(rules.forbidden_commands.is_empty());
        assert_eq!(*DEFAULT_TEAM_RULES, rules);
    }
}
