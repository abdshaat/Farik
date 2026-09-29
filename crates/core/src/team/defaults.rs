//! What putting a setting back puts back (`docs/SPEC.md` section 10): the schema's defaults plus
//! the starter values `farik init` writes, held in one place that `farik init` and the web page
//! both read.

use serde_json::json;

use super::{TeamBudgets, TeamPermissions, TeamPolicy};
use crate::generated::team::Judgment;

/// The plan check's optional third question, which a team may turn on; off by default.
pub const SMALL_ENOUGH_QUESTION: &str = "Is it small enough to finish in one go?";

/// A team's defaults, apart from its name, its agents and its rules (whose defaults are the
/// schema's, filled in by `Team::rules`).
#[derive(Debug, Clone, PartialEq)]
pub struct TeamDefaults {
    /// No daily dollar limit (ADR 0015), and the role's session limits.
    pub budgets: TeamBudgets,
    /// The starter policy, with the plan check and the permissions written out.
    pub policy: TeamPolicy,
}

/// The defaults.
///
/// # Panics
///
/// Never: the literal below is a policy the schema accepts, which `accepts_every_existing_team`
/// proves.
#[must_use]
pub fn defaults() -> TeamDefaults {
    let mut policy: TeamPolicy = serde_json::from_value(json!({
        "human_accepts_contracts": "high_risk",
        "wip_limit_per_agent": 1,
        "blocked_limit_hours": 24,
        "max_iterations": 3,
        "integration": "auto_merge"
    }))
    .expect("the starter policy is one the team schema accepts");
    policy.judgment = Some(Judgment::default());
    policy.permissions = Some(TeamPermissions::default());
    TeamDefaults {
        budgets: TeamBudgets::default(),
        policy,
    }
}
