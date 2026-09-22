//! Which role reviews a task (D7, `docs/SPEC.md` section 5.1).

use farik_core::contract::{Role, TaskKind};
use farik_core::team::Team;

/// The roles that review a task, in order of preference, by its assignee's role: a Developer's
/// work to the Architect, then another Developer; an Architect's and a Marketing Specialist's to
/// the Product Manager. The Product Manager's and the Scrum Master's own tasks have no row until
/// phase 4 decides them.
pub const REVIEWER_ROLE_FOR: &[(Role, &[Role])] = &[
    (
        Role::SoftwareDeveloper,
        &[Role::Architect, Role::SoftwareDeveloper],
    ),
    (Role::Architect, &[Role::ProductManager]),
    (Role::MarketingSpecialist, &[Role::ProductManager]),
];

/// The `reviewer_role` the contract's writer fills in for a task whose assignee will hold
/// `assignee_role`: the first preferred role the team can staff, counted as the Definition of
/// Ready's `ReviewerAvailable` counts (two active agents when the reviewer's role is the
/// assignee's, one otherwise).
///
/// `None` when the team can staff none of them, when the assignee's role has no row, and for an
/// epic, whose reviewer the assignment gate decides.
#[must_use]
pub fn default_reviewer_role(team: &Team, kind: TaskKind, assignee_role: Role) -> Option<Role> {
    if kind == TaskKind::Epic {
        return None;
    }
    let (_, preferred) = REVIEWER_ROLE_FOR
        .iter()
        .find(|(assignee, _)| *assignee == assignee_role)?;
    preferred.iter().copied().find(|&reviewer| {
        let needed = if reviewer == assignee_role { 2 } else { 1 };
        team.active_agents()
            .filter(|agent| Role::from(agent.role) == reviewer)
            .count()
            >= needed
    })
}

#[cfg(test)]
mod tests {
    use farik_core::contract::{Role, TaskKind};
    use farik_core::governor::readiness::fixtures::{a_contract, a_ready_context};
    use farik_core::governor::readiness::{ReadinessRule, evaluate_readiness};
    use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
    use farik_core::team::{Team, validate_team};
    use serde_json::{Value, json};

    use super::default_reviewer_role;

    /// A team of an active Product Manager and the agents given as (id, role, status).
    fn a_team(agents: &[(&str, &str, &str)]) -> Team {
        let mut wire = a_team_wire();
        let mut list = vec![an_agent_wire("pm", "product_manager")];
        for (id, role, status) in agents {
            let mut agent = an_agent_wire(id, role);
            agent["status"] = json!(status);
            list.push(agent);
        }
        wire["agents"] = Value::Array(list);
        validate_team(&wire).expect("the fixture is a team")
    }

    fn two_developers_and_an_architect() -> Team {
        a_team(&[
            ("dev-a", "software_developer", "active"),
            ("dev-b", "software_developer", "active"),
            ("arch", "architect", "active"),
        ])
    }

    fn two_developers() -> Team {
        a_team(&[
            ("dev-a", "software_developer", "active"),
            ("dev-b", "software_developer", "active"),
        ])
    }

    fn a_lone_developer() -> Team {
        a_team(&[("dev-a", "software_developer", "active")])
    }

    fn a_paused_architect_and_two_developers() -> Team {
        a_team(&[
            ("dev-a", "software_developer", "active"),
            ("dev-b", "software_developer", "active"),
            ("arch", "architect", "paused"),
        ])
    }

    fn one_active_developer_and_one_paused() -> Team {
        a_team(&[
            ("dev-a", "software_developer", "active"),
            ("dev-b", "software_developer", "paused"),
        ])
    }

    fn a_team_with_an_architect_and_a_marketer() -> Team {
        a_team(&[
            ("dev-a", "software_developer", "active"),
            ("arch", "architect", "active"),
            ("mark", "marketing_specialist", "active"),
        ])
    }

    #[test]
    fn prefers_an_architect_for_a_developers_task() {
        assert_eq!(
            default_reviewer_role(
                &two_developers_and_an_architect(),
                TaskKind::Task,
                Role::SoftwareDeveloper
            ),
            Some(Role::Architect)
        );
    }

    #[test]
    fn falls_back_to_another_developer() {
        assert_eq!(
            default_reviewer_role(&two_developers(), TaskKind::Task, Role::SoftwareDeveloper),
            Some(Role::SoftwareDeveloper)
        );
    }

    #[test]
    fn finds_no_reviewer_for_a_lone_developer() {
        assert_eq!(
            default_reviewer_role(&a_lone_developer(), TaskKind::Task, Role::SoftwareDeveloper),
            None
        );
    }

    #[test]
    fn sends_an_architects_task_to_the_product_manager() {
        assert_eq!(
            default_reviewer_role(
                &a_team_with_an_architect_and_a_marketer(),
                TaskKind::Task,
                Role::Architect
            ),
            Some(Role::ProductManager)
        );
    }

    #[test]
    fn sends_a_marketing_specialists_task_to_the_product_manager() {
        assert_eq!(
            default_reviewer_role(
                &a_team_with_an_architect_and_a_marketer(),
                TaskKind::Task,
                Role::MarketingSpecialist
            ),
            Some(Role::ProductManager)
        );
    }

    #[test]
    fn answers_only_what_readiness_accepts() {
        let teams = [
            two_developers_and_an_architect(),
            two_developers(),
            a_lone_developer(),
            a_paused_architect_and_two_developers(),
            one_active_developer_and_one_paused(),
            a_team_with_an_architect_and_a_marketer(),
        ];
        let assignees = [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::MarketingSpecialist,
        ];
        let mut answered = 0;
        for team in &teams {
            for assignee in assignees {
                let Some(reviewer) = default_reviewer_role(team, TaskKind::Task, assignee) else {
                    continue;
                };
                answered += 1;
                let mut contract = a_contract();
                contract.assignee_role = assignee;
                contract.reviewer_role = reviewer;
                let mut context = a_ready_context();
                context.active_agents_by_role.clear();
                for agent in team.active_agents() {
                    *context
                        .active_agents_by_role
                        .entry(Role::from(agent.role))
                        .or_insert(0) += 1;
                }
                let failed: Vec<ReadinessRule> = evaluate_readiness(&contract, &context)
                    .err()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|failure| failure.rule)
                    .collect();
                assert!(
                    !failed.contains(&ReadinessRule::ReviewerAvailable),
                    "{assignee} reviewed by {reviewer}: {failed:?}"
                );
            }
        }
        assert!(answered >= 6, "the fixtures answer something to check");
    }

    #[test]
    fn passes_over_a_paused_architect() {
        assert_eq!(
            default_reviewer_role(
                &a_paused_architect_and_two_developers(),
                TaskKind::Task,
                Role::SoftwareDeveloper
            ),
            Some(Role::SoftwareDeveloper)
        );
        assert_eq!(
            default_reviewer_role(
                &one_active_developer_and_one_paused(),
                TaskKind::Task,
                Role::SoftwareDeveloper
            ),
            None
        );
    }

    #[test]
    fn leaves_an_epics_reviewer_to_the_assignment_gate() {
        for assignee in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::MarketingSpecialist,
        ] {
            assert_eq!(
                default_reviewer_role(&two_developers_and_an_architect(), TaskKind::Epic, assignee),
                None
            );
        }
    }
}
