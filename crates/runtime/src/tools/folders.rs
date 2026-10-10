//! `catervas_write_folder_doc` (`docs/SPEC.md` 5.17) and the folder change it makes (5.14): at a
//! sprint ceremony an agent keeps a small recurring document of its own folder under
//! `docs/catervas/`. A document for agents is committed by Catervas on a branch of its own, and the
//! team's integration policy adds it to the project as it does an accepted task's branch; a
//! document the owner approves is proposed at the sprint review and waits for the owner.

use catervas_core::folders::{
    FolderDocPath, FolderDocPathRefusal, agent_twin, check_folder_doc_path, folder_doc_author,
    folder_doc_message, role_folder,
};
use catervas_core::governor::paths::check_protected_paths;
use catervas_core::team::{Integration, Team};
use catervas_protocol::event::{EventBody, Thread};
use catervas_store::folder_docs::folder_docs;
use catervas_store::{CommitOutcome, Git};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolDeps, ToolError, failed};
use crate::ceremonies::ended_sprint;
use crate::session::SessionPurpose;
use crate::transitions::{integration_branch, integration_lock};

/// `catervas_write_folder_doc`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteFolderDocInput {
    /// The document: a `.md` file under your folder, `docs/catervas/<folder>/...`.
    pub path: String,
    /// The document, in full: at most 262,144 bytes. For the product's `spec.md` and
    /// `roadmap.md`, the version for people.
    pub text: String,
    /// Only for `spec.md` and `roadmap.md`: their `.agent.md` version, in full.
    pub agent_text: Option<String>,
    /// Only for `spec.md` and `roadmap.md`: what changed and why, in 1 to 300 characters, for the
    /// owner.
    pub summary: Option<String>,
}

/// The most a document says, in bytes: the Files page's cap.
const DOC_BYTES: usize = 262_144;
/// The most a proposal's summary says, in characters.
const SUMMARY_CHARS: usize = 300;

/// What the tool answers to a proposal.
const PROPOSED: &str =
    "The owner approves it or sends it back on Today; you are told at your next sprint review.";

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

/// What the team's policy does with a folder change, in the sentence the tool answers with.
fn next_by(policy: Integration, branch: &str, into: &str) -> String {
    match policy {
        Integration::AutoMerge => format!("Catervas merges {branch} into {into} on its next tick."),
        Integration::PullRequest => format!(
            "Catervas opens a pull request for {branch} on its next tick; the document is in {into} once it merges."
        ),
        Integration::Manual => format!("{branch} waits for the owner to add it."),
    }
}

/// Makes `files` (`(path, text)`) a folder change: the first number above the log's highest for
/// which git has no branch `docs/folder-<n>`, here or on `origin`, and that branch from the
/// integration branch with one commit by `author`. The caller holds the integration lock.
///
/// # Errors
///
/// `Waiting` for a file an earlier change holds that is neither integrated nor escalated, `Link`,
/// `Busy`, and `Git` when git or the log failed. `Ok(None)` when every file already says its text on
/// the integration branch, which makes nothing.
pub(crate) fn land(
    deps: &ToolDeps,
    team: &Team,
    files: &[(String, String)],
    message: &str,
    author: &str,
) -> Result<Option<Landed>, LandRefusal> {
    let words = |error: &dyn std::fmt::Display| LandRefusal::Git(error.to_string());
    let into = integration_branch(team, &deps.git).map_err(|error| words(&error))?;
    let docs = folder_docs(&deps.log).map_err(|error| words(&error))?;
    for (path, _) in files {
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
    let mut change = docs
        .changes
        .iter()
        .map(|held| held.change)
        .max()
        .unwrap_or(0)
        + 1;
    let branch = loop {
        let branch = format!("docs/folder-{change}");
        if deps.git.tree(&branch).is_err() && deps.git.tree(&format!("origin/{branch}")).is_err() {
            break branch;
        }
        change += 1;
    };
    match deps
        .git
        .commit_files(&branch, &into, files, message, Some(author))
        .map_err(|error| words(&error))?
    {
        CommitOutcome::Committed { sha } => Ok(Some(Landed {
            change,
            next: next_by(team.policy.integration, &branch, &into),
            branch,
            sha,
        })),
        CommitOutcome::Unchanged => Ok(None),
        CommitOutcome::Busy(paths) => Err(LandRefusal::Busy(paths)),
        CommitOutcome::Linked(path) => Err(LandRefusal::Link(path)),
    }
}

/// What a call asks for once its input is checked.
struct Checked<'a> {
    folder: &'static str,
    path: String,
    text: &'a str,
    /// For a human document: its `.agent.md` path and text, and the summary for the owner.
    proposal: Option<(String, &'a str, String)>,
}

/// Writes a document of the caller's folder as a folder change, or proposes a human document at
/// the sprint review (`docs/SPEC.md` 5.17). Holds the integration lock from its checks to its event,
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
    if checked.proposal.is_some() {
        return propose(call, &checked);
    }
    let path = &checked.path;
    let name = call.agent.display_name.to_string();
    let files = [(path.clone(), checked.text.to_string())];
    let message = folder_doc_message(checked.folder, &[path], &name, false);
    let author = folder_doc_author(&name, call.agent_id());
    let landed = match land(deps, &call.team, &files, &message, &author) {
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
    }))
    .map_err(failed)?;
    call.append(None, EventBody::FolderDocWritten(body))?;
    Ok(json!({
        "path": path, "change": landed.change, "branch": landed.branch, "next": landed.next,
    }))
}

/// Holds the call to the rules of its session, its path and its fields, in that order.
fn check_input<'a>(
    call: &Call<'_>,
    input: &'a WriteFolderDocInput,
) -> Result<Checked<'a>, ToolError> {
    let role = call.role();
    let in_a_document_ceremony = call.context.purpose == SessionPurpose::Ceremony
        && matches!(
            call.context.thread,
            Some(Thread::Planning | Thread::Review | Thread::Retro)
        );
    let Some(folder) = role_folder(role).filter(|_| in_a_document_ceremony) else {
        return Err(refused(
            "folder_doc_refused",
            "a folder document is written at a sprint planning, review or retro, by a role that owns a folder",
        ));
    };
    let (target, twin) = checked_path(call, folder, &input.path)?;
    let path = target.path;
    if target.owner_accepted && call.context.thread != Some(Thread::Review) {
        return Err(refused(
            "folder_doc_at_review",
            format!(
                "{path} is proposed to the owner at the sprint review, not at another ceremony"
            ),
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
            proposal: None,
        });
    };
    let agent_text = input.agent_text.as_deref().ok_or_else(|| {
        refused(
            "folder_doc_agent_text",
            format!("{path} needs agent_text, its .agent.md version"),
        )
    })?;
    let agent_text = checked_text("agent_text", agent_text)?;
    let summary = input.summary.as_deref().map(str::trim).ok_or_else(|| {
        refused(
            "folder_doc_summary",
            format!("{path} needs a summary of what changed and why, for the owner"),
        )
    })?;
    if summary.is_empty() || summary.chars().count() > SUMMARY_CHARS {
        return Err(refused(
            "folder_doc_summary",
            format!("summary is 1 to {SUMMARY_CHARS} characters"),
        ));
    }
    Ok(Checked {
        folder,
        path,
        text,
        proposal: Some((twin, agent_text, summary.to_string())),
    })
}

/// Holds the path to the caller's folder and to the team's protected paths.
fn checked_path(
    call: &Call<'_>,
    folder: &str,
    named: &str,
) -> Result<(FolderDocPath, Option<String>), ToolError> {
    let target = check_folder_doc_path(call.role(), named).map_err(|why| match why {
        FolderDocPathRefusal::NoFolder => refused("folder_doc_refused", "your role owns no folder"),
        FolderDocPathRefusal::Outside => refused(
            "folder_doc_path",
            format!("{named} is not a file below {folder}/"),
        ),
        FolderDocPathRefusal::NotMarkdown => {
            refused("folder_doc_path", format!("{named} is not a .md file"))
        }
        FolderDocPathRefusal::AgentTwin => refused(
            "folder_doc_path",
            format!("{named} is an .agent.md version, which is written from agent_text"),
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
    if check_protected_paths(&protected, &call.team.rules().protected_paths).is_err() {
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

/// Records the proposal of a human document and its twin for the sprint review's sprint, the lock
/// held. Nothing is written.
fn propose(call: &Call<'_>, checked: &Checked<'_>) -> Result<Value, ToolError> {
    let Some((twin, agent_text, summary)) = &checked.proposal else {
        return Err(failed("a proposal was asked for with none to make"));
    };
    let (path, text) = (&checked.path, checked.text);
    let deps = call.deps();
    let sprint = ended_sprint(&deps.log).map_err(failed)?.ok_or_else(|| {
        refused(
            "folder_doc_at_review",
            "no sprint has ended, so there is no review to propose at",
        )
    })?;
    let into = integration_branch(&call.team, &deps.git).map_err(failed)?;
    if holds(&deps.git, &into, path, text) && holds(&deps.git, &into, twin, agent_text) {
        return Err(refused(
            "folder_doc_unchanged",
            format!("{path} and {twin} already say this on the integration branch"),
        ));
    }
    let body = serde_json::from_value(json!({
        "path": path, "text": text, "agent_text": agent_text, "summary": summary,
        "sprint_id": sprint.sprint_id, "proposed_by": call.agent_id(),
    }))
    .map_err(failed)?;
    let proposed = call.append(None, EventBody::FolderDocProposed(body))?;
    Ok(json!({ "proposal": proposed.envelope.seq, "next": PROPOSED }))
}

/// Whether `rev` holds `path` with exactly `text`.
fn holds(git: &Git, rev: &str, path: &str, text: &str) -> bool {
    git.file_at(rev, path).is_ok_and(|held| held == text)
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
    const NEXT: &str =
        "The owner approves it or sends it back on Today; you are told at your next sprint review.";

    /// A team of `pm`, two Developers, the Scrum Master `sm` ("Sol") and, when asked, the Finance
    /// Specialist `fin`, integrating `policy`.
    fn project(name: &str, policy: &str, scrum_master: bool, finance: bool) -> TestProject {
        let team = a_team_of_three(|wire| {
            wire["policy"]["integration"] = json!(policy);
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
        project.repo.git_output(&[
            "branch",
            "--list",
            "docs/folder-*",
            "--format=%(refname:short)",
        ])
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
                "path": CADENCE, "change": 1, "branch": "docs/folder-1",
                "next": "Catervas merges docs/folder-1 into main on its next tick."
            })
        );
        let repo = &project.repo;
        assert_eq!(
            repo.git_output(&["log", "-1", "--format=%an <%ae>|%s", "docs/folder-1"]),
            "Sol (Catervas) <catervas@localhost>|docs(delivery): cadence.md by Sol"
        );
        assert_eq!(
            repo.git_output(&["rev-list", "--count", "main..docs/folder-1"]),
            "1"
        );
        assert_eq!(
            repo.git_output(&["diff", "--name-only", "main", "docs/folder-1"]),
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
        assert_eq!(
            wire["body"]["sha"],
            repo.git_output(&["rev-parse", "docs/folder-1"])
        );

        let second = write(&context, "docs/catervas/delivery/notes.md", "n\n").expect("another");
        assert_eq!(second["change"], 2);
        // A branch git already has is not reused, nor is a number the log has handed out.
        repo.git(&["branch", "docs/folder-3", "main"]);
        let third = write(&context, "docs/catervas/delivery/more.md", "m\n").expect("a third");
        assert_eq!(third["change"], 4);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_what_happens_next_by_the_policy() {
        for (policy, next) in [
            (
                "auto_merge",
                "Catervas merges docs/folder-1 into main on its next tick.",
            ),
            (
                "pull_request",
                "Catervas opens a pull request for docs/folder-1 on its next tick; the document is in main once it merges.",
            ),
            ("manual", "docs/folder-1 waits for the owner to add it."),
        ] {
            let project = project(&format!("folder-doc-next-{policy}"), policy, true, false);
            let context = ceremony(&project, "sm", Thread::Planning);
            let answer = write(&context, CADENCE, "a\n").expect("written");
            assert_eq!(answer["next"], next, "{policy}");
            assert!(
                project.events(&[EventKind::MessagePosted]).is_empty(),
                "{policy} posts no line in the channel"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn proposes_the_product_plan_at_the_review() {
        let project = project("folder-doc-propose", "auto_merge", false, false);
        s4_ended(&project);
        let input = json!({
            "path": ROADMAP, "text": "Now: pie pre-orders.\n", "agent_text": "- now: pie pre-orders\n",
            "summary": "Pie pre-orders are done, so they move to Done."
        });

        let answer = run(
            &ceremony(&project, "pm", Thread::Review),
            TOOL,
            input.clone(),
        )
        .expect("the proposal is recorded");

        let proposed = project.events(&[EventKind::FolderDocProposed]);
        assert_eq!(proposed.len(), 1);
        let wire = catervas_protocol::event::event_to_value(&proposed[0]);
        assert_eq!(
            wire["body"],
            json!({
                "path": ROADMAP, "text": "Now: pie pre-orders.\n",
                "agent_text": "- now: pie pre-orders\n",
                "summary": "Pie pre-orders are done, so they move to Done.",
                "sprint_id": "S4", "proposed_by": "pm"
            })
        );
        assert_eq!(
            answer,
            json!({ "proposal": proposed[0].envelope.seq, "next": NEXT })
        );
        assert_eq!(branches(&project), "", "a proposal makes no branch");

        let before = project.event_count();
        let refused = run(&ceremony(&project, "pm", Thread::Retro), TOOL, input)
            .expect_err("only the review proposes");
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
                &pm,
                json!({ "path": ROADMAP, "text": "a", "agent_text": "b" }),
                "folder_doc_summary",
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
        assert_eq!(branches(&waiting), "docs/folder-1");
        assert_eq!(waiting.events(&[EventKind::FolderDocWritten]).len(), 1);
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
            ceremony(&project, "sm", Thread::Standup),
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
