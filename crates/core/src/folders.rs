//! The folder each role owns under `docs/catervas/`, and the documents in them that come as a pair
//! (`docs/SPEC.md` section 5.17, ADR 0051). Pure: paths in, answers out.

use crate::governor::paths::{normalise, stays_within};
use crate::team::{Role, plain_role};

/// Each role that owns a folder, with its folder, in the order the Team rules list them. The Finance
/// and Procurement Specialists keep private folders (`team::private_folder`) and own none of these.
pub const ROLE_FOLDERS: [(Role, &str); 6] = [
    (Role::ProductManager, "docs/catervas/product"),
    (Role::Architect, "docs/catervas/architecture"),
    (Role::SoftwareDeveloper, "docs/catervas/engineering"),
    (Role::UiUxDesigner, "docs/catervas/design"),
    (Role::ScrumMaster, "docs/catervas/delivery"),
    (Role::MarketingSpecialist, "docs/catervas/marketing"),
];

const TWIN_SUFFIX: &str = ".agent.md";
const PLANS: &str = "docs/catervas/marketing/plans/";

/// The folder a role owns, or `None` for a role that has none.
#[must_use]
pub fn role_folder(role: Role) -> Option<&'static str> {
    ROLE_FOLDERS
        .iter()
        .find_map(|(owner, folder)| (*owner == role).then_some(*folder))
}

/// Whether a path is a human document: the product's `spec.md` and `roadmap.md`, or a marketing
/// plan `plans/MP-<n>.md`, `n` decimal digits not starting with `0`. Each has an agent twin.
#[must_use]
pub fn is_human_document(path: &str) -> bool {
    let Some(path) = normalise(path) else {
        return false;
    };
    if path == "docs/catervas/product/spec.md" || path == "docs/catervas/product/roadmap.md" {
        return true;
    }
    path.strip_prefix(PLANS)
        .and_then(|name| name.strip_prefix("MP-"))
        .and_then(|name| name.strip_suffix(".md"))
        .is_some_and(|n| {
            n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() && !n.starts_with('0')
        })
}

/// The twin of a human document: the path with `.agent` before its final `.md`.
#[must_use]
pub fn agent_twin(path: &str) -> Option<String> {
    if !is_human_document(path) {
        return None;
    }
    let path = normalise(path)?;
    Some(format!("{}{TWIN_SUFFIX}", path.strip_suffix(".md")?))
}

/// The human document a twin belongs to.
#[must_use]
pub fn human_of_twin(path: &str) -> Option<String> {
    let human = format!("{}.md", normalise(path)?.strip_suffix(TWIN_SUFFIX)?);
    is_human_document(&human).then_some(human)
}

/// Each path, normalised, that is one file of a human document's pair while the other is not in
/// the list, in first-seen order, each once.
#[must_use]
pub fn pair_changed_alone(changed_paths: &[String]) -> Vec<String> {
    let changed: Vec<String> = changed_paths.iter().filter_map(|p| normalise(p)).collect();
    let mut alone: Vec<String> = Vec::new();
    for path in &changed {
        let partner = agent_twin(path).or_else(|| human_of_twin(path));
        if partner.is_some_and(|partner| !changed.contains(&partner)) && !alone.contains(path) {
            alone.push(path.clone());
        }
    }
    alone
}

/// What a role may read of the project (`docs/SPEC.md` section 5.6): everything, or only the
/// listed read paths, each `<folder>/**`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadAccess {
    /// The whole project.
    Open,
    /// Only what these read paths, each `<folder>/**`, hold.
    Only(Vec<String>),
}

/// What the role may read: the Marketing Specialist reads the Product Manager's folder and its own,
/// every other role (the human included) reads the project.
#[must_use]
pub fn read_access(role: Role) -> ReadAccess {
    if role != Role::MarketingSpecialist {
        return ReadAccess::Open;
    }
    ReadAccess::Only(
        [Role::ProductManager, Role::MarketingSpecialist]
            .into_iter()
            .filter_map(role_folder)
            .map(|folder| format!("{folder}/**"))
            .collect(),
    )
}

impl ReadAccess {
    /// Whether every path the glob could match is readable.
    #[must_use]
    pub fn allows(&self, glob: &str) -> bool {
        match self {
            Self::Open => true,
            Self::Only(reads) => reads.iter().any(|read| stays_within(glob, read)),
        }
    }

    /// The folders it allows, each without its `**`, as the sentence names them.
    #[must_use]
    pub fn named(&self) -> String {
        match self {
            Self::Open => "everything".to_string(),
            Self::Only(reads) => reads
                .iter()
                .map(|read| read.strip_suffix("**").unwrap_or(read))
                .collect::<Vec<_>>()
                .join(" and "),
        }
    }
}

/// The Team rules' line about folders (`docs/SPEC.md` 5.17): each folder of an active role, and of
/// the reader's own, in table order; which one is the reader's; how to read them.
#[must_use]
pub fn folders_line(role: Role, active_roles: &[Role]) -> String {
    let listed: Vec<String> = ROLE_FOLDERS
        .iter()
        .filter(|(owner, _)| *owner == role || active_roles.contains(owner))
        .map(|(owner, folder)| format!("{folder}/ ({})", plain_role(*owner)))
        .collect();
    if listed.is_empty() {
        return "- folders: none".to_string();
    }
    let yours = role_folder(role).map_or_else(
        || "You have none: write none of them.".to_string(),
        |folder| format!("Yours is {folder}/: write no other folder named here."),
    );
    let reading = match read_access(role) {
        ReadAccess::Open => "Read any of them, and where a document has an .agent.md twin beside it, read the twin, which is written for agents.".to_string(),
        held @ ReadAccess::Only(_) => format!(
            "Read only {}: Catervas refuses a read anywhere else, and a search that names no path. Where a document there has an .agent.md twin beside it, read the twin, which is written for agents.",
            held.named()
        ),
    };
    format!("- folders: {}. {yours} {reading}", listed.join(", "))
}

#[cfg(test)]
mod tests {
    use super::{
        ROLE_FOLDERS, ReadAccess, agent_twin, folders_line, human_of_twin, is_human_document,
        pair_changed_alone, read_access, role_folder,
    };
    use crate::team::Role;

    const READ: &str = "Read any of them, and where a document has an .agent.md twin beside it, read the twin, which is written for agents.";

    fn product(name: &str) -> String {
        format!("docs/catervas/product/{name}")
    }

    #[test]
    fn each_owning_role_has_its_folder() {
        let table = [
            (Role::ProductManager, "docs/catervas/product"),
            (Role::Architect, "docs/catervas/architecture"),
            (Role::SoftwareDeveloper, "docs/catervas/engineering"),
            (Role::UiUxDesigner, "docs/catervas/design"),
            (Role::ScrumMaster, "docs/catervas/delivery"),
            (Role::MarketingSpecialist, "docs/catervas/marketing"),
        ];
        assert_eq!(ROLE_FOLDERS, table);
        for (role, folder) in table {
            assert_eq!(role_folder(role), Some(folder));
        }
        for role in [
            Role::FinanceSpecialist,
            Role::ProcurementSpecialist,
            Role::Human,
        ] {
            assert_eq!(role_folder(role), None);
        }
    }

    #[test]
    fn names_the_human_documents_and_no_other() {
        for path in [
            "docs/catervas/product/spec.md",
            "docs/catervas/product/roadmap.md",
            "docs/catervas/marketing/plans/MP-1.md",
            "docs/catervas/marketing/plans/MP-12.md",
            "./docs/catervas/product/spec.md",
        ] {
            assert!(is_human_document(path), "{path}");
        }
        for path in [
            "docs/catervas/product/spec.agent.md",
            "docs/catervas/product/Spec.md",
            "docs/catervas/product/notes.md",
            "docs/catervas/architecture/spec.md",
            "product/spec.md",
            "../docs/catervas/product/spec.md",
            "docs/catervas/marketing/plans/MP-0.md",
            "docs/catervas/marketing/plans/MP-01.md",
            "docs/catervas/marketing/plans/MP-x.md",
            "docs/catervas/marketing/plans/MP-1.agent.md",
            "docs/catervas/marketing/plans/sub/MP-1.md",
        ] {
            assert!(!is_human_document(path), "{path}");
        }
    }

    #[test]
    fn pairs_a_human_document_with_its_twin() {
        for (human, twin) in [
            (
                "docs/catervas/product/spec.md",
                "docs/catervas/product/spec.agent.md",
            ),
            (
                "docs/catervas/product/roadmap.md",
                "docs/catervas/product/roadmap.agent.md",
            ),
            (
                "docs/catervas/marketing/plans/MP-1.md",
                "docs/catervas/marketing/plans/MP-1.agent.md",
            ),
            (
                "docs/catervas/marketing/plans/MP-12.md",
                "docs/catervas/marketing/plans/MP-12.agent.md",
            ),
        ] {
            assert_eq!(agent_twin(human).as_deref(), Some(twin));
            assert_eq!(human_of_twin(twin).as_deref(), Some(human));
        }
        assert_eq!(
            agent_twin("./docs/catervas/product/spec.md").as_deref(),
            Some("docs/catervas/product/spec.agent.md")
        );
        assert_eq!(
            human_of_twin("./docs/catervas/product/spec.agent.md").as_deref(),
            Some("docs/catervas/product/spec.md")
        );
        assert_eq!(agent_twin(&product("notes.md")), None);
        assert_eq!(agent_twin(&product("spec.agent.md")), None);
        assert_eq!(human_of_twin(&product("notes.agent.md")), None);
        assert_eq!(human_of_twin(&product("spec.md")), None);
    }

    #[test]
    fn finds_a_pair_changed_alone() {
        let spec = product("spec.md");
        let spec_twin = product("spec.agent.md");
        let roadmap = product("roadmap.md");
        let roadmap_twin = product("roadmap.agent.md");
        let list = |paths: &[&str]| -> Vec<String> {
            paths.iter().map(|path| (*path).to_string()).collect()
        };
        assert_eq!(
            pair_changed_alone(&list(&[&spec, "src/x.rs"])),
            list(&[&spec])
        );
        assert_eq!(
            pair_changed_alone(&list(&[&spec_twin])),
            list(&[&spec_twin])
        );
        assert!(pair_changed_alone(&list(&[&spec, &spec_twin])).is_empty());
        assert!(
            pair_changed_alone(&list(&["./docs/catervas/product/spec.md", &spec_twin])).is_empty()
        );
        assert_eq!(
            pair_changed_alone(&list(&[&roadmap, &spec_twin, &spec, &roadmap])),
            list(&[&roadmap])
        );
        let plan = "docs/catervas/marketing/plans/MP-2.md";
        assert_eq!(pair_changed_alone(&list(&[plan])), list(&[plan]));
        assert!(pair_changed_alone(&list(&[&product("notes.md")])).is_empty());
        assert_eq!(
            pair_changed_alone(&list(&[&spec, &roadmap_twin])),
            list(&[&spec, &roadmap_twin])
        );
    }

    #[test]
    fn lists_the_active_roles_folders_in_table_order() {
        let line = folders_line(
            Role::ProductManager,
            &[
                Role::SoftwareDeveloper,
                Role::ProductManager,
                Role::MarketingSpecialist,
                Role::FinanceSpecialist,
            ],
        );
        assert_eq!(
            line,
            format!(
                "- folders: docs/catervas/product/ (Product Manager), docs/catervas/engineering/ (Software Developer), docs/catervas/marketing/ (Marketing Specialist). Yours is docs/catervas/product/: write no other folder named here. {READ}"
            )
        );
    }

    #[test]
    fn tells_a_role_without_a_folder_to_write_none() {
        assert_eq!(
            folders_line(Role::FinanceSpecialist, &[Role::ProductManager]),
            format!(
                "- folders: docs/catervas/product/ (Product Manager). You have none: write none of them. {READ}"
            )
        );
        assert_eq!(
            folders_line(Role::FinanceSpecialist, &[]),
            "- folders: none"
        );
    }

    #[test]
    fn lists_the_readers_own_folder_when_it_is_not_active() {
        let line = folders_line(Role::Architect, &[Role::ProductManager]);
        assert!(
            line.starts_with(
                "- folders: docs/catervas/product/ (Product Manager), docs/catervas/architecture/ (Architect). Yours is docs/catervas/architecture/"
            ),
            "{line}"
        );
    }

    #[test]
    fn only_the_marketing_specialist_s_reads_are_held() {
        assert_eq!(
            read_access(Role::MarketingSpecialist),
            ReadAccess::Only(vec![
                "docs/catervas/product/**".to_string(),
                "docs/catervas/marketing/**".to_string()
            ])
        );
        for role in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::UiUxDesigner,
            Role::FinanceSpecialist,
            Role::ProcurementSpecialist,
            Role::Human,
        ] {
            assert_eq!(read_access(role), ReadAccess::Open, "{role:?}");
        }
    }

    #[test]
    fn a_held_reader_reads_its_folders_alone() {
        let held = read_access(Role::MarketingSpecialist);
        for glob in [
            "docs/catervas/product/spec.agent.md",
            "docs/catervas/marketing/plans/MP-1.md",
        ] {
            assert!(held.allows(glob), "{glob}");
        }
        for glob in [
            "docs/catervas/architecture/overview.md",
            "README.md",
            "docs/catervas/*.md",
        ] {
            assert!(!held.allows(glob), "{glob}");
        }
        for glob in [
            "docs/catervas/product/spec.agent.md",
            "docs/catervas/marketing/plans/MP-1.md",
            "docs/catervas/architecture/overview.md",
            "README.md",
            "docs/catervas/*.md",
        ] {
            assert!(ReadAccess::Open.allows(glob), "{glob}");
        }
        assert_eq!(
            held.named(),
            "docs/catervas/product/ and docs/catervas/marketing/"
        );
        assert_eq!(ReadAccess::Open.named(), "everything");
    }

    #[test]
    fn tells_the_marketing_specialist_what_it_reads() {
        assert_eq!(
            folders_line(
                Role::MarketingSpecialist,
                &[
                    Role::ProductManager,
                    Role::Architect,
                    Role::MarketingSpecialist
                ]
            ),
            "- folders: docs/catervas/product/ (Product Manager), docs/catervas/architecture/ (Architect), docs/catervas/marketing/ (Marketing Specialist). Yours is docs/catervas/marketing/: write no other folder named here. Read only docs/catervas/product/ and docs/catervas/marketing/: Catervas refuses a read anywhere else, and a search that names no path. Where a document there has an .agent.md twin beside it, read the twin, which is written for agents."
        );
    }
}
