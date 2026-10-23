//! The Files page's server (`docs/SPEC.md` 5.17): the team's folders and their documents read from
//! the integration branch, one document with its twin, who last changed each, and whether the twin
//! is stale. `web.rs` answers the frames; this module answers what they ask.

use std::fmt::Display;
use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};

use catervas_core::contract::{TaskKind, TaskStatus};
use catervas_core::folders::{
    AttachKind, DocumentFormat, Named, OwnerEditRefusal, ROLE_FOLDERS, agent_twin,
    check_owner_edit, document_name, document_order, free_attached_path, human_of_twin,
    is_human_document, named_in_subject, plain_name,
};
use catervas_core::governor::paths::normalise;
use catervas_core::team::{Role, Team};
use catervas_store::document_source::{
    DocumentSource, documents_mode_of, documents_on_disk, stale_twins_now, team_documents,
};
use catervas_store::folder_docs::{
    FolderChange, FolderDocs, LocalWriter, folder_docs, local_writes,
};
use catervas_store::git::{PathCommit, TreeEntry, integration_branch};
use catervas_store::local_documents::write_document;
use catervas_store::marketing::marketing_plans;
use catervas_store::requests::rederive_tasks;
use catervas_store::waiting::name_of;
use serde_json::{Value, json};

use catervas_protocol::event::{EventBody, EventKind, TaskCreatedBodyFiledAs};
use catervas_store::EventQuery;
use catervas_store::requests::{
    change_request, file_small_request, placeholder_budget_usd, rewrite_request, rewrite_tasks,
};

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
pub(super) const METHODS: [&str; 5] = [
    "files.save",
    "files.ask_change",
    "files.rewrite",
    "document.attach",
    "documents.set_local",
];

/// Where the team's documents live.
const ROOT: &str = "docs/catervas";

/// Where the marketing plans are; each is the version the owner approved, so none is rewritten.
const MARKETING_PLANS: &str = "docs/catervas/marketing/plans/";

/// Whether `path` is a marketing plan `MP-<n>` or its agent version.
fn is_marketing_plan(path: &str) -> bool {
    path.starts_with(MARKETING_PLANS)
        && (is_human_document(path)
            || human_of_twin(path).is_some_and(|human| is_human_document(&human)))
}

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
    refuse_while_switching(deps, &team, "/path")?;
    if documents_on_disk(&team, &deps.log).map_err(|e| internal(&e))? {
        return save_on_disk(
            deps,
            &team,
            &path,
            text,
            params["base"].as_str().unwrap_or_default(),
        );
    }
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
        "path": path, "change": landed.change, "sha": landed.sha, "branch": landed.branch,
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::FolderDocEdited(body))?;
    Ok(json!({ "change": landed.change, "branch": landed.branch }))
}

/// The words a refused attachment is shown, by its code.
fn attach_words(code: &str) -> &'static str {
    match code {
        "unsupported_type" => "Catervas reads Markdown, text, PDF and Word files.",
        "no_text" => "This file has no text Catervas can read. It may be a scan.",
        "too_large" => {
            "This file is too large. Catervas takes up to 10 MB, and up to 256 KB of text."
        }
        _ => "Catervas could not open this file.",
    }
}

/// Refuses at `at` while the documents are being moved (ADR 0074), when nothing may change them.
fn refuse_while_switching(deps: &ToolDeps, team: &Team, at: &str) -> Result<(), Failure> {
    if documents_mode_of(team, &deps.log)
        .map_err(|e| internal(&e))?
        .writes_open()
    {
        return Ok(());
    }
    Err(refusal(
        "documents_switching",
        at,
        "Documents can't be changed until you merge the pull request that moves them.",
    ))
}

/// `document.attach { name, kind, data_base64 }`: the file turned into Markdown and committed, as
/// the owner's own change (as `files.save` does), at a free `attached/<slug>.md` of the kind's
/// folder; recorded as `folder_doc.edited` then `document.attached`. Its integration files the task
/// that writes the agent version.
fn attach(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    use base64::Engine as _;
    let kind = AttachKind::parse(params["kind"].as_str().unwrap_or_default())
        .ok_or_else(|| internal(&"the schema let through a kind that is not one"))?;
    let name = params["name"].as_str().unwrap_or_default();
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    if !team
        .active_agents()
        .any(|agent| Role::from(agent.role) == kind.role())
    {
        return Err(refusal(
            "no_folder_owner",
            "/kind",
            "No one on the team keeps these documents. Add a Product Manager or an Architect.",
        ));
    }
    let unreadable = || refusal("unreadable", "/data_base64", attach_words("unreadable"));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(params["data_base64"].as_str().unwrap_or_default())
        .map_err(|_| unreadable())?;
    let converted = crate::attach::to_markdown(name, &bytes)
        .map_err(|why| refusal(why.code(), "/data_base64", attach_words(why.code())))?;
    let _lock = integration_lock(deps.files.root()).map_err(|e| internal(&e))?;
    refuse_while_switching(deps, &team, "/name")?;
    if documents_on_disk(&team, &deps.log).map_err(|e| internal(&e))? {
        return attach_on_disk(deps, &team, kind, name, &converted);
    }
    let branch = integration_branch(&team, &deps.git).map_err(|e| internal(&e))?;
    // A path is taken when the integration branch holds it or its agent version, or a folder
    // change not yet integrated does.
    let mut held: Vec<String> = deps
        .git
        .tree_entries(&branch, &format!("{}/attached", kind.folder()))
        .map_err(|e| internal(&e))?
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    for change in folder_docs(&deps.log)
        .map_err(|e| internal(&e))?
        .changes
        .iter()
        .filter(|change| !change.integrated)
    {
        held.extend(change.paths.iter().cloned());
    }
    let path = free_attached_path(kind, name, |path| {
        held.iter()
            .any(|held| held == path || Some(held) == agent_twin(path).as_ref())
    });
    let file = path.rsplit('/').next().unwrap_or(&path);
    let scope = kind.folder().rsplit('/').next().unwrap_or_default();
    let message = format!("docs({scope}): attach {file}");
    let landed = match land(
        deps,
        &team,
        &[(path.clone(), converted.text)],
        &[],
        &message,
        None,
    ) {
        Ok(Some(landed)) => landed,
        Ok(None) => return Err(internal(&format!("{path} was already there"))),
        Err(LandRefusal::Busy(_)) => {
            return Err(refusal(
                "file_busy",
                "/name",
                "A file of this name was also changed on your computer, outside Catervas. Finish or undo that change, then attach it again.",
            ));
        }
        Err(LandRefusal::Link(_)) => {
            return Err(refusal(
                "is_a_link",
                "/name",
                "This folder holds a link, which Catervas does not write through.",
            ));
        }
        Err(LandRefusal::Waiting { .. }) => {
            return Err(refusal(
                "change_waiting",
                "/name",
                "An earlier change to this file is not in your project yet. Attach it again once it is.",
            ));
        }
        Err(LandRefusal::Git(detail)) => return Err(Failure::new(INTERNAL_ERROR, detail)),
    };
    let edited = serde_json::from_value(json!({
        "path": path, "change": landed.change, "sha": landed.sha, "branch": landed.branch,
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::FolderDocEdited(edited))?;
    let attached = serde_json::from_value(json!({
        "path": path, "name": name, "media_type": converted.media_type,
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::DocumentAttached(attached))?;
    Ok(json!({ "path": path, "change": landed.change }))
}

/// `document.attach` with the documents kept on this computer (ADR 0074): the file is written at
/// a free path of the folder on disk, recorded as `local_documents.written` then
/// `document.attached`, and the task that writes its agent version is filed at once, as
/// `save_on_disk` does. The caller holds the integration lock.
fn attach_on_disk(
    deps: &ToolDeps,
    team: &Team,
    kind: AttachKind,
    name: &str,
    converted: &crate::attach::Converted,
) -> Result<Value, Failure> {
    let source = DocumentSource::Disk(deps.files.root().to_path_buf());
    let held: Vec<String> = source
        .entries(&deps.git, &format!("{}/attached", kind.folder()))
        .map_err(|e| internal(&e))?
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    let path = free_attached_path(kind, name, |path| {
        held.iter()
            .any(|held| held == path || Some(held) == agent_twin(path).as_ref())
    });
    write_document(deps.files.root(), &path, &converted.text).map_err(|_| {
        refusal(
            "is_a_link",
            "/name",
            "This folder holds a link, which Catervas does not write through.",
        )
    })?;
    let written = serde_json::from_value(json!({
        "paths": [path], "removed": [], "by": "owner",
        "written_by": null, "task_id": null, "set_aside": [],
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::LocalDocumentsWritten(written))?;
    let attached = serde_json::from_value(json!({
        "path": path, "name": name, "media_type": converted.media_type,
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::DocumentAttached(attached))?;
    if let Err(reason) = crate::tools::folders::file_rederive_for(deps, team, &path) {
        crate::channel::post_system(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            None,
            &format!(
                "{name} was attached, but the task that writes the team's version of it could not be filed: {reason}"
            ),
        )
        .map_err(|e| internal(&e))?;
    }
    Ok(json!({ "path": path, "change": null }))
}

/// The name of the file the owner attached as `path`, from the newest `document.attached`.
fn attached_from(deps: &ToolDeps, path: &str) -> Result<Value, Failure> {
    let events = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::DocumentAttached],
            newest_first: true,
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    Ok(events
        .iter()
        .find_map(|event| match &event.body {
            EventBody::DocumentAttached(body) if body.path.to_string() == path => {
                Some(json!({ "name": body.name.to_string() }))
            }
            _ => None,
        })
        .unwrap_or(Value::Null))
}

/// `files.save` with the documents kept on this computer (ADR 0074): the file is written at once,
/// whole or not at all, after `base` is checked against the file's own bytes. A human document's
/// twin is brought up to date by a task filed at once. The caller holds the integration lock.
fn save_on_disk(
    deps: &ToolDeps,
    team: &Team,
    path: &str,
    text: &str,
    base: &str,
) -> Result<Value, Failure> {
    let source = DocumentSource::Disk(deps.files.root().to_path_buf());
    let held = source
        .entries(&deps.git, path)
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|entry| entry.path == path && is_file(entry));
    if held.is_none_or(|entry| entry.blob != base) {
        return Err(refusal(
            "file_changed",
            "/base",
            "This document changed since you opened it. Open it again to see what changed.",
        ));
    }
    if source
        .text(&deps.git, path)
        .map_err(|e| internal(&e))?
        .as_deref()
        == Some(text)
    {
        return Ok(json!({ "change": null, "branch": null }));
    }
    write_document(deps.files.root(), path, text).map_err(|error| {
        refusal(
            "is_a_link",
            "/path",
            &format!("This file is a link, which Catervas does not write through ({error})."),
        )
    })?;
    let body = serde_json::from_value(json!({
        "paths": [path], "removed": [], "by": "owner",
        "written_by": null, "task_id": null, "set_aside": [],
    }))
    .map_err(|e| internal(&e))?;
    append(deps, EventBody::LocalDocumentsWritten(body))?;
    if is_human_document(path)
        && let Err(reason) = crate::tools::folders::file_rederive_for(deps, team, path)
    {
        crate::channel::post_system(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            None,
            &format!(
                "{} was saved, but the task that brings the team's version of it up to date could not be filed: {reason}",
                document_name(path, &|_: &str| None)
            ),
        )
        .map_err(|e| internal(&e))?;
    }
    Ok(json!({ "change": null, "branch": null }))
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
    let document = folders(&team, &entries, &[])
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

/// Held while a rewrite checks that none is open and files its tasks, so that two presses file one.
static REWRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `files.rewrite {}`: one small, whole task for each role whose folder holds a Markdown document,
/// to rewrite them as HTML, and `document_rewrite.started` naming the tasks. The Scrum Master's
/// documents are rewritten at its next retro, so they file no task and only keep the rewrite
/// from being refused as nothing to do.
fn rewrite(deps: &ToolDeps) -> Result<Value, Failure> {
    // ponytail: one lock for the process, as a Catervafication's start has; the command line files none.
    let _filing = crate::locked(&REWRITING);
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let (team, _, entries) = listing(deps)?;
    if team.document_format() != DocumentFormat::Html {
        return Err(refusal(
            "not_html",
            "",
            "Turn on Write documents as HTML first.",
        ));
    }
    for id in rewrite_tasks(&deps.log).map_err(|e| internal(&e))? {
        let row = deps.projections.task(&id).map_err(|e| internal(&e))?;
        if row
            .is_some_and(|row| !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
        {
            return Err(refusal(
                "rewrite_open",
                "",
                "Your team is still rewriting its documents.",
            ));
        }
    }
    let mut delivery_holds_markdown = false;
    let mut wanted = Vec::new();
    for folder in folders(&team, &entries, &[]) {
        if folder.role == Role::ScrumMaster {
            // It takes no task: its retro rewrites `delivery/`, if it has an active agent.
            delivery_holds_markdown = folder
                .documents
                .iter()
                .flat_map(|document| std::iter::once(&document.path).chain(&document.twin))
                .any(|path| DocumentFormat::of(path) == Some(DocumentFormat::Markdown));
            continue;
        }
        let documents: Vec<String> = folder
            .documents
            .iter()
            .flat_map(|document| std::iter::once(&document.path).chain(&document.twin))
            .filter(|path| {
                DocumentFormat::of(path) == Some(DocumentFormat::Markdown)
                    && !is_marketing_plan(path)
            })
            .cloned()
            .collect();
        // A role the team cannot staff a reviewer for gets no task.
        if let Some(reviewer) =
            catervas_roles::default_reviewer_role(&team, TaskKind::Task, folder.role)
            && !documents.is_empty()
        {
            wanted.push((folder.path, folder.role, reviewer, documents));
        }
    }
    if wanted.is_empty() && !delivery_holds_markdown {
        return Err(refusal(
            "nothing_to_rewrite",
            "",
            "Every document your team can rewrite is already HTML.",
        ));
    }
    let mut tasks = Vec::new();
    for (folder, role, reviewer, documents) in wanted {
        let wire = rewrite_request(
            folder,
            &documents,
            role,
            reviewer,
            placeholder_budget_usd(&team.rules()),
        );
        let filed = file_small_request(
            &deps.files,
            &deps.log,
            wire,
            "human",
            TaskCreatedBodyFiledAs::DocumentRewrite,
            &format!("the owner's rewrite of {folder} as HTML"),
            (deps.clock.now(), &deps.ids),
        )
        .map_err(|e| internal(&e))?;
        tasks.push(filed.id.to_string());
    }
    let body = serde_json::from_value(json!({ "tasks": tasks })).map_err(|e| internal(&e))?;
    append(deps, EventBody::DocumentRewriteStarted(body))?;
    Ok(json!({ "tasks": tasks }))
}

/// The title of the folder change that takes the documents out of the repository, and the one
/// that puts them back (ADR 0074): the words their branches are named for.
const KEEP_TITLE: &str = "Keep documents on this computer";
const PUT_BACK_TITLE: &str = "Put documents back in the repository";

/// Where a scratch worktree for the switch is made: the first `documents-<n>` that is not there.
fn scratch_worktree(root: &std::path::Path) -> std::path::PathBuf {
    let folder = root.join(".catervas/local/worktrees");
    (1_u32..1000)
        .map(|n| folder.join(format!("documents-{n}")))
        .find(|path| !path.exists())
        .unwrap_or_else(|| folder.join("documents-0"))
}

/// The refusals of `documents.set_local`, in order, at `pointer`; `Ok(Some(mode))` when the team
/// already keeps its documents as asked, which changes nothing. The caller holds the integration
/// lock.
fn check_set_local(
    deps: &ToolDeps,
    team: &Team,
    local: bool,
    pointer: &str,
) -> Result<Option<catervas_core::folders::DocumentsMode>, Failure> {
    use catervas_core::folders::DocumentsMode;

    if super::team::members_present(deps)? {
        return Err(refusal(
            "project_folder",
            pointer,
            "Your documents already stay on this computer: a project folder is never pushed.",
        ));
    }
    let mode = documents_mode_of(team, &deps.log).map_err(|e| internal(&e))?;
    if matches!(mode, DocumentsMode::TakingOut | DocumentsMode::PuttingBack) {
        return Err(refusal(
            "documents_switching",
            pointer,
            "Your documents are being moved. Merge the pull request that moves them first.",
        ));
    }
    if (mode == DocumentsMode::Local) == local {
        return Ok(Some(mode));
    }
    let mut busy_with = Vec::new();
    for row in deps.projections.board().map_err(|e| internal(&e))? {
        let in_flight = match row.status {
            TaskStatus::Assigned
            | TaskStatus::InProgress
            | TaskStatus::Blocked
            | TaskStatus::Verifying
            | TaskStatus::Rejected
            | TaskStatus::Escalated => true,
            TaskStatus::Accepted => row.awaiting_integration,
            _ => false,
        };
        if !in_flight || row.kind != TaskKind::Task {
            continue;
        }
        let Ok(contract) = deps.files.read_contract(&row.task_id) else {
            continue;
        };
        if contract
            .allowed_paths
            .iter()
            .any(|path| catervas_core::governor::paths::reaches_the_folder(path, ROOT))
        {
            busy_with.push(row.title.clone());
        }
    }
    if !busy_with.is_empty() {
        return Err(refusal(
            "documents_in_flight",
            pointer,
            &format!(
                "Finish or merge the work that changes documents first: {}.",
                busy_with.join(", ")
            ),
        ));
    }
    let docs = folder_docs(&deps.log).map_err(|e| internal(&e))?;
    if docs
        .changes
        .iter()
        .any(|change| !change.integrated && !change.escalated)
    {
        return Err(refusal(
            "change_waiting",
            pointer,
            "A document change waits for you to merge it. Merge it first.",
        ));
    }
    let root = deps.files.root();
    if local {
        let uncommitted = deps
            .git
            .uncommitted_in(root)
            .map_err(|e| internal(&e))?
            .into_iter()
            .any(|entry| match entry {
                catervas_store::Uncommitted::Changed(path)
                | catervas_store::Uncommitted::Other(path) => path.starts_with("docs/catervas/"),
                catervas_store::Uncommitted::Untracked(_) => false,
            });
        if uncommitted {
            return Err(refusal(
                "documents_busy",
                pointer,
                "Some documents have changes on your computer that aren't committed. Commit or undo them first.",
            ));
        }
    }
    for link in ["docs", ROOT] {
        if std::fs::symlink_metadata(root.join(link))
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(refusal(
                "is_a_link",
                pointer,
                &format!("{link} is a link, which Catervas does not write through."),
            ));
        }
    }
    Ok(None)
}

/// Whether a team start that keeps its documents local may go on: `set_local(true)`'s refusals,
/// worked out before anything is written, at the field of the team they come from.
pub(super) fn check_before_start(deps: &ToolDeps) -> Result<(), Failure> {
    let _lock = integration_lock(deps.files.root()).map_err(|e| internal(&e))?;
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    check_set_local(deps, &team, true, "/team/policy/documents_local").map(|_| ())
}

/// Tells the channel, once, how many tasks that change documents wait for the pull request that
/// moves them: the ready ones, which rule 7 passes over until it is merged.
fn say_what_waits(deps: &ToolDeps) -> Result<(), Failure> {
    let mut waiting = 0_usize;
    for row in deps.projections.board().map_err(|e| internal(&e))? {
        if row.kind != TaskKind::Task || row.status != TaskStatus::Ready {
            continue;
        }
        let reaches = deps
            .files
            .read_contract(&row.task_id)
            .is_ok_and(|contract| {
                contract
                    .allowed_paths
                    .iter()
                    .any(|path| catervas_core::governor::paths::reaches_the_folder(path, ROOT))
            });
        waiting += usize::from(reaches);
    }
    if waiting > 0 {
        let noun = if waiting == 1 {
            "task waits"
        } else {
            "tasks wait"
        };
        crate::channel::post_system(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            None,
            &format!("{waiting} {noun} for the pull request that moves your documents."),
        )
        .map_err(|e| internal(&e))?;
    }
    Ok(())
}

/// `documents.set_local { local }`: the owner's choice of where the documents are kept, with the
/// folder change that moves them (ADR 0074). Under the integration lock.
pub(super) fn set_local(deps: &ToolDeps, local: bool) -> Result<Value, Failure> {
    use catervas_core::folders::DocumentsMode;
    use catervas_store::local_documents::{exclude_documents, include_documents};

    let _lock = integration_lock(deps.files.root()).map_err(|e| internal(&e))?;
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let mut team = deps.files.read_team().map_err(|e| internal(&e))?;
    if let Some(mode) = check_set_local(deps, &team, local, "/local")? {
        return Ok(json!({ "mode": mode.wire(), "change": null }));
    }
    let root = deps.files.root().to_path_buf();
    let into = integration_branch(&team, &deps.git).map_err(|e| internal(&e))?;
    let docs = folder_docs(&deps.log).map_err(|e| internal(&e))?;
    let (title, message) = if local {
        (
            KEEP_TITLE,
            "docs: keep the team's documents on this computer",
        )
    } else {
        (
            PUT_BACK_TITLE,
            "docs: put the team's documents back in the repository",
        )
    };
    if local {
        exclude_documents(&deps.git).map_err(|e| internal(&e))?;
    }
    let scratch = scratch_worktree(&root);
    let branch = crate::tools::folders::docs_branch(deps, title).map_err(|e| internal(&e))?;
    let commit = if local {
        deps.git
            .commit_untracking(&scratch, &branch, &into, ROOT, message)
    } else if catervas_store::local_documents::has_documents(&root).map_err(|e| internal(&e))? {
        deps.git
            .commit_folder_from(&scratch, &branch, &into, ROOT, &root.join(ROOT), message)
    } else {
        Ok(None)
    };
    let sha = match commit {
        Ok(sha) => sha,
        Err(error) => {
            if local {
                let _ = include_documents(&deps.git);
            }
            return Err(internal(&error));
        }
    };
    let change = sha
        .as_ref()
        .map(|_| crate::tools::folders::next_change(deps, &docs));
    let before = team.clone();
    team.policy.documents_local = local.then_some(true);
    deps.files.write_team(&team).map_err(|e| internal(&e))?;
    let body = serde_json::from_value(json!({
        "local": local,
        "change": change,
        "branch": sha.as_ref().map(|_| branch.clone()),
        "sha": sha,
    }))
    .map_err(|e| internal(&e))?;
    let recorded = append(deps, EventBody::DocumentStorageChanged(body))
        .and_then(|()| append(deps, super::team::team_updated(&team, None)));
    if let Err(failure) = recorded {
        let _ = deps.files.write_team(&before);
        if local {
            let _ = include_documents(&deps.git);
        }
        return Err(failure);
    }
    if change.is_none() && !local {
        include_documents(&deps.git).map_err(|e| internal(&e))?;
    }
    if change.is_some() {
        say_what_waits(deps)?;
    }
    let mode = match (change, local) {
        (Some(_), true) => DocumentsMode::TakingOut,
        (Some(_), false) => DocumentsMode::PuttingBack,
        (None, true) => DocumentsMode::Local,
        (None, false) => DocumentsMode::Committed,
    };
    Ok(json!({ "mode": mode.wire(), "change": change }))
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
    if method == "document.attach" {
        let attached = off_the_worker(move || attach(&deps, &params)).await?;
        state.wakes().notify_one();
        return Ok(attached);
    }
    if method == "documents.set_local" {
        let local = params["local"].as_bool().unwrap_or_default();
        let moved = off_the_worker(move || set_local(&deps, local)).await?;
        state.wakes().notify_one();
        return Ok(moved);
    }
    if method == "files.rewrite" {
        let filed = off_the_worker(move || rewrite(&deps)).await?;
        state.wakes().notify_one();
        return Ok(filed);
    }
    let saved = off_the_worker(move || save(&deps, &params)).await?;
    if !saved["change"].is_null() {
        state.wakes().notify_one();
    }
    Ok(saved)
}

/// One document of a folder, and its twin when it has one that is a file. `task` is the accepted
/// task whose branch alone holds it while it waits to merge.
struct Document {
    path: String,
    twin: Option<String>,
    task: Option<String>,
}

/// A task accepted and still waiting to merge, with its branch's entries under the root.
struct Pending {
    task: String,
    branch: String,
    entries: Vec<TreeEntry>,
}

/// The tasks accepted and still waiting to merge, in board order; a branch git cannot read is left
/// out.
fn pending(deps: &ToolDeps) -> Result<Vec<Pending>, Failure> {
    // With the documents kept on this computer nothing waits to merge: a task's documents land
    // when it is accepted.
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    if documents_on_disk(&team, &deps.log).map_err(|e| internal(&e))? {
        return Ok(Vec::new());
    }
    Ok(deps
        .projections
        .board()
        .map_err(|e| internal(&e))?
        .into_iter()
        .filter(|row| row.status == TaskStatus::Accepted && row.awaiting_integration)
        .filter_map(|row| {
            let contract = deps.files.read_contract(&row.task_id).ok()?;
            let branch = catervas_core::branch::task_branch(&contract);
            let entries = deps.git.tree_entries(&branch, ROOT).ok()?;
            Some(Pending {
                task: row.task_id.to_string(),
                branch,
                entries,
            })
        })
        .collect())
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
/// at any depth except the twin of a document that is present, which rides on its document. A
/// document only a `pending` task's branch holds is listed with that task; the integrated one wins.
fn folders(team: &Team, entries: &[TreeEntry], pending: &[Pending]) -> Vec<Folder> {
    let mut held: Vec<(&str, Option<&str>)> = entries
        .iter()
        .filter(|entry| is_file(entry))
        .map(|entry| (entry.path.as_str(), None))
        .collect();
    for waiting in pending {
        for entry in waiting.entries.iter().filter(|entry| is_file(entry)) {
            if !held.iter().any(|(path, _)| *path == entry.path) {
                held.push((entry.path.as_str(), Some(waiting.task.as_str())));
            }
        }
    }
    let files: Vec<&str> = held.iter().map(|(path, _)| *path).collect();
    let task_of = |path: &str| held.iter().find(|(p, _)| *p == path).and_then(|(_, t)| *t);
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
                    twin: agent_twin(path)
                        .filter(|twin| files.contains(&twin.as_str()))
                        .filter(|twin| task_of(twin) == task_of(path)),
                    task: task_of(path).map(ToString::to_string),
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

/// The team, where its documents are read from, and the `docs/catervas` entries there.
fn listing(deps: &ToolDeps) -> Result<(Team, DocumentSource, Vec<TreeEntry>), Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let mode = documents_mode_of(&team, &deps.log).map_err(|e| internal(&e))?;
    let source = team_documents(&team, &deps.git, mode).map_err(|e| internal(&e))?;
    let entries = source.entries(&deps.git, ROOT).map_err(|e| internal(&e))?;
    Ok((team, source, entries))
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
    let listed: Vec<Value> = folders(&team, &entries, &pending(deps)?)
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
                        "task": document.task,
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
            Some(Named::PullRequest(number)) => match named_by_pull_request(deps, number)? {
                Some(Named::Task(id)) => {
                    match deps.projections.task(&id).map_err(|e| internal(&e))? {
                        Some(row) => (Some(id.to_string()), row.assignee_id, false),
                        None => (None, None, false),
                    }
                }
                Some(Named::FolderChange(number)) => {
                    match docs.changes.iter().find(|change| change.change == number) {
                        Some(change) if change.edited => (None, None, true),
                        Some(change) => (None, writer_of(docs, change), false),
                        None => (None, None, false),
                    }
                }
                _ => (None, None, false),
            },
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

/// What pull request `number` was opened for in the project folder, newest first; `None` when the
/// log opened none with that number. A task's pull request counts only when its contract names no
/// `repo`: a member's pull request numbers are the member's, not the project folder's.
fn named_by_pull_request(deps: &ToolDeps, number: u64) -> Result<Option<Named>, Failure> {
    let opened = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::PullRequestOpened, EventKind::FolderChangeOpened],
            newest_first: true,
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    for event in opened {
        match &event.body {
            EventBody::PullRequestOpened(body) if body.number == number => {
                let Some(task) = event.envelope.ids.task_id.clone() else {
                    continue;
                };
                let in_a_member = deps
                    .files
                    .read_contract(&task)
                    .is_ok_and(|contract| contract.repo.is_some());
                return Ok((!in_a_member).then_some(Named::Task(task)));
            }
            EventBody::FolderChangeOpened(body) if body.number.get() == number => {
                return Ok(Some(Named::FolderChange(body.change.get())));
            }
            _ => {}
        }
    }
    Ok(None)
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

/// `path` as `source` holds it: its text, its blob and its last change. `waiting` is the owner's
/// own change that the branch is the branch of, whose commit is the last change, made by them. On
/// disk the last change is the newest write the log records, and none when it records none.
fn version(
    deps: &ToolDeps,
    docs: &FolderDocs,
    source: &DocumentSource,
    path: &str,
    waiting: bool,
) -> Result<Value, Failure> {
    let entry = source
        .entries(&deps.git, path)
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|entry| entry.path == path && is_file(entry))
        .ok_or_else(|| internal(&format!("there is no file {path}")))?;
    let text = source
        .text(&deps.git, path)
        .map_err(|e| internal(&e))?
        .ok_or_else(|| internal(&format!("there is no file {path}")))?;
    let last = match source {
        DocumentSource::Branch(rev) => deps
            .git
            .last_commit(rev, path, !waiting)
            .map_err(|e| internal(&e))?
            .map(|commit| last_change(deps, docs, &commit, waiting))
            .transpose()?,
        DocumentSource::Disk(_) => local_last_change(deps, path)?,
    };
    Ok(json!({ "path": path, "text": text, "blob": entry.blob, "last_change": last }))
}

/// Who wrote `path` last through Catervas, from the log of local writes: the owner, an agent, or a
/// task and its assignee; `None` for a document nobody has written since the switch. An edit made
/// in an editor is not seen (ADR 0074).
fn local_last_change(deps: &ToolDeps, path: &str) -> Result<Option<Value>, Failure> {
    let writes = local_writes(&deps.log).map_err(|e| internal(&e))?;
    let Some(write) = writes.get(path) else {
        return Ok(None);
    };
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let (agent_id, task_id) = match write.by {
        LocalWriter::Owner => (None, None),
        LocalWriter::Agent => (write.written_by.clone(), None),
        LocalWriter::Task => {
            let assignee = write
                .task_id
                .as_deref()
                .and_then(|id| id.parse::<catervas_core::contract::TaskId>().ok())
                .and_then(|id| deps.projections.task(&id).ok().flatten())
                .and_then(|row| row.assignee_id);
            (assignee, write.task_id.clone())
        }
    };
    let by_you = write.by == LocalWriter::Owner;
    let author = if by_you {
        "You".to_string()
    } else {
        agent_id
            .as_deref()
            .map_or_else(|| "Catervas".to_string(), |id| name_of(&team, id))
    };
    Ok(Some(json!({
        "at": write.at.to_rfc3339(),
        "task_id": task_id,
        "agent_id": agent_id,
        "author": author,
        "by_you": by_you,
    })))
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
        .map(|change| (change.change, change.branch.clone()))
        .filter(|(_, held)| deps.git.tree(held).is_ok())
}

/// `files.read`: one document of the tree with its twin.
fn read(deps: &ToolDeps, asked: &str) -> Result<Value, Failure> {
    let missing = || Failure::new(NOT_FOUND, format!("there is no document {asked}"));
    let path = normalise(asked).ok_or_else(missing)?;
    let (team, source, entries) = listing(deps)?;
    let mode = documents_mode_of(&team, &deps.log).map_err(|e| internal(&e))?;
    let pending = pending(deps)?;
    let document = folders(&team, &entries, &pending)
        .into_iter()
        .flat_map(|folder| folder.documents)
        .find(|document| document.path == path)
        .ok_or_else(missing)?;
    let docs = folder_docs(&deps.log).map_err(|e| internal(&e))?;
    // A document waiting to merge is read from its task's branch, and only read.
    if let Some(waiting) = pending
        .iter()
        .find(|waiting| document.task.as_deref() == Some(waiting.task.as_str()))
    {
        let branch = DocumentSource::Branch(waiting.branch.clone());
        // A commit whose subject names no task is still its task's work: the task's assignee is
        // who wrote it.
        let assignee = deps
            .projections
            .task(&waiting.task.parse().map_err(|_| missing())?)
            .map_err(|e| internal(&e))?
            .and_then(|row| row.assignee_id);
        let on_branch = |path: &str| {
            let mut found = version(deps, &docs, &branch, path, false)?;
            if let (Some(assignee), Some(last)) = (&assignee, found.get_mut("last_change"))
                && last["agent_id"].is_null()
                && last["by_you"] == false
            {
                last["agent_id"] = json!(assignee);
            }
            Ok::<_, Failure>(found)
        };
        return Ok(json!({
            "document": on_branch(&path)?,
            "twin": document.twin.as_deref().map(on_branch).transpose()?,
            "stale": false,
            "rederiving": false,
            "editable": false,
            "askable": false,
            "waiting": null,
            "attached": attached_from(deps, &path)?,
        }));
    }
    let waiting = if mode.reads_disk() {
        None
    } else {
        waiting_change(deps, &docs, &path)
    };
    let shown = match &waiting {
        Some((_, held)) => version(
            deps,
            &docs,
            &DocumentSource::Branch(held.clone()),
            &path,
            true,
        )?,
        None => version(deps, &docs, &source, &path, false)?,
    };
    let twin = document
        .twin
        .as_deref()
        .map(|twin| version(deps, &docs, &source, twin, false))
        .transpose()?;
    let stale = match &document.twin {
        Some(twin) => stale_twins_now(&deps.git, &deps.log, &team, mode).contains(twin),
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
    // While the documents move, none can be changed (ADR 0074).
    let editable = mode.writes_open() && check_owner_edit(&path, 0).is_ok();
    Ok(json!({
        "document": shown,
        "twin": twin,
        "stale": stale,
        "rederiving": rederiving,
        "editable": editable,
        "askable": editable && !path.starts_with("docs/catervas/delivery/"),
        "waiting": waiting.map(|(change, held)| json!({ "change": change, "branch": held })),
        "attached": attached_from(deps, &path)?,
    }))
}

const POLICY: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:; \
                      font-src data:; frame-ancestors 'self'";

/// What fits a document to the reader's screen; a document's own styles come after it and win.
const FIT: &str = concat!(
    r#"<meta name="viewport" content="width=device-width, initial-scale=1">"#,
    "<style>",
    "body { margin: 0 auto; padding: 16px; max-width: 72rem; overflow-wrap: anywhere }",
    "img, svg, video, canvas { max-width: 100%; height: auto }",
    "table, pre { display: block; max-width: 100%; overflow-x: auto }",
    "table { overflow-wrap: break-word }",
    "</style>"
);

/// `FIT`, then the app's page and ink (the brand's `--catervas-color-page` and `-ink`, light or
/// `dark`), which win over a document's own: a page colour of its own would not match the app's.
fn fit(dark: bool) -> String {
    let (page, ink) = if dark {
        ("#161616", "#F3E7D3")
    } else {
        ("#F3E7D3", "#161616")
    };
    format!(
        "{FIT}<style>html, body {{ background: {page} !important; color: {ink} !important }}</style>"
    )
}

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

/// `document` with `fit(dark)` after its `<head>`, else its `<html>`, else its doctype, else first.
/// ponytail: the tag is found by text, not by parsing; use a parser if a document defeats it.
fn fit_to_screen(document: &str, dark: bool) -> String {
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
    format!("{}{}{}", &document[..at], fit(dark), &document[at..])
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
    // With the documents kept on this computer the file is read from the folder, or from the task's
    // copy of it.
    if let Some(team) = deps.files.read_team().ok()
        && documents_on_disk(&team, &deps.log).ok()?
    {
        let base = match task {
            Some(task) => deps.files.root().join(".catervas/local/worktrees").join(
                task.parse::<catervas_core::contract::TaskId>()
                    .ok()?
                    .as_str(),
            ),
            None => deps.files.root().to_path_buf(),
        };
        return plain_file(&base, &path);
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
    // Else the first task waiting to merge that holds it, as the tree lists it.
    in_branch(&rev, &path).or_else(|| {
        pending(deps)
            .ok()?
            .iter()
            .find_map(|waiting| in_branch(&waiting.branch, &path))
    })
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
    let dark = named("theme").as_deref() == Some("dark");
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
            fit_to_screen(&text, dark),
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

    /// What a save of the owner's leaves, as `files.save` does it: folder change on its own branch
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
            &json!({ "path": path, "change": landed.change, "sha": landed.sha, "branch": landed.branch }),
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

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn names_who_made_a_change_merged_by_pull_request() {
        let harness = project("files-read-pr", "pull_request");
        put_documents(&harness);
        harness.project.moved(
            "CTV-1",
            "draft",
            "assigned",
            &json!({ "assignee": "pm", "reviewer": "dev-b" }),
        );
        // A task's pull request, merged on the forge with a merge commit.
        git(&harness, &["checkout", "-b", "feat/pie-pre-orders"]);
        write(&harness, ROADMAP, "a new roadmap\n");
        commit(&harness, "update the roadmap");
        git(&harness, &["checkout", "main"]);
        git(
            &harness,
            &[
                "merge",
                "--no-ff",
                "-m",
                "Merge pull request #4 from me/feat/pie-pre-orders",
                "feat/pie-pre-orders",
            ],
        );
        harness.project.record(
            "CTV-1",
            "pull_request.opened",
            &json!({
                "url": "https://github.com/pie/shop/pull/4", "number": 4,
                "branch": "feat/pie-pre-orders"
            }),
        );
        let last = read_path(&harness, ROADMAP)["document"]["last_change"].clone();
        assert_eq!(
            (&last["task_id"], &last["agent_id"], &last["by_you"]),
            (&json!("CTV-1"), &json!("pm"), &json!(false))
        );

        // A folder change's pull request, squashed on the forge.
        git(&harness, &["checkout", "-b", "docs/overview"]);
        write(&harness, OVERVIEW, "a rewritten overview\n");
        commit(&harness, "docs(architecture): overview.md by Ada");
        git(&harness, &["checkout", "main"]);
        git(&harness, &["merge", "--squash", "docs/overview"]);
        commit(&harness, "Docs: Update Overview (#9)");
        harness.project.record_in(
            Some("ada"),
            Some("session-1"),
            "",
            "folder_doc.written",
            &json!({
                "path": OVERVIEW, "written_by": "ada", "change": 2,
                "sha": git_output(&harness, &["rev-parse", "docs/overview"]),
                "branch": "docs/overview"
            }),
        );
        harness.project.record(
            "",
            "folder_change.opened",
            &json!({ "change": 2, "url": "https://github.com/pie/shop/pull/9", "number": 9 }),
        );
        let last = read_overview(&harness)["document"]["last_change"].clone();
        assert_eq!(
            (&last["task_id"], &last["agent_id"], &last["by_you"]),
            (&Value::Null, &json!("ada"), &json!(false))
        );
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
            json!({ "change": 1, "branch": "docs/roadmap" })
        );
        assert_eq!(waiting["document"]["text"], "the owner's roadmap\n");
        assert_eq!(
            waiting["document"]["blob"],
            git_output(&harness, &["rev-parse", &format!("docs/roadmap:{ROADMAP}")])
        );
        assert_eq!(waiting["document"]["last_change"]["by_you"], true);
        assert_eq!(waiting["stale"], false);

        // Merged by hand with a message that names the change, as an older history does; a pull
        // request's merge is read by its number (`names_who_made_a_change_merged_by_pull_request`).
        git(
            &harness,
            &[
                "merge",
                "--no-ff",
                "-m",
                "Merge docs/folder-1: roadmap.md",
                "docs/roadmap",
            ],
        );
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
    fn shows_a_document_whose_task_waits_to_merge() {
        let harness = project("files-pending", "pull_request");
        let spec = format!("{PRODUCT}/spec.html");
        harness.project.filed("CTV-2", "in_progress", "task", None);
        commit(&harness, "the task");
        let branch = harness.branch("CTV-2");
        git(&harness, &["checkout", "-b", &branch]);
        write(&harness, &spec, "<h1>the task's spec</h1>\n");
        commit(&harness, "the spec (CTV-2)");
        git(&harness, &["checkout", "main"]);
        harness
            .project
            .moved("CTV-2", "in_progress", "accepted", &json!({}));
        let listed = tree(&harness.daemon);
        assert_eq!(
            listed["folders"][0]["documents"],
            json!([{ "path": spec, "name": "Product description", "twin": null, "task": "CTV-2" }])
        );
        let opened = read_path(&harness, &spec);
        assert_eq!(opened["document"]["text"], "<h1>the task's spec</h1>\n");
        assert_eq!(opened["editable"], false);
        let cookie = browse(&harness);
        let (status, body) = tokio::runtime::Runtime::new()
            .expect("a runtime")
            .block_on(served(&harness, &cookie, &spec, ""));
        assert_eq!(status, 200);
        assert!(body.contains("the task's spec"), "{body}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn names_the_agent_of_a_pending_documents_task() {
        let harness = project("files-pending-agent", "pull_request");
        let spec = format!("{PRODUCT}/spec.html");
        harness.project.filed("CTV-2", "in_progress", "task", None);
        commit(&harness, "the task");
        let branch = harness.branch("CTV-2");
        git(&harness, &["checkout", "-b", &branch]);
        write(&harness, &spec, "<h1>the task's spec</h1>\n");
        // The commit's subject names no task.
        commit(&harness, "the spec");
        git(&harness, &["checkout", "main"]);
        harness.project.moved(
            "CTV-2",
            "in_progress",
            "accepted",
            &json!({ "assignee": "ada", "reviewer": "pm" }),
        );

        let opened = read_path(&harness, &spec);

        assert_eq!(
            opened["document"]["last_change"]["agent_id"], "ada",
            "{opened}"
        );
        assert_eq!(opened["document"]["last_change"]["by_you"], false);
    }

    #[test]
    fn keeps_table_words_whole() {
        let fitted = super::fit(false);
        assert!(
            fitted.contains("table { overflow-wrap: break-word }"),
            "{fitted}"
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
            json!({ "change": 1, "branch": "docs/roadmap" }),
            "{saved}"
        );
        assert_eq!(
            git_output(&harness, &["show", &format!("docs/roadmap:{path}")]),
            "<h1>new roadmap</h1>"
        );
        harness.merge_branch_by_hand("docs/roadmap");
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
            json!({ "change": 1, "branch": "docs/overview" })
        );
        assert_eq!(
            git_output(&harness, &["rev-list", "--count", "main..docs/overview"]),
            "1"
        );
        assert_eq!(
            git_output(&harness, &["log", "-1", "--format=%s", "docs/overview"]),
            "docs(architecture): edit overview.md"
        );
        assert_eq!(
            git_output(&harness, &["log", "-1", "--format=%an", "docs/overview"]),
            "Catervas Test"
        );
        assert_eq!(
            git_output(&harness, &["show", &format!("docs/overview:{OVERVIEW}")]),
            "the owner's overview"
        );
        let edited = harness.project.events(&[EventKind::FolderDocEdited]);
        assert_eq!(edited.len(), 1);
        assert_eq!(
            catervas_protocol::event::event_to_value(&edited[0])["body"],
            json!({
                "path": OVERVIEW,
                "change": 1,
                "sha": git_output(&harness, &["rev-parse", "docs/overview"]),
                "branch": "docs/overview"
            })
        );
        assert_eq!(edited[0].envelope.ids.agent_id, None);
        assert_eq!(edited[0].envelope.ids.task_id, None);
        assert_eq!(
            git_output(&harness, &["show", &format!("main:{OVERVIEW}")]),
            "the overview",
            "main holds the old text until the owner merges the branch"
        );

        harness.merge_branch_by_hand("docs/overview");
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
            assert_eq!(git_output(&harness, &["branch", "--list", "docs/*"]), "");
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
            account: Arc::new(crate::account::fixtures::FakeAccount::new()),
            kept_key_removed: false,
            port: PORT,
            clock: Arc::new(FixedClock::new(now)),
            take_on_error: std::sync::Mutex::default(),
            leaving: std::sync::Mutex::default(),
            converting: std::sync::Mutex::default(),
            env: std::collections::BTreeMap::new(),
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
        assert_eq!(body.matches(&super::fit(false)).count(), 1, "{body}");
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
            for dark in [false, true] {
                let block = super::fit(dark);
                let fitted = super::fit_to_screen(document, dark);
                assert_eq!(fitted.matches(&block).count(), 1, "{fitted}");
                assert_eq!(fitted.replacen(&block, "", 1), document);
                let at = fitted.find(&block).expect("the block");
                let wanted = document.find(after).map_or(0, |n| n + after.len());
                assert_eq!(at, wanted, "{fitted}");
            }
        }
        assert!(
            super::fit(false).contains(
                r#"<meta name="viewport" content="width=device-width, initial-scale=1">"#
            )
        );
        // The app's page and ink, light or dark, over whatever the document paints its page with.
        let light = super::fit(false);
        assert!(
            light.contains(
                "html, body { background: #F3E7D3 !important; color: #161616 !important }"
            ),
            "{light}"
        );
        let dark = super::fit(true);
        assert!(
            dark.contains(
                "html, body { background: #161616 !important; color: #F3E7D3 !important }"
            ),
            "{dark}"
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

    /// The reply to `files.rewrite`.
    fn rewrite(harness: &Harness) -> Value {
        rpc(&harness.daemon, "files.rewrite", &json!({}))
    }

    /// The ids `files.rewrite` answered.
    fn rewritten(reply: &Value) -> Vec<String> {
        conforms(&reply["result"], "filesRewriteResult", reply);
        reply["result"]["tasks"]
            .as_array()
            .unwrap_or_else(|| panic!("tasks: {reply}"))
            .iter()
            .map(|id| id.as_str().expect("an id").to_string())
            .collect()
    }

    /// `document_rewrite.started`'s tasks, one array for each event.
    fn rewrite_events(harness: &Harness) -> Vec<Value> {
        harness
            .project
            .events(&[EventKind::DocumentRewriteStarted])
            .iter()
            .map(|event| catervas_protocol::event::event_to_value(event)["body"]["tasks"].clone())
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_rewrite_task_for_each_folder_with_markdown() {
        let harness = project("files-rewrite", "manual");
        write(&harness, &format!("{PRODUCT}/spec.md"), "spec\n");
        write(
            &harness,
            &format!("{PRODUCT}/spec.agent.md"),
            "spec agents\n",
        );
        write(&harness, OVERVIEW, "the overview\n");
        write(
            &harness,
            "docs/catervas/marketing/plans/MP-1.md",
            "a marketing plan\n",
        );
        commit(&harness, "the documents");
        let tasks = rewritten(&rewrite(&harness));
        assert_eq!(tasks, ["CTV-1", "CTV-2"]);

        let product = contract_of(&harness, "CTV-1");
        assert_eq!(product["assignee_role"], "product_manager", "{product}");
        assert_eq!(product["allowed_paths"], json!([format!("{PRODUCT}/**")]));
        assert_eq!(product["created_by"], "human");
        assert_eq!(
            product["title"],
            "Rewrite the Product Manager's documents as HTML"
        );
        assert_eq!(
            product["intent"],
            "The owner asked for the team's documents as HTML. Rewrite each of these documents in \
             docs/catervas/product as <name>.html, saying everything it says, and delete the .md \
             it replaces, with its agent version, by naming each in `removed` when you commit with \
             catervas_git_commit: spec.md, spec.agent.md."
        );
        let architect = contract_of(&harness, "CTV-2");
        assert_eq!(architect["assignee_role"], "architect", "{architect}");
        assert_eq!(
            architect["allowed_paths"],
            json!(["docs/catervas/architecture/**"])
        );
        let artifact = |contract: &Value, path: &str| {
            contract["exit_criteria"]
                .as_array()
                .expect("criteria")
                .iter()
                .find(|criterion| criterion["verification"]["path"] == path)
                .map_or_else(
                    || panic!("no criterion for {path}: {contract}"),
                    |criterion| criterion["verification"].clone(),
                )
        };
        let spec = artifact(&product, &format!("{PRODUCT}/spec.html"));
        assert_eq!(spec["method"], "artifact");
        assert_eq!(spec["must_contain"], json!(["<h1"]));
        artifact(&product, &format!("{PRODUCT}/spec.agent.html"));
        let overview = artifact(&architect, "docs/catervas/architecture/overview.html");
        assert_eq!(overview["must_contain"], json!(["<h1", "<svg"]));

        let created: Vec<Value> = harness
            .project
            .events(&[EventKind::TaskCreated])
            .iter()
            .map(|event| {
                catervas_protocol::event::event_to_value(event)["body"]["filed_as"].clone()
            })
            .collect();
        assert_eq!(
            created,
            [json!("document_rewrite"), json!("document_rewrite")]
        );
        assert_eq!(rewrite_events(&harness), [json!(["CTV-1", "CTV-2"])]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_role_without_a_reviewer_gets_no_rewrite_task() {
        // No Architect and no Developer to review the Designer; the Scrum Master reviews the
        // Product Manager.
        let harness = Harness::new("files-rewrite-no-reviewer", |wire| {
            use catervas_core::team::fixtures::an_agent_wire;
            wire["agents"] = json!([
                an_agent_wire("pm", "product_manager"),
                an_agent_wire("sm", "scrum_master"),
                an_agent_wire("iris", "ui_ux_designer"),
            ]);
        });
        write(&harness, &format!("{PRODUCT}/spec.md"), "spec\n");
        write(&harness, "docs/catervas/design/screens.md", "screens\n");
        commit(&harness, "the documents");
        let tasks = rewritten(&rewrite(&harness));
        assert_eq!(tasks, ["CTV-1"]);
        assert_eq!(
            contract_of(&harness, "CTV-1")["assignee_role"],
            "product_manager"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_delivery_only_rewrite_records_no_tasks() {
        let harness = Harness::new("files-rewrite-delivery", |wire| {
            with_the_designer(wire);
            wire["agents"].as_array_mut().expect("agents").push(
                catervas_core::team::fixtures::an_agent_wire("sm", "scrum_master"),
            );
        });
        write(&harness, "docs/catervas/delivery/cadence.md", "cadence\n");
        commit(&harness, "the cadence");
        let reply = rewrite(&harness);
        assert_eq!(rewritten(&reply), Vec::<String>::new(), "{reply}");
        assert_eq!(rewrite_events(&harness), [json!([])]);
        assert!(
            harness
                .project
                .events(&[EventKind::TaskCreated, EventKind::RequestTriaged])
                .is_empty()
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_rewrite_it_cannot_start() {
        let refused = |reply: &Value, code: &str, words: &str| {
            assert_eq!(refusal(reply), (code.to_string(), String::new()), "{reply}");
            assert_eq!(said(reply), words);
        };
        // A Markdown team.
        let markdown = Harness::new("files-rewrite-markdown", |wire| {
            with_the_designer(wire);
            wire["policy"]["document_format"] = json!("markdown");
        });
        write(&markdown, &format!("{PRODUCT}/spec.md"), "spec\n");
        commit(&markdown, "the spec");
        refused(
            &rewrite(&markdown),
            "not_html",
            "Turn on Write documents as HTML first.",
        );

        // A Markdown team holding only HTML is still refused for its format first.
        let only_html = Harness::new("files-rewrite-markdown-html", |wire| {
            with_the_designer(wire);
            wire["policy"]["document_format"] = json!("markdown");
        });
        write(&only_html, &format!("{PRODUCT}/spec.html"), "<h1>s</h1>\n");
        commit(&only_html, "the spec");
        refused(
            &rewrite(&only_html),
            "not_html",
            "Turn on Write documents as HTML first.",
        );

        // Delivery's Markdown is no one's to rewrite without a Scrum Master.
        let lone = Harness::new("files-rewrite-no-sm", with_the_designer);
        write(&lone, "docs/catervas/delivery/cadence.md", "cadence\n");
        commit(&lone, "the cadence");
        refused(
            &rewrite(&lone),
            "nothing_to_rewrite",
            "Every document your team can rewrite is already HTML.",
        );

        // `plans/MP-notes.md` is not a marketing plan, so it is rewritten.
        let notes = Harness::new("files-rewrite-notes", with_the_marketing_specialist);
        write(
            &notes,
            "docs/catervas/marketing/plans/MP-notes.md",
            "notes\n",
        );
        commit(&notes, "the notes");
        assert_eq!(rewritten(&rewrite(&notes)), ["CTV-1"]);

        // Nothing but a plan, and no delivery `.md`.
        let plans = Harness::new("files-rewrite-nothing", with_the_marketing_specialist);
        write(
            &plans,
            "docs/catervas/marketing/plans/MP-1.md",
            "a marketing plan\n",
        );
        write(
            &plans,
            "docs/catervas/delivery/cadence.html",
            "<h1>c</h1>\n",
        );
        commit(&plans, "the plan");
        refused(
            &rewrite(&plans),
            "nothing_to_rewrite",
            "Every document your team can rewrite is already HTML.",
        );
        assert!(rewrite_events(&plans).is_empty());

        // A rewrite whose tasks are still on the board.
        let harness = project("files-rewrite-open", "manual");
        write(&harness, &format!("{PRODUCT}/spec.md"), "spec\n");
        write(&harness, OVERVIEW, "the overview\n");
        commit(&harness, "the documents");
        assert_eq!(rewritten(&rewrite(&harness)), ["CTV-1", "CTV-2"]);
        let open = "Your team is still rewriting its documents.";
        refused(&rewrite(&harness), "rewrite_open", open);
        harness
            .project
            .moved("CTV-1", "ready", "accepted", &json!({}));
        refused(&rewrite(&harness), "rewrite_open", open);
        harness
            .project
            .moved("CTV-2", "ready", "cancelled", &json!({}));
        assert_eq!(rewritten(&rewrite(&harness)), ["CTV-3", "CTV-4"]);
        assert_eq!(rewrite_events(&harness).len(), 2);
        // The newest rewrite is the one that counts.
        refused(&rewrite(&harness), "rewrite_open", open);
        // And an open rewrite is refused as open, though no Markdown is left.
        git(
            &harness,
            &["rm", "-q", &format!("{PRODUCT}/spec.md"), OVERVIEW],
        );
        commit(&harness, "the rewrite lands");
        refused(&rewrite(&harness), "rewrite_open", open);
    }

    /// The reply to `document.attach`.
    fn attach(harness: &Harness, name: &str, kind: &str, bytes: &[u8]) -> Value {
        use base64::Engine as _;
        rpc(
            &harness.daemon,
            "document.attach",
            &json!({
                "name": name,
                "kind": kind,
                "data_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
        )
    }

    const TRIP_SPEC: &str = "docs/catervas/product/attached/trip-spec.md";
    const TRIP_SPEC_TWIN: &str = "docs/catervas/product/attached/trip-spec.agent.md";

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn attaches_a_file_as_the_owners_change_and_its_agent_writes_the_twin() {
        let harness = project("files-attach", "manual");
        put_documents(&harness);
        let reply = attach(
            &harness,
            "Trip spec.md",
            "product",
            b"# Trips\n\nPlan a trip.\n",
        );
        conforms(&reply["result"], "documentAttachResult", &reply);
        assert_eq!(
            reply["result"],
            json!({ "path": TRIP_SPEC, "change": 1 }),
            "{reply}"
        );
        let branch = git_output(
            &harness,
            &["branch", "--list", "docs/*", "--format=%(refname:short)"],
        );
        assert_eq!(
            git_output(&harness, &["show", &format!("{branch}:{TRIP_SPEC}")]),
            "# Trips\n\nPlan a trip."
        );
        assert_eq!(
            git_output(&harness, &["log", "-1", "--format=%s", &branch]),
            "docs(product): attach trip-spec.md"
        );
        let kinds = harness
            .project
            .events(&[EventKind::FolderDocEdited, EventKind::DocumentAttached]);
        let wire: Vec<Value> = kinds
            .iter()
            .map(|event| {
                let mut value = catervas_protocol::event::event_to_value(event);
                value["body"].as_object_mut().map(|body| body.remove("sha"));
                json!({ "kind": value["kind"], "body": value["body"] })
            })
            .collect();
        assert_eq!(
            wire,
            [
                json!({ "kind": "folder_doc.edited", "body": {
                    "path": TRIP_SPEC, "change": 1, "branch": branch
                } }),
                json!({ "kind": "document.attached", "body": {
                    "path": TRIP_SPEC, "name": "Trip spec.md", "media_type": "text/markdown"
                } }),
            ]
        );
        assert_eq!(kinds[1].envelope.ids.agent_id, None);
        assert_eq!(kinds[1].envelope.ids.session_id, None);

        // Integrated, the change files the task that writes the twin.
        harness.merge_branch_by_hand(&branch);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(orchestrator.handle(Command::FolderChangeIntegrate { change: 1 }))
            .expect("the owner adds it");
        let rederives = catervas_store::requests::rederive_tasks(&harness.project.deps.log)
            .expect("the log reads");
        assert_eq!(
            rederives
                .iter()
                .map(|(_, twin)| twin.as_str())
                .collect::<Vec<_>>(),
            [TRIP_SPEC_TWIN]
        );
        let read = read_path(&harness, TRIP_SPEC);
        assert_eq!(read["attached"], json!({ "name": "Trip spec.md" }));
        assert_eq!(read_path(&harness, ROADMAP)["attached"], Value::Null);
    }

    /// The reply to `documents.set_local`.
    fn set_local(harness: &Harness, local: bool) -> Value {
        rpc(
            &harness.daemon,
            "documents.set_local",
            &json!({ "local": local }),
        )
    }

    fn documents_mode(harness: &Harness) -> Value {
        query(&harness.daemon, "team.get", &json!({}), "teamGetResult")["documents_mode"].clone()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_attachment_with_a_code_at_its_field() {
        // The team of three has no Architect.
        let harness = Harness::new("files-attach-refused", |_| {});
        let scan = include_bytes!("../../tests/fixtures/attach/scan.pdf");
        let refused = |name: &str, kind: &str, bytes: &[u8], code: &str, at: &str| {
            let reply = attach(&harness, name, kind, bytes);
            assert_eq!(
                refusal(&reply),
                (code.to_string(), at.to_string()),
                "{reply}"
            );
            assert!(!said(&reply).is_empty(), "{reply}");
            assert!(
                harness
                    .project
                    .events(&[EventKind::FolderDocEdited, EventKind::DocumentAttached])
                    .is_empty()
            );
            assert_eq!(git_output(&harness, &["branch", "--list", "docs/*"]), "");
        };
        refused(
            "api.md",
            "architecture",
            b"# API\n",
            "no_folder_owner",
            "/kind",
        );
        refused("scan.pdf", "product", scan, "no_text", "/data_base64");
    }

    /// The body of every event of `kind`, as the wire shows it.
    fn bodies(harness: &Harness, kind: EventKind) -> Vec<Value> {
        harness
            .project
            .events(&[kind])
            .iter()
            .map(|event| catervas_protocol::event::event_to_value(event)["body"].clone())
            .collect()
    }

    /// What `harness`'s orchestrator answers for `command`.
    fn commanded(
        harness: &Harness,
        command: Command,
    ) -> Result<crate::orchestrator::CommandReport, crate::orchestrator::CommandError> {
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(orchestrator.handle(command))
    }

    fn chat(text: &str, as_task: bool, attachments: &[&str]) -> Command {
        Command::ChatMessagePost {
            agent_id: "dev-a".to_string(),
            text: text.to_string(),
            as_task,
            attachments: attachments.iter().map(ToString::to_string).collect(),
        }
    }

    fn post(text: &str, attachments: &[&str]) -> Command {
        Command::MessagePost {
            text: text.to_string(),
            attachments: attachments.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_request_a_chat_message_and_a_post_record_their_attachments() {
        let harness = project("files-attachments", "manual");
        put_documents(&harness);
        attach(
            &harness,
            "Trip spec.md",
            "product",
            b"# Trips\n\nPlan a trip.\n",
        );
        let named = json!([TRIP_SPEC]);
        let before = |kind| bodies(&harness, kind).len();
        let (tasks, chats, posts) = (
            before(EventKind::TaskCreated),
            before(EventKind::ChatMessagePosted),
            before(EventKind::MessagePosted),
        );

        let filed = rpc(
            &harness.daemon,
            "request.file",
            &json!({
                "text": "Let people share a trip with their friends by a link.",
                "attachments": [TRIP_SPEC],
            }),
        );
        conforms(&filed["result"], "requestFileResult", &filed);
        commanded(
            &harness,
            chat("Please read the trip spec.", false, &[TRIP_SPEC]),
        )
        .expect("the chat message is sent");
        commanded(
            &harness,
            chat("Build what the trip spec describes.", true, &[TRIP_SPEC]),
        )
        .expect("the task is given");
        commanded(
            &harness,
            post("@dev-a the trip spec is attached", &[TRIP_SPEC]),
        )
        .expect("the post is made");
        commanded(&harness, post("No documents this time", &[])).expect("the post is made");

        let created = bodies(&harness, EventKind::TaskCreated).split_off(tasks);
        assert_eq!(created.len(), 2, "{created:?}");
        assert!(created.iter().all(|body| body["attachments"] == named));
        let chatted = bodies(&harness, EventKind::ChatMessagePosted).split_off(chats);
        assert_eq!(chatted.len(), 2, "{chatted:?}");
        assert!(chatted.iter().all(|body| body["attachments"] == named));
        let posted = bodies(&harness, EventKind::MessagePosted).split_off(posts);
        assert_eq!(posted.len(), 2, "{posted:?}");
        assert_eq!(posted[0]["attachments"], named);
        assert!(posted[1].get("attachments").is_none(), "{}", posted[1]);

        // A solo team's request is its one agent's chat message and task (ADR 0071).
        let solo = Harness::new("files-attachments-solo", |wire| {
            wire["agents"] = json!([catervas_core::team::fixtures::an_agent_wire(
                "pm",
                "product_manager"
            )]);
            wire["policy"]["solo"] = json!(true);
        });
        attach(
            &solo,
            "Trip spec.md",
            "product",
            b"# Trips\n\nPlan a trip.\n",
        );
        let filed = rpc(
            &solo.daemon,
            "request.file",
            &json!({
                "text": "Let people share a trip with their friends by a link.",
                "attachments": [TRIP_SPEC],
            }),
        );
        conforms(&filed["result"], "requestFileResult", &filed);
        for kind in [EventKind::ChatMessagePosted, EventKind::TaskCreated] {
            let seen = bodies(&solo, kind);
            assert_eq!(seen.len(), 1, "{seen:?}");
            assert_eq!(seen[0]["attachments"], named, "{kind:?}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeping_documents_local_moves_them_by_one_change_each_way() {
        let harness = project("documents-local", "pull_request");
        let spec = "docs/catervas/product/spec.md";
        write(&harness, spec, "the spec\n");
        commit(&harness, "the spec");
        let root = harness.project.repo.path.clone();

        // A task that changes documents holds the switch back, by its title.
        harness.assigned_with("CTV-7", "dev-a", "dev-b", |wire| {
            wire["allowed_paths"] = json!(["docs/catervas/product/**"]);
        });
        let blocked = set_local(&harness, true);
        assert_eq!(
            refusal(&blocked),
            ("documents_in_flight".to_string(), "/local".to_string()),
            "{blocked}"
        );
        assert!(
            said(&blocked).starts_with("Finish or merge the work that changes documents first: ")
        );
        harness.project.moved(
            "CTV-7",
            "assigned",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );

        // Turning it on takes the folder out in one change, and nothing is written meanwhile.
        let on = set_local(&harness, true);
        conforms(&on["result"], "documentsSetLocalResult", &on);
        assert_eq!(on["result"]["mode"], "taking_out", "{on}");
        assert_eq!(on["result"]["change"], 1, "{on}");
        assert_eq!(documents_mode(&harness), "taking_out");
        let branch = "docs/keep-documents-on-this-computer";
        assert_eq!(
            git_output(
                &harness,
                &["ls-tree", "-r", "--name-only", branch, "--", "docs"]
            ),
            ""
        );
        let switching = save(&harness, spec, "changed\n", &blob(&harness, spec));
        assert_eq!(
            refusal(&switching),
            ("documents_switching".to_string(), "/path".to_string()),
            "{switching}"
        );
        let attaching = attach(&harness, "Trip spec.md", "product", b"# Trip\n");
        assert_eq!(
            refusal(&attaching),
            ("documents_switching".to_string(), "/name".to_string()),
            "{attaching}"
        );
        // It waits on Today, and a task that changes documents is not started meanwhile.
        let listed = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        let row = listed["waiting"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["kind"] == "integration"))
            .expect("the switch waits on Today");
        assert_eq!(row["title"], "Keep documents on this computer");
        assert_eq!(row["folder_change"], 1);
        harness.assigned_with("CTV-8", "dev-a", "dev-b", |wire| {
            wire["allowed_paths"] = json!(["docs/catervas/product/**"]);
        });
        ticks(&harness);
        assert_eq!(
            harness.row("CTV-8").status,
            catervas_core::contract::TaskStatus::Assigned,
            "rule 7 passes over it while the documents move"
        );
        let again = set_local(&harness, false);
        assert_eq!(
            refusal(&again),
            ("documents_switching".to_string(), "/local".to_string()),
            "{again}"
        );

        // The owner merges it; one tick records it and the file is back, ignored and untracked.
        harness.merge_branch_by_hand(branch);
        assert!(!root.join(spec).exists(), "git removed it with the merge");
        ticks(&harness);
        assert_eq!(
            harness
                .project
                .events(&[EventKind::FolderChangeIntegrated])
                .len(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(root.join(spec)).expect("the file is back"),
            "the spec\n"
        );
        assert_eq!(
            git_output(&harness, &["status", "--porcelain", "--", "docs"]),
            ""
        );
        assert_eq!(git_output(&harness, &["ls-files", "docs/catervas"]), "");
        assert_eq!(documents_mode(&harness), "local");
        assert_eq!(
            set_local(&harness, true)["result"],
            json!({ "mode": "local", "change": null })
        );

        harness.project.moved(
            "CTV-8",
            "assigned",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
        // Turning it off puts what is on disk back in one change.
        let off = set_local(&harness, false);
        assert_eq!(off["result"]["mode"], "putting_back", "{off}");
        assert_eq!(off["result"]["change"], 2, "{off}");
        harness.merge_branch_by_hand("docs/put-documents-back-in-the-repository");
        ticks(&harness);
        assert!(git_output(&harness, &["ls-files", "docs/catervas"]).contains(spec));
        assert_eq!(documents_mode(&harness), "committed");
        let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap_or_default();
        assert!(!exclude.contains("/docs/catervas"), "{exclude}");
        assert_eq!(
            harness
                .project
                .events(&[EventKind::DocumentStorageChanged])
                .len(),
            2
        );
    }

    /// A project whose team keeps its documents on this computer, with the Product Manager's spec
    /// and its agent version on disk, ignored.
    fn local_project(name: &str) -> Harness {
        let harness = Harness::new(name, |wire| {
            with_the_designer(wire);
            wire["policy"]["integration"] = json!("pull_request");
            wire["policy"]["documents_local"] = json!(true);
        });
        catervas_store::local_documents::exclude_documents(&harness.project.deps.git)
            .expect("ignored");
        write(
            &harness,
            "docs/catervas/product/spec.html",
            "spec for people\n",
        );
        write(
            &harness,
            "docs/catervas/product/spec.agent.html",
            "spec for agents\n",
        );
        harness
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_attachments_never_attached() {
        use crate::orchestrator::CommandError;

        let harness = project("files-attachments-refused", "manual");
        put_documents(&harness);
        attach(
            &harness,
            "Trip spec.md",
            "product",
            b"# Trips\n\nPlan a trip.\n",
        );
        let text = "Let people share a trip with their friends by a link.";
        let six = [TRIP_SPEC; 6];
        let never = [
            "docs/catervas/product/spec.md",
            "docs/catervas/product/attached/never.md",
        ];
        let kinds = [
            EventKind::TaskCreated,
            EventKind::ChatMessagePosted,
            EventKind::MessagePosted,
        ];
        let held: Vec<usize> = kinds
            .iter()
            .map(|kind| harness.project.events(&[*kind]).len())
            .collect();

        for path in never {
            let reply = rpc(
                &harness.daemon,
                "request.file",
                &json!({ "text": text, "attachments": [TRIP_SPEC, path] }),
            );
            assert_eq!(
                refusal(&reply),
                ("not_attached".to_string(), "/attachments/1".to_string()),
                "{reply}"
            );
            assert!(!said(&reply).is_empty(), "{reply}");
        }
        let reply = rpc(
            &harness.daemon,
            "request.file",
            &json!({ "text": text, "attachments": six }),
        );
        assert_eq!(
            refusal(&reply),
            (
                "too_many_attachments".to_string(),
                "/attachments".to_string()
            ),
            "{reply}"
        );
        assert!(!said(&reply).is_empty(), "{reply}");

        // A command's refusal is `<code>: <sentence>`, and the code is the same.
        let refused = |command: Command, code: &str| match commanded(&harness, command) {
            Err(CommandError::Refused { reason }) => {
                assert!(reason.starts_with(&format!("{code}: ")), "{reason}");
            }
            other => panic!("refused {code}, not {other:?}"),
        };
        for path in never {
            refused(chat("Please read it.", false, &[path]), "not_attached");
            refused(
                chat("Build what it describes.", true, &[path]),
                "not_attached",
            );
            refused(post("Please read it", &[path]), "not_attached");
        }
        refused(chat("Please read it.", false, &six), "too_many_attachments");
        refused(post("Please read it", &six), "too_many_attachments");
        for (kind, before) in kinds.iter().zip(held) {
            assert_eq!(harness.project.events(&[*kind]).len(), before, "{kind:?}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_files_page_reads_and_writes_the_folder_on_disk() {
        let harness = local_project("files-local");
        let spec = "docs/catervas/product/spec.html";
        let twin = "docs/catervas/product/spec.agent.html";
        let root = harness.project.repo.path.clone();
        let listed = tree(&harness.daemon);
        assert_eq!(
            listed["folders"][0]["documents"][0]["path"], spec,
            "{listed}"
        );
        assert_eq!(listed["folders"][0]["documents"][0]["twin"], twin);
        let before = read(&harness.daemon, spec);
        assert_eq!(before["stale"], false);
        assert_eq!(before["document"]["last_change"], Value::Null);

        // A save writes the file at once: no branch, no pull request, and the twin falls behind.
        let old = git_output(&harness, &["hash-object", spec]);
        assert_eq!(before["document"]["blob"], old);
        let saved = save(&harness, spec, "the owner's spec\n", &old);
        conforms(&saved["result"], "filesSaveResult", &saved);
        assert_eq!(saved["result"], json!({ "change": null, "branch": null }));
        assert_eq!(
            std::fs::read_to_string(root.join(spec)).expect("written"),
            "the owner's spec\n"
        );
        assert_eq!(git_output(&harness, &["branch", "--list", "docs/*"]), "");
        assert!(
            harness
                .project
                .events(&[EventKind::FolderDocEdited])
                .is_empty()
        );
        let written = harness.project.events(&[EventKind::LocalDocumentsWritten]);
        assert_eq!(written.len(), 1);
        let after = read(&harness.daemon, spec);
        assert_eq!(after["document"]["last_change"]["by_you"], true, "{after}");
        assert_eq!(after["stale"], true);
        assert_eq!(after["rederiving"], true);
        assert_eq!(after["waiting"], Value::Null);
        assert_eq!(after["twin"]["text"], "spec for agents\n");
        assert_eq!(
            super::rederive_tasks(&harness.project.deps.log)
                .expect("reads")
                .len(),
            1
        );
        // The old blob is a document that changed.
        let stale = save(&harness, spec, "another\n", &old);
        assert_eq!(
            refusal(&stale),
            ("file_changed".to_string(), "/base".to_string()),
            "{stale}"
        );
        // The same text makes nothing.
        let same = save(
            &harness,
            spec,
            "the owner's spec\n",
            &git_output(&harness, &["hash-object", spec]),
        );
        assert_eq!(same["result"], json!({ "change": null, "branch": null }));
        assert_eq!(
            harness
                .project
                .events(&[EventKind::LocalDocumentsWritten])
                .len(),
            1
        );

        // An agent's document at a retro is written on disk too.
        let mut context = harness.project.context("ada", None);
        context.purpose = crate::session::SessionPurpose::Ceremony;
        context.thread = Some(catervas_protocol::event::Thread::Retro);
        let cadence = "docs/catervas/architecture/cadence.html";
        let answer = crate::tools::fixtures::run(
            &context,
            "catervas_write_folder_doc",
            json!({ "path": cadence, "text": "the cadence\n" }),
        )
        .expect("written");
        assert_eq!(answer["change"], Value::Null, "{answer}");
        assert_eq!(
            std::fs::read_to_string(root.join(cadence)).expect("on disk"),
            "the cadence\n"
        );
        assert_eq!(git_output(&harness, &["branch", "--list", "docs/*"]), "");
        assert!(
            harness
                .project
                .events(&[EventKind::FolderDocWritten])
                .is_empty()
        );
        let written = harness.project.events(&[EventKind::LocalDocumentsWritten]);
        assert_eq!(
            catervas_protocol::event::event_to_value(&written[1])["body"]["written_by"],
            "ada"
        );
        let listed = read(&harness.daemon, cadence);
        assert_eq!(listed["document"]["last_change"]["agent_id"], "ada");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_attachment_is_written_on_disk_when_the_documents_are_local() {
        let harness = local_project("files-attach-local");
        let root = harness.project.repo.path.clone();
        let attached = attach(
            &harness,
            "Trip spec.md",
            "product",
            b"# Trip\n\nThe spec.\n",
        );
        conforms(&attached["result"], "documentAttachResult", &attached);
        assert_eq!(
            attached["result"],
            json!({ "path": TRIP_SPEC, "change": null }),
            "{attached}"
        );
        assert!(
            std::fs::read_to_string(root.join(TRIP_SPEC))
                .expect("on disk")
                .contains("The spec.")
        );
        assert_eq!(git_output(&harness, &["branch", "--list", "docs/*"]), "");
        assert!(
            harness
                .project
                .events(&[EventKind::FolderDocEdited])
                .is_empty()
        );
        assert_eq!(
            harness
                .project
                .events(&[EventKind::LocalDocumentsWritten])
                .len(),
            1
        );
        assert_eq!(
            harness.project.events(&[EventKind::DocumentAttached]).len(),
            1
        );
        // The agent version's task is filed at once, and a second file of the name takes the next.
        assert_eq!(
            super::rederive_tasks(&harness.project.deps.log)
                .expect("reads")
                .len(),
            1
        );
        let again = attach(&harness, "Trip spec.md", "product", b"# Trip again\n");
        assert_eq!(
            again["result"]["path"], "docs/catervas/product/attached/trip-spec-2.md",
            "{again}"
        );
        assert_eq!(
            read_path(&harness, TRIP_SPEC)["attached"],
            json!({ "name": "Trip spec.md" })
        );
    }
}
