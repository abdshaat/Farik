//! The Files page's server (`docs/SPEC.md` 5.17): the team's folders and their documents read from
//! the integration branch, one document with its twin, who last changed each, and whether the twin
//! is stale. `web.rs` answers the frames; this module answers what they ask.

use std::fmt::Display;
use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};

use catervas_core::contract::TaskStatus;
use catervas_core::folders::{
    Named, OwnerEditRefusal, ROLE_FOLDERS, agent_twin, check_owner_edit, document_name,
    document_order, human_of_twin, named_in_subject, plain_name,
};
use catervas_core::governor::paths::normalise;
use catervas_core::team::{Role, Team};
use catervas_store::folder_docs::{FolderChange, FolderDocs, folder_docs};
use catervas_store::git::{PathCommit, TreeEntry, integration_branch, stale_twins};
use catervas_store::marketing::marketing_plans;
use catervas_store::requests::rederive_tasks;
use serde_json::{Value, json};

use catervas_protocol::event::{EventBody, TaskCreatedBodyFiledAs};
use catervas_store::requests::{change_request, file_small_request, placeholder_budget_usd};

use super::DaemonState;
use super::gates::off_the_worker;
use super::team::append;
use super::web::{Failure, INTERNAL_ERROR, NO_PROJECT, NOT_FOUND, REFUSED, UNKNOWN_QUERY};
use crate::tools::ToolDeps;
use crate::tools::folders::{LandRefusal, land};
use crate::transitions::integration_lock;

/// The queries this module answers.
pub(super) const QUERIES: [&str; 2] = ["files.tree", "files.read"];

/// The methods this module answers.
pub(super) const METHODS: [&str; 2] = ["files.save", "files.ask_change"];

/// Where the team's documents live.
const ROOT: &str = "docs/catervas";

fn internal(error: &dyn Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// The queries, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str, params: &Value) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    match name {
        "files.tree" => tree(deps),
        "files.read" => read(deps, params["path"].as_str().unwrap_or_default()),
        _ => Err(Failure::new(
            UNKNOWN_QUERY,
            format!("there is no query {name}"),
        )),
    }
}

/// A refusal the page words by `code`, at the JSON pointer `pointer`.
fn refusal(code: &str, pointer: &str, message: &str) -> Failure {
    let mut failure = Failure::new(REFUSED, message);
    failure.data =
        Some(json!({ "errors": [{ "path": pointer, "message": message, "code": code }] }));
    failure
}

/// The refusal of an edit that `check_owner_edit` turned away.
fn edit_refusal(why: OwnerEditRefusal) -> Failure {
    let (pointer, message) = match why {
        OwnerEditRefusal::OutsideFolders => (
            "/path",
            "That document is not in one of the team's folders.",
        ),
        OwnerEditRefusal::NotADocument => (
            "/path",
            "Only a document that ends in .md or .html can be saved here.",
        ),
        OwnerEditRefusal::AgentTwin => (
            "/path",
            "That version is written for the team: it follows when you change the document beside it.",
        ),
        OwnerEditRefusal::MarketingPlan => (
            "/path",
            "A marketing plan changes through a new version on its page.",
        ),
        OwnerEditRefusal::TooLarge => (
            "/text",
            "This is too long to save here: a document can be at most about 256 KB.",
        ),
    };
    refusal(why.code(), pointer, message)
}

/// `files.save { path, text, base }`: the owner's edit committed as a folder change, and recorded.
fn save(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let text = params["text"].as_str().unwrap_or_default();
    let path = check_owner_edit(params["path"].as_str().unwrap_or_default(), text.len())
        .map_err(edit_refusal)?;
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let folder = ROLE_FOLDERS
        .iter()
        .filter(|(role, _)| {
            team.active_agents()
                .any(|agent| Role::from(agent.role) == *role)
        })
        .map(|(_, folder)| *folder)
        .find(|folder| path.starts_with(&format!("{folder}/")))
        .ok_or_else(|| edit_refusal(OwnerEditRefusal::OutsideFolders))?;
    let _lock = integration_lock(deps.files.root()).map_err(|e| internal(&e))?;
    let branch = integration_branch(&team, &deps.git).map_err(|e| internal(&e))?;
    let held = deps
        .git
        .tree_entries(&branch, &path)
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|entry| entry.path == path && is_file(entry));
    if held.is_none_or(|entry| entry.blob != params["base"].as_str().unwrap_or_default()) {
        return Err(refusal(
            "file_changed",
            "/base",
            "This document changed since you opened it. Open it again to see what changed.",
        ));
    }
    let within = path.strip_prefix(&format!("{folder}/")).unwrap_or(&path);
    let scope = folder.rsplit('/').next().unwrap_or(folder);
    let message = format!("docs({scope}): edit {within}");
    let landed = match land(
        deps,
        &team,
        &[(path.clone(), text.to_string())],
        &[],
        &message,
        None,
    ) {
        Ok(Some(landed)) => landed,
        Ok(None) => return Ok(json!({ "change": null, "branch": null })),
        Err(LandRefusal::Busy(_)) => {
            return Err(refusal(
                "file_busy",
                "/path",
                "This file was also changed on your computer, outside Catervas. Finish or undo that change, then save here again.",
            ));
        }
        Err(LandRefusal::Link(_)) => {
            return Err(refusal(
                "is_a_link",
                "/path",
                "This file is a link, which Catervas does not write through.",
            ));
        }
        Err(LandRefusal::Waiting { .. }) => {
            return Err(refusal(
                "change_waiting",
                "/path",
                "An earlier change to this file is not in your project yet. Edit it once it is.",
            ));
        }
        Err(LandRefusal::Git(detail)) => return Err(Failure::new(INTERNAL_ERROR, detail)),
    };
    let body = serde_json::from_value(json!({
        "path": path, "change": landed.change, "sha": landed.sha,
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::FolderDocEdited(body))?;
    Ok(json!({ "change": landed.change, "branch": landed.branch }))
}

/// `files.ask_change { path, words }`: a small, whole task for the role that owns the document,
/// to make the change the owner's words ask for. Nothing is filed when the owner may not ask.
fn ask_change(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let cannot = || {
        refusal(
            "not_editable",
            "/path",
            "This document can't be changed from here.",
        )
    };
    let path =
        check_owner_edit(params["path"].as_str().unwrap_or_default(), 0).map_err(|_| cannot())?;
    let words = params["words"].as_str().unwrap_or_default().trim();
    if words.is_empty() {
        return Err(refusal("no_words", "/words", "Say what should change."));
    }
    let (team, _, entries) = listing(deps)?;
    let owner = ROLE_FOLDERS
        .iter()
        .find(|(_, folder)| path.starts_with(&format!("{folder}/")))
        .map(|(role, _)| *role)
        .ok_or_else(cannot)?;
    if owner == Role::ScrumMaster {
        return Err(refusal(
            "no_task_taker",
            "/path",
            "The Scrum Master keeps this document up to date at its retro.",
        ));
    }
    if !team
        .active_agents()
        .any(|agent| Role::from(agent.role) == owner)
    {
        return Err(refusal(
            "no_owner",
            "/path",
            "No one on your team owns this folder right now. Resume or add its role first.",
        ));
    }
    let document = folders(&team, &entries)
        .into_iter()
        .flat_map(|folder| folder.documents)
        .find(|document| document.path == path)
        .ok_or_else(cannot)?;
    // A human document's twin is named even when it is not there yet: the team writes it.
    let twin = agent_twin(&path).or(document.twin);
    let reviewer = catervas_roles::reviewer_for(&team, owner)
        .ok_or_else(|| internal(&format!("no role reviews the tasks of {owner}")))?;
    let board = deps.projections.board().map_err(|e| internal(&e))?;
    let name = document_name(&path, &|stem: &str| {
        board
            .iter()
            .find(|row| row.task_id.as_str() == stem)
            .map(|row| row.title.clone())
    });
    let wire = change_request(
        &path,
        twin.as_deref(),
        &name,
        words,
        owner,
        reviewer,
        placeholder_budget_usd(&team.rules()),
    );
    let filed = file_small_request(
        &deps.files,
        &deps.log,
        wire,
        "human",
        TaskCreatedBodyFiledAs::OwnerChange,
        &format!("the owner's change of {name}"),
        (deps.clock.now(), &deps.ids),
    )
    .map_err(|e| internal(&e))?;
    Ok(json!({ "task_id": filed.id }))
}

/// The methods, whose params the schema already passed.
pub(super) async fn call(
    state: &DaemonState,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    let params = params.clone();
    if method == "files.ask_change" {
        let filed = off_the_worker(move || ask_change(&deps, &params)).await?;
        state.wakes().notify_one();
        return Ok(filed);
    }
    let saved = off_the_worker(move || save(&deps, &params)).await?;
    if !saved["change"].is_null() {
        state.wakes().notify_one();
    }
    Ok(saved)
}

/// One document of a folder, and its twin when it has one that is a file.
struct Document {
    path: String,
    twin: Option<String>,
}

/// One folder of an active role, with the documents the tree lists in it.
struct Folder {
    path: &'static str,
    role: Role,
    agent_id: String,
    documents: Vec<Document>,
}

fn is_file(entry: &TreeEntry) -> bool {
    matches!(entry.mode.as_str(), "100644" | "100755")
}

fn is_document(path: &str) -> bool {
    catervas_core::folders::DocumentFormat::of(path).is_some()
}

/// The folders of the roles with an active agent, in the table's order, each with its `.md` and `.html` files
/// at any depth except the twin of a document that is present, which rides on its document.
fn folders(team: &Team, entries: &[TreeEntry]) -> Vec<Folder> {
    let files: Vec<&str> = entries
        .iter()
        .filter(|entry| is_file(entry))
        .map(|entry| entry.path.as_str())
        .collect();
    ROLE_FOLDERS
        .iter()
        .filter_map(|(role, folder)| {
            let agent = team
                .active_agents()
                .find(|agent| Role::from(agent.role) == *role)?;
            let inside = format!("{folder}/");
            let mut documents: Vec<Document> = files
                .iter()
                .filter(|path| path.starts_with(&inside) && is_document(path))
                .filter(|path| {
                    !human_of_twin(path).is_some_and(|human| files.contains(&human.as_str()))
                })
                .map(|path| Document {
                    path: (*path).to_string(),
                    twin: agent_twin(path).filter(|twin| files.contains(&twin.as_str())),
                })
                .collect();
            documents.sort_by_key(|document| document_order(&document.path));
            Some(Folder {
                path: folder,
                role: *role,
                agent_id: agent.id.to_string(),
                documents,
            })
        })
        .collect()
}

/// The team and the integration branch's `docs/catervas` entries.
fn listing(deps: &ToolDeps) -> Result<(Team, String, Vec<TreeEntry>), Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let branch = integration_branch(&team, &deps.git).map_err(|e| internal(&e))?;
    let entries = deps
        .git
        .tree_entries(&branch, ROOT)
        .map_err(|e| internal(&e))?;
    Ok((team, branch, entries))
}

/// `files.tree`: the folders, the documents in them with their names, and the integration policy.
fn tree(deps: &ToolDeps) -> Result<Value, Failure> {
    let (team, _, entries) = listing(deps)?;
    let board = deps.projections.board().map_err(|e| internal(&e))?;
    let plans = marketing_plans(&deps.log).map_err(|e| internal(&e))?;
    let title_of = |stem: &str| {
        board
            .iter()
            .find(|row| row.task_id.as_str() == stem)
            .map(|row| row.title.clone())
            .or_else(|| {
                plans
                    .iter()
                    .find(|plan| plan.record.id == stem)
                    .map(|plan| plan.proposal.title.clone())
            })
    };
    let listed: Vec<Value> = folders(&team, &entries)
        .iter()
        .map(|folder| {
            let documents: Vec<Value> = folder
                .documents
                .iter()
                .map(|document| {
                    json!({
                        "path": document.path,
                        "name": document_name(&document.path, &title_of),
                        "twin": document.twin,
                    })
                })
                .collect();
            json!({
                "folder": folder.path,
                "name": plain_name(folder.path.rsplit('/').next().unwrap_or(folder.path)),
                "role": folder.role.to_string(),
                "agent_id": folder.agent_id,
                "documents": documents,
            })
        })
        .collect();
    Ok(json!({
        "integration": team.policy.integration.to_string(),
        "folders": listed,
    }))
}

/// Who made `commit`, by what its subject names: a task on the board, or a folder change.
fn last_change(
    deps: &ToolDeps,
    docs: &FolderDocs,
    commit: &PathCommit,
    by_you: bool,
) -> Result<Value, Failure> {
    let (task_id, agent_id, by_you) = if by_you {
        (None, None, true)
    } else {
        match named_in_subject(&commit.subject) {
            Some(Named::Task(id)) => match deps.projections.task(&id).map_err(|e| internal(&e))? {
                Some(row) => (Some(id.to_string()), row.assignee_id, false),
                None => (None, None, false),
            },
            Some(Named::FolderChange(number)) => {
                match docs.changes.iter().find(|change| change.change == number) {
                    Some(change) if change.edited => (None, None, true),
                    Some(change) => (None, writer_of(docs, change), false),
                    None => (None, None, false),
                }
            }
            None => (None, None, false),
        }
    };
    Ok(json!({
        "at": commit.committed_at,
        "task_id": task_id,
        "agent_id": agent_id,
        "author": commit.author_name,
        "by_you": by_you,
    }))
}

/// The agent that wrote a folder change, or whose proposals the owner approved in it.
fn writer_of(docs: &FolderDocs, change: &FolderChange) -> Option<String> {
    change.written_by.clone().or_else(|| {
        docs.proposals
            .iter()
            .find(|proposal| change.proposals.contains(&proposal.proposal))
            .map(|proposal| proposal.agent_id.clone())
    })
}

/// `path` as `rev` holds it: its text, its blob and its last change. `waiting` is the owner's own
/// change that `rev` is the branch of, whose commit is the last change, made by them.
fn version(
    deps: &ToolDeps,
    docs: &FolderDocs,
    rev: &str,
    path: &str,
    waiting: bool,
) -> Result<Value, Failure> {
    let entry = deps
        .git
        .tree_entries(rev, path)
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|entry| entry.path == path && is_file(entry))
        .ok_or_else(|| internal(&format!("{rev} has no file {path}")))?;
    let text = deps.git.file_at(rev, path).map_err(|e| internal(&e))?;
    let commit = deps
        .git
        .last_commit(rev, path, !waiting)
        .map_err(|e| internal(&e))?;
    let last = commit
        .map(|commit| last_change(deps, docs, &commit, waiting))
        .transpose()?;
    Ok(json!({ "path": path, "text": text, "blob": entry.blob, "last_change": last }))
}

/// The owner's own change to `path` that is still waiting, and the branch that holds it.
fn waiting_change(deps: &ToolDeps, docs: &FolderDocs, path: &str) -> Option<(u64, String)> {
    docs.changes
        .iter()
        .rev()
        .find(|change| {
            change.edited
                && !change.integrated
                && !change.escalated
                && change.paths.iter().any(|p| p == path)
        })
        .map(|change| (change.change, format!("docs/folder-{}", change.change)))
        .filter(|(_, held)| deps.git.tree(held).is_ok())
}

/// `files.read`: one document of the tree with its twin.
fn read(deps: &ToolDeps, asked: &str) -> Result<Value, Failure> {
    let missing = || Failure::new(NOT_FOUND, format!("there is no document {asked}"));
    let path = normalise(asked).ok_or_else(missing)?;
    let (team, branch, entries) = listing(deps)?;
    let document = folders(&team, &entries)
        .into_iter()
        .flat_map(|folder| folder.documents)
        .find(|document| document.path == path)
        .ok_or_else(missing)?;
    let docs = folder_docs(&deps.log).map_err(|e| internal(&e))?;
    let waiting = waiting_change(deps, &docs, &path);
    let shown = match &waiting {
        Some((_, held)) => version(deps, &docs, held, &path, true)?,
        None => version(deps, &docs, &branch, &path, false)?,
    };
    let twin = document
        .twin
        .as_deref()
        .map(|twin| version(deps, &docs, &branch, twin, false))
        .transpose()?;
    let stale = match &document.twin {
        Some(twin) => stale_twins(&deps.git, &branch)
            .map_err(|e| internal(&e))?
            .contains(twin),
        None => false,
    };
    let rederiving = match &document.twin {
        Some(twin) => rederive_tasks(&deps.log)
            .map_err(|e| internal(&e))?
            .iter()
            .filter(|(_, aimed)| aimed == twin)
            .map(|(id, _)| deps.projections.task(id).map_err(|e| internal(&e)))
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .flatten()
            .any(|row| !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled)),
        None => false,
    };
    let editable = check_owner_edit(&path, 0).is_ok();
    Ok(json!({
        "document": shown,
        "twin": twin,
        "stale": stale,
        "rederiving": rederiving,
        "editable": editable,
        "askable": editable && !path.starts_with("docs/catervas/delivery/"),
        "waiting": waiting.map(|(change, held)| json!({ "change": change, "branch": held })),
    }))
}

/// The policy an HTML document is answered under: no script, no network, its own origin, and
/// framed by this app alone.
const POLICY: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:; \
                      font-src data:; frame-ancestors 'self'";

/// What fits a document to the reader's screen; a document's own styles come after it and win.
const FIT: &str = concat!(
    r#"<meta name="viewport" content="width=device-width, initial-scale=1">"#,
    "<style>",
    "body { margin: 0 auto; padding: 16px; max-width: 72rem; overflow-wrap: anywhere }",
    "img, svg, video, canvas { max-width: 100%; height: auto }",
    "table, pre { display: block; max-width: 100%; overflow-x: auto }",
    "</style>"
);

/// The end of the first `open` tag (`<head`, `<html`) in `lower`, which is not a longer name.
fn after_tag(lower: &str, open: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = lower[from..].find(open) {
        let start = from + found + open.len();
        if matches!(
            lower.as_bytes().get(start),
            Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r')
        ) {
            return lower[start..].find('>').map(|end| start + end + 1);
        }
        from = start;
    }
    None
}

/// `document` with `FIT` after its `<head>`, else its `<html>`, else its doctype, else first.
/// ponytail: the tag is found by text, not by parsing; use a parser if a document defeats it.
fn fit_to_screen(document: &str) -> String {
    let lower = document.to_ascii_lowercase();
    let at = ["<head", "<html"]
        .iter()
        .find_map(|open| after_tag(&lower, open))
        .or_else(|| {
            lower
                .find("<!doctype")
                .and_then(|n| lower[n..].find('>').map(|end| n + end + 1))
        })
        .unwrap_or(0);
    format!("{}{FIT}{}", &document[..at], &document[at..])
}

/// A regular file with no link on the way down from `base` along `path`, as text.
fn plain_file(base: &std::path::Path, path: &str) -> Option<String> {
    let mut at = base.to_path_buf();
    for segment in path.split('/') {
        at.push(segment);
        if std::fs::symlink_metadata(&at)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
    }
    // ponytail: the file is opened after the links were checked, so a link swapped in between
    // is followed; the frame is sandboxed and has no network, and no dependency gives O_NOFOLLOW.
    let regular = std::fs::symlink_metadata(&at).ok()?.is_file();
    regular.then(|| std::fs::read_to_string(&at).ok()).flatten()
}

/// The text of the HTML document `path`, as the Files page reads it, or as `task` holds it.
fn raw_text(deps: &ToolDeps, path: &str, task: Option<&str>) -> Option<String> {
    let path = normalise(path)?;
    if !path.starts_with("docs/catervas/")
        || catervas_core::folders::DocumentFormat::of(&path)
            != Some(catervas_core::folders::DocumentFormat::Html)
    {
        return None;
    }
    let in_branch = |rev: &str, path: &str| {
        deps.git
            .tree_entries(rev, path)
            .ok()?
            .into_iter()
            .find(|entry| entry.path == path && is_file(entry))?;
        deps.git.file_at(rev, path).ok()
    };
    if let Some(task) = task {
        let id: catervas_core::contract::TaskId = task.parse().ok()?;
        let contract = deps.files.read_contract(&id).ok()?;
        return in_branch(&catervas_core::branch::task_branch(&contract), &path).or_else(|| {
            let base = deps
                .files
                .root()
                .join(".catervas/local/worktrees")
                .join(id.as_str());
            plain_file(&base, &path)
        });
    }
    // A twin has no waiting change: only the human document does.
    let held = match catervas_core::folders::human_of_twin(&path) {
        Some(_) => None,
        None => folder_docs(&deps.log)
            .ok()
            .and_then(|docs| waiting_change(deps, &docs, &path))
            .map(|(_, held)| held),
    };
    let rev = match held {
        Some(held) => held,
        None => integration_branch(&deps.files.read_team().ok()?, &deps.git).ok()?,
    };
    in_branch(&rev, &path)
}

/// `GET /documents/raw?path=<path>[&task=<id>]`: an HTML document under its own policy, fitted to
/// the screen. 403 from another host, 401 without a session, and 404, with no body, for all else.
pub(super) async fn raw(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let (Some(web), Some(deps)) = (state.web(), state.deps()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !super::web::own_host(&headers, web) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !super::web::has_session(&headers, web) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let asked = query.unwrap_or_default();
    let named = |name: &str| {
        url::form_urlencoded::parse(asked.as_bytes())
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    let (Some(path), task) = (named("path"), named("task")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let deps = Arc::clone(deps);
    let text = off_the_worker(move || {
        raw_text(&deps, &path, task.as_deref()).ok_or_else(|| Failure::new(NOT_FOUND, ""))
    })
    .await;
    match text {
        Ok(text) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CONTENT_SECURITY_POLICY, POLICY),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::REFERRER_POLICY, "no-referrer"),
                (HeaderName::from_static("x-dns-prefetch-control"), "off"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            fit_to_screen(&text),
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use catervas_protocol::command::Command;
    use catervas_protocol::event::EventKind;
    use serde_json::{Value, json};

    use crate::daemon::DaemonState;
    use crate::daemon::gates::tests::{conforms, query, rpc};
    use crate::orchestrator::fixtures::Harness;
    use crate::tools::fixtures::{with_the_designer, with_the_marketing_specialist};

    const PRODUCT: &str = "docs/catervas/product";
    const OVERVIEW: &str = "docs/catervas/architecture/overview.md";
    const ROADMAP: &str = "docs/catervas/product/roadmap.md";
    const ROADMAP_TWIN: &str = "docs/catervas/product/roadmap.agent.md";

    /// A project with the Designer, the Architect and the Marketing Specialist, under `policy`.
    fn project(name: &str, policy: &str) -> Harness {
        Harness::new(name, |wire| {
            with_the_designer(wire);
            with_the_marketing_specialist(wire);
            wire["policy"]["integration"] = json!(policy);
        })
    }

    fn write(harness: &Harness, path: &str, text: &str) {
        harness.project.repo.write(path, text);
    }

    fn commit(harness: &Harness, message: &str) {
        harness.project.repo.commit(message);
    }

    fn git(harness: &Harness, arguments: &[&str]) {
        harness.project.repo.git(arguments);
    }

    fn git_output(harness: &Harness, arguments: &[&str]) -> String {
        harness.project.repo.git_output(arguments)
    }

    fn tree(state: &Arc<DaemonState>) -> Value {
        query(state, "files.tree", &json!({}), "filesTreeResult")
    }

    fn read(state: &Arc<DaemonState>, path: &str) -> Value {
        query(
            state,
            "files.read",
            &json!({ "path": path }),
            "filesReadResult",
        )
    }

    /// The documents main holds in the Product Manager's, the Architect's and the Marketing
    /// Specialist's folders.
    fn put_documents(harness: &Harness) {
        for file in ["spec", "roadmap"] {
            write(
                harness,
                &format!("{PRODUCT}/{file}.md"),
                &format!("{file} for people\n"),
            );
            write(
                harness,
                &format!("{PRODUCT}/{file}.agent.md"),
                &format!("{file} for agents\n"),
            );
        }
        write(harness, OVERVIEW, "the overview\n");
        write(
            harness,
            "docs/catervas/architecture/plans/CTV-1.md",
            "the plan\n",
        );
        write(
            harness,
            "docs/catervas/marketing/brand/persona.md",
            "the persona\n",
        );
        write(
            harness,
            "docs/catervas/marketing/plans/MP-1.md",
            "a marketing plan\n",
        );
        write(
            harness,
            "docs/catervas/marketing/plans/MP-1.agent.md",
            "its twin\n",
        );
        std::os::unix::fs::symlink(
            "spec.md",
            harness.project.repo.path.join(format!("{PRODUCT}/l.md")),
        )
        .expect("a link");
        commit(harness, "the documents");
        harness.project.record(
            "CTV-1",
            "task.created",
            &json!({
                "summary": { "kind": "task", "title": "Pie pre-orders", "status": "draft", "risk": "low" },
                "created_by": "human"
            }),
        );
    }

    /// What a save of the owner's leaves, as `files.save` does it: folder change `docs/folder-<n>`
    /// holding `text` for `path`, and `folder_doc.edited`.
    fn owner_edit(harness: &Harness, path: &str, text: &str) -> u64 {
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team reads");
        let landed = crate::tools::folders::land(
            deps,
            &team,
            &[(path.to_string(), text.to_string())],
            &[],
            "docs(product): edit roadmap.md",
            Some("You <you@catervas.invalid>"),
        )
        .expect("the change is made")
        .expect("the text differs");
        harness.project.record(
            "",
            "folder_doc.edited",
            &json!({ "path": path, "change": landed.change, "sha": landed.sha }),
        );
        landed.change
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_each_active_roles_folder_with_its_documents() {
        let harness = project("files-tree", "pull_request");
        put_documents(&harness);
        let listed = tree(&harness.daemon);
        assert_eq!(listed["integration"], "pull_request");
        let folders: Vec<(&str, &str, &str, &str)> = listed["folders"]
            .as_array()
            .expect("folders")
            .iter()
            .map(|folder| {
                (
                    folder["folder"].as_str().expect("folder"),
                    folder["name"].as_str().expect("name"),
                    folder["role"].as_str().expect("role"),
                    folder["agent_id"].as_str().expect("agent"),
                )
            })
            .collect();
        assert_eq!(
            folders,
            [
                ("docs/catervas/product", "Product", "product_manager", "pm"),
                (
                    "docs/catervas/architecture",
                    "Architecture",
                    "architect",
                    "ada"
                ),
                (
                    "docs/catervas/engineering",
                    "Engineering",
                    "software_developer",
                    "dev-a"
                ),
                ("docs/catervas/design", "Design", "ui_ux_designer", "iris"),
                (
                    "docs/catervas/marketing",
                    "Marketing",
                    "marketing_specialist",
                    "kai"
                ),
            ]
        );
        let documents = |index: usize| -> Vec<(String, String, Value)> {
            listed["folders"][index]["documents"]
                .as_array()
                .expect("documents")
                .iter()
                .map(|document| {
                    (
                        document["path"].as_str().expect("path").to_string(),
                        document["name"].as_str().expect("name").to_string(),
                        document["twin"].clone(),
                    )
                })
                .collect()
        };
        assert_eq!(
            documents(0),
            [
                (
                    format!("{PRODUCT}/spec.md"),
                    "Product description".to_string(),
                    json!(format!("{PRODUCT}/spec.agent.md"))
                ),
                (
                    ROADMAP.to_string(),
                    "Roadmap".to_string(),
                    json!(ROADMAP_TWIN)
                ),
            ]
        );
        assert_eq!(
            documents(1),
            [
                (OVERVIEW.to_string(), "Overview".to_string(), Value::Null),
                (
                    "docs/catervas/architecture/plans/CTV-1.md".to_string(),
                    "Plan: Pie pre-orders".to_string(),
                    Value::Null
                ),
            ]
        );
        assert!(documents(2).is_empty());
        assert_eq!(listed["folders"][2]["documents"], json!([]));

        // A paused agent holds no folder.
        let paused = Harness::new("files-tree-paused", |wire| {
            with_the_designer(wire);
            with_the_marketing_specialist(wire);
            wire["agents"][5]["status"] = json!("paused");
        });
        put_documents(&paused);
        let listed = tree(&paused.daemon);
        assert!(
            listed["folders"]
                .as_array()
                .expect("folders")
                .iter()
                .all(|folder| folder["folder"] != "docs/catervas/marketing")
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_a_document_with_its_twin_and_who_changed_it() {
        let harness = project("files-read", "pull_request");
        put_documents(&harness);
        harness.project.moved(
            "CTV-1",
            "draft",
            "assigned",
            &json!({ "assignee": "pm", "reviewer": "dev-b" }),
        );
        git(&harness, &["checkout", "-b", "feature/CTV-1"]);
        write(&harness, ROADMAP, "a new roadmap\n");
        write(&harness, ROADMAP_TWIN, "a new roadmap for agents\n");
        commit(&harness, "update the roadmap");
        git(&harness, &["checkout", "main"]);
        git(
            &harness,
            &[
                "merge",
                "--no-ff",
                "-m",
                "Merge CTV-1: Pie pre-orders",
                "feature/CTV-1",
            ],
        );

        let read = read(&harness.daemon, ROADMAP);
        assert_eq!(read["document"]["text"], "a new roadmap\n");
        assert_eq!(
            read["document"]["blob"],
            git_output(&harness, &["rev-parse", &format!("main:{ROADMAP}")])
        );
        assert_eq!(read["twin"]["path"], ROADMAP_TWIN);
        assert_eq!(read["twin"]["text"], "a new roadmap for agents\n");
        assert_eq!(
            read["twin"]["blob"],
            git_output(&harness, &["rev-parse", &format!("main:{ROADMAP_TWIN}")])
        );
        let last = &read["document"]["last_change"];
        assert_eq!(
            (&last["task_id"], &last["agent_id"], &last["by_you"]),
            (&json!("CTV-1"), &json!("pm"), &json!(false))
        );
        assert_eq!(
            last["at"],
            git_output(&harness, &["log", "-1", "--format=%cI", "main"])
        );
        assert_eq!(read["stale"], false);
        assert_eq!(read["waiting"], Value::Null);
        assert_eq!(read["rederiving"], false);

        let overview = read_overview(&harness);
        assert_eq!(overview["twin"], Value::Null);
        assert_eq!(overview["editable"], true);
        assert_eq!(overview["document"]["last_change"]["task_id"], Value::Null);
        assert_eq!(
            overview["document"]["last_change"]["author"],
            "Catervas Test"
        );
        assert_eq!(
            read_path(&harness, "docs/catervas/marketing/plans/MP-1.md")["editable"],
            false
        );

        // A folder change an agent wrote names the agent that wrote it.
        git(&harness, &["checkout", "-b", "docs/folder-2"]);
        write(&harness, OVERVIEW, "a rewritten overview\n");
        commit(&harness, "docs(architecture): overview.md by Ada");
        git(&harness, &["checkout", "main"]);
        git(
            &harness,
            &[
                "merge",
                "--no-ff",
                "-m",
                "Merge docs/folder-2: architecture/overview.md",
                "docs/folder-2",
            ],
        );
        harness.project.record_in(
            Some("ada"),
            Some("session-1"),
            "",
            "folder_doc.written",
            &json!({ "path": OVERVIEW, "written_by": "ada", "change": 2, "sha": "abc" }),
        );
        let overview = read_overview(&harness);
        assert_eq!(overview["document"]["last_change"]["agent_id"], "ada");
        assert_eq!(overview["document"]["last_change"]["by_you"], false);
    }

    fn read_path(harness: &Harness, path: &str) -> Value {
        read(&harness.daemon, path)
    }

    fn read_overview(harness: &Harness) -> Value {
        read_path(harness, OVERVIEW)
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_owners_waiting_change_from_its_branch() {
        let harness = project("files-waiting", "manual");
        put_documents(&harness);
        // A re-derive of the twin is already under way.
        harness.project.record(
            "CTV-2",
            "task.created",
            &json!({
                "summary": { "kind": "task", "title": "Bring roadmap.agent.md up to date", "status": "draft", "risk": "low" },
                "created_by": "catervas",
                "rederives": ROADMAP_TWIN
            }),
        );
        let change = owner_edit(&harness, ROADMAP, "the owner's roadmap\n");
        assert_eq!(change, 1);

        let waiting = read_path(&harness, ROADMAP);
        assert_eq!(
            waiting["waiting"],
            json!({ "change": 1, "branch": "docs/folder-1" })
        );
        assert_eq!(waiting["document"]["text"], "the owner's roadmap\n");
        assert_eq!(
            waiting["document"]["blob"],
            git_output(
                &harness,
                &["rev-parse", &format!("docs/folder-1:{ROADMAP}")]
            )
        );
        assert_eq!(waiting["document"]["last_change"]["by_you"], true);
        assert_eq!(waiting["stale"], false);

        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(orchestrator.handle(Command::FolderChangeIntegrate { change: 1 }))
            .expect("the owner adds it");
        let integrated = read_path(&harness, ROADMAP);
        assert_eq!(integrated["waiting"], Value::Null);
        assert_eq!(integrated["document"]["text"], "the owner's roadmap\n");
        assert_eq!(integrated["document"]["last_change"]["by_you"], true);
        assert_eq!(integrated["stale"], true);
        assert_eq!(integrated["rederiving"], true);

        // A change the team could not add no longer holds the path: the page shows main's text.
        owner_edit(&harness, ROADMAP, "a second roadmap\n");
        harness.project.record(
            "",
            "folder_change.escalated",
            &json!({ "change": 2, "detail": "it conflicts" }),
        );
        let escalated = read_path(&harness, ROADMAP);
        assert_eq!(escalated["waiting"], Value::Null);
        assert_eq!(escalated["document"]["text"], "the owner's roadmap\n");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_html_documents_with_their_agent_versions() {
        let harness = project("files-html-tree", "pull_request");
        write(&harness, &format!("{PRODUCT}/spec.html"), "<h1>spec</h1>\n");
        write(
            &harness,
            &format!("{PRODUCT}/spec.agent.html"),
            "<h1>agents</h1>\n",
        );
        write(&harness, &format!("{PRODUCT}/overview.md"), "overview\n");
        write(&harness, &format!("{PRODUCT}/notes.txt"), "notes\n");
        write(&harness, &format!("{PRODUCT}/x.htm"), "<h1>x</h1>\n");
        commit(&harness, "the documents");
        let listed = tree(&harness.daemon);
        let found: Vec<(&str, Value)> = listed["folders"][0]["documents"]
            .as_array()
            .expect("documents")
            .iter()
            .map(|d| (d["path"].as_str().expect("path"), d["twin"].clone()))
            .collect();
        assert_eq!(
            found,
            [
                (
                    "docs/catervas/product/spec.html",
                    json!("docs/catervas/product/spec.agent.html")
                ),
                ("docs/catervas/product/overview.md", Value::Null),
            ]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn saves_the_owners_edit_of_an_html_document() {
        let harness = project("files-html-save", "auto_merge");
        let path = format!("{PRODUCT}/roadmap.html");
        write(&harness, &path, "<h1>roadmap</h1>\n");
        write(
            &harness,
            &format!("{PRODUCT}/roadmap.agent.html"),
            "<h1>agents</h1>\n",
        );
        commit(&harness, "the documents");
        let saved = save(
            &harness,
            &path,
            "<h1>new roadmap</h1>\n",
            &blob(&harness, &path),
        );
        assert_eq!(
            saved["result"],
            json!({ "change": 1, "branch": "docs/folder-1" }),
            "{saved}"
        );
        assert_eq!(
            git_output(&harness, &["show", &format!("docs/folder-1:{path}")]),
            "<h1>new roadmap</h1>"
        );
        ticks(&harness);
        let read = read(&harness.daemon, &path);
        assert_eq!(read["document"]["text"], "<h1>new roadmap</h1>\n");
        assert_eq!(read["stale"], true, "{read}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn askable_where_editable_but_delivery() {
        let harness = Harness::new("files-askable", |wire| {
            with_the_designer(wire);
            wire["agents"].as_array_mut().expect("agents").push(
                catervas_core::team::fixtures::an_agent_wire("sm", "scrum_master"),
            );
        });
        write(
            &harness,
            "docs/catervas/architecture/overview.html",
            "<h1>o</h1>\n",
        );
        write(&harness, "docs/catervas/delivery/cadence.md", "cadence\n");
        write(
            &harness,
            "docs/catervas/architecture/notes.agent.html",
            "<h1>n</h1>\n",
        );
        write(&harness, &format!("{PRODUCT}/spec.html"), "<h1>spec</h1>\n");
        write(
            &harness,
            &format!("{PRODUCT}/spec.agent.html"),
            "<h1>agents</h1>\n",
        );
        commit(&harness, "the documents");
        let overview = read_path(&harness, "docs/catervas/architecture/overview.html");
        assert_eq!(overview["askable"], true, "{overview}");
        assert_eq!(overview["editable"], true);
        let cadence = read_path(&harness, "docs/catervas/delivery/cadence.md");
        assert_eq!(cadence["editable"], true, "{cadence}");
        assert_eq!(cadence["askable"], false);
        let stray = read_path(&harness, "docs/catervas/architecture/notes.agent.html");
        assert_eq!(
            (&stray["editable"], &stray["askable"]),
            (&json!(false), &json!(false))
        );
        let reply = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "files.read", "params": { "path": format!("{PRODUCT}/spec.agent.html") } }),
        );
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_nothing_the_tree_would_not_list() {
        let harness = project("files-not-listed", "pull_request");
        put_documents(&harness);
        for path in [
            ROADMAP_TWIN,
            "docs/catervas/product/l.md",
            "docs/catervas/product/missing.md",
            "src/a.rs",
            "docs/catervas/../README.md",
            "docs/catervas/delivery/cadence.md",
        ] {
            let reply = rpc(
                &harness.daemon,
                "query",
                &json!({ "name": "files.read", "params": { "path": path } }),
            );
            assert_eq!(reply["error"]["code"], -32002, "{path}: {reply}");
        }
    }
    /// The blob `main` holds for `path`.
    fn blob(harness: &Harness, path: &str) -> String {
        git_output(harness, &["rev-parse", &format!("main:{path}")])
    }

    /// The reply to `files.save`.
    fn save(harness: &Harness, path: &str, text: &str, base: &str) -> Value {
        rpc(
            &harness.daemon,
            "files.save",
            &json!({ "path": path, "text": text, "base": base }),
        )
    }

    fn ticks(harness: &Harness) {
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(orchestrator.tick())
            .expect("the tick runs");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn saves_the_owners_edit_as_a_folder_change() {
        let harness = project("files-save", "auto_merge");
        put_documents(&harness);
        let old = blob(&harness, OVERVIEW);

        // Text the file already holds makes nothing.
        let same = save(&harness, OVERVIEW, "the overview\n", &old);
        assert_eq!(
            same["result"],
            json!({ "change": null, "branch": null }),
            "{same}"
        );
        assert!(
            harness
                .project
                .events(&[EventKind::FolderDocEdited])
                .is_empty()
        );

        let saved = save(&harness, OVERVIEW, "the owner's overview\n", &old);
        conforms(&saved["result"], "filesSaveResult", &saved);
        assert_eq!(
            saved["result"],
            json!({ "change": 1, "branch": "docs/folder-1" })
        );
        assert_eq!(
            git_output(&harness, &["rev-list", "--count", "main..docs/folder-1"]),
            "1"
        );
        assert_eq!(
            git_output(&harness, &["log", "-1", "--format=%s", "docs/folder-1"]),
            "docs(architecture): edit overview.md"
        );
        assert_eq!(
            git_output(&harness, &["log", "-1", "--format=%an", "docs/folder-1"]),
            "Catervas Test"
        );
        assert_eq!(
            git_output(&harness, &["show", &format!("docs/folder-1:{OVERVIEW}")]),
            "the owner's overview"
        );
        let edited = harness.project.events(&[EventKind::FolderDocEdited]);
        assert_eq!(edited.len(), 1);
        assert_eq!(
            catervas_protocol::event::event_to_value(&edited[0])["body"],
            json!({
                "path": OVERVIEW,
                "change": 1,
                "sha": git_output(&harness, &["rev-parse", "docs/folder-1"])
            })
        );
        assert_eq!(edited[0].envelope.ids.agent_id, None);
        assert_eq!(edited[0].envelope.ids.task_id, None);
        assert_eq!(
            git_output(&harness, &["show", &format!("main:{OVERVIEW}")]),
            "the overview",
            "main holds the old text until the next tick"
        );

        ticks(&harness);
        assert_eq!(
            git_output(&harness, &["show", &format!("main:{OVERVIEW}")]),
            "the owner's overview"
        );
        let again = save(
            &harness,
            OVERVIEW,
            "the owner's overview\n",
            &blob(&harness, OVERVIEW),
        );
        assert_eq!(
            again["result"],
            json!({ "change": null, "branch": null }),
            "{again}"
        );
        assert_eq!(
            harness.project.events(&[EventKind::FolderDocEdited]).len(),
            1
        );

        // A path with a `.` segment in it is saved as the path it names.
        let dotted = save(
            &harness,
            "docs/catervas/architecture/./overview.md",
            "the overview again\n",
            &blob(&harness, OVERVIEW),
        );
        assert_eq!(dotted["result"]["change"], 2, "{dotted}");
        let edited = harness.project.events(&[EventKind::FolderDocEdited]);
        assert_eq!(
            catervas_protocol::event::event_to_value(&edited[1])["body"]["path"],
            OVERVIEW
        );
    }

    /// The words of the first error of `reply`, which the page shows as they are.
    fn said(reply: &Value) -> &str {
        reply["error"]["data"]["errors"][0]["message"]
            .as_str()
            .expect("a message")
    }

    /// The first error of `reply`: its code and its pointer.
    fn refusal(reply: &Value) -> (String, String) {
        assert_eq!(reply["error"]["code"], -32005, "{reply}");
        let first = &reply["error"]["data"]["errors"][0];
        (
            first["code"].as_str().expect("a code").to_string(),
            first["path"].as_str().expect("a path").to_string(),
        )
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_each_edit_the_owner_may_not_save() {
        let harness = project("files-save-refused", "manual");
        put_documents(&harness);
        let overview = blob(&harness, OVERVIEW);
        let gone = "0".repeat(40);
        let long = "x".repeat(262_145);
        let refused = |path: &str, text: &str, base: &str, code: &str, at: &str| -> Value {
            let reply = save(&harness, path, text, base);
            assert_eq!(
                refusal(&reply),
                (code.to_string(), at.to_string()),
                "{path}: {reply}"
            );
            assert!(
                reply["error"]["data"]["errors"][0]["message"]
                    .as_str()
                    .is_some_and(|message| !message.is_empty()),
                "{reply}"
            );
            assert!(
                harness
                    .project
                    .events(&[EventKind::FolderDocEdited])
                    .is_empty()
            );
            assert_eq!(
                git_output(&harness, &["branch", "--list", "docs/folder-*"]),
                ""
            );
            reply
        };
        for outside in [
            "docs/catervas/../x.md",
            "README.md",
            "docs/catervas/delivery/x.md",
        ] {
            refused(outside, "x\n", &gone, "outside_folders", "/path");
        }
        let not_a_document = refused(
            "docs/catervas/product/notes.txt",
            "x\n",
            &gone,
            "not_a_document",
            "/path",
        );
        assert_eq!(
            said(&not_a_document),
            "Only a document that ends in .md or .html can be saved here."
        );
        refused(ROADMAP_TWIN, "x\n", &gone, "agent_twin", "/path");
        refused(
            "docs/catervas/marketing/plans/MP-1.md",
            "x\n",
            &gone,
            "marketing_plan",
            "/path",
        );
        let too_large = refused(OVERVIEW, &long, &overview, "too_large", "/text");
        assert_eq!(
            said(&too_large),
            "This is too long to save here: a document can be at most about 256 KB."
        );
        refused(OVERVIEW, "x\n", &gone, "file_changed", "/base");
        refused(
            "docs/catervas/product/gone.md",
            "x\n",
            &gone,
            "file_changed",
            "/base",
        );

        // An uncommitted change at the project's root to the same file.
        write(&harness, ROADMAP, "the owner's own, not committed\n");
        let busy = refused(
            ROADMAP,
            "x\n",
            &blob(&harness, ROADMAP),
            "file_busy",
            "/path",
        );
        assert_eq!(
            said(&busy),
            "This file was also changed on your computer, outside Catervas. Finish or undo that change, then save here again."
        );
        git(&harness, &["checkout", "--", ROADMAP]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_save_over_a_waiting_change_or_a_link() {
        let harness = project("files-save-waiting", "manual");
        put_documents(&harness);
        let overview = blob(&harness, OVERVIEW);

        // A change that waits to be added holds its document.
        let saved = save(&harness, OVERVIEW, "first\n", &overview);
        assert_eq!(saved["result"]["change"], 1, "{saved}");
        let reply = save(&harness, OVERVIEW, "second\n", &overview);
        assert_eq!(
            refusal(&reply),
            ("change_waiting".to_string(), "/path".to_string())
        );
        assert_eq!(
            said(&reply),
            "An earlier change to this file is not in your project yet. Edit it once it is."
        );
        assert_eq!(
            harness.project.events(&[EventKind::FolderDocEdited]).len(),
            1
        );

        // A link on the way to the file.
        let elsewhere = harness.project.repo.path.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("a directory");
        std::fs::remove_dir_all(harness.project.repo.path.join("docs/catervas")).expect("removed");
        std::os::unix::fs::symlink(&elsewhere, harness.project.repo.path.join("docs/catervas"))
            .expect("a link");
        let reply = save(&harness, ROADMAP, "x\n", &blob(&harness, ROADMAP));
        assert_eq!(
            refusal(&reply),
            ("is_a_link".to_string(), "/path".to_string())
        );
        assert_eq!(
            said(&reply),
            "This file is a link, which Catervas does not write through."
        );
        assert!(
            !elsewhere.join("product").exists(),
            "nothing was written through it"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waits_for_an_integration_to_finish() {
        let harness = project("files-save-lock", "auto_merge");
        put_documents(&harness);
        let base = blob(&harness, OVERVIEW);
        let saved = crate::tools::fixtures::waits_for_the_lock_file(
            &harness.project.repo.path,
            &harness.project,
            || save(&harness, OVERVIEW, "the owner's overview\n", &base),
        );
        assert_eq!(saved["result"]["change"], 1, "{saved}");
        assert_eq!(
            harness.project.events(&[EventKind::FolderDocEdited]).len(),
            1
        );
    }

    const PORT: u16 = 49_731;
    const HOST: &str = "127.0.0.1:49731";
    const OVERVIEW_HTML: &str = "docs/catervas/architecture/overview.html";
    const POLICY: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; frame-ancestors 'self'";

    /// Turns the browser routes on and returns a live session's cookie.
    fn browse(harness: &Harness) -> String {
        use crate::daemon::web::{BrowserSessions, ConnectCodes, WebState};
        use catervas_protocol::clock::FixedClock;
        use chrono::TimeZone;
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 10, 12, 9, 0, 0)
            .single()
            .expect("a time");
        let sessions = BrowserSessions::open(None).expect("the sessions open");
        let secret = sessions.issue(now).expect("a session");
        assert!(harness.daemon.set_web(WebState {
            codes: ConnectCodes::default(),
            sessions,
            project_root: harness.project.repo.path.clone(),
            credential: None,
            port: PORT,
            clock: Arc::new(FixedClock::new(now)),
            take_on_error: std::sync::Mutex::default(),
            leaving: std::sync::Mutex::default(),
            stores: Vec::new(),
            env: std::collections::BTreeMap::new(),
            in_use: None,
            templates: None,
            #[cfg(feature = "e2e")]
            admit_local_preview: false,
        }));
        format!("catervas_session={secret}")
    }

    /// `GET /documents/raw?<query>`: its status, headers and body.
    async fn raw(
        state: &Arc<DaemonState>,
        query: &str,
        host: &str,
        cookie: Option<&str>,
    ) -> (axum::http::StatusCode, axum::http::HeaderMap, String) {
        use tower::ServiceExt;
        let mut request = axum::http::Request::get(format!("/documents/raw?{query}"))
            .header(axum::http::header::HOST, host);
        if let Some(cookie) = cookie {
            request = request.header(axum::http::header::COOKIE, cookie);
        }
        let answer = crate::daemon::router(
            Arc::clone(state),
            "token",
            tokio_util::sync::CancellationToken::new(),
        )
        .oneshot(request.body(axum::body::Body::empty()).expect("a request"))
        .await
        .expect("the router answers");
        let (parts, body) = answer.into_parts();
        let body = axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("a body");
        (
            parts.status,
            parts.headers,
            String::from_utf8(body.to_vec()).expect("text"),
        )
    }

    /// The document `path` asked by a signed-in browser, `extra` after the path in the query.
    async fn served(harness: &Harness, cookie: &str, path: &str, extra: &str) -> (u16, String) {
        let (status, _, body) = raw(
            &harness.daemon,
            &format!("path={path}{extra}"),
            HOST,
            Some(cookie),
        )
        .await;
        (status.as_u16(), body)
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn serves_an_html_document_sandboxed() {
        let harness = project("raw-sandboxed", "manual");
        write(&harness, OVERVIEW_HTML, "<h1>the overview</h1>\n");
        commit(&harness, "the overview");
        let cookie = browse(&harness);
        // The page reloads the frame with `v`, which the route ignores.
        let (status, headers, body) = raw(
            &harness.daemon,
            &format!("path={OVERVIEW_HTML}&v=abc"),
            HOST,
            Some(&cookie),
        )
        .await;
        assert_eq!(status, 200);
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
        };
        assert_eq!(header("content-type"), "text/html; charset=utf-8");
        assert_eq!(header("content-security-policy"), POLICY);
        assert_eq!(header("x-content-type-options"), "nosniff");
        assert_eq!(header("referrer-policy"), "no-referrer");
        assert_eq!(header("x-dns-prefetch-control"), "off");
        assert_eq!(header("cache-control"), "no-store");
        assert!(body.contains("<h1>the overview</h1>"), "{body}");
        assert_eq!(body.matches(super::FIT).count(), 1, "{body}");
    }

    #[test]
    fn fits_the_document_to_the_screen() {
        for (document, after) in [
            (
                "<!DOCTYPE html><html><head lang=\"en\"><title>t</title></head>",
                "<head lang=\"en\">",
            ),
            ("<!doctype html><HTML lang=en><p>x</p>", "<HTML lang=en>"),
            ("<!DOCTYPE html><p>x</p>", "<!DOCTYPE html>"),
            ("<p>x</p>", ""),
        ] {
            let fitted = super::fit_to_screen(document);
            assert_eq!(fitted.matches(super::FIT).count(), 1, "{fitted}");
            assert_eq!(fitted.replacen(super::FIT, "", 1), document);
            let at = fitted.find(super::FIT).expect("the block");
            let wanted = document.find(after).map_or(0, |n| n + after.len());
            assert_eq!(at, wanted, "{fitted}");
        }
        assert!(
            super::FIT.contains(
                r#"<meta name="viewport" content="width=device-width, initial-scale=1">"#
            )
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn serves_the_owners_waiting_change_and_the_agent_version() {
        let harness = project("raw-waiting", "manual");
        let roadmap = format!("{PRODUCT}/roadmap.html");
        let twin = format!("{PRODUCT}/roadmap.agent.html");
        write(&harness, &roadmap, "<h1>roadmap</h1>\n");
        write(&harness, &twin, "<h1>for agents</h1>\n");
        commit(&harness, "the documents");
        let cookie = browse(&harness);
        let before = served(&harness, &cookie, &roadmap, "").await;
        assert!(before.1.contains("<h1>roadmap</h1>"), "{before:?}");
        owner_edit(&harness, &roadmap, "<h1>the owner's roadmap</h1>\n");
        let (status, body) = served(&harness, &cookie, &roadmap, "").await;
        assert_eq!(status, 200);
        assert!(body.contains("<h1>the owner's roadmap</h1>"), "{body}");
        let (status, body) = served(&harness, &cookie, &twin, "").await;
        assert_eq!(status, 200);
        assert!(body.contains("<h1>for agents</h1>"), "{body}");
        assert!(!body.contains("owner's"), "{body}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn serves_a_tasks_branch_then_its_worktree() {
        let harness = project("raw-task", "manual");
        let spec = format!("{PRODUCT}/spec.html");
        let plan = format!("{PRODUCT}/plan.html");
        for id in ["CTV-2", "CTV-3", "CTV-4"] {
            harness.project.filed(id, "in_progress", "task", None);
        }
        commit(&harness, "the tasks");
        let branch = harness.project.branch("CTV-2");
        git(&harness, &["checkout", "-b", &branch]);
        write(&harness, &spec, "<h1>the task's spec</h1>\n");
        write(&harness, "docs/other.html", "<h1>other</h1>\n");
        commit(&harness, "the spec");
        git(&harness, &["checkout", "main"]);
        let cookie = browse(&harness);
        let (status, body) = served(&harness, &cookie, &spec, "&task=CTV-2").await;
        assert_eq!(status, 200);
        assert!(body.contains("<h1>the task's spec</h1>"), "{body}");
        // The branch wins over the worktree.
        let worktree_spec = harness
            .project
            .repo
            .path
            .join(".catervas/local/worktrees/CTV-2")
            .join(&spec);
        std::fs::create_dir_all(worktree_spec.parent().expect("a folder")).expect("a folder");
        std::fs::write(&worktree_spec, "<h1>the worktree's spec</h1>\n").expect("written");
        let (_, body) = served(&harness, &cookie, &spec, "&task=CTV-2").await;
        assert!(body.contains("the task's spec"), "{body}");
        // Only documents under docs/catervas/ are served, with a task too.
        let other = served(&harness, &cookie, "docs/other.html", "&task=CTV-2").await;
        assert_eq!(other, (404, String::new()));
        // Main does not hold it.
        assert_eq!(served(&harness, &cookie, &spec, "").await.0, 404);

        // A plan only in the worktree.
        let root = harness.project.repo.path.join(".catervas/local/worktrees");
        let file = root.join("CTV-3").join(&plan);
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("a folder");
        std::fs::write(&file, "<h1>drafted</h1>\n").expect("written");
        let (status, body) = served(&harness, &cookie, &plan, "&task=CTV-3").await;
        assert_eq!(status, 200);
        assert!(body.contains("<h1>drafted</h1>"), "{body}");

        // A link on the way gives nothing.
        let outside = harness.project.repo.path.join("outside");
        std::fs::create_dir_all(outside.join("catervas/product")).expect("a folder");
        std::fs::write(
            outside.join("catervas/product/plan.html"),
            "<h1>secret</h1>\n",
        )
        .expect("written");
        std::fs::create_dir_all(root.join("CTV-4")).expect("a folder");
        std::os::unix::fs::symlink(&outside, root.join("CTV-4/docs")).expect("a link");
        let through = served(&harness, &cookie, &plan, "&task=CTV-4").await;
        assert_eq!(through, (404, String::new()));
        // A path that climbs out of the worktree gives nothing, written or encoded.
        for climb in [
            "docs/catervas/../../../../../../outside/catervas/product/plan.html",
            "docs%2Fcatervas%2F%2e%2e%2F%2e%2e%2F%2e%2e%2F%2e%2e%2F%2e%2e%2F%2e%2e%2Foutside%2Fcatervas%2Fproduct%2Fplan.html",
        ] {
            let up = served(&harness, &cookie, climb, "&task=CTV-3").await;
            assert_eq!(up, (404, String::new()), "{climb}");
        }
        // And a link as the file itself.
        let linked = format!("{PRODUCT}/linked.html");
        std::os::unix::fs::symlink(&file, root.join("CTV-3").join(&linked)).expect("a link");
        let itself = served(&harness, &cookie, &linked, "&task=CTV-3").await;
        assert_eq!(itself, (404, String::new()));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_what_it_does_not_serve() {
        let harness = project("raw-refused", "manual");
        write(&harness, OVERVIEW_HTML, "<h1>the overview</h1>\n");
        write(&harness, OVERVIEW, "the overview\n");
        write(&harness, "docs/other.html", "<h1>other</h1>\n");
        commit(&harness, "the documents");
        let ask = format!("path={OVERVIEW_HTML}");
        // No browser routes: 404.
        let (status, _, body) = raw(&harness.daemon, &ask, HOST, None).await;
        assert_eq!((status.as_u16(), body.as_str()), (404, ""));
        let cookie = browse(&harness);
        let (status, _, body) = raw(&harness.daemon, &ask, HOST, None).await;
        assert_eq!((status.as_u16(), body.as_str()), (401, ""));
        let (status, _, body) = raw(&harness.daemon, &ask, "evil.example", Some(&cookie)).await;
        assert_eq!((status.as_u16(), body.as_str()), (403, ""));
        assert_eq!(served(&harness, &cookie, OVERVIEW_HTML, "").await.0, 200);
        for query in [
            format!("path={OVERVIEW}"),
            "path=docs/catervas/../other.html".to_string(),
            "path=docs/other.html".to_string(),
            "path=docs/catervas/architecture/missing.html".to_string(),
            "path=docs/catervas/architecture".to_string(),
            String::new(),
            format!("{ask}&task=nope"),
            format!("{ask}&task=CTV-99"),
        ] {
            let (status, _, body) = raw(&harness.daemon, &query, HOST, Some(&cookie)).await;
            assert_eq!((status.as_u16(), body.as_str()), (404, ""), "{query}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn answers_404_with_no_project() {
        use tower::ServiceExt;
        let home = std::env::temp_dir().join(format!("catervas-raw-setup-{}", std::process::id()));
        let (state, _host) = crate::daemon::web::tests::in_setup(&home, "");
        let web = state.web().expect("the browser routes are on");
        let secret = web.sessions.issue(web.clock.now()).expect("a session");
        let request = axum::http::Request::get(
            "/documents/raw?path=docs/catervas/architecture/overview.html",
        )
        .header(axum::http::header::HOST, HOST)
        .header(
            axum::http::header::COOKIE,
            format!("catervas_session={secret}"),
        )
        .body(axum::body::Body::empty())
        .expect("a request");
        let answer =
            crate::daemon::router(state, "token", tokio_util::sync::CancellationToken::new())
                .oneshot(request)
                .await
                .expect("the router answers");
        let status = answer.status().as_u16();
        let body = axum::body::to_bytes(answer.into_body(), usize::MAX)
            .await
            .expect("a body");
        assert_eq!((status, body.len()), (404, 0));
    }

    /// The reply to `files.ask_change`.
    fn ask(harness: &Harness, path: &str, words: &str) -> Value {
        rpc(
            &harness.daemon,
            "files.ask_change",
            &json!({ "path": path, "words": words }),
        )
    }

    /// A project with a Scrum Master, holding the documents the asks are about.
    fn asked_project(name: &str) -> Harness {
        let harness = Harness::new(name, |wire| {
            with_the_designer(wire);
            wire["agents"].as_array_mut().expect("agents").push(
                catervas_core::team::fixtures::an_agent_wire("sm", "scrum_master"),
            );
        });
        write(&harness, OVERVIEW_HTML, "<h1>overview</h1>\n");
        write(&harness, &format!("{PRODUCT}/spec.html"), "<h1>spec</h1>\n");
        write(
            &harness,
            &format!("{PRODUCT}/spec.agent.html"),
            "<h1>agents</h1>\n",
        );
        write(&harness, "docs/catervas/delivery/cadence.md", "cadence\n");
        write(
            &harness,
            "docs/catervas/marketing/plans/MP-1.md",
            "a marketing plan\n",
        );
        write(
            &harness,
            "docs/catervas/marketing/brand/persona.md",
            "the persona\n",
        );
        commit(&harness, "the documents");
        harness
    }

    /// The contract `id` as JSON.
    fn contract_of(harness: &Harness, id: &str) -> Value {
        serde_json::to_value(
            harness
                .project
                .deps
                .files
                .read_contract(&id.parse().expect("an id"))
                .expect("the contract is filed"),
        )
        .expect("a contract is JSON")
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_the_owners_change_for_the_folders_role() {
        let harness = asked_project("files-ask-change");
        let reply = ask(&harness, OVERVIEW_HTML, "  Say less about the cache.  ");
        conforms(&reply["result"], "filesAskChangeResult", &reply);
        assert_eq!(reply["result"], json!({ "task_id": "CTV-1" }), "{reply}");

        let contract = contract_of(&harness, "CTV-1");
        assert_eq!(contract["assignee_role"], "architect", "{contract}");
        assert_eq!(contract["created_by"], "human");
        assert_eq!(contract["title"], "Change Overview");
        assert_eq!(contract["allowed_paths"], json!([OVERVIEW_HTML]));
        assert!(
            contract["intent"].as_str().is_some_and(
                |intent| intent.ends_with("in these words:\n\nSay less about the cache.")
            ),
            "{contract}"
        );

        let filed = harness
            .project
            .events(&[EventKind::TaskCreated, EventKind::RequestTriaged]);
        let kinds: Vec<_> = filed.iter().map(|event| event.body.kind()).collect();
        assert_eq!(kinds, [EventKind::TaskCreated, EventKind::RequestTriaged]);
        let created = catervas_protocol::event::event_to_value(&filed[0])["body"].clone();
        assert_eq!(created["filed_as"], "owner_change", "{created}");
        assert_eq!(created["created_by"], "human");
        let triaged = catervas_protocol::event::event_to_value(&filed[1])["body"].clone();
        assert_eq!(triaged["size"], "small", "{triaged}");
        assert_eq!(triaged["triaged_by"], "catervas");

        // A pair's ask reaches both files.
        let spec = format!("{PRODUCT}/spec.html");
        let reply = ask(&harness, &spec, "Add a refund limit.");
        assert_eq!(reply["result"], json!({ "task_id": "CTV-2" }), "{reply}");
        let pair = contract_of(&harness, "CTV-2");
        assert_eq!(pair["assignee_role"], "product_manager");
        assert_eq!(
            pair["allowed_paths"],
            json!([spec, format!("{PRODUCT}/spec.agent.html")])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_ask_the_owner_may_not_make() {
        let harness = asked_project("files-ask-refused");
        let refused = |path: &str, code: &str, words: &str| {
            let reply = ask(&harness, path, "Change it.");
            assert_eq!(
                refusal(&reply),
                (code.to_string(), "/path".to_string()),
                "{path}: {reply}"
            );
            assert_eq!(said(&reply), words, "{path}");
            assert!(
                harness
                    .project
                    .events(&[EventKind::TaskCreated, EventKind::RequestTriaged])
                    .is_empty(),
                "{path}"
            );
        };
        let cannot = "This document can't be changed from here.";
        refused(
            &format!("{PRODUCT}/spec.agent.html"),
            "not_editable",
            cannot,
        );
        refused(
            "docs/catervas/marketing/plans/MP-1.md",
            "not_editable",
            cannot,
        );
        refused("README.md", "not_editable", cannot);
        refused(
            "docs/catervas/architecture/gone.html",
            "not_editable",
            cannot,
        );
        refused(
            "docs/catervas/marketing/brand/persona.md",
            "no_owner",
            "No one on your team owns this folder right now. Resume or add its role first.",
        );
        refused(
            "docs/catervas/delivery/cadence.md",
            "no_task_taker",
            "The Scrum Master keeps this document up to date at its retro.",
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_delivery_ask_is_for_the_scrum_master_even_without_one() {
        let harness = Harness::new("files-ask-no-sm", with_the_designer);
        write(&harness, "docs/catervas/delivery/cadence.md", "cadence\n");
        commit(&harness, "the documents");
        let reply = ask(&harness, "docs/catervas/delivery/cadence.md", "Change it.");
        assert_eq!(
            refusal(&reply),
            ("no_task_taker".to_string(), "/path".to_string()),
            "{reply}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_human_documents_ask_names_its_twin_even_when_it_has_none() {
        let harness = Harness::new("files-ask-lone-spec", with_the_designer);
        let spec = format!("{PRODUCT}/spec.html");
        write(&harness, &spec, "<h1>spec</h1>\n");
        commit(&harness, "the spec alone");
        let reply = ask(&harness, &spec, "Add a refund limit.");
        assert_eq!(reply["result"], json!({ "task_id": "CTV-1" }), "{reply}");
        assert_eq!(
            contract_of(&harness, "CTV-1")["allowed_paths"],
            json!([spec, format!("{PRODUCT}/spec.agent.html")])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_blank_words() {
        let harness = asked_project("files-ask-blank");
        let reply = ask(&harness, OVERVIEW_HTML, " \t\n ");
        assert_eq!(
            refusal(&reply),
            ("no_words".to_string(), "/words".to_string()),
            "{reply}"
        );
        assert_eq!(said(&reply), "Say what should change.");
        assert!(
            harness
                .project
                .events(&[EventKind::TaskCreated, EventKind::RequestTriaged])
                .is_empty()
        );
    }
}
