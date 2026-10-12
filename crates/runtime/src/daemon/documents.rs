//! The Files page's server (`docs/SPEC.md` 5.17): the team's folders and their documents read from
//! the integration branch, one document with its twin, who last changed each, and whether the twin
//! is stale. `web.rs` answers the frames; this module answers what they ask.

use std::fmt::Display;

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

use catervas_protocol::event::EventBody;

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
pub(super) const METHODS: [&str; 1] = ["files.save"];

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

/// The methods, whose params the schema already passed.
pub(super) async fn call(
    state: &DaemonState,
    _method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(NO_PROJECT, super::NO_PROJECT));
    };
    let params = params.clone();
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
    let waiting = docs
        .changes
        .iter()
        .rev()
        .find(|change| {
            change.edited && !change.integrated && !change.escalated && change.paths.contains(&path)
        })
        .map(|change| (change.change, format!("docs/folder-{}", change.change)))
        .filter(|(_, held)| deps.git.tree(held).is_ok());
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
        refused(
            "docs/catervas/product/notes.txt",
            "x\n",
            &gone,
            "not_a_document",
            "/path",
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
}
