use std::collections::BTreeMap;
use std::sync::LazyLock;

use super::ReadinessContext;
use crate::contract::fixtures::a_contract_wire;
use crate::contract::{Role, TaskContract, validate_contract};
use crate::governor::team_rules::TeamRules;

static A_CONTRACT: LazyLock<TaskContract> = LazyLock::new(|| {
    validate_contract(&a_contract_wire())
        .expect("the wire fixture is schema-valid: contract::tests pins it")
});

/// The minimal valid contract of `contract::fixtures::a_contract_wire`, typed: a Software
/// Developer's task reviewed by the Architect, one `test` criterion, a five-dollar budget.
#[must_use]
pub fn a_contract() -> TaskContract {
    A_CONTRACT.clone()
}

/// A context in which `a_contract()` is ready: a sprint with a hundred dollars left, one active
/// agent of every launch role, the default team rules, no parent, and no judgment review
/// required.
#[must_use]
pub fn a_ready_context() -> ReadinessContext {
    ReadinessContext {
        remaining_sprint_budget_usd: 100.0,
        dependency_statuses: BTreeMap::new(),
        active_agents_by_role: [
            (Role::ProductManager, 1),
            (Role::ScrumMaster, 1),
            (Role::Architect, 1),
            (Role::SoftwareDeveloper, 1),
            (Role::MarketingSpecialist, 1),
        ]
        .into_iter()
        .collect(),
        parent: None,
        rules: TeamRules::default(),
        requires_judgment_review: false,
        judgment_review: None,
    }
}
