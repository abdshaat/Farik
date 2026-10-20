use std::sync::LazyLock;

use crate::contract::{Role, TaskContract};
use crate::governor::paths::check_protected_paths;

/// The protected paths every team starts with (`docs/SPEC.md` section 5.6): secrets that no
/// tool may read or write.
pub const DEFAULT_PROTECTED_PATHS: [&str; 5] = [
    ".env",
    ".env.*",
    "**/*.pem",
    "**/*.key",
    ".catervas/local/**",
];

/// Catervas's own state (ADR 0080 decision 4): no `write_workspace` call writes a path under it,
/// whatever the owner lifts. It is a constant, never read from the team file.
pub const CATERVAS_STATE: &str = ".catervas/local/**";

/// The daemon's keys (ADR 0080 decision 4): no call reads these, whatever its tier and whatever
/// the owner lifts, because reading the token is writing everything.
pub const CATERVAS_KEYS: [&str; 2] = [".catervas/local/daemon.json", ".catervas/local/sessions/**"];

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

/// Constraints the human writes once in `.catervas/team.yaml` under `rules`, applied by the governor
/// to every contract and every tool call (`docs/SPEC.md` section 5.12). Rules never loosen a
/// permission tier; they only narrow what a granted tier allows.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamRules {
    /// Globs no tool may read or write, whatever its tier.
    pub protected_paths: Vec<String>,
    /// Globs an agent may read and no tool may write (ADR 0080 decision 3).
    pub read_only_paths: Vec<String>,
    /// Git subcommands `catervas_exec` runs on top of the read-only ones (ADR 0080 decision 6).
    pub git_subcommands: Vec<String>,
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
            read_only_paths: Vec::new(),
            git_subcommands: Vec::new(),
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

impl TeamRules {
    /// The globs no tool may write: the protected paths, then the read-only ones, without
    /// repeats.
    #[must_use]
    pub fn unwritable(&self) -> Vec<String> {
        let mut globs = self.protected_paths.clone();
        for glob in &self.read_only_paths {
            if !globs.contains(glob) {
                globs.push(glob.clone());
            }
        }
        globs
    }
}

/// The team's own documents (ADR 0051), which are never a change to the interface (ADR 0069).
pub const TEAM_DOCUMENTS: &str = "docs/catervas/";

/// Whether a task's change touches the interface (ADR 0026): its assignee is a Software Developer,
/// and its contract sets `ui_change` or one of its changed paths matches a UI glob. The paths are
/// the task branch's committed changes against its merge base, never the worktree.
///
/// Matched as the protected paths are (`check_protected_paths`: case-insensitive, normalised), and
/// it fails closed: a glob that does not compile, or a path that climbs, counts as a UI change, so
/// the Designer looks rather than nobody. A path under `docs/catervas/` is a document, not the
/// interface, whatever its extension.
#[must_use]
pub fn is_ui_change(
    contract: &TaskContract,
    assignee_role: Role,
    changed_paths: &[String],
    ui_paths: &[String],
) -> bool {
    let outside_the_documents: Vec<String> = changed_paths
        .iter()
        .filter(|path| {
            !path.starts_with(TEAM_DOCUMENTS) || path.split('/').any(|part| part == "..")
        })
        .cloned()
        .collect();
    assignee_role == Role::SoftwareDeveloper
        && (contract.ui_change == Some(true)
            || check_protected_paths(&outside_the_documents, ui_paths).is_err())
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
        // A team's own documents are never a change to the interface, whatever their extension
        // (ADR 0069), while the field and every other path still count.
        assert!(!is_ui_change(
            &plain,
            developer,
            &changed("docs/catervas/engineering/bug-hunt.html"),
            &defaults
        ));
        assert!(is_ui_change(
            &plain,
            developer,
            &changed("site/index.html"),
            &defaults
        ));
        assert!(is_ui_change(
            &flagged,
            developer,
            &changed("docs/catervas/engineering/bug-hunt.html"),
            &defaults
        ));
        assert!(
            is_ui_change(
                &plain,
                developer,
                &changed("docs/catervas/../../app/Button.tsx"),
                &defaults
            ),
            "a path that climbs out of the documents is still judged"
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
            [
                ".env",
                ".env.*",
                "**/*.pem",
                "**/*.key",
                ".catervas/local/**"
            ]
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
