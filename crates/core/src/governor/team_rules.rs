use std::sync::LazyLock;

/// The protected paths every team starts with (`docs/SPEC.md` section 5.6): secrets that no
/// tool may read or write.
pub const DEFAULT_PROTECTED_PATHS: [&str; 5] =
    [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"];

/// The cap on one task's budget in dollars unless the human raises it (project plan D3).
pub const DEFAULT_MAX_TASK_BUDGET_USD: f64 = 5.0;

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
    /// The most a contract's `max_cost_usd` may be; `None` means no cap.
    pub max_task_budget_usd: Option<f64>,
    /// Regular expressions a command must not match.
    pub forbidden_commands: Vec<String>,
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
            max_task_budget_usd: Some(DEFAULT_MAX_TASK_BUDGET_USD),
            forbidden_commands: Vec::new(),
        }
    }
}

/// The default team rules, built once.
pub static DEFAULT_TEAM_RULES: LazyLock<TeamRules> = LazyLock::new(TeamRules::default);

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TEAM_RULES, TeamRules};

    #[test]
    fn protects_the_secret_paths_and_caps_a_task_at_five_dollars_by_default() {
        let rules = TeamRules::default();
        assert_eq!(
            rules.protected_paths,
            [".env", ".env.*", "**/*.pem", "**/*.key", ".farik/local/**"]
        );
        assert_eq!(rules.max_task_budget_usd, Some(5.0));
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
