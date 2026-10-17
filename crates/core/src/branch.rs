//! The one place a task's branch name is read and made (`docs/SPEC.md` section 5.14, ADR 0066).
//!
//! A task's branch is `<feat|fix|docs>/<slug>`, the slug made from its title, chosen by Catervas
//! once when the task's worktree is first made and recorded in the contract's `branch`; no task id
//! is in it. `task_branch` is where every caller reads a task's branch: the recorded one, else the
//! rule before 0.119 (`old_task_branch`: `feature/<id>`, `fix/<id>` or `docs/<id>`), so a task
//! started before keeps its branch. `task_number_of_branch` reads the old shapes for the id
//! counter alone.

use crate::contract::{TaskContract, TaskId};
use crate::generated::task_contract::CatervasTaskContractChange as Change;
use crate::team::changes_code;

/// The type of a task's change (ADR 0066), from its contract alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// A new capability.
    Feat,
    /// A repair.
    Fix,
    /// Only documents change.
    Docs,
}

impl ChangeType {
    /// `feat`, `fix`, `docs`: a branch's first segment.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Docs => "docs",
        }
    }

    /// `Feat`, `Fix`, `Docs`: a pull request title's first word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Feat => "Feat",
            Self::Fix => "Fix",
            Self::Docs => "Docs",
        }
    }
}

/// Docs when the assignee's role changes no code or every allowed path starts with `docs/`; else
/// Fix when the contract's `change` is `fix`; else Feat.
#[must_use]
pub fn change_type(contract: &TaskContract) -> ChangeType {
    let only_docs = !contract.allowed_paths.is_empty()
        && contract
            .allowed_paths
            .iter()
            .all(|path| path.starts_with("docs/"));
    if !changes_code(contract.assignee_role) || only_docs {
        return ChangeType::Docs;
    }
    match contract.change {
        Some(Change::Fix) => ChangeType::Fix,
        Some(Change::Feature) | None => ChangeType::Feat,
    }
}

/// The longest slug of a branch name.
pub const SLUG_MAX: usize = 40;

/// The slug of `words`: lower-cased ASCII, every run of other characters one `-`, trimmed of `-`,
/// cut to `SLUG_MAX` at the last `-` within its first `SLUG_MAX + 1` characters (at `SLUG_MAX` with
/// none), and `change` when nothing is left.
#[must_use]
pub fn branch_slug(words: &str) -> String {
    let mut slug = String::new();
    for character in words.chars() {
        let lower = character.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            slug.push(lower);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug = slug.trim_matches('-').to_string();
    if slug.len() > SLUG_MAX {
        let window = &slug[..=SLUG_MAX];
        let cut = window.rfind('-').unwrap_or(SLUG_MAX);
        slug.truncate(cut);
        slug = slug.trim_matches('-').to_string();
    }
    if slug.is_empty() {
        "change".to_string()
    } else {
        slug
    }
}

/// `<prefix>/<slug>`, else with `-2`, `-3`, ... appended, the first that `taken` does not hold. A
/// name is held when an entry equals it or starts with it and a `/`.
#[must_use]
pub fn free_branch(kind: ChangeType, slug: &str, taken: &[String]) -> String {
    let held = |name: &str| {
        taken
            .iter()
            .any(|entry| entry == name || entry.starts_with(&format!("{name}/")))
    };
    let first = format!("{}/{slug}", kind.prefix());
    if !held(&first) {
        return first;
    }
    let mut n = 2_u32;
    loop {
        let name = format!("{first}-{n}");
        if !held(&name) {
            return name;
        }
        n += 1;
    }
}

/// The rule before ADR 0066: `feature/<id>`, `fix/<id>` or `docs/<id>`.
#[must_use]
pub fn old_task_branch(contract: &TaskContract) -> String {
    let id = contract.id.as_str();
    if !changes_code(contract.assignee_role) {
        return format!("docs/{id}");
    }
    match contract.change {
        Some(Change::Fix) => format!("fix/{id}"),
        Some(Change::Feature) | None => format!("feature/{id}"),
    }
}

/// The branch a task works on: the recorded `branch`, else `old_task_branch`.
#[must_use]
pub fn task_branch(contract: &TaskContract) -> String {
    contract.branch.as_ref().map_or_else(
        || old_task_branch(contract),
        |branch| branch.as_str().to_string(),
    )
}

/// The task number a branch name holds, when it is one of the three shapes `old_task_branch` makes
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
    use super::{
        ChangeType, branch_slug, change_type, free_branch, old_task_branch, task_branch,
        task_number_of_branch,
    };
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
    fn names_a_task_branch_from_its_type_and_title() {
        let kind = |role, change, paths: &[&str]| {
            let mut contract = a_task(role, change);
            contract.allowed_paths = paths.iter().map(|p| p.parse().expect("a path")).collect();
            change_type(&contract)
        };
        assert_eq!(
            kind(Role::SoftwareDeveloper, None, &["src/**"]),
            ChangeType::Feat
        );
        assert_eq!(
            kind(Role::SoftwareDeveloper, Some(Change::Fix), &["src/**"]),
            ChangeType::Fix
        );
        assert_eq!(
            kind(Role::UiUxDesigner, Some(Change::Fix), &["src/**"]),
            ChangeType::Fix
        );
        assert_eq!(
            kind(Role::Architect, Some(Change::Fix), &["src/**"]),
            ChangeType::Docs
        );
        assert_eq!(
            kind(
                Role::SoftwareDeveloper,
                Some(Change::Fix),
                &["docs/catervas/engineering/bug-hunt.md"]
            ),
            ChangeType::Docs
        );
        assert_eq!(
            kind(Role::SoftwareDeveloper, None, &["docs/api.md", "src/**"]),
            ChangeType::Feat
        );
        for (kind, prefix, word) in [
            (ChangeType::Feat, "feat", "Feat"),
            (ChangeType::Fix, "fix", "Fix"),
            (ChangeType::Docs, "docs", "Docs"),
        ] {
            assert_eq!((kind.prefix(), kind.word()), (prefix, word));
        }

        assert_eq!(
            branch_slug("Character counter on the note input"),
            "character-counter-on-the-note-input"
        );
        assert_eq!(
            branch_slug("  Fix: the caf\u{e9}'s \u{a3}5 bug!! "),
            "fix-the-caf-s-5-bug"
        );
        assert_eq!(branch_slug("???"), "change");
        assert_eq!(
            branch_slug("Make the onboarding checklist remember skipped steps across devices"),
            "make-the-onboarding-checklist-remember"
        );
        assert_eq!(branch_slug(&"a".repeat(45)), "a".repeat(40));

        let taken = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();
        let free =
            |names: &[&str]| free_branch(ChangeType::Feat, "add-a-login-page", &taken(names));
        assert_eq!(free(&[]), "feat/add-a-login-page");
        assert_eq!(free(&["feat/add-a-login-page"]), "feat/add-a-login-page-2");
        assert_eq!(
            free(&["feat/add-a-login-page", "feat/add-a-login-page-2/x"]),
            "feat/add-a-login-page-3"
        );
        assert_eq!(free(&["feat"]), "feat/add-a-login-page");

        let mut contract = a_task(Role::SoftwareDeveloper, None);
        assert_eq!(task_branch(&contract), "feature/CTV-7");
        assert_eq!(
            task_branch(&a_task(Role::Architect, Some(Change::Fix))),
            "docs/CTV-7"
        );
        contract.branch = Some("feat/add-a-login-page".parse().expect("a branch"));
        assert_eq!(task_branch(&contract), "feat/add-a-login-page");
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
            "feature/FRK-7",
            "fix/FRK-7",
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
                task_number_of_branch(&old_task_branch(&contract)),
                Some(41),
                "{role}"
            );
        }
    }
}
