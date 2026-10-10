//! The folder each role owns under `docs/catervas/`, and the documents in them that come as a pair
//! (`docs/SPEC.md` section 5.17, ADR 0051). Pure: paths in, answers out.

use crate::contract::TaskId;
use crate::governor::paths::{PathRefusal, check_allowed_paths, normalise, stays_within};
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
/// the list, in first-seen order, each once. A twin in `stale_twins` may change alone: it is
/// being brought up to date with a document the owner already changed.
#[must_use]
pub fn pair_changed_alone(changed_paths: &[String], stale_twins: &[String]) -> Vec<String> {
    let changed: Vec<String> = changed_paths.iter().filter_map(|p| normalise(p)).collect();
    let stale: Vec<String> = stale_twins.iter().filter_map(|p| normalise(p)).collect();
    let mut alone: Vec<String> = Vec::new();
    for path in &changed {
        let partner = agent_twin(path).or_else(|| human_of_twin(path));
        if partner.is_some_and(|partner| !changed.contains(&partner))
            && !stale.contains(path)
            && !alone.contains(path)
        {
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

/// Whether a task with these `allowed_paths` may change a document of [`OWNER_ACCEPTED`]: either
/// path passes the Definition of Done's own test of a diff (`check_allowed_paths`), or a glob does
/// not compile, which fails closed.
#[must_use]
pub fn may_change_an_owner_document(allowed_paths: &[String]) -> bool {
    OWNER_ACCEPTED.iter().any(|path| {
        !matches!(
            check_allowed_paths(&[(*path).to_string()], allowed_paths),
            Err(PathRefusal::Violations(_))
        )
    })
}

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

/// The documents with a fixed name, in the order the Files page lists them. A step that adds a
/// document adds its name here.
pub const DOCUMENT_NAMES: [(&str, &str); 11] = [
    ("docs/catervas/product/spec.md", "Product description"),
    ("docs/catervas/product/roadmap.md", "Roadmap"),
    ("docs/catervas/architecture/overview.md", "Overview"),
    ("docs/catervas/architecture/features.md", "Features"),
    ("docs/catervas/architecture/jobs.md", "Scheduled jobs"),
    ("docs/catervas/architecture/structure.md", "Structure"),
    ("docs/catervas/engineering/conventions.md", "Conventions"),
    ("docs/catervas/engineering/workflow.md", "Workflow"),
    ("docs/catervas/delivery/cadence.md", "Sprint rhythm"),
    ("docs/catervas/marketing/brand/brand-kit.md", "Brand kit"),
    ("docs/catervas/marketing/brand/persona.md", "Brand persona"),
];

/// A file stem or folder name as words: `-` and `_` as spaces, the first letter upper case.
#[must_use]
pub fn plain_name(stem: &str) -> String {
    let spaced = stem.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// What the Files page calls a document: its fixed name, "Plan: <title>" for a file directly in a
/// folder's `plans/` whose stem `title_of` knows, else its file stem as [`plain_name`] has it.
#[must_use]
pub fn document_name(path: &str, title_of: &dyn Fn(&str) -> Option<String>) -> String {
    let path = normalise(path).unwrap_or_else(|| path.to_string());
    if let Some((_, name)) = DOCUMENT_NAMES.iter().find(|(known, _)| *known == path) {
        return (*name).to_string();
    }
    let file = path.rsplit('/').next().unwrap_or(&path);
    let stem = file.strip_suffix(".md").unwrap_or(file);
    let in_plans = ROLE_FOLDERS
        .iter()
        .any(|(_, folder)| path == format!("{folder}/plans/{file}"));
    if in_plans && let Some(title) = title_of(stem) {
        return format!("Plan: {title}");
    }
    plain_name(stem)
}

/// The sort key of a document: its place in [`DOCUMENT_NAMES`], the others after, then its path.
#[must_use]
pub fn document_order(path: &str) -> (usize, String) {
    let place = DOCUMENT_NAMES
        .iter()
        .position(|(known, _)| *known == path)
        .unwrap_or(DOCUMENT_NAMES.len());
    (place, path.to_string())
}

/// The most the owner may save in one document, in bytes.
pub const OWNER_EDIT_LIMIT_BYTES: usize = 262_144;

/// Why the owner may not save a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerEditRefusal {
    /// The path climbs out, or is not under `docs/catervas/`.
    OutsideFolders,
    /// The path is not a `.md` file.
    NotMarkdown,
    /// The path is an `.agent.md` twin, which the team writes.
    AgentTwin,
    /// The path is a marketing plan, which changes through a new version on its page.
    MarketingPlan,
    /// The text is over [`OWNER_EDIT_LIMIT_BYTES`].
    TooLarge,
}

impl OwnerEditRefusal {
    /// The refusal's code on the wire.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::OutsideFolders => "outside_folders",
            Self::NotMarkdown => "not_markdown",
            Self::AgentTwin => "agent_twin",
            Self::MarketingPlan => "marketing_plan",
            Self::TooLarge => "too_large",
        }
    }
}

/// Holds the owner's edit of a document to a `.md` file under `docs/catervas/` that is not an
/// `.agent.md` twin or a marketing plan, within [`OWNER_EDIT_LIMIT_BYTES`]. Whether a folder of
/// an active role holds it is the caller's to check.
///
/// # Errors
///
/// The first [`OwnerEditRefusal`] that applies.
pub fn check_owner_edit(path: &str, text_bytes: usize) -> Result<String, OwnerEditRefusal> {
    let path = normalise(path)
        .filter(|path| path.starts_with("docs/catervas/"))
        .ok_or(OwnerEditRefusal::OutsideFolders)?;
    if path.strip_suffix(".md").is_none() {
        return Err(OwnerEditRefusal::NotMarkdown);
    }
    if path.ends_with(TWIN_SUFFIX) {
        return Err(OwnerEditRefusal::AgentTwin);
    }
    if is_human_document(&path) && !OWNER_ACCEPTED.contains(&path.as_str()) {
        return Err(OwnerEditRefusal::MarketingPlan);
    }
    if text_bytes > OWNER_EDIT_LIMIT_BYTES {
        return Err(OwnerEditRefusal::TooLarge);
    }
    Ok(path)
}

/// What a commit's subject names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Named {
    /// A task, as `CTV-<n>`.
    Task(TaskId),
    /// A folder change, as `folder-<n>` or `Folder change <n>`.
    FolderChange(u64),
}

fn number_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(prefix)?;
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// The first task or folder change a commit's subject names, in subject order; a name counts
/// only at the start of a word.
#[must_use]
pub fn named_in_subject(subject: &str) -> Option<Named> {
    subject
        .char_indices()
        .filter(|(i, _)| !subject[..*i].ends_with(char::is_alphanumeric))
        .find_map(|(i, _)| {
            let rest = &subject[i..];
            if let Some(n) = number_after(rest, "CTV-") {
                return format!("CTV-{n}").parse().ok().map(Named::Task);
            }
            let n =
                number_after(rest, "folder-").or_else(|| number_after(rest, "Folder change "))?;
            n.parse().ok().map(Named::FolderChange)
        })
}

/// Whether a twin is behind its document: the document's last commit is neither the twin's last
/// nor an ancestor of it (`human_commits_not_in_twin` is zero for an ancestor).
#[must_use]
pub fn twin_is_stale(human_last: &str, twin_last: &str, human_commits_not_in_twin: u32) -> bool {
    human_last != twin_last && human_commits_not_in_twin > 0
}

/// The Team rules' line about stale twins, or nothing when none is.
#[must_use]
pub fn stale_line(stale_twins: &[String]) -> Option<String> {
    (!stale_twins.is_empty()).then(|| {
        format!(
            "- stale twins: {}. Each is behind the document for people beside it, which the owner changed: read that document as well until the twin is brought up to date.",
            stale_twins.join(", ")
        )
    })
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

/// How a line of a document differs between two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChange {
    /// In both versions.
    Same,
    /// Only in the new version.
    Added,
    /// Only in the old version.
    Removed,
}

/// A line to show the owner, with how it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownLine {
    /// How the line changed.
    pub change: LineChange,
    /// The line, without its line ending.
    pub text: String,
}

/// A middle whose two lengths multiply past this is shown whole rather than compared.
const COMPARE_LIMIT: usize = 1_000_000;

fn document_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// The lines of two versions of a document that are worth showing: every changed line, the
/// unchanged line just before and just after each run of changes, and the nearest unchanged
/// heading (a line starting with `#`) above each run (section 5.7). Blank lines and a trailing
/// `\r` are not compared. Within a run, the removed lines come before the added ones.
///
/// ponytail: a middle whose two lengths multiply past 1,000,000 is shown whole, every old line
/// removed then every new one added, rather than compared; compare it in pieces if a real
/// document's change reads badly.
#[must_use]
pub fn changed_lines(before: &str, after: &str) -> Vec<ShownLine> {
    let old = document_lines(before);
    let new = document_lines(after);
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_mid, new_mid) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);

    let mut all: Vec<(LineChange, &str)> = old[..head]
        .iter()
        .map(|line| (LineChange::Same, *line))
        .collect();
    all.extend(compare(old_mid, new_mid));
    all.extend(
        old[old.len() - tail..]
            .iter()
            .map(|line| (LineChange::Same, *line)),
    );

    let mut shown = vec![false; all.len()];
    let mut start = 0;
    while start < all.len() {
        if all[start].0 == LineChange::Same {
            start += 1;
            continue;
        }
        let end = start
            + all[start..]
                .iter()
                .take_while(|(change, _)| *change != LineChange::Same)
                .count();
        shown[start..end].fill(true);
        if start > 0 {
            shown[start - 1] = true;
        }
        if end < all.len() {
            shown[end] = true;
        }
        if let Some(heading) = all[..start]
            .iter()
            .rposition(|(change, text)| *change == LineChange::Same && text.starts_with('#'))
        {
            shown[heading] = true;
        }
        start = end;
    }
    all.iter()
        .zip(shown)
        .filter(|(_, shown)| *shown)
        .map(|((change, text), _)| ShownLine {
            change: *change,
            text: (*text).to_string(),
        })
        .collect()
}

/// The middle of two documents as changes: by longest common subsequence, adding first where
/// removing and adding keep as long a common sequence; each run of changes lists its removed
/// lines, then its added ones.
fn compare<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<(LineChange, &'a str)> {
    if old.len().saturating_mul(new.len()) > COMPARE_LIMIT {
        let removed = old.iter().map(|line| (LineChange::Removed, *line));
        return removed
            .chain(new.iter().map(|line| (LineChange::Added, *line)))
            .collect();
    }
    let width = new.len() + 1;
    let mut common = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i * width + j] = if old[i] == new[j] {
                common[(i + 1) * width + j + 1] + 1
            } else {
                common[(i + 1) * width + j].max(common[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out: Vec<(LineChange, &str)> = Vec::new();
    let (mut removed, mut added): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let flush = |out: &mut Vec<(LineChange, &'a str)>,
                 removed: &mut Vec<&'a str>,
                 added: &mut Vec<&'a str>| {
        out.extend(removed.drain(..).map(|line| (LineChange::Removed, line)));
        out.extend(added.drain(..).map(|line| (LineChange::Added, line)));
    };
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            flush(&mut out, &mut removed, &mut added);
            out.push((LineChange::Same, old[i]));
            i += 1;
            j += 1;
        } else if j < new.len()
            && (i == old.len() || common[i * width + j + 1] >= common[(i + 1) * width + j])
        {
            added.push(new[j]);
            j += 1;
        } else {
            removed.push(old[i]);
            i += 1;
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}

#[cfg(test)]
mod tests {
    use super::{
        FolderDocPathRefusal, OWNER_ACCEPTED, ROLE_FOLDERS, ReadAccess, agent_twin,
        check_folder_doc_path, folder_doc_author, folder_doc_message, folders_line, human_of_twin,
        is_human_document, pair_changed_alone, read_access, role_folder,
    };
    use super::{LineChange, ShownLine, changed_lines, may_change_an_owner_document};
    use super::{
        Named, OwnerEditRefusal, check_owner_edit, document_name, document_order, named_in_subject,
        plain_name, stale_line, twin_is_stale,
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
            pair_changed_alone(&list(&[&spec, "src/x.rs"]), &[]),
            list(&[&spec])
        );
        assert_eq!(
            pair_changed_alone(&list(&[&spec_twin]), &[]),
            list(&[&spec_twin])
        );
        assert!(pair_changed_alone(&list(&[&spec, &spec_twin]), &[]).is_empty());
        assert!(
            pair_changed_alone(&list(&["./docs/catervas/product/spec.md", &spec_twin]), &[])
                .is_empty()
        );
        assert_eq!(
            pair_changed_alone(&list(&[&roadmap, &spec_twin, &spec, &roadmap]), &[]),
            list(&[&roadmap])
        );
        let plan = "docs/catervas/marketing/plans/MP-2.md";
        assert_eq!(pair_changed_alone(&list(&[plan]), &[]), list(&[plan]));
        assert!(pair_changed_alone(&list(&[&product("notes.md")]), &[]).is_empty());
        assert_eq!(
            pair_changed_alone(&list(&[&spec, &roadmap_twin]), &[]),
            list(&[&spec, &roadmap_twin])
        );
    }

    #[test]
    fn names_the_known_documents_and_the_rest_plainly() {
        let none = |_: &str| None;
        let name = |path: &str| document_name(&format!("docs/catervas/{path}"), &none);
        assert_eq!(name("product/spec.md"), "Product description");
        assert_eq!(name("architecture/jobs.md"), "Scheduled jobs");
        assert_eq!(name("marketing/brand/persona.md"), "Brand persona");
        assert_eq!(name("delivery/cadence.md"), "Sprint rhythm");
        assert_eq!(name("delivery/ceremony-notes.md"), "Ceremony notes");
        assert_eq!(name("engineering/how_we_test.md"), "How we test");
        assert_eq!(plain_name("product"), "Product");
    }

    #[test]
    fn names_a_plan_by_its_title() {
        let title_of = |stem: &str| match stem {
            "CTV-12" => Some("pie pre-orders".to_string()),
            "MP-3" => Some("Autumn at Corner Bakery".to_string()),
            _ => None,
        };
        let name = |path: &str| document_name(&format!("docs/catervas/{path}"), &title_of);
        assert_eq!(name("architecture/plans/CTV-12.md"), "Plan: pie pre-orders");
        assert_eq!(
            name("marketing/plans/MP-3.md"),
            "Plan: Autumn at Corner Bakery"
        );
        assert_eq!(name("architecture/plans/CTV-99.md"), "CTV 99");
        assert_eq!(name("architecture/plans/old/CTV-12.md"), "CTV 12");
    }

    #[test]
    fn orders_the_known_documents_first() {
        let mut paths = vec![
            "docs/catervas/architecture/plans/CTV-9.md",
            "docs/catervas/architecture/overview.md",
            "docs/catervas/architecture/notes.md",
            "docs/catervas/architecture/features.md",
        ];
        paths.sort_by_key(|path| document_order(path));
        assert_eq!(
            paths,
            [
                "docs/catervas/architecture/overview.md",
                "docs/catervas/architecture/features.md",
                "docs/catervas/architecture/notes.md",
                "docs/catervas/architecture/plans/CTV-9.md",
            ]
        );
    }

    #[test]
    fn checks_the_owners_edit() {
        let roadmap = "docs/catervas/product/roadmap.md";
        assert_eq!(check_owner_edit(roadmap, 10), Ok(roadmap.to_string()));
        assert_eq!(
            check_owner_edit("./docs/catervas/product/roadmap.md", 10),
            Ok(roadmap.to_string())
        );
        for outside in [
            "docs/catervas/../../etc/x.md",
            "/docs/catervas/x.md",
            "docs/other/x.md",
            "docs/Catervas/product/x.md",
        ] {
            assert_eq!(
                check_owner_edit(outside, 10),
                Err(OwnerEditRefusal::OutsideFolders),
                "{outside}"
            );
        }
        assert_eq!(
            check_owner_edit("docs/catervas/product/notes.txt", 10),
            Err(OwnerEditRefusal::NotMarkdown)
        );
        assert_eq!(
            check_owner_edit("docs/catervas/product/roadmap.agent.md", 10),
            Err(OwnerEditRefusal::AgentTwin)
        );
        assert_eq!(
            check_owner_edit("docs/catervas/marketing/plans/MP-3.md", 10),
            Err(OwnerEditRefusal::MarketingPlan)
        );
        assert!(check_owner_edit("docs/catervas/marketing/plans/notes.md", 10).is_ok());
        assert_eq!(
            check_owner_edit(roadmap, 262_145),
            Err(OwnerEditRefusal::TooLarge)
        );
        assert!(check_owner_edit(roadmap, 262_144).is_ok());
        let codes: Vec<&str> = [
            OwnerEditRefusal::OutsideFolders,
            OwnerEditRefusal::NotMarkdown,
            OwnerEditRefusal::AgentTwin,
            OwnerEditRefusal::MarketingPlan,
            OwnerEditRefusal::TooLarge,
        ]
        .iter()
        .map(|refusal| refusal.code())
        .collect();
        assert_eq!(
            codes,
            [
                "outside_folders",
                "not_markdown",
                "agent_twin",
                "marketing_plan",
                "too_large"
            ]
        );
    }

    #[test]
    fn finds_what_a_merge_subject_names() {
        let task = |id: &str| Some(Named::Task(id.parse().expect("a task id")));
        let change = |n| Some(Named::FolderChange(n));
        assert_eq!(
            named_in_subject("Merge CTV-12: Pie pre-orders"),
            task("CTV-12")
        );
        assert_eq!(named_in_subject("CTV-7: Gift cards (#4)"), task("CTV-7"));
        assert_eq!(
            named_in_subject("Merge CTV-12: follow CTV-3"),
            task("CTV-12")
        );
        assert_eq!(
            named_in_subject("Merge docs/folder-3: product/roadmap.md"),
            change(3)
        );
        assert_eq!(
            named_in_subject("Folder change 4: product/spec.md (#9)"),
            change(4)
        );
        assert_eq!(
            named_in_subject("Merge pull request #4 from me/docs/folder-5"),
            change(5)
        );
        assert_eq!(
            named_in_subject("Merge docs/folder-3: architecture/plans/CTV-12.md"),
            change(3)
        );
        assert_eq!(named_in_subject("subfolder-3"), None);
        assert_eq!(named_in_subject("ACTV-1: x"), None);
        assert_eq!(named_in_subject("docs(product): edit roadmap.md"), None);
        assert_eq!(named_in_subject("folder-x and CTV-x"), None);
    }

    #[test]
    fn a_twin_is_stale_when_its_document_moved_past_it() {
        assert!(!twin_is_stale("a1", "a1", 3));
        assert!(!twin_is_stale("a1", "b2", 0));
        assert!(twin_is_stale("a1", "b2", 1));
    }

    #[test]
    fn says_which_twins_are_stale() {
        assert_eq!(stale_line(&[]), None);
        assert_eq!(
            stale_line(&["docs/catervas/product/roadmap.agent.md".to_string()]),
            Some("- stale twins: docs/catervas/product/roadmap.agent.md. Each is behind the document for people beside it, which the owner changed: read that document as well until the twin is brought up to date.".to_string())
        );
    }

    #[test]
    fn lets_a_stale_twin_change_alone() {
        let list = |paths: &[&str]| -> Vec<String> {
            paths.iter().map(|path| (*path).to_string()).collect()
        };
        let roadmap = product("roadmap.md");
        let roadmap_twin = product("roadmap.agent.md");
        let spec_twin = product("spec.agent.md");
        assert!(pair_changed_alone(&list(&[&roadmap_twin]), &list(&[&roadmap_twin])).is_empty());
        assert_eq!(
            pair_changed_alone(&list(&[&roadmap_twin]), &list(&[&spec_twin])),
            list(&[&roadmap_twin])
        );
        assert_eq!(
            pair_changed_alone(&list(&[&roadmap]), &list(&[&roadmap_twin])),
            list(&[&roadmap])
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

    fn shown(lines: &[(LineChange, &str)]) -> Vec<ShownLine> {
        lines
            .iter()
            .map(|(change, text)| ShownLine {
                change: *change,
                text: (*text).to_string(),
            })
            .collect()
    }

    const GIFT: &str = "- Gift cards. Buy a card online, spend it in the shop.";
    const OLD_PIE: &str = "- Pie pre-orders. Customers order Thanksgiving pies ahead and collect them on 25 or 26 November.";
    const NEW_PIE: &str = "- Pie pre-orders. 41 pies ordered in the first week.";
    const CHRISTMAS: &str =
        "- Christmas orders. Stollen, yule logs and mince pies, collected from 19 to 24 December.";

    #[test]
    fn shows_the_roadmaps_changes_as_drawn() {
        let before = format!(
            "# Roadmap\n\n## Done\n\n## Now\n\n{OLD_PIE}\n\n## Next\n\n{GIFT}\n\n- Opening hours on every page.\n"
        );
        let after = format!(
            "# Roadmap\n\n## Done\n\n{NEW_PIE}\n\n## Now\n\n{GIFT}\n\n## Next\n\n{CHRISTMAS}\n\n- Opening hours on every page.\n"
        );
        assert_eq!(
            changed_lines(&before, &after),
            shown(&[
                (LineChange::Same, "## Done"),
                (LineChange::Added, NEW_PIE),
                (LineChange::Same, "## Now"),
                (LineChange::Removed, OLD_PIE),
                (LineChange::Added, GIFT),
                (LineChange::Same, "## Next"),
                (LineChange::Removed, GIFT),
                (LineChange::Added, CHRISTMAS),
                (LineChange::Same, "- Opening hours on every page."),
            ])
        );
    }

    #[test]
    fn shows_the_heading_above_a_change() {
        let before = "# Little Oak Bakery\n\nWhat we sell.\n\n## Ordering ahead\n\nCustomers pick a day and a time to collect their order.\n";
        let after = format!(
            "{before}\nPre-orders close 3 days before collection, so the kitchen can plan.\n"
        );
        assert_eq!(
            changed_lines(before, &after),
            shown(&[
                (LineChange::Same, "## Ordering ahead"),
                (
                    LineChange::Same,
                    "Customers pick a day and a time to collect their order."
                ),
                (
                    LineChange::Added,
                    "Pre-orders close 3 days before collection, so the kitchen can plan."
                ),
            ])
        );
    }

    #[test]
    fn shows_a_new_document_whole_and_an_unchanged_one_empty() {
        assert_eq!(
            changed_lines("", "# New\n\n\nOne.\n\nTwo.\n"),
            shown(&[
                (LineChange::Added, "# New"),
                (LineChange::Added, "One."),
                (LineChange::Added, "Two."),
            ])
        );
        assert!(changed_lines("# A\nB\n", "# A\nB\n").is_empty());
        assert!(changed_lines("# A\nB\n", "# A\r\n\r\n\nB\r\n\n").is_empty());
    }

    #[test]
    fn shows_a_change_too_big_to_compare_whole() {
        let old: Vec<String> = (0..1001).map(|n| format!("old {n}")).collect();
        let new: Vec<String> = (0..1001).map(|n| format!("new {n}")).collect();
        let before = format!("first\n{}\nlast\n", old.join("\n"));
        let after = format!("first\n{}\nlast\n", new.join("\n"));
        let mut expected = vec![(LineChange::Same, "first".to_string())];
        expected.extend(old.iter().map(|t| (LineChange::Removed, t.clone())));
        expected.extend(new.iter().map(|t| (LineChange::Added, t.clone())));
        expected.push((LineChange::Same, "last".to_string()));
        let expected: Vec<ShownLine> = expected
            .into_iter()
            .map(|(change, text)| ShownLine { change, text })
            .collect();
        assert_eq!(changed_lines(&before, &after), expected);
    }

    #[test]
    fn knows_which_paths_may_change_the_product_plan() {
        let globs = |globs: &[&str]| globs.iter().map(ToString::to_string).collect::<Vec<_>>();
        for may in [
            &["docs/catervas/product/**"][..],
            &["docs/catervas/product/*.md"],
            &["docs/catervas/product/spec.md"],
            &["docs/catervas/product/{spec,notes}.md"],
            &["**"],
            &["docs/**"],
            &["docs/["],
        ] {
            assert!(may_change_an_owner_document(&globs(may)), "{may:?}");
        }
        for may_not in [
            &["docs/catervas/product/reports/**"][..],
            &["docs/catervas/product/spec.agent.md"],
            &["docs/catervas/product"],
            &["Docs/Catervas/Product/spec.md"],
            &["src/**"],
            &[],
        ] {
            assert!(
                !may_change_an_owner_document(&globs(may_not)),
                "{may_not:?}"
            );
        }
    }
}
