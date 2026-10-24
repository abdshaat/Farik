//! `catervas_write_folder_doc` (`docs/SPEC.md` 5.17) and the folder change it makes (5.14): at a
//! delivery's ceremony an agent keeps a small recurring document of its own folder under
//! `docs/catervas/`. A document for agents is committed by Catervas on a branch of its own, and the
//! team's integration policy adds it to the project as it does an accepted task's branch. The
//! product's spec and roadmap, which the owner edits, are written the same way, both files and their
//! agent versions as one change, at the review (ADR 0061).

use catervas_core::branch::{ChangeType, branch_slug, free_branch};
use catervas_core::contract::{TaskId, TaskStatus};
use catervas_core::folders::{
    DocumentFormat, FolderDocPath, FolderDocPathRefusal, FormatRefusal, LineChange, ROLE_FOLDERS,
    agent_twin, changed_lines, check_document_format, check_folder_doc_path, document_name,
    folder_doc_author, folder_doc_message, is_human_document, role_folder,
};
use catervas_core::governor::paths::check_protected_paths;
use catervas_core::team::Team;
use catervas_protocol::event::{EventBody, EventKind, Thread};
use catervas_store::CommitOutcome;
use catervas_store::branches::taken_branches;
use catervas_store::folder_docs::{FolderDocProposal, ProposalState, folder_docs};
use catervas_store::requests::{
    RequestError, file_rederive_request, placeholder_budget_usd, rederive_request, rederive_tasks,
};
use catervas_store::waiting::name_of;
use catervas_store::{EventQuery, StoreError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolDeps, ToolError, failed};
use crate::session::SessionPurpose;
use crate::transitions::{integration_branch, integration_lock};

/// `catervas_write_folder_doc`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteFolderDocInput {
    /// The document: a `.md` or `.html` file under your folder, `docs/catervas/<folder>/...`.
    pub path: String,
    /// The document, in full: at most 262,144 bytes. For the product's spec and roadmap, the
    /// version for people.
    pub text: String,
    /// Only for the product's spec and roadmap: their agent version (`.agent.md` or
    /// `.agent.html`), in full.
    pub agent_text: Option<String>,
    /// Only for the product's spec and roadmap, and no longer needed: it is not recorded, whether
    /// given or not.
    pub summary: Option<String>,
}

/// The most a document says, in bytes: the Files page's cap.
const DOC_BYTES: usize = 262_144;
/// A folder change made: its number, its branch, its commit, and what happens to it next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Landed {
    pub change: u64,
    pub branch: String,
    pub sha: String,
    pub next: String,
}

/// Why a folder change was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LandRefusal {
    /// The owner has an uncommitted change to these files at the project's root.
    Busy(Vec<String>),
    /// This path, or a directory on the way to a file, is a link.
    Link(String),
    /// An earlier folder change holds `path` and is neither integrated nor escalated.
    Waiting { path: String, change: u64 },
    /// Git or the log failed, in their words.
    Git(String),
}

impl LandRefusal {
    /// The refusal's code and what it says, for the words an agent or the owner reads; `None` for
    /// a failure, which is not a refusal.
    pub(crate) fn words(&self) -> Option<(&'static str, String)> {
        match self {
            Self::Busy(paths) => Some((
                "folder_doc_busy",
                format!(
                    "{} has a change at the project's root that is not committed; commit it or put it aside first",
                    paths.join(", ")
                ),
            )),
            Self::Link(path) => Some((
                "folder_doc_link",
                format!("{path} is a link, and Catervas writes no file through one"),
            )),
            Self::Waiting { path, change } => Some((
                "folder_doc_waiting",
                format!("{path} has a change waiting to be added to the project (folder-{change})"),
            )),
            Self::Git(_) => None,
        }
    }
}

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::FolderDoc {
        code,
        detail: detail.into(),
    }
    .into()
}

/// What becomes of a folder change, in the sentence the tool answers with: a pull request when
/// there is an `origin`, else a merge by hand (Catervas never merges, ADR 0061).
fn next_by(has_origin: bool, keeps_local: bool, branch: &str, into: &str) -> String {
    if keeps_local {
        format!("Catervas adds {branch} to {into} on its next tick.")
    } else if has_origin {
        format!(
            "Catervas opens a pull request for {branch} on its next tick; the document is in {into} once it merges."
        )
    } else {
        format!("{branch} waits for you to merge it into {into}.")
    }
}

/// The number of the next folder change: the first above the log's highest for which git has no
/// branch `docs/folder-<n>`, here or on `origin`. The number stays the internal name; it is still
/// past every older `docs/folder-<n>` branch.
pub(crate) fn next_change(deps: &ToolDeps, docs: &catervas_store::folder_docs::FolderDocs) -> u64 {
    let mut change = docs
        .changes
        .iter()
        .map(|held| held.change)
        .max()
        .unwrap_or(0)
        + 1;
    loop {
        let numbered = format!("docs/folder-{change}");
        if deps.git.tree(&numbered).is_err()
            && deps.git.tree(&format!("origin/{numbered}")).is_err()
        {
            return change;
        }
        change += 1;
    }
}

/// The branch a folder change named for `title` is made on: `docs/<slug>`, past any branch git or
/// a task's contract already holds (ADR 0066).
pub(crate) fn docs_branch(deps: &ToolDeps, title: &str) -> Result<String, StoreError> {
    let taken = taken_branches(&deps.files, &deps.log, &deps.git, None)?;
    Ok(free_branch(ChangeType::Docs, &branch_slug(title), &taken))
}

/// Makes `files` (`(path, text)`) a folder change: the first number above the log's highest for
/// which git has no branch `docs/folder-<n>`, here or on `origin`, and a branch named for the
/// documents (`docs/<slug>`, ADR 0066) from the integration branch with one commit by `author`, or
/// by the person git knows when there is none.
/// The caller holds the integration lock.
///
/// # Errors
///
/// `Waiting` for a file or removed path an earlier change holds that is neither integrated nor
/// escalated, `Link`, `Busy`, and `Git` when git or the log failed. `Ok(None)` when every file
/// already says its text on the integration branch, which makes nothing.
pub(crate) fn land(
    deps: &ToolDeps,
    team: &Team,
    files: &[(String, String)],
    removed: &[String],
    message: &str,
    author: Option<&str>,
) -> Result<Option<Landed>, LandRefusal> {
    let words = |error: &dyn std::fmt::Display| LandRefusal::Git(error.to_string());
    let into = integration_branch(team, &deps.git).map_err(|error| words(&error))?;
    let docs = folder_docs(&deps.log).map_err(|error| words(&error))?;
    // ponytail: `folder_doc.written` names only the written path. A write of the removed file
    // while its removal waits conflicts at integration and is escalated there, not refused here;
    // record `removed` in the event if that happens.
    let touched = files.iter().map(|(path, _)| path).chain(removed);
    for path in touched {
        if let Some(held) = docs
            .changes
            .iter()
            .find(|change| !change.integrated && !change.escalated && change.paths.contains(path))
        {
            return Err(LandRefusal::Waiting {
                path: path.clone(),
                change: held.change,
            });
        }
    }
    let change = next_change(deps, &docs);
    // The branch is named for the documents (ADR 0066) and recorded in the event that makes the
    // change.
    let named: Vec<&str> = files
        .iter()
        .map(|(path, _)| path.as_str())
        .chain(removed.iter().map(String::as_str))
        .collect();
    let taken =
        taken_branches(&deps.files, &deps.log, &deps.git, None).map_err(|error| words(&error))?;
    let branch = free_branch(ChangeType::Docs, &branch_slug(&plain_names(&named)), &taken);
    match deps
        .git
        .commit_files(&branch, &into, files, removed, message, author)
        .map_err(|error| words(&error))?
    {
        CommitOutcome::Committed { sha } => Ok(Some(Landed {
            change,
            next: next_by(
                deps.git.has_remote("origin").unwrap_or(false),
                team.keeps_local(),
                &branch,
                &into,
            ),
            branch,
            sha,
        })),
        CommitOutcome::Unchanged => Ok(None),
        CommitOutcome::Busy(paths) => Err(LandRefusal::Busy(paths)),
        CommitOutcome::Linked(path) => Err(LandRefusal::Link(path)),
    }
}

/// Files the task that brings the twin of the document the owner changed up to date, when folder
/// change `change` is the owner's (`folder_doc.edited`) and has just been integrated
/// (`docs/SPEC.md` 5.17): a whole contract, `created_by: catervas`, triaged small at once. Unless a
/// task for the same twin is still a `draft`, `refining`, `ready` or `assigned`, which has not
/// reached its worktree and will hold the owner's change; then, and for a change that is not the
/// owner's or changes no human document, nothing is filed.
///
/// # Errors
///
/// `Refused` when no role reviews the owner of the folder, and what filing the request refuses.
pub(crate) fn file_rederive(
    deps: &ToolDeps,
    team: &Team,
    change: u64,
) -> Result<Option<TaskId>, RequestError> {
    let docs = folder_docs(&deps.log)?;
    let Some(human) = docs
        .changes
        .iter()
        .find(|held| held.change == change && held.edited)
        .and_then(|held| held.paths.iter().find(|path| is_human_document(path)))
    else {
        return Ok(None);
    };
    file_rederive_for(deps, team, human)
}

/// Files the task that brings the twin of the human document `human` up to date, the owner having
/// changed it (`docs/SPEC.md` 5.17), under the rule that keeps one open for each twin: nothing is
/// filed for a document with no twin, in a folder no role owns, or when a task for the twin has
/// not reached its worktree yet.
///
/// # Errors
///
/// `Refused` when no role reviews the owner of the folder, and what filing the request refuses.
pub(crate) fn file_rederive_for(
    deps: &ToolDeps,
    team: &Team,
    human: &str,
) -> Result<Option<TaskId>, RequestError> {
    let Some(twin) = agent_twin(human) else {
        return Ok(None);
    };
    deps.projections.catch_up()?;
    let board = deps.projections.board()?;
    let waiting = rederive_tasks(&deps.log)?.iter().any(|(id, aimed)| {
        *aimed == twin
            && board.iter().any(|row| {
                row.task_id == *id
                    && matches!(
                        row.status,
                        TaskStatus::Draft
                            | TaskStatus::Refining
                            | TaskStatus::Ready
                            | TaskStatus::Assigned
                    )
            })
    });
    let owner = ROLE_FOLDERS
        .iter()
        .find(|(_, folder)| human.starts_with(&format!("{folder}/")))
        .map(|(role, _)| *role);
    let (false, Some(owner)) = (waiting, owner) else {
        return Ok(None);
    };
    let Some(reviewer) = catervas_roles::reviewer_for(team, owner) else {
        return Err(RequestError::Refused {
            reason: format!("no role reviews the tasks of {owner}"),
        });
    };
    let wire = rederive_request(
        human,
        &twin,
        owner,
        reviewer,
        placeholder_budget_usd(&team.rules()),
    );
    let filed = file_rederive_request(
        &deps.files,
        &deps.log,
        wire,
        &twin,
        (deps.clock.now(), &deps.ids),
    )?;
    Ok(Some(filed.id))
}

/// The documents' plain names, joined by " and ": each as the Files page names it.
pub(crate) fn plain_names(paths: &[&str]) -> String {
    // A human document's agent version is the same document to the people who read the name.
    let twins: Vec<String> = paths.iter().filter_map(|path| agent_twin(path)).collect();
    paths
        .iter()
        .filter(|path| !twins.iter().any(|twin| twin == *path))
        .map(|path| document_name(path, &|_: &str| None))
        .collect::<Vec<_>>()
        .join(" and ")
}

/// `folder_doc_proposals.list` (`docs/SPEC.md` 5.7): the team's integration policy and the
/// proposals still pending as one card for each review and agent, ordered by its first
/// proposal, each document with its lines changed from the integration branch's file.
pub(crate) fn proposals_list(deps: &ToolDeps, team: &Team) -> Result<Value, StoreError> {
    let io = |error: &dyn std::fmt::Display| StoreError::Io {
        detail: error.to_string(),
    };
    let pending: Vec<_> = folder_docs(&deps.log)?
        .proposals
        .into_iter()
        .filter(|each| each.state == ProposalState::Pending)
        .collect();
    let mut cards: Vec<Vec<&FolderDocProposal>> = Vec::new();
    for each in &pending {
        match cards
            .iter_mut()
            .find(|card| card[0].sprint_id == each.sprint_id && card[0].agent_id == each.agent_id)
        {
            Some(card) => card.push(each),
            None => cards.push(vec![each]),
        }
    }
    let mut listed = Vec::new();
    let mode = catervas_store::document_source::documents_mode_of(team, &deps.log)?;
    let source = catervas_store::document_source::team_documents(team, &deps.git, mode)
        .map_err(|error| io(&error))?;
    for card in &cards {
        let documents: Vec<Value> = card
            .iter()
            .map(|each| {
                let before = source
                    .text(&deps.git, &each.path)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let lines: Vec<Value> = changed_lines(&before, &each.text)
                    .into_iter()
                    .map(|line| {
                        let change = match line.change {
                            LineChange::Same => "same",
                            LineChange::Added => "added",
                            LineChange::Removed => "removed",
                        };
                        json!({ "change": change, "text": line.text })
                    })
                    .collect();
                json!({ "proposal": each.proposal, "path": each.path, "lines": lines })
            })
            .collect();
        let summaries: Vec<&str> = card.iter().map(|each| each.summary.as_str()).collect();
        listed.push(json!({
            "sprint_id": card[0].sprint_id, "agent_id": card[0].agent_id,
            "summary": summaries.join(" "), "documents": documents,
        }));
    }
    Ok(json!({
        "integration": team.policy.integration.to_string(), "cards": listed,
    }))
}

/// The `integration` rows of `waiting.list` for the folder changes that are neither integrated nor
/// escalated: each waits for the owner to merge it, with its pull request when one was opened.
pub(crate) fn integration_rows(deps: &ToolDeps, team: &Team) -> Result<Vec<Value>, StoreError> {
    let opened: Vec<u64> = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::FolderChangeOpened],
            ..EventQuery::default()
        })?
        .into_iter()
        .filter_map(|event| match event.body {
            EventBody::FolderChangeOpened(body) => Some(body.change.get()),
            _ => None,
        })
        .collect();
    let docs = folder_docs(&deps.log)?;
    let waits_for_the_merge = if catervas_store::autopilot::merges_on_its_own(&deps.log, team)? {
        "Catervas merges its pull request when GitHub's checks pass"
    } else {
        "Its pull request waits for you to merge it"
    };
    let mut rows = Vec::new();
    for change in docs
        .changes
        .iter()
        .filter(|change| !change.integrated && !change.escalated)
    {
        let proposals: Vec<&FolderDocProposal> = docs
            .proposals
            .iter()
            .filter(|each| change.proposals.contains(&each.proposal))
            .collect();
        let (agent, title, line) = if let Some(local) = change.storage {
            (
                None,
                if local {
                    "Keep documents on this computer".to_string()
                } else {
                    "Put documents back in your repository".to_string()
                },
                if opened.contains(&change.change) {
                    "Its pull request waits for you to merge it".to_string()
                } else {
                    "You chose it; it waits for you to merge it".to_string()
                },
            )
        } else if let Some(first) = proposals.first() {
            let mut paths: Vec<&str> = Vec::new();
            for each in &proposals {
                if !paths.contains(&each.path.as_str()) {
                    paths.push(&each.path);
                }
            }
            (
                Some(first.agent_id.clone()),
                format!(
                    "{} from review {}",
                    plain_names(&paths),
                    first.sprint_id.trim_start_matches('S')
                ),
                if opened.contains(&change.change) {
                    waits_for_the_merge.to_string()
                } else {
                    "You approved it; it waits for you to merge it".to_string()
                },
            )
        } else if let Some(writer) = &change.written_by {
            let name = name_of(team, writer);
            let paths: Vec<&str> = change.paths.iter().map(String::as_str).collect();
            (
                Some(writer.clone()),
                format!("{} by {name}", plain_names(&paths)),
                if opened.contains(&change.change) {
                    waits_for_the_merge.to_string()
                } else {
                    format!("{name} wrote it; it waits for you to merge it")
                },
            )
        } else {
            continue;
        };
        rows.push(json!({
            "kind": "integration", "folder_change": change.change, "agent_id": agent,
            "title": title, "line": line,
        }));
    }
    Ok(rows)
}

/// What a call asks for once its input is checked.
struct Checked<'a> {
    folder: &'static str,
    path: String,
    text: &'a str,
    /// The same document in the other format, which the write replaces.
    removed: Vec<String>,
    /// For a human document: its agent version's path and text, written with it.
    twin: Option<(String, &'a str)>,
}

/// Writes a document of the caller's folder as a folder change, a human document and its agent
/// version together at the review (`docs/SPEC.md` 5.17). Holds the integration lock from its checks to its event,
/// so that a write, an approval and an integration never interleave on the root's checkout.
///
// ponytail: the lock is taken on the async thread, as `git::commit` already runs git there;
// `spawn_blocking` as `attempt` does if a ceremony stalls on it.
pub(super) fn write_folder_doc(
    call: &Call<'_>,
    input: &WriteFolderDocInput,
) -> Result<Value, ToolError> {
    let checked = check_input(call, input)?;
    let deps = call.deps();
    let _lock = integration_lock(deps.files.root()).map_err(failed)?;
    // While the documents move out of the repository or back (ADR 0074) nothing is written.
    if !catervas_store::document_source::documents_mode_of(&call.team, &deps.log)
        .map_err(failed)?
        .writes_open()
    {
        return Err(refused(
            "folder_doc_switching",
            "documents can't be changed until the owner merges the pull request that moves them",
        ));
    }
    let path = &checked.path;
    if catervas_store::document_source::documents_on_disk(&call.team, &deps.log).map_err(failed)? {
        return write_on_disk(call, &checked);
    }
    let name = call.agent.display_name.to_string();
    let mut files = vec![(path.clone(), checked.text.to_string())];
    let mut paths = vec![path.as_str()];
    if let Some((twin, agent_text)) = &checked.twin {
        files.push((twin.clone(), (*agent_text).to_string()));
        paths.push(twin);
    }
    let message = folder_doc_message(checked.folder, &paths, &name, false);
    let author = folder_doc_author(&name, call.agent_id());
    let landed = match land(
        deps,
        &call.team,
        &files,
        &checked.removed,
        &message,
        Some(&author),
    ) {
        Ok(Some(landed)) => landed,
        Ok(None) => {
            return Err(refused(
                "folder_doc_unchanged",
                format!("{path} already says this on the integration branch"),
            ));
        }
        Err(LandRefusal::Git(detail)) => return Err(ToolError::Failed { detail }),
        Err(refusal) => {
            let (code, detail) = refusal
                .words()
                .unwrap_or(("folder_doc_refused", String::new()));
            return Err(refused(code, detail));
        }
    };
    let body = serde_json::from_value(json!({
        "path": path, "written_by": call.agent_id(), "change": landed.change, "sha": landed.sha,
        "branch": landed.branch,
    }))
    .map_err(failed)?;
    call.append(None, EventBody::FolderDocWritten(body))?;
    Ok(json!({
        "path": path, "change": landed.change, "branch": landed.branch, "next": landed.next,
    }))
}

/// Writes the checked document, and its agent version, in the owner's folder at once (ADR 0074):
/// each file whole or not at all, the other format's file removed, and one `local_documents.written`
/// on the session's envelope. Text the folder already says is `folder_doc_unchanged`. The caller
/// holds the integration lock.
fn write_on_disk(call: &Call<'_>, checked: &Checked<'_>) -> Result<Value, ToolError> {
    use catervas_store::document_source::DocumentSource;
    use catervas_store::local_documents::{remove_document, write_document};

    let deps = call.deps();
    let root = deps.files.root();
    let source = DocumentSource::Disk(root.to_path_buf());
    let mut files = vec![(checked.path.clone(), checked.text.to_string())];
    if let Some((twin, agent_text)) = &checked.twin {
        files.push((twin.clone(), (*agent_text).to_string()));
    }
    let mut unchanged = checked.removed.is_empty();
    for (path, text) in &files {
        unchanged &= source.text(&deps.git, path).map_err(failed)?.as_deref() == Some(text);
    }
    if unchanged {
        return Err(refused(
            "folder_doc_unchanged",
            format!("{} already says this on this computer", checked.path),
        ));
    }
    for (path, text) in &files {
        write_document(root, path, text)
            .map_err(|error| refused("folder_doc_link", error.to_string()))?;
    }
    for path in &checked.removed {
        remove_document(root, path).map_err(failed)?;
    }
    let body = serde_json::from_value(json!({
        "paths": files.iter().map(|(path, _)| path).collect::<Vec<_>>(),
        "removed": checked.removed, "by": "agent", "written_by": call.agent_id(),
        "task_id": null, "set_aside": [],
    }))
    .map_err(failed)?;
    call.append(None, EventBody::LocalDocumentsWritten(body))?;
    Ok(json!({
        "path": checked.path, "change": null, "branch": null,
        "next": "It is saved in your folder on this computer; nothing is committed.",
    }))
}

/// Holds the call to the rules of its session, its path and its fields, in that order.
fn check_input<'a>(
    call: &Call<'_>,
    input: &'a WriteFolderDocInput,
) -> Result<Checked<'a>, ToolError> {
    let role = call.role();
    let in_a_document_ceremony = call.context.purpose == SessionPurpose::Ceremony
        && matches!(call.context.thread, Some(Thread::Review | Thread::Retro));
    let Some(folder) = role_folder(role).filter(|_| in_a_document_ceremony) else {
        return Err(refused(
            "folder_doc_refused",
            "a folder document is written at a review or retro, by a role that owns a folder",
        ));
    };
    let (target, twin) = checked_path(call, folder, &input.path)?;
    let path = target.path;
    let git = &call.deps().git;
    let mode = catervas_store::document_source::documents_mode_of(&call.team, &call.deps().log)
        .map_err(failed)?;
    let source =
        catervas_store::document_source::team_documents(&call.team, git, mode).map_err(failed)?;
    // A git error reads as "absent" inside the check, so the first one is kept and fails the call
    // after it: an unreadable branch must not let a document change format.
    let unreadable = std::cell::RefCell::new(None);
    let format_check = check_document_format(&path, call.team.document_format(), &|held: &str| {
        source.has_file(git, held).unwrap_or_else(|error| {
            unreadable.borrow_mut().get_or_insert(error.to_string());
            false
        })
    });
    if let Some(detail) = unreadable.into_inner() {
        return Err(ToolError::Failed { detail });
    }
    let removed = format_check
        .map_err(|why| format_refusal(&path, call.team.document_format(), why))?
        .into_iter()
        .collect::<Vec<_>>();
    if check_protected_paths(
        &removed,
        &call.team.rules_for(call.agent_id()).protected_paths,
    )
    .is_err()
    {
        return Err(refused(
            "folder_doc_path",
            format!("{} is a path the team protects", removed[0]),
        ));
    }
    if target.owner_accepted && call.context.thread != Some(Thread::Review) {
        return Err(refused(
            "folder_doc_at_review",
            format!("{path} is written at the review, not at another ceremony"),
        ));
    }
    let text = checked_text("text", &input.text)?;
    let Some(twin) = twin.filter(|_| target.owner_accepted) else {
        if input.agent_text.is_some() {
            return Err(refused(
                "folder_doc_agent_text",
                format!("{path} is written for agents and has no agent_text"),
            ));
        }
        if input.summary.is_some() {
            return Err(refused(
                "folder_doc_summary",
                format!("{path} is written for agents and has no summary"),
            ));
        }
        return Ok(Checked {
            folder,
            path,
            text,
            removed,
            twin: None,
        });
    };
    let agent_text = input.agent_text.as_deref().ok_or_else(|| {
        refused(
            "folder_doc_agent_text",
            format!("{path} needs agent_text, its agent version"),
        )
    })?;
    let agent_text = checked_text("agent_text", agent_text)?;
    Ok(Checked {
        folder,
        path,
        text,
        removed,
        twin: Some((twin, agent_text)),
    })
}

/// The refusal of a path in the wrong format, in the words the agent reads.
fn format_refusal(path: &str, format: DocumentFormat, why: FormatRefusal) -> ToolError {
    match why {
        FormatRefusal::WrongFormat { instead } => {
            let team = match format {
                DocumentFormat::Html => "HTML",
                DocumentFormat::Markdown => "Markdown",
            };
            refused(
                "wrong_format",
                format!(
                    "{path} is a new document and this team writes new documents as {team}: write {instead}"
                ),
            )
        }
        FormatRefusal::OtherFormatExists { existing } => {
            let rewrite = match format {
                DocumentFormat::Html => "; its rewrite as HTML reaches the owner through a task",
                DocumentFormat::Markdown => ".",
            };
            refused(
                "other_format_exists",
                format!("{existing} is this document: change it there{rewrite}"),
            )
        }
    }
}

/// Holds the path to the caller's folder and to the team's protected paths.
fn checked_path(
    call: &Call<'_>,
    folder: &str,
    named: &str,
) -> Result<(FolderDocPath, Option<String>), ToolError> {
    let target = check_folder_doc_path(call.role(), named, &[]).map_err(|why| match why {
        FolderDocPathRefusal::NoFolder => refused("folder_doc_refused", "your role owns no folder"),
        FolderDocPathRefusal::Outside => refused(
            "folder_doc_path",
            format!("{named} is not a file below {folder}/"),
        ),
        FolderDocPathRefusal::NotADocument => {
            refused("folder_doc_path", format!("{named} is not a .md or .html file"))
        }
        FolderDocPathRefusal::AgentTwin => refused(
            "folder_doc_path",
            format!("{named} is an agent version, which is written from agent_text"),
        ),
        FolderDocPathRefusal::MarketingPlan => refused(
            "folder_doc_marketing_plan",
            format!("{named} is a marketing plan, proposed with catervas_propose_marketing_plan in a task"),
        ),
    })?;
    let path = target.path.clone();
    let twin = agent_twin(&path);
    let protected: Vec<String> = std::iter::once(path.clone()).chain(twin.clone()).collect();
    if path.len() > 512 {
        return Err(refused(
            "folder_doc_path",
            "a path is at most 512 characters",
        ));
    }
    if check_protected_paths(
        &protected,
        &call.team.rules_for(call.agent_id()).protected_paths,
    )
    .is_err()
    {
        return Err(refused(
            "folder_doc_path",
            format!("{path} is a path the team protects"),
        ));
    }
    Ok((target, twin))
}

/// A text of the call: not blank and at most `DOC_BYTES`.
fn checked_text<'a>(field: &str, text: &'a str) -> Result<&'a str, ToolError> {
    if text.trim().is_empty() {
        return Err(refused("folder_doc_empty", format!("{field} says nothing")));
    }
    if text.len() > DOC_BYTES {
        return Err(refused(
            "folder_doc_too_large",
            format!("{field} is at most {DOC_BYTES} bytes"),
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use catervas_protocol::event::{EventKind, Thread};
    use serde_json::{Value, json};

    use crate::session::SessionPurpose;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, run, waits_for_the_lock_file, with_the_finance_specialist,
    };
    use crate::tools::{ToolContext, ToolError};

    const TOOL: &str = "catervas_write_folder_doc";
    const CADENCE: &str = "docs/catervas/delivery/cadence.md";
    const ROADMAP: &str = "docs/catervas/product/roadmap.md";

    /// A team of `pm`, two Developers, the Scrum Master `sm` ("Sol") and, when asked, the Finance
    /// Specialist `fin`, integrating `policy`.
    fn project(name: &str, policy: &str, scrum_master: bool, finance: bool) -> TestProject {
        project_in(name, policy, scrum_master, finance, "markdown")
    }

    /// As `project`, writing new documents in `format` and with the Architect `arch` ("Ada").
    fn project_in(
        name: &str,
        policy: &str,
        scrum_master: bool,
        finance: bool,
        format: &str,
    ) -> TestProject {
        let team = a_team_of_three(|wire| {
            wire["policy"]["integration"] = json!(policy);
            wire["policy"]["document_format"] = json!(format);
            wire["agents"].as_array_mut().expect("agents").push(json!({
                "id": "arch", "display_name": "Ada", "role": "architect", "status": "active"
            }));
            if scrum_master {
                wire["agents"].as_array_mut().expect("agents").push(json!({
                    "id": "sm", "display_name": "Sol", "role": "scrum_master", "status": "active"
                }));
            }
            if finance {
                with_the_finance_specialist(wire);
            }
            wire["rules"]["protected_paths"] = json!(["docs/catervas/delivery/secret.md"]);
        });
        TestProject::new(name, &team)
    }

    /// `agent`'s ceremony session in `thread`.
    fn ceremony(project: &TestProject, agent: &str, thread: Thread) -> ToolContext {
        let mut context = project.context(agent, None);
        context.purpose = SessionPurpose::Ceremony;
        context.thread = Some(thread);
        context
    }

    fn write(context: &ToolContext, path: &str, text: &str) -> Result<Value, ToolError> {
        run(context, TOOL, json!({ "path": path, "text": text }))
    }

    /// Sprint S4 started and ended, as a review is about.
    fn s4_ended(project: &TestProject) {
        project.record(
            "",
            "sprint.started",
            &json!({ "sprint_id": "S4", "budget_usd": null, "started_by": "human" }),
        );
        project.record(
            "",
            "sprint.ended",
            &json!({ "sprint_id": "S4", "ended_by": "human", "left": [] }),
        );
    }

    fn branches(project: &TestProject) -> String {
        project
            .repo
            .git_output(&["branch", "--list", "docs/*", "--format=%(refname:short)"])
    }

    fn code(error: &ToolError) -> &str {
        match error {
            ToolError::Refused { reason } => reason.split(':').next().unwrap_or_default(),
            other => panic!("a refusal, not {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_an_agent_only_document_as_a_folder_change() {
        let project = project("folder-doc-write", "auto_merge", true, false);
        let context = ceremony(&project, "sm", Thread::Retro);

        let answer = write(&context, CADENCE, "a\n").expect("the document is written");

        assert_eq!(
            answer,
            json!({
                "path": CADENCE, "change": 1, "branch": "docs/delivery-rhythm",
                "next": "docs/delivery-rhythm waits for you to merge it into main."
            })
        );
        let repo = &project.repo;
        assert_eq!(
            repo.git_output(&["log", "-1", "--format=%an <%ae>|%s", "docs/delivery-rhythm"]),
            "Sol (Catervas) <catervas@localhost>|docs(delivery): cadence.md by Sol"
        );
        assert_eq!(
            repo.git_output(&["rev-list", "--count", "main..docs/delivery-rhythm"]),
            "1"
        );
        assert_eq!(
            repo.git_output(&["diff", "--name-only", "main", "docs/delivery-rhythm"]),
            CADENCE
        );
        assert!(project.deps.git.file_at("main", CADENCE).is_err());
        let written = project.events(&[EventKind::FolderDocWritten]);
        assert_eq!(written.len(), 1);
        let ids = &written[0].envelope.ids;
        assert_eq!(
            (
                ids.agent_id.as_deref(),
                ids.session_id.as_deref(),
                ids.task_id.is_none()
            ),
            (Some("sm"), Some("session-1"), true)
        );
        let wire = catervas_protocol::event::event_to_value(&written[0]);
        assert_eq!(wire["body"]["path"], CADENCE);
        assert_eq!(wire["body"]["written_by"], "sm");
        assert_eq!(wire["body"]["change"], 1);
        assert_eq!(wire["body"]["branch"], "docs/delivery-rhythm");
        assert_eq!(
            wire["body"]["sha"],
            repo.git_output(&["rev-parse", "docs/delivery-rhythm"])
        );

        let second = write(&context, "docs/catervas/delivery/notes.md", "n\n").expect("another");
        assert_eq!(second["change"], 2);
        // A branch git already has is not reused, nor is a number the log has handed out.
        repo.git(&["branch", "docs/folder-3", "main"]);
        let third = write(&context, "docs/catervas/delivery/more.md", "m\n").expect("a third");
        assert_eq!(third["change"], 4);
        // Nor is a number a branch on `origin` already has.
        repo.git(&["update-ref", "refs/remotes/origin/docs/folder-5", "main"]);
        let fourth = write(&context, "docs/catervas/delivery/last.md", "l\n").expect("a fourth");
        assert_eq!(fourth["change"], 6);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_a_pull_request_opens_only_where_there_is_an_origin() {
        for (origin, next) in [
            (
                false,
                "docs/delivery-rhythm waits for you to merge it into main.",
            ),
            (
                true,
                "Catervas opens a pull request for docs/delivery-rhythm on its next tick; the document is in main once it merges.",
            ),
        ] {
            let project = project(
                &format!("folder-doc-next-{origin}"),
                "auto_merge",
                true,
                false,
            );
            let remote = origin.then(|| {
                crate::orchestrator::fixtures::Harness::with_origin_in(&project.repo.path)
            });
            let context = ceremony(&project, "sm", Thread::Review);
            let answer = write(&context, CADENCE, "a\n").expect("written");
            assert_eq!(answer["next"], next, "{origin}");
            assert!(
                project.events(&[EventKind::MessagePosted]).is_empty(),
                "{origin} posts no line in the channel"
            );
            if let Some(remote) = remote {
                let _ = std::fs::remove_dir_all(remote);
            }
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_the_product_spec_without_a_proposal() {
        let project = project("folder-doc-propose", "auto_merge", false, false);
        s4_ended(&project);
        let input = json!({
            "path": ROADMAP, "text": "Now: pie pre-orders.\n", "agent_text": "- now: pie pre-orders\n",
        });

        let answer = run(
            &ceremony(&project, "pm", Thread::Review),
            TOOL,
            input.clone(),
        )
        .expect("the change is made");

        assert!(project.events(&[EventKind::FolderDocProposed]).is_empty());
        assert_eq!(answer["change"], 1);
        assert_eq!(answer["branch"], "docs/roadmap");
        // The document and its agent version are one folder change.
        let git = &project.deps.git;
        assert_eq!(
            git.file_at("docs/roadmap", ROADMAP).expect("the document"),
            "Now: pie pre-orders.\n"
        );
        assert_eq!(
            git.file_at("docs/roadmap", "docs/catervas/product/roadmap.agent.md")
                .expect("its agent version"),
            "- now: pie pre-orders\n"
        );
        assert_eq!(project.events(&[EventKind::FolderDocWritten]).len(), 1);

        // Only the review writes the owner's documents; a summary, given or not, is not recorded.
        let before = project.event_count();
        let refused = run(&ceremony(&project, "pm", Thread::Retro), TOOL, input)
            .expect_err("only the review writes it");
        assert_eq!(code(&refused), "folder_doc_at_review");
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_what_the_tool_does_not_write() {
        let project = project("folder-doc-refuses", "auto_merge", true, false);
        s4_ended(&project);
        // The integration branch already says one document; another is changed at the root.
        project
            .repo
            .write("docs/catervas/delivery/held.md", "held\n");
        project.repo.commit("a held document");
        project
            .repo
            .write("docs/catervas/delivery/busy.md", "the owner's\n");
        let sm = ceremony(&project, "sm", Thread::Retro);
        let pm = ceremony(&project, "pm", Thread::Review);
        let long = "x".repeat(262_145);
        let cases: Vec<(&ToolContext, Value, &str)> = vec![
            (
                &sm,
                json!({ "path": "docs/catervas/product/x.md", "text": "a" }),
                "folder_doc_path",
            ),
            (
                &sm,
                json!({ "path": "docs/catervas/delivery/x.agent.md", "text": "a" }),
                "folder_doc_path",
            ),
            (
                &sm,
                json!({ "path": "docs/catervas/delivery/x.txt", "text": "a" }),
                "folder_doc_path",
            ),
            (
                &sm,
                json!({ "path": "docs/catervas/delivery/secret.md", "text": "a" }),
                "folder_doc_path",
            ),
            (
                &sm,
                json!({ "path": CADENCE, "text": "a", "agent_text": "b" }),
                "folder_doc_agent_text",
            ),
            (
                &sm,
                json!({ "path": CADENCE, "text": "a", "summary": "why" }),
                "folder_doc_summary",
            ),
            (
                &pm,
                json!({ "path": ROADMAP, "text": "a", "summary": "why" }),
                "folder_doc_agent_text",
            ),
            (
                &sm,
                json!({ "path": CADENCE, "text": "  \n" }),
                "folder_doc_empty",
            ),
            (
                &sm,
                json!({ "path": CADENCE, "text": long }),
                "folder_doc_too_large",
            ),
            (
                &sm,
                json!({ "path": "docs/catervas/delivery/held.md", "text": "held\n" }),
                "folder_doc_unchanged",
            ),
            (
                &sm,
                json!({ "path": "docs/catervas/delivery/busy.md", "text": "a" }),
                "folder_doc_busy",
            ),
        ];
        for (context, input, expected) in cases {
            let before = project.event_count();
            let refused = run(context, TOOL, input.clone()).expect_err("refused");
            assert_eq!(code(&refused), expected, "{input}");
            assert_eq!(project.event_count(), before, "{expected} records nothing");
            assert_eq!(branches(&project), "", "{expected} makes no branch");
        }

        // A path through a link, and a document whose change is not integrated yet.
        let linked = self::project("folder-doc-link", "auto_merge", true, false);
        let elsewhere = linked.repo.path.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("a directory");
        std::fs::create_dir_all(linked.repo.path.join("docs/catervas")).expect("docs");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, linked.repo.path.join("docs/catervas/delivery"))
            .expect("a link");
        let refused = write(&ceremony(&linked, "sm", Thread::Retro), CADENCE, "a\n")
            .expect_err("a link is refused");
        assert_eq!(code(&refused), "folder_doc_link");
        assert_eq!(branches(&linked), "");

        let waiting = self::project("folder-doc-waiting", "auto_merge", true, false);
        let context = ceremony(&waiting, "sm", Thread::Retro);
        write(&context, CADENCE, "a\n").expect("change 1");
        let refused = write(&context, CADENCE, "b\n").expect_err("change 1 holds it");
        assert_eq!(code(&refused), "folder_doc_waiting");
        assert_eq!(branches(&waiting), "docs/delivery-rhythm");
        assert_eq!(waiting.events(&[EventKind::FolderDocWritten]).len(), 1);
    }

    const FEATURES_HTML: &str = "docs/catervas/architecture/features.html";
    const CADENCE_HTML: &str = "docs/catervas/delivery/cadence.html";

    fn html_project(name: &str) -> TestProject {
        project_in(name, "auto_merge", true, false, "html")
    }

    fn reason_of(error: &ToolError) -> &str {
        match error {
            ToolError::Refused { reason } => reason,
            other => panic!("a refusal, not {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_a_new_document_in_the_teams_format() {
        let project = html_project("folder-doc-new-html");
        let context = ceremony(&project, "arch", Thread::Review);
        let answer = write(&context, FEATURES_HTML, "<p>a</p>\n").expect("written");
        assert_eq!(answer["path"], FEATURES_HTML);
        assert_eq!(project.events(&[EventKind::FolderDocWritten]).len(), 1);
        assert_eq!(
            project
                .deps
                .git
                .file_at(answer["branch"].as_str().expect("a branch"), FEATURES_HTML)
                .expect("on the branch"),
            "<p>a</p>\n"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_new_document_in_the_other_format() {
        let project = html_project("folder-doc-wrong-format");
        let context = ceremony(&project, "arch", Thread::Review);
        let refused =
            write(&context, "docs/catervas/architecture/features.md", "a\n").expect_err("refused");
        assert_eq!(code(&refused), "wrong_format");
        assert!(
            reason_of(&refused).contains(
                "docs/catervas/architecture/features.md is a new document and this team writes new documents as HTML: write docs/catervas/architecture/features.html"
            ),
            "{refused:?}"
        );
        assert!(project.events(&[EventKind::FolderDocWritten]).is_empty());
        assert_eq!(branches(&project), "");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_an_existing_markdown_document() {
        let project = html_project("folder-doc-keeps-md");
        project
            .repo
            .write("docs/catervas/architecture/features.md", "old\n");
        project.repo.commit("a document");
        let context = ceremony(&project, "arch", Thread::Review);
        write(&context, "docs/catervas/architecture/features.md", "new\n").expect("lands");
        assert_eq!(project.events(&[EventKind::FolderDocWritten]).len(), 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn rewrites_an_agent_document_in_place() {
        let project = html_project("folder-doc-rewrite");
        project.repo.write(CADENCE, "old\n");
        project.repo.commit("a document");
        let context = ceremony(&project, "sm", Thread::Retro);
        let answer = write(&context, CADENCE_HTML, "<p>new</p>\n").expect("rewritten");
        let branch = answer["branch"].as_str().expect("a branch");
        let git = &project.deps.git;
        assert!(git.file_at(branch, CADENCE_HTML).is_ok());
        assert!(git.file_at(branch, CADENCE).is_err());
        let written = project.events(&[EventKind::FolderDocWritten]);
        let wire = catervas_protocol::event::event_to_value(&written[0]);
        assert_eq!(wire["body"]["path"], CADENCE_HTML);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_the_spec_changing_format() {
        let project = html_project("folder-doc-spec-format");
        s4_ended(&project);
        project.repo.write("docs/catervas/product/spec.md", "old\n");
        project.repo.commit("a spec");
        let input = json!({
            "path": "docs/catervas/product/spec.html", "text": "<p>a</p>\n",
            "agent_text": "- a\n", "summary": "why"
        });
        let refused =
            run(&ceremony(&project, "pm", Thread::Review), TOOL, input).expect_err("refused");
        assert_eq!(code(&refused), "other_format_exists");
        assert!(
            reason_of(&refused).contains("docs/catervas/product/spec.md is this document: change it there; its rewrite as HTML reaches the owner through a task"),
            "{refused:?}"
        );
        assert!(project.events(&[EventKind::FolderDocProposed]).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_markdown_spec_beside_an_html_one() {
        let project = project("folder-doc-spec-md", "auto_merge", false, false);
        s4_ended(&project);
        project
            .repo
            .write("docs/catervas/product/spec.html", "<p>old</p>\n");
        project.repo.commit("a spec");
        let input = json!({
            "path": "docs/catervas/product/spec.md", "text": "a\n",
            "agent_text": "- a\n", "summary": "why"
        });
        let refused =
            run(&ceremony(&project, "pm", Thread::Review), TOOL, input).expect_err("refused");
        assert_eq!(code(&refused), "other_format_exists");
        assert!(
            reason_of(&refused)
                .ends_with("docs/catervas/product/spec.html is this document: change it there."),
            "{refused:?}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_an_html_spec_with_its_agent_version() {
        let project = html_project("folder-doc-html-spec");
        s4_ended(&project);
        let input = json!({
            "path": "docs/catervas/product/spec.html", "text": "<p>a</p>\n",
            "agent_text": "<p>b</p>\n"
        });
        let answer = run(&ceremony(&project, "pm", Thread::Review), TOOL, input).expect("written");
        // The document and its agent version are written in the same format, as one change.
        let branch = answer["branch"].as_str().expect("a branch");
        assert_eq!(branch, "docs/product-description");
        let git = &project.deps.git;
        assert_eq!(
            git.file_at(branch, "docs/catervas/product/spec.html")
                .expect("the document"),
            "<p>a</p>\n"
        );
        assert_eq!(
            git.file_at(branch, "docs/catervas/product/spec.agent.html")
                .expect("its agent version"),
            "<p>b</p>\n"
        );
        assert!(
            git.file_at(branch, "docs/catervas/product/spec.agent.md")
                .is_err()
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_replace_a_protected_document_with_its_other_format() {
        let project = html_project("folder-doc-protected-rewrite");
        project
            .repo
            .write("docs/catervas/delivery/secret.md", "old\n");
        project.repo.commit("a secret");
        let before = project.event_count();
        let refused = write(
            &ceremony(&project, "sm", Thread::Retro),
            "docs/catervas/delivery/secret.html",
            "<p>new</p>\n",
        )
        .expect_err("refused");
        assert_eq!(code(&refused), "folder_doc_path");
        assert_eq!(project.event_count(), before);
        assert_eq!(branches(&project), "");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_integration_branch_not_the_working_tree() {
        let project = html_project("folder-doc-branch-not-tree");
        // Uncommitted at the root: the integration branch does not hold it.
        project
            .repo
            .write("docs/catervas/architecture/features.md", "draft\n");
        let refused = write(
            &ceremony(&project, "arch", Thread::Review),
            "docs/catervas/architecture/features.md",
            "a\n",
        )
        .expect_err("a new document in the other format");
        assert_eq!(code(&refused), "wrong_format");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_markdown_team_is_told_to_write_markdown() {
        let project = project_in("folder-doc-md-team", "auto_merge", true, false, "markdown");
        let refused = write(
            &ceremony(&project, "arch", Thread::Review),
            FEATURES_HTML,
            "<p>a</p>\n",
        )
        .expect_err("refused");
        assert_eq!(code(&refused), "wrong_format");
        assert!(
            reason_of(&refused)
                .ends_with("as Markdown: write docs/catervas/architecture/features.md"),
            "{refused:?}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn fails_when_the_integration_branch_cannot_be_read() {
        let team = a_team_of_three(|wire| {
            wire["policy"]["document_format"] = json!("html");
            wire["policy"]["integration_branch"] = json!("nowhere");
        });
        let project = TestProject::new("folder-doc-unreadable", &team);
        s4_ended(&project);
        let input = json!({
            "path": "docs/catervas/product/spec.html", "text": "<p>a</p>\n",
            "agent_text": "<p>b</p>\n", "summary": "why"
        });
        let failed = run(&ceremony(&project, "pm", Thread::Review), TOOL, input)
            .expect_err("a branch that cannot be read fails the call");
        assert!(matches!(failed, ToolError::Failed { .. }), "{failed:?}");
        assert!(project.events(&[EventKind::FolderDocProposed]).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn land_refuses_removing_a_file_a_change_holds() {
        let project = self::project("folder-doc-removal-waiting", "auto_merge", true, false);
        write(&ceremony(&project, "sm", Thread::Retro), CADENCE, "a\n").expect("change 1");
        let deps = &project.deps;
        let team = deps.files.read_team().expect("the team reads");
        let refused = super::land(
            deps,
            &team,
            &[(
                "docs/catervas/delivery/cadence.html".to_string(),
                "<p>a</p>\n".to_string(),
            )],
            &[CADENCE.to_string()],
            "docs(delivery): cadence as html",
            None,
        )
        .expect_err("change 1 holds the removed file");
        assert!(
            matches!(refused, super::LandRefusal::Waiting { change: 1, .. }),
            "{refused:?}"
        );
        assert_eq!(branches(&project), "docs/delivery-rhythm");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_outside_a_ceremony_of_a_role_with_a_folder() {
        let project = project("folder-doc-outside", "auto_merge", true, true);
        project.filed("CTV-1", "in_progress", "task", None);
        let mut chat = project.context("sm", None);
        chat.purpose = SessionPurpose::Chat;
        let contexts = [
            project.context("sm", Some("CTV-1")),
            chat,
            ceremony(&project, "sm", Thread::Planning),
            ceremony(&project, "fin", Thread::Retro),
        ];
        for context in &contexts {
            let before = project.event_count();
            let refused = write(context, CADENCE, "a\n").expect_err("refused");
            assert_eq!(code(&refused), "folder_doc_refused", "{:?}", context.thread);
            assert_eq!(project.event_count(), before);
            assert_eq!(branches(&project), "");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waits_for_the_integration_lock() {
        let project = project("folder-doc-lock", "auto_merge", true, false);
        let context = ceremony(&project, "sm", Thread::Retro);
        let answer = waits_for_the_lock_file(&project.repo.path, &project, || {
            write(&context, CADENCE, "a\n")
        });
        assert_eq!(
            answer.expect("written once the lock is let go")["change"],
            1
        );
        assert_eq!(project.events(&[EventKind::FolderDocWritten]).len(), 1);
    }
}
