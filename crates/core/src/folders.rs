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

const TWIN_MARK: &str = ".agent";
const PLANS: &str = "docs/catervas/marketing/plans/";

/// The folder a role owns, or `None` for a role that has none.
#[must_use]
pub fn role_folder(role: Role) -> Option<&'static str> {
    ROLE_FOLDERS
        .iter()
        .find_map(|(owner, folder)| (*owner == role).then_some(*folder))
}

/// The role folders each repository of a project folder has of its own, under
/// `docs/catervas/repos/<name>/` (ADR 0057): the same role owns it as owns the project-wide folder
/// of the same name. Product, delivery and marketing stay project-wide.
pub const MEMBER_FOLDERS: [(Role, &str); 3] = [
    (Role::Architect, "architecture"),
    (Role::SoftwareDeveloper, "engineering"),
    (Role::UiUxDesigner, "design"),
];

const MEMBERS_ROOT: &str = "docs/catervas/repos";

/// A role's folder of its own in the repository `repo`, or `None` for a role that has none.
#[must_use]
pub fn member_folder(role: Role, repo: &str) -> Option<String> {
    MEMBER_FOLDERS.iter().find_map(|(owner, segment)| {
        (*owner == role).then(|| format!("{MEMBERS_ROOT}/{repo}/{segment}"))
    })
}

/// The folder that holds `path` and the role that owns it: a role folder, or a member folder of
/// one of `members`. The path must be at least one segment below the folder.
#[must_use]
pub fn folder_of(path: &str, members: &[&str]) -> Option<(Role, String)> {
    let path = normalise(path)?;
    let below = |folder: &str| {
        path.strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
    };
    ROLE_FOLDERS
        .iter()
        .find(|(_, folder)| below(folder))
        .map(|(owner, folder)| (*owner, (*folder).to_string()))
        .or_else(|| {
            members.iter().find_map(|name| {
                MEMBER_FOLDERS.iter().find_map(|(owner, _)| {
                    let folder = member_folder(*owner, name)?;
                    below(&folder).then_some((*owner, folder))
                })
            })
        })
}

/// The format a team document is written in: its file's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    /// A `.md` file.
    Markdown,
    /// An `.html` file.
    Html,
}

impl DocumentFormat {
    /// The extension, without its dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Html => "html",
        }
    }

    /// The format of a path's extension, `None` for any other.
    #[must_use]
    pub fn of(path: &str) -> Option<Self> {
        if path.strip_suffix(".md").is_some() {
            Some(Self::Markdown)
        } else if path.strip_suffix(".html").is_some() {
            Some(Self::Html)
        } else {
            None
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Markdown => Self::Html,
            Self::Html => Self::Markdown,
        }
    }
}

/// The path without its `.md` or `.html`, `None` for any other extension. An agent version's stem
/// ends `.agent`.
#[must_use]
pub fn stem_of(path: &str) -> Option<&str> {
    path.strip_suffix(".md")
        .or_else(|| path.strip_suffix(".html"))
}

/// The stem as a file in this format.
#[must_use]
pub fn with_format(stem: &str, format: DocumentFormat) -> String {
    format!("{stem}.{}", format.extension())
}

/// The Architect's implementation plan for an epic, without its extension.
#[must_use]
pub fn plan_stem(epic: &str) -> String {
    format!("docs/catervas/architecture/plans/{epic}")
}

/// Whether the task writes an epic's plan: its `plan` is set and its `allowed_paths` are exactly
/// that.
#[must_use]
pub fn is_plan_task(contract: &crate::contract::TaskContract) -> bool {
    contract.plan.as_ref().is_some_and(|own| {
        contract
            .allowed_paths
            .iter()
            .map(ToString::to_string)
            .eq([own.to_string()])
    })
}

/// Whether the path is the Architect's plan for the epic, in either format.
#[must_use]
pub fn is_plan_of(path: &str, epic: &str) -> bool {
    stem_of(path).is_some_and(|stem| stem == plan_stem(epic))
}

/// Whether a path is a human document: the product's `spec` and `roadmap`, or a marketing plan
/// `plans/MP-<n>`, `n` decimal digits not starting with `0`, each as `.md` or `.html`. Each has an
/// agent twin.
#[must_use]
pub fn is_human_document(path: &str) -> bool {
    let Some(path) = normalise(path) else {
        return false;
    };
    let Some(stem) = stem_of(&path) else {
        return false;
    };
    if PRODUCT_PLAN_STEMS.contains(&stem) {
        return true;
    }
    stem.strip_prefix(PLANS)
        .and_then(|name| name.strip_prefix("MP-"))
        .is_some_and(|n| {
            n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() && !n.starts_with('0')
        })
}

/// The twin of a human document: the path with `.agent` before its extension.
#[must_use]
pub fn agent_twin(path: &str) -> Option<String> {
    if !is_human_document(path) {
        return None;
    }
    let path = normalise(path)?;
    let format = DocumentFormat::of(&path)?;
    Some(with_format(
        &format!("{}{TWIN_MARK}", stem_of(&path)?),
        format,
    ))
}

/// The human document a twin belongs to.
#[must_use]
pub fn human_of_twin(path: &str) -> Option<String> {
    let path = normalise(path)?;
    let stem = stem_of(&path)?.strip_suffix(TWIN_MARK)?;
    let human = with_format(stem, DocumentFormat::of(&path)?);
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

/// The stems of the human documents the owner approves through `catervas_write_folder_doc`
/// (section 5.7).
pub const OWNER_ACCEPTED_STEMS: [&str; 2] = [
    "docs/catervas/product/spec",
    "docs/catervas/product/roadmap",
];

/// The stems of the two documents of a new project's product plan, which the owner approves
/// (section 5.3).
pub const PRODUCT_PLAN_STEMS: [&str; 2] = [
    "docs/catervas/product/spec",
    "docs/catervas/product/roadmap",
];

/// Whether both documents of [`PRODUCT_PLAN_STEMS`], each as `.md` or `.html`, are among the
/// paths on the integration branch, compared after `normalise` and with letter case.
#[must_use]
pub fn product_plan_approved(on_integration_branch: &[String]) -> bool {
    let present: Vec<String> = on_integration_branch
        .iter()
        .filter_map(|path| normalise(path))
        .collect();
    PRODUCT_PLAN_STEMS
        .iter()
        .all(|stem| present.iter().any(|path| stem_of(path) == Some(stem)))
}

/// Whether a task with these `allowed_paths` may change a document of [`OWNER_ACCEPTED_STEMS`], in
/// either format: a path passes the Definition of Done's own test of a diff
/// (`check_allowed_paths`), or a glob does not compile, which fails closed.
#[must_use]
pub fn may_change_an_owner_document(allowed_paths: &[String]) -> bool {
    OWNER_ACCEPTED_STEMS.iter().any(|stem| {
        [DocumentFormat::Markdown, DocumentFormat::Html]
            .into_iter()
            .any(|format| {
                !matches!(
                    check_allowed_paths(&[with_format(stem, format)], allowed_paths),
                    Err(PathRefusal::Violations(_))
                )
            })
    })
}

/// A path `catervas_write_folder_doc` may write: normalised, and whether the owner approves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDocPath {
    /// The path, normalised.
    pub path: String,
    /// Whether the path's stem is in [`OWNER_ACCEPTED_STEMS`].
    pub owner_accepted: bool,
}

/// Why a path is not one the caller may write with `catervas_write_folder_doc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderDocPathRefusal {
    /// The role owns no folder.
    NoFolder,
    /// The path is not below the caller's folder.
    Outside,
    /// The path is not a `.md` or `.html` file.
    NotADocument,
    /// The path is an agent version, written from the human document's `agent_text`.
    AgentTwin,
    /// The path is a marketing plan, proposed with `catervas_propose_marketing_plan`.
    MarketingPlan,
}

/// Holds a path to a `.md` or `.html` file at least one segment below the role's folder, or the
/// role's folder of one of `members` (letter case counts), not an agent version, and not a human document the owner does not approve here.
///
/// # Errors
///
/// The first [`FolderDocPathRefusal`] that applies.
pub fn check_folder_doc_path(
    role: Role,
    path: &str,
    members: &[&str],
) -> Result<FolderDocPath, FolderDocPathRefusal> {
    role_folder(role).ok_or(FolderDocPathRefusal::NoFolder)?;
    let path = normalise(path).ok_or(FolderDocPathRefusal::Outside)?;
    if folder_of(&path, members).is_none_or(|(owner, _)| owner != role) {
        return Err(FolderDocPathRefusal::Outside);
    }
    let stem = stem_of(&path).ok_or(FolderDocPathRefusal::NotADocument)?;
    if stem.ends_with(TWIN_MARK) {
        return Err(FolderDocPathRefusal::AgentTwin);
    }
    let owner_accepted = OWNER_ACCEPTED_STEMS.contains(&stem);
    if is_human_document(&path) && !owner_accepted {
        return Err(FolderDocPathRefusal::MarketingPlan);
    }
    Ok(FolderDocPath {
        path,
        owner_accepted,
    })
}

/// Why a path is not in a format the team writes (`docs/design/html-documents.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatRefusal {
    /// A new document, written in the other format than the team's.
    WrongFormat {
        /// The path in the team's format.
        instead: String,
    },
    /// An owner-accepted document that is there in the other format, changed there.
    OtherFormatExists {
        /// The path of the file that is there.
        existing: String,
    },
}

/// Holds a write to the team's format, or to the format a document is already in. `on_branch`
/// says whether the integration branch holds a path. `Ok(Some(other))` is the same stem's file in
/// the other format, which the write replaces; `Ok(None)` is a plain write.
///
/// # Errors
///
/// [`FormatRefusal::WrongFormat`] for a new document in the other format, and
/// [`FormatRefusal::OtherFormatExists`] for a team-format write of an owner-accepted document
/// whose other-format file is there.
pub fn check_document_format(
    path: &str,
    format: DocumentFormat,
    on_branch: &dyn Fn(&str) -> bool,
) -> Result<Option<String>, FormatRefusal> {
    let path = normalise(path).unwrap_or_else(|| path.to_string());
    let (Some(stem), Some(written)) = (stem_of(&path), DocumentFormat::of(&path)) else {
        return Ok(None);
    };
    if written == format {
        let other = with_format(stem, format.other());
        if !on_branch(&other) {
            return Ok(None);
        }
        if OWNER_ACCEPTED_STEMS.contains(&stem) {
            return Err(FormatRefusal::OtherFormatExists { existing: other });
        }
        return Ok(Some(other));
    }
    if on_branch(&path) {
        return Ok(None);
    }
    Err(FormatRefusal::WrongFormat {
        instead: with_format(stem, format),
    })
}

/// What an artifact check requires of a document: a heading, and for the architecture's overview a
/// diagram (an inline SVG, or a Mermaid block).
#[must_use]
pub fn artifact_marks(path: &str, format: DocumentFormat) -> Vec<&'static str> {
    let (heading, diagram) = match format {
        DocumentFormat::Markdown => ("# ", "```mermaid"),
        DocumentFormat::Html => ("<h1", "<svg"),
    };
    let mut marks = vec![heading];
    if path.ends_with(&with_format("architecture/overview", format)) {
        marks.push(diagram);
    }
    marks
}

/// The Team rules' line about the format of documents (`docs/design/html-documents.md`).
#[must_use]
pub fn format_line(format: DocumentFormat) -> String {
    match format {
        DocumentFormat::Html => "- documents: write a new document under docs/catervas/ as HTML (`.html`, a human document's agent version `.agent.html`): one self-contained page with `<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">`, one `<h1>`, styles in one `<style>` block, widths that fit a phone (percentages, `max-width`), diagrams, flowcharts and charts as inline SVG with a `viewBox`, one block element per line, no scripts and nothing loaded from anywhere else. A document already in `.md` stays Markdown when you change it. The kit names documents without their extension.".to_string(),
        DocumentFormat::Markdown => "- documents: write a new document under docs/catervas/ as Markdown (`.md`, a human document's agent version `.agent.md`), diagrams in Mermaid blocks. A document already in `.html` stays HTML when you change it. The kit names documents without their extension.".to_string(),
    }
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

/// The commit message of a folder change: `docs(<the folder's last segment, or `<repo>-<segment>`
/// for a member folder>): <each path within
/// the folder, joined by ", "> by <agent name>`, with `, approved by the owner` after an approval.
#[must_use]
pub fn folder_doc_message(
    folder: &str,
    paths: &[&str],
    agent_name: &str,
    approved: bool,
) -> String {
    let scope = folder
        .strip_prefix(MEMBERS_ROOT)
        .and_then(|rest| rest.strip_prefix('/'))
        .map_or_else(
            || folder.rsplit('/').next().unwrap_or(folder).to_string(),
            |rest| rest.replace('/', "-"),
        );
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

/// The documents with a fixed name, by stem, in the order the Files page lists them. A step that
/// adds a document adds its name here.
pub const DOCUMENT_STEMS: [(&str, &str); 12] = [
    ("docs/catervas/product/spec", "Product description"),
    ("docs/catervas/product/roadmap", "Roadmap"),
    ("docs/catervas/architecture/overview", "Overview"),
    ("docs/catervas/architecture/features", "Features"),
    ("docs/catervas/architecture/jobs", "Scheduled jobs"),
    ("docs/catervas/architecture/structure", "Structure"),
    ("docs/catervas/engineering/conventions", "Conventions"),
    ("docs/catervas/engineering/workflow", "Workflow"),
    ("docs/catervas/design/ui-inventory", "UI inventory"),
    ("docs/catervas/delivery/cadence", "Sprint rhythm"),
    ("docs/catervas/marketing/brand/brand-kit", "Brand kit"),
    ("docs/catervas/marketing/brand/persona", "Brand persona"),
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

/// A path under `docs/catervas/repos/<name>/` as the same path without those two segments, so a
/// member's document is named and placed as the project-wide one of its stem; any other path as is.
fn without_member(path: &str) -> String {
    path.strip_prefix(MEMBERS_ROOT)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split_once('/'))
        .map_or_else(
            || path.to_string(),
            |(_, rest)| format!("docs/catervas/{rest}"),
        )
}

/// What the Files page calls a document: its fixed name, "Plan: <title>" for a file directly in a
/// folder's `plans/` whose stem `title_of` knows, else its file stem as [`plain_name`] has it.
#[must_use]
pub fn document_name(path: &str, title_of: &dyn Fn(&str) -> Option<String>) -> String {
    let path = normalise(path).unwrap_or_else(|| path.to_string());
    let project_wide = without_member(&path);
    if let Some((_, name)) = DOCUMENT_STEMS
        .iter()
        .find(|(known, _)| stem_of(&project_wide) == Some(*known))
    {
        return (*name).to_string();
    }
    let file = path.rsplit('/').next().unwrap_or(&path);
    let stem = stem_of(file).unwrap_or(file);
    let in_plans = ROLE_FOLDERS
        .iter()
        .any(|(_, folder)| path == format!("{folder}/plans/{file}"));
    if in_plans && let Some(title) = title_of(stem) {
        return format!("Plan: {title}");
    }
    plain_name(stem)
}

/// The sort key of a document: its place in [`DOCUMENT_STEMS`], the others after, then its path.
#[must_use]
pub fn document_order(path: &str) -> (usize, String) {
    let place = DOCUMENT_STEMS
        .iter()
        .position(|(known, _)| stem_of(&without_member(path)) == Some(*known))
        .unwrap_or(DOCUMENT_STEMS.len());
    (place, path.to_string())
}

/// The most the owner may save in one document, in bytes.
pub const OWNER_EDIT_LIMIT_BYTES: usize = 262_144;

/// Why the owner may not save a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerEditRefusal {
    /// The path climbs out, or is not under `docs/catervas/`.
    OutsideFolders,
    /// The path is not a `.md` or `.html` file.
    NotADocument,
    /// The path is an agent version, which the team writes.
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
            Self::NotADocument => "not_a_document",
            Self::AgentTwin => "agent_twin",
            Self::MarketingPlan => "marketing_plan",
            Self::TooLarge => "too_large",
        }
    }
}

/// Holds the owner's edit of a document to a `.md` or `.html` file under `docs/catervas/` that is
/// not an agent version or a marketing plan, within [`OWNER_EDIT_LIMIT_BYTES`]. Whether a folder of
/// an active role holds it is the caller's to check.
///
/// # Errors
///
/// The first [`OwnerEditRefusal`] that applies.
pub fn check_owner_edit(path: &str, text_bytes: usize) -> Result<String, OwnerEditRefusal> {
    let path = normalise(path)
        .filter(|path| path.starts_with("docs/catervas/"))
        .ok_or(OwnerEditRefusal::OutsideFolders)?;
    let stem = stem_of(&path).ok_or(OwnerEditRefusal::NotADocument)?;
    if stem.ends_with(TWIN_MARK) {
        return Err(OwnerEditRefusal::AgentTwin);
    }
    if is_human_document(&path) && !OWNER_ACCEPTED_STEMS.contains(&stem) {
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
/// the reader's own, in table order; which one is the reader's; how to read them; and, in a
/// project folder, the reader's member folders.
#[must_use]
pub fn folders_line(role: Role, active_roles: &[Role], members: &[&str]) -> String {
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
        ReadAccess::Open => "Read any of them, and where a document has an .agent.html or .agent.md twin beside it, read the twin, which is written for agents.".to_string(),
        held @ ReadAccess::Only(_) => format!(
            "Read only {}: Catervas refuses a read anywhere else, and a search that names no path. Where a document there has an .agent.html or .agent.md twin beside it, read the twin, which is written for agents.",
            held.named()
        ),
    };
    let own = member_folders_sentence(role, members);
    format!("- folders: {}. {yours} {reading}{own}", listed.join(", "))
}

/// ` Each repository has its own <segment>/ folder at <list>, which is yours too.` for a role with
/// member folders in a project folder of `members`, else nothing.
fn member_folders_sentence(role: Role, members: &[&str]) -> String {
    let Some((_, segment)) = MEMBER_FOLDERS.iter().find(|(owner, _)| *owner == role) else {
        return String::new();
    };
    let Some((final_name, others)) = members.split_last() else {
        return String::new();
    };
    let at = |name: &str| format!("repos/{name}/{segment}/");
    let places = if others.is_empty() {
        at(final_name)
    } else {
        let first: Vec<String> = others.iter().map(|name| at(name)).collect();
        format!("{} and {}", first.join(", "), at(final_name))
    };
    format!(" Each repository has its own {segment}/ folder at {places}, which is yours too.")
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

/// Whether a line is a heading: it starts with `#`, or, trimmed, with `<h1` to `<h6` and then `>`
/// or a space or tab.
fn is_heading(line: &str) -> bool {
    let bytes = line.trim_start().as_bytes();
    line.starts_with('#')
        || (bytes.len() > 3
            && bytes.starts_with(b"<h")
            && (b'1'..=b'6').contains(&bytes[2])
            && matches!(bytes[3], b'>' | b' ' | b'\t'))
}

/// The lines of two versions of a document that are worth showing: every changed line, the
/// unchanged line just before and just after each run of changes, and the nearest unchanged
/// heading (a line starting with `#`, or an HTML heading) above each run (section 5.7). Blank lines and a trailing
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
            .rposition(|(change, text)| *change == LineChange::Same && is_heading(text))
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
        DocumentFormat, FolderDocPathRefusal, FormatRefusal, OWNER_ACCEPTED_STEMS, ROLE_FOLDERS,
        ReadAccess, agent_twin, check_document_format, check_folder_doc_path, folder_doc_author,
        folder_doc_message, folder_of, folders_line, format_line, human_of_twin, is_human_document,
        is_plan_of, pair_changed_alone, plan_stem, product_plan_approved, read_access, role_folder,
        stem_of, with_format,
    };
    use super::{
        LineChange, ShownLine, artifact_marks, changed_lines, is_heading,
        may_change_an_owner_document,
    };
    use super::{
        Named, OwnerEditRefusal, check_owner_edit, document_name, document_order, named_in_subject,
        plain_name, stale_line, twin_is_stale,
    };
    use crate::team::Role;

    #[test]
    fn the_plan_is_approved_when_both_documents_are_there() {
        let paths = |list: &[&str]| list.iter().map(|p| (*p).to_string()).collect::<Vec<_>>();
        let spec = "docs/catervas/product/spec.md";
        let roadmap = "docs/catervas/product/roadmap.md";
        assert!(product_plan_approved(&paths(&[spec, roadmap])));
        assert!(product_plan_approved(&paths(&[roadmap, "README.md", spec])));
        assert!(product_plan_approved(&paths(&[
            "./docs/catervas/product/spec.md",
            roadmap
        ])));
        assert!(!product_plan_approved(&[]));
        assert!(!product_plan_approved(&paths(&[spec])));
        assert!(!product_plan_approved(&paths(&[roadmap])));
        assert!(!product_plan_approved(&paths(&[
            "docs/catervas/product/spec.agent.md",
            "docs/catervas/product/roadmap.agent.md"
        ])));
        assert!(!product_plan_approved(&paths(&[
            "Docs/Catervas/Product/spec.md",
            roadmap
        ])));
    }

    #[test]
    fn names_an_epics_plan() {
        assert_eq!(plan_stem("CTV-3"), "docs/catervas/architecture/plans/CTV-3");
    }

    const READ: &str = "Read any of them, and where a document has an .agent.html or .agent.md twin beside it, read the twin, which is written for agents.";

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
        assert_eq!(name("design/ui-inventory.md"), "UI inventory");
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
            Err(OwnerEditRefusal::NotADocument)
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
            OwnerEditRefusal::NotADocument,
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
                "not_a_document",
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
            &[],
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
            folders_line(Role::FinanceSpecialist, &[Role::ProductManager], &[]),
            format!(
                "- folders: docs/catervas/product/ (Product Manager). You have none: write none of them. {READ}"
            )
        );
        assert_eq!(
            folders_line(Role::FinanceSpecialist, &[], &[]),
            "- folders: none"
        );
    }

    #[test]
    fn lists_the_readers_own_folder_when_it_is_not_active() {
        let line = folders_line(Role::Architect, &[Role::ProductManager], &[]);
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
                ],
                &[]
            ),
            "- folders: docs/catervas/product/ (Product Manager), docs/catervas/architecture/ (Architect), docs/catervas/marketing/ (Marketing Specialist). Yours is docs/catervas/marketing/: write no other folder named here. Read only docs/catervas/product/ and docs/catervas/marketing/: Catervas refuses a read anywhere else, and a search that names no path. Where a document there has an .agent.html or .agent.md twin beside it, read the twin, which is written for agents."
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
            let ok = check_folder_doc_path(Role::ScrumMaster, path, &[]).expect(path);
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
                check_folder_doc_path(Role::ScrumMaster, path, &[]),
                Err(FolderDocPathRefusal::Outside),
                "{path}"
            );
        }
        assert_eq!(
            check_folder_doc_path(Role::ScrumMaster, "docs/catervas/delivery/x.txt", &[]),
            Err(FolderDocPathRefusal::NotADocument)
        );
        assert_eq!(
            check_folder_doc_path(Role::ScrumMaster, "docs/catervas/delivery/x.agent.md", &[]),
            Err(FolderDocPathRefusal::AgentTwin)
        );
        for name in ["spec.md", "roadmap.md"] {
            let ok = check_folder_doc_path(Role::ProductManager, &product(name), &[]).expect(name);
            assert!(ok.owner_accepted, "{name}");
        }
        assert!(
            !check_folder_doc_path(Role::ProductManager, &product("notes.md"), &[])
                .expect("notes")
                .owner_accepted
        );
        assert_eq!(
            check_folder_doc_path(
                Role::MarketingSpecialist,
                "docs/catervas/marketing/plans/MP-3.md",
                &[]
            ),
            Err(FolderDocPathRefusal::MarketingPlan)
        );
        assert!(
            check_folder_doc_path(
                Role::MarketingSpecialist,
                "docs/catervas/marketing/plans/notes.md",
                &[]
            )
            .is_ok()
        );
        assert_eq!(
            check_folder_doc_path(Role::FinanceSpecialist, "docs/catervas/finance/x.md", &[]),
            Err(FolderDocPathRefusal::NoFolder)
        );
        assert_eq!(OWNER_ACCEPTED_STEMS, [product("spec"), product("roadmap")]);
        assert!(
            OWNER_ACCEPTED_STEMS
                .iter()
                .all(|stem| is_human_document(&with_format(stem, DocumentFormat::Markdown)))
        );
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

    fn list(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_string()).collect()
    }

    fn cat(path: &str) -> String {
        format!("docs/catervas/{path}")
    }

    #[test]
    fn stem_drops_md_or_html_and_nothing_else() {
        assert_eq!(stem_of("a/b.md"), Some("a/b"));
        assert_eq!(stem_of("a/b.html"), Some("a/b"));
        assert_eq!(stem_of("a/b.txt"), None);
        assert_eq!(stem_of("a/b"), None);
        assert_eq!(stem_of("a/spec.agent.html"), Some("a/spec.agent"));
        assert_eq!(with_format("a/b", DocumentFormat::Html), "a/b.html");
        assert_eq!(DocumentFormat::of("a/b.md"), Some(DocumentFormat::Markdown));
        assert_eq!(DocumentFormat::of("a/b.htm"), None);
        assert_eq!(DocumentFormat::Html.extension(), "html");
    }

    #[test]
    fn pairs_in_either_format() {
        for path in [
            "product/spec.md",
            "product/spec.html",
            "product/roadmap.html",
            "marketing/plans/MP-3.html",
        ] {
            assert!(is_human_document(&cat(path)), "{path}");
        }
        for path in ["marketing/plans/MP-03.html", "product/spec.htm"] {
            assert!(!is_human_document(&cat(path)), "{path}");
        }
        let html = cat("product/spec.html");
        assert_eq!(agent_twin(&html), Some(cat("product/spec.agent.html")));
        assert_eq!(human_of_twin(&cat("product/spec.agent.html")), Some(html));
        assert_eq!(
            human_of_twin(&cat("product/spec.agent.md")),
            Some(cat("product/spec.md"))
        );
    }

    #[test]
    fn pair_changed_alone_holds_html_pairs() {
        let (spec, twin) = (cat("product/spec.html"), cat("product/spec.agent.html"));
        assert_eq!(
            pair_changed_alone(std::slice::from_ref(&spec), &[]),
            [cat("product/spec.html")]
        );
        assert!(pair_changed_alone(&[spec, twin.clone()], &[]).is_empty());
        let md = cat("product/spec.md");
        assert_eq!(
            pair_changed_alone(&[md.clone(), twin.clone()], &[]),
            [md, twin]
        );
    }

    #[test]
    fn owner_documents_and_product_plan_by_stem() {
        assert!(may_change_an_owner_document(&list(&[
            "docs/catervas/product/spec.html"
        ])));
        let plan = |paths: &[&str]| product_plan_approved(&list(paths));
        assert!(plan(&[
            "docs/catervas/product/spec.html",
            "docs/catervas/product/roadmap.md"
        ]));
        assert!(!plan(&["docs/catervas/product/spec.html"]));
    }

    #[test]
    fn folder_doc_path_takes_md_and_html() {
        let architect = |path: &str| check_folder_doc_path(Role::Architect, &cat(path), &[]);
        assert!(architect("architecture/overview.html").is_ok());
        assert_eq!(
            architect("architecture/overview.txt"),
            Err(FolderDocPathRefusal::NotADocument)
        );
        assert_eq!(
            architect("architecture/spec.agent.html"),
            Err(FolderDocPathRefusal::AgentTwin)
        );
        assert_eq!(
            check_folder_doc_path(
                Role::MarketingSpecialist,
                &cat("marketing/plans/MP-2.html"),
                &[]
            ),
            Err(FolderDocPathRefusal::MarketingPlan)
        );
        let spec = check_folder_doc_path(Role::ProductManager, &cat("product/spec.html"), &[]);
        assert!(spec.expect("spec").owner_accepted);
    }

    #[test]
    fn owner_edit_takes_md_and_html() {
        let roadmap = cat("product/roadmap.html");
        assert_eq!(check_owner_edit(&roadmap, 10), Ok(roadmap));
        assert_eq!(
            check_owner_edit(&cat("product/notes.txt"), 10),
            Err(OwnerEditRefusal::NotADocument)
        );
        assert_eq!(
            check_owner_edit(&cat("product/roadmap.agent.html"), 10),
            Err(OwnerEditRefusal::AgentTwin)
        );
        assert_eq!(OwnerEditRefusal::NotADocument.code(), "not_a_document");
    }

    fn on(held: &[&str]) -> impl Fn(&str) -> bool {
        let held = list(held);
        move |path: &str| held.iter().any(|h| h == path)
    }

    #[test]
    fn format_check_writes_the_teams_format() {
        let check = |path: &str| check_document_format(&cat(path), DocumentFormat::Html, &on(&[]));
        assert_eq!(check("architecture/overview.html"), Ok(None));
        assert_eq!(
            check("architecture/overview.md"),
            Err(FormatRefusal::WrongFormat {
                instead: cat("architecture/overview.html")
            })
        );
    }

    #[test]
    fn format_check_keeps_an_existing_documents_format() {
        let held = on(&["docs/catervas/delivery/cadence.md"]);
        let html = |path: &str| check_document_format(&cat(path), DocumentFormat::Html, &held);
        assert_eq!(html("delivery/cadence.md"), Ok(None));
        assert_eq!(
            html("delivery/cadence.html"),
            Ok(Some(cat("delivery/cadence.md")))
        );
        let held = on(&["docs/catervas/architecture/overview.html"]);
        assert_eq!(
            check_document_format(
                &cat("architecture/overview.html"),
                DocumentFormat::Markdown,
                &held
            ),
            Ok(None)
        );
    }

    #[test]
    fn format_check_refuses_a_spec_changing_format() {
        let held = on(&["docs/catervas/product/spec.md"]);
        assert_eq!(
            check_document_format(&cat("product/spec.html"), DocumentFormat::Html, &held),
            Err(FormatRefusal::OtherFormatExists {
                existing: cat("product/spec.md")
            })
        );
    }

    #[test]
    fn document_names_by_stem() {
        let name = |path: &str, title: Option<&str>| {
            document_name(&cat(path), &|_: &str| title.map(str::to_string))
        };
        assert_eq!(name("product/spec.html", None), "Product description");
        assert_eq!(
            name("architecture/plans/CTV-3.html", Some("Title")),
            "Plan: Title"
        );
        assert_eq!(name("engineering/notes-on-x.html", None), "Notes on x");
        let mut paths = vec![
            cat("architecture/notes.html"),
            cat("architecture/overview.html"),
            cat("product/roadmap.md"),
            cat("product/spec.html"),
        ];
        paths.sort_by_key(|path| document_order(path));
        assert_eq!(
            paths,
            [
                cat("product/spec.html"),
                cat("product/roadmap.md"),
                cat("architecture/overview.html"),
                cat("architecture/notes.html")
            ]
        );
    }

    #[test]
    fn plans_in_either_format() {
        let plans = |name: &str| format!("docs/catervas/architecture/plans/{name}");
        assert!(is_plan_of(&plans("CTV-3.html"), "CTV-3"));
        assert!(is_plan_of(&plans("CTV-3.md"), "CTV-3"));
        assert!(!is_plan_of(&plans("CTV-30.md"), "CTV-3"));
    }

    #[test]
    fn artifact_marks_ask_for_a_heading_and_the_overviews_diagram() {
        use DocumentFormat::{Html, Markdown};
        assert_eq!(
            artifact_marks("docs/catervas/product/spec.html", Html),
            ["<h1"]
        );
        assert_eq!(
            artifact_marks("docs/catervas/product/spec.md", Markdown),
            ["# "]
        );
        assert_eq!(
            artifact_marks("docs/catervas/architecture/overview.html", Html),
            ["<h1", "<svg"]
        );
        assert_eq!(
            artifact_marks("docs/catervas/architecture/overview.md", Markdown),
            ["# ", "```mermaid"]
        );
    }

    #[test]
    fn a_heading_may_be_followed_by_a_tab() {
        assert!(is_heading("<h2\tclass=x>"));
        assert!(!is_heading("<h2x>"));
    }

    #[test]
    fn format_line_says_each_format() {
        let html = format_line(DocumentFormat::Html);
        assert!(html.starts_with("- documents:"), "{html}");
        for needle in [
            ".agent.html",
            "inline SVG",
            "no scripts",
            "viewport",
            "one block element per line",
            "stays Markdown",
        ] {
            assert!(html.contains(needle), "{needle}: {html}");
        }
        let md = format_line(DocumentFormat::Markdown);
        assert!(md.starts_with("- documents:"), "{md}");
        for needle in [".agent.md", "Mermaid", "stays HTML"] {
            assert!(md.contains(needle), "{needle}: {md}");
        }
    }

    #[test]
    fn html_headings_head_a_run() {
        let page = |last: &str| {
            format!("<h1>T</h1>\n<p>a</p>\n<h2>Risks</h2>\n<p>b</p>\n<p>mid</p>\n<p>{last}</p>\n")
        };
        let under_risks = changed_lines(&page("c"), &page("d"));
        assert_eq!(under_risks[0].text, "<h2>Risks</h2>");
        let before = "<header>\n<p>x</p>\n<p>y</p>\n<p>z</p>\n<p>w</p>";
        let after = "<header>\n<p>x</p>\n<p>y</p>\n<p>z</p>\n<p>q</p>";
        assert!(
            changed_lines(before, after)
                .iter()
                .all(|line| line.text != "<header>")
        );
    }

    #[test]
    fn folders_line_names_either_agent_version() {
        assert!(folders_line(Role::Architect, &[], &[]).contains(".agent.html or .agent.md"));
    }

    #[test]
    fn gives_each_member_its_own_role_folders() {
        let app = |tail: &str| format!("docs/catervas/repos/app/{tail}");
        let members = ["app", "infra"];
        assert_eq!(
            folder_of(&app("architecture/overview.md"), &members),
            Some((
                Role::Architect,
                "docs/catervas/repos/app/architecture".to_string()
            ))
        );
        assert_eq!(
            folder_of(
                "docs/catervas/repos/infra/engineering/conventions.md",
                &members
            )
            .map(|(role, _)| role),
            Some(Role::SoftwareDeveloper)
        );
        assert_eq!(
            folder_of("docs/catervas/architecture/system.html", &members),
            Some((Role::Architect, "docs/catervas/architecture".to_string()))
        );
        for none in [
            "docs/catervas/repos/web/architecture/x.md",
            "docs/catervas/repos/app/product/x.md",
            "docs/catervas/repos/app/x.md",
        ] {
            assert_eq!(folder_of(none, &members), None, "{none}");
        }
        // The write tool takes only the caller's own member folders.
        let features = app("architecture/features.md");
        assert!(check_folder_doc_path(Role::Architect, &features, &["app"]).is_ok());
        for refused in [
            check_folder_doc_path(Role::SoftwareDeveloper, &features, &["app"]),
            check_folder_doc_path(Role::Architect, &features, &[]),
        ] {
            assert_eq!(refused.unwrap_err(), FolderDocPathRefusal::Outside);
        }
        assert!(
            check_folder_doc_path(
                Role::SoftwareDeveloper,
                &app("engineering/workflow.md"),
                &["app"]
            )
            .is_ok()
        );
        // The Files page names a member's document as the project's own.
        let none = |_: &str| None;
        assert_eq!(
            document_name(&app("architecture/overview.html"), &none),
            "Overview"
        );
        assert_eq!(
            document_order(&app("architecture/overview.html")).0,
            document_order("docs/catervas/architecture/overview.md").0
        );
        let title_of = |stem: &str| (stem == "CTV-1").then(|| "Login".to_string());
        assert_eq!(
            document_name(&app("architecture/plans/CTV-1.html"), &title_of),
            "CTV 1"
        );
        // Each session is told its member folders; the commit scope names the repository.
        let line = folders_line(Role::Architect, &[Role::Architect], &["app", "infra"]);
        assert!(line.ends_with(
            "Each repository has its own architecture/ folder at repos/app/architecture/ and repos/infra/architecture/, which is yours too."
        ), "{line}");
        assert!(
            !folders_line(Role::Architect, &[Role::Architect], &[]).contains("Each repository")
        );
        assert_eq!(
            folder_doc_message(
                "docs/catervas/repos/app/architecture",
                &["docs/catervas/repos/app/architecture/features.html"],
                "Ada",
                false
            ),
            "docs(app-architecture): features.html by Ada"
        );
    }
}
