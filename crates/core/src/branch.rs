//! The one place a task's branch name is made (`docs/SPEC.md` section 5.14): `feature/<id>` or
//! `fix/<id>` for a Software Developer's or a UI/UX Designer's task, by its contract's `change`, and
//! `docs/<id>` for every other role's task, because only those two change code.

use crate::contract::{TaskContract, TaskId};
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

/// The task number a branch name holds, when it is one of the three shapes `task_branch` makes
/// (`feature/CTV-<n>`, `fix/CTV-<n>`, `docs/CTV-<n>`), with or without a remote's name before it
/// (`origin/fix/CTV-7`). Any single leading segment is read as a remote's name, so `x/feature/CTV-7`
/// counts too, which at worst skips numbers. A number past what a task id can hold, and any other
/// name, answers `None`.
#[must_use]
pub fn task_number_of_branch(name: &str) -> Option<u64> {
    let parts: Vec<&str> = name.split('/').collect();
    let [kind, id] = match parts.as_slice() {
        [_, kind, id] | [kind, id] => [*kind, *id],
        _ => return None,
    };
    if !matches!(kind, "feature" | "fix" | "docs") {
        return None;
    }
    // Only a number a contract can hold: an id past the limit would be a floor no task id follows.
    let task_id: TaskId = id.parse().ok()?;
    task_id.as_str().strip_prefix("CTV-")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{task_branch, task_number_of_branch};
    use crate::contract::Role;
    use crate::generated::task_contract::CatervasTaskContractChange as Change;
    use crate::governor::readiness::fixtures::a_contract;

    fn a_task(role: Role, change: Option<Change>) -> crate::contract::TaskContract {
        let mut contract = a_contract();
        contract.id = "CTV-7".parse().expect("a task id");
        contract.assignee_role = role;
        contract.change = change;
        contract
    }

    #[test]
    fn names_a_developers_feature_branch() {
        let contract = a_task(Role::SoftwareDeveloper, None);
        assert_eq!(task_branch(&contract), "feature/CTV-7");
    }

    #[test]
    fn names_a_developers_fix_branch() {
        let contract = a_task(Role::SoftwareDeveloper, Some(Change::Fix));
        assert_eq!(task_branch(&contract), "fix/CTV-7");
    }

    #[test]
    fn puts_a_designers_task_on_a_feature_or_fix_branch() {
        let feature = a_task(Role::UiUxDesigner, None);
        assert_eq!(task_branch(&feature), "feature/CTV-7");
        let feature = a_task(Role::UiUxDesigner, Some(Change::Feature));
        assert_eq!(task_branch(&feature), "feature/CTV-7");
        let fix = a_task(Role::UiUxDesigner, Some(Change::Fix));
        assert_eq!(task_branch(&fix), "fix/CTV-7");
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
            assert_eq!(task_branch(&contract), "docs/CTV-7", "{role}");
        }
    }

    #[test]
    fn task_number_of_branch_reads_the_three_shapes() {
        for (name, number) in [
            ("feature/CTV-7", 7),
            ("fix/CTV-12", 12),
            ("docs/CTV-3", 3),
            ("origin/feature/CTV-9", 9),
        ] {
            assert_eq!(task_number_of_branch(name), Some(number), "{name}");
        }
        for name in [
            "main",
            "feature/login",
            "feature/CTV-",
            "feature/CTV-x",
            "wip/CTV-4",
            "feature/CTV-1/more",
            "feature/CTV-1234567",
        ] {
            assert_eq!(task_number_of_branch(name), None, "{name}");
        }
    }

    #[test]
    fn task_number_of_branch_inverts_task_branch() {
        for (role, change) in [
            (Role::Architect, None),
            (Role::SoftwareDeveloper, Some(Change::Fix)),
            (Role::SoftwareDeveloper, Some(Change::Feature)),
        ] {
            let mut contract = a_task(role, change);
            contract.id = "CTV-41".parse().expect("a task id");
            assert_eq!(
                task_number_of_branch(&task_branch(&contract)),
                Some(41),
                "{role}"
            );
        }
    }
}
