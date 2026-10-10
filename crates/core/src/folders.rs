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

/// The human documents the owner approves through `catervas_write_folder_doc` (section 5.7).
pub const OWNER_ACCEPTED: [&str; 2] = [
    "docs/catervas/product/spec.md",
    "docs/catervas/product/roadmap.md",
];

/// A path `catervas_write_folder_doc` may write: normalised, and whether the owner approves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDocPath {
    /// The path, normalised.
    pub path: String,
    /// Whether the path is in [`OWNER_ACCEPTED`].
    pub owner_accepted: bool,
}

/// Why a path is not one the caller may write with `catervas_write_folder_doc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderDocPathRefusal {
    /// The role owns no folder.
    NoFolder,
    /// The path is not below the caller's folder.
    Outside,
    /// The path is not a `.md` file.
    NotMarkdown,
    /// The path is an `.agent.md` twin, written from the human document's `agent_text`.
    AgentTwin,
    /// The path is a marketing plan, proposed with `catervas_propose_marketing_plan`.
    MarketingPlan,
}

/// Holds a path to a `.md` file at least one segment below the role's folder (letter case counts),
/// not an `.agent.md` twin, and not a human document the owner does not approve here.
///
/// # Errors
///
/// The first [`FolderDocPathRefusal`] that applies.
pub fn check_folder_doc_path(
    role: Role,
    path: &str,
) -> Result<FolderDocPath, FolderDocPathRefusal> {
    let folder = role_folder(role).ok_or(FolderDocPathRefusal::NoFolder)?;
    let path = normalise(path).ok_or(FolderDocPathRefusal::Outside)?;
    if !path
        .strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
    {
        return Err(FolderDocPathRefusal::Outside);
    }
    if path.strip_suffix(".md").is_none() {
        return Err(FolderDocPathRefusal::NotMarkdown);
    }
    if path.strip_suffix(TWIN_SUFFIX).is_some() {
        return Err(FolderDocPathRefusal::AgentTwin);
    }
    let owner_accepted = OWNER_ACCEPTED.contains(&path.as_str());
    if is_human_document(&path) && !owner_accepted {
        return Err(FolderDocPathRefusal::MarketingPlan);
    }
    Ok(FolderDocPath {
        path,
        owner_accepted,
    })
}

/// The git author of a folder change: `<display name> (Catervas) <catervas@localhost>`, the name
/// without `<`, `>` and control characters, or the agent's id when nothing is left.
#[must_use]
pub fn folder_doc_author(display_name: &str, agent_id: &str) -> String {
    let name: String = display_name
        .chars()
        .filter(|c| !c.is_control() && *c != '<' && *c != '>')
        .collect();
    let name = name.trim();
    let name = if name.is_empty() { agent_id } else { name };
    format!("{name} (Catervas) <catervas@localhost>")
}

/// The commit message of a folder change: `docs(<the folder's last segment>): <each path within
/// the folder, joined by ", "> by <agent name>`, with `, approved by the owner` after an approval.
#[must_use]
pub fn folder_doc_message(
    folder: &str,
    paths: &[&str],
    agent_name: &str,
    approved: bool,
) -> String {
    let scope = folder.rsplit('/').next().unwrap_or(folder);
    let within = format!("{folder}/");
    let names: Vec<&str> = paths
        .iter()
        .map(|path| path.strip_prefix(&within).unwrap_or(path))
        .collect();
    let owner = if approved {
        ", approved by the owner"
    } else {
        ""
    };
    format!("docs({scope}): {} by {agent_name}{owner}", names.join(", "))
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
        FolderDocPathRefusal, OWNER_ACCEPTED, ROLE_FOLDERS, ReadAccess, agent_twin,
        check_folder_doc_path, folder_doc_author, folder_doc_message, folders_line, human_of_twin,
        is_human_document, pair_changed_alone, read_access, role_folder,
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

    #[test]
    fn holds_a_document_to_its_writers_folder() {
        for (path, normalised) in [
            (
                "docs/catervas/delivery/cadence.md",
                "docs/catervas/delivery/cadence.md",
            ),
            (
                "./docs/catervas/delivery/notes/s4.md",
                "docs/catervas/delivery/notes/s4.md",
            ),
        ] {
            let ok = check_folder_doc_path(Role::ScrumMaster, path).expect(path);
            assert_eq!((ok.path.as_str(), ok.owner_accepted), (normalised, false));
        }
        for path in [
            "docs/catervas/product/x.md",
            "docs/catervas/delivery",
            "docs/catervas/delivery/../product/x.md",
            "/docs/catervas/delivery/x.md",
            "Docs/catervas/delivery/x.md",
        ] {
            assert_eq!(
                check_folder_doc_path(Role::ScrumMaster, path),
                Err(FolderDocPathRefusal::Outside),
                "{path}"
            );
        }
        assert_eq!(
            check_folder_doc_path(Role::ScrumMaster, "docs/catervas/delivery/x.txt"),
            Err(FolderDocPathRefusal::NotMarkdown)
        );
        assert_eq!(
            check_folder_doc_path(Role::ScrumMaster, "docs/catervas/delivery/x.agent.md"),
            Err(FolderDocPathRefusal::AgentTwin)
        );
        for name in ["spec.md", "roadmap.md"] {
            let ok = check_folder_doc_path(Role::ProductManager, &product(name)).expect(name);
            assert!(ok.owner_accepted, "{name}");
        }
        assert!(
            !check_folder_doc_path(Role::ProductManager, &product("notes.md"))
                .expect("notes")
                .owner_accepted
        );
        assert_eq!(
            check_folder_doc_path(
                Role::MarketingSpecialist,
                "docs/catervas/marketing/plans/MP-3.md"
            ),
            Err(FolderDocPathRefusal::MarketingPlan)
        );
        assert!(
            check_folder_doc_path(
                Role::MarketingSpecialist,
                "docs/catervas/marketing/plans/notes.md"
            )
            .is_ok()
        );
        assert_eq!(
            check_folder_doc_path(Role::FinanceSpecialist, "docs/catervas/finance/x.md"),
            Err(FolderDocPathRefusal::NoFolder)
        );
        assert_eq!(OWNER_ACCEPTED, [product("spec.md"), product("roadmap.md")]);
        assert!(OWNER_ACCEPTED.iter().all(|path| is_human_document(path)));
    }

    #[test]
    fn writes_the_author_and_the_message() {
        assert_eq!(
            folder_doc_author("Sol", "sm"),
            "Sol (Catervas) <catervas@localhost>"
        );
        assert_eq!(
            folder_doc_author("<Mal>\nory", "sm"),
            "Malory (Catervas) <catervas@localhost>"
        );
        assert_eq!(
            folder_doc_author("<>", "sm"),
            "sm (Catervas) <catervas@localhost>"
        );
        assert_eq!(
            folder_doc_message(
                "docs/catervas/delivery",
                &["docs/catervas/delivery/cadence.md"],
                "Sol",
                false
            ),
            "docs(delivery): cadence.md by Sol"
        );
        assert_eq!(
            folder_doc_message(
                "docs/catervas/product",
                &[&product("roadmap.md"), &product("spec.md")],
                "Mira",
                true
            ),
            "docs(product): roadmap.md, spec.md by Mira, approved by the owner"
        );
    }
}
