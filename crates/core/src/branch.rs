//! The one place a task's branch name is made (`docs/SPEC.md` section 5.14): `feature/<id>` or
//! `fix/<id>` for a Software Developer's or a UI/UX Designer's task, by its contract's `change`, and
//! `docs/<id>` for every other role's task, because only those two change code.

use crate::contract::TaskContract;
use crate::generated::task_contract::CatervasTaskContractChange as Change;
use crate::team::changes_code;

/// The branch a task works on. A Software Developer's or a UI/UX Designer's task works on
/// `feature/<id>`, or on `fix/<id>` when its contract's `change` is `fix`; an absent `change` reads
/// as `feature`. Every other role's task works on `docs/<id>`, because a document role's
/// `write_workspace` is already held to `document_paths`.
#[must_use]
pub fn task_branch(contract: &TaskContract) -> String {
    let id = contract.id.as_str();
    if !changes_code(contract.assignee_role) {
        return format!("docs/{id}");
    }
    match contract.change {
        Some(Change::Fix) => format!("fix/{id}"),
        Some(Change::Feature) | None => format!("feature/{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::task_branch;
    use crate::contract::Role;
    use crate::generated::task_contract::CatervasTaskContractChange as Change;
    use crate::governor::readiness::fixtures::a_contract;

    fn a_task(role: Role, change: Option<Change>) -> crate::contract::TaskContract {
        let mut contract = a_contract();
        contract.id = "FRK-7".parse().expect("a task id");
        contract.assignee_role = role;
        contract.change = change;
        contract
    }

    #[test]
    fn names_a_developers_feature_branch() {
        let contract = a_task(Role::SoftwareDeveloper, None);
        assert_eq!(task_branch(&contract), "feature/FRK-7");
    }

    #[test]
    fn names_a_developers_fix_branch() {
        let contract = a_task(Role::SoftwareDeveloper, Some(Change::Fix));
        assert_eq!(task_branch(&contract), "fix/FRK-7");
    }

    #[test]
    fn puts_a_designers_task_on_a_feature_or_fix_branch() {
        let feature = a_task(Role::UiUxDesigner, None);
        assert_eq!(task_branch(&feature), "feature/FRK-7");
        let feature = a_task(Role::UiUxDesigner, Some(Change::Feature));
        assert_eq!(task_branch(&feature), "feature/FRK-7");
        let fix = a_task(Role::UiUxDesigner, Some(Change::Fix));
        assert_eq!(task_branch(&fix), "fix/FRK-7");
    }

    #[test]
    fn names_every_other_roles_branch_docs() {
        for role in [
            Role::Architect,
            Role::MarketingSpecialist,
            Role::ProductManager,
            Role::ScrumMaster,
        ] {
            let contract = a_task(role, Some(Change::Fix));
            assert_eq!(task_branch(&contract), "docs/FRK-7", "{role}");
        }
    }
}
