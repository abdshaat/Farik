//! A task's diff against the integration branch (`docs/SPEC.md` 5.14), and an epic's, which is
//! its tasks' integrated diffs joined: for `farik task show --diff` and the browser alike.

use farik_core::branch::task_branch;
use farik_core::contract::{TaskContract, TaskKind};
use farik_core::team::{Team, task_private_folder};
use farik_protocol::event::{EventBody, FarikEvent};

use crate::baseline::{baseline_of, changes_since_baseline, folder_in};
use crate::git::{Git, integration_branch};

/// A diff and what it touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDiff {
    /// The unified diff, or for an epic its tasks' under a `# FRK-n` line each.
    pub diff: String,
    /// Every file it touches, once each, in the order it first touches them.
    pub files: Vec<String>,
    /// Lines added.
    pub added: u64,
    /// Lines removed.
    pub removed: u64,
    /// Whether the task works in a private folder (6.6), which is not shown: `diff` is then empty,
    /// and `files` names what changed in the folder since the copy taken when it was assigned.
    pub private_folder: bool,
}

/// The diff of `contract`, whose events are `history`. A task's branch is diffed against the
/// integration branch before integration, and after it against the first parent of the merge
/// commit it was integrated by, or, when its integration was not a merge commit, against the
/// integration branch, which then holds it all. An epic has no branch: its `children`, each with
/// its history, give their diffs, those integrated only, in id order, each under a `# FRK-n` line.
///
/// # Errors
///
/// A sentence saying the task has no branch yet, or what git refused.
pub fn diff_of(
    git: &Git,
    team: &Team,
    contract: &TaskContract,
    history: &[FarikEvent],
    children: &[(TaskContract, Vec<FarikEvent>)],
) -> Result<TaskDiff, String> {
    // A task in a private folder has no branch, and its files are not shown: only which changed.
    // Once accepted, the move into `accepted` carries them, since the folder holds later tasks'
    // changes too; before that, or in a log from before the move carried them, they are read from
    // the folder. Until it is assigned it has no copy to differ from, so it has changed nothing.
    if let Some(folder) = task_private_folder(contract) {
        let files = match accepted_changes(history) {
            Some(files) => files,
            None => folder_in(git.root(), folder)
                .and_then(|folder| {
                    if baseline_of(&folder, &contract.id).is_dir() {
                        changes_since_baseline(&folder, &contract.id)
                    } else {
                        Ok(Vec::new())
                    }
                })
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|change| change.path)
                .collect(),
        };
        return Ok(TaskDiff {
            diff: String::new(),
            files,
            added: 0,
            removed: 0,
            private_folder: true,
        });
    }
    if contract.kind != TaskKind::Epic {
        return Ok(counted(branch_diff(git, team, contract, history)?));
    }
    let mut ordered: Vec<&(TaskContract, Vec<FarikEvent>)> = children
        .iter()
        .filter(|(_, history)| integrated(history).is_some())
        .collect();
    ordered.sort_by_key(|(child, _)| id_number(child));
    let mut joined = String::new();
    for (child, history) in ordered {
        let diff = branch_diff(git, team, child, history)?;
        joined.push_str("# ");
        joined.push_str(child.id.as_str());
        joined.push('\n');
        joined.push_str(&diff);
        if !joined.ends_with('\n') {
            joined.push('\n');
        }
    }
    Ok(counted(joined))
}

/// The files a task in a private folder changed, as its last move into `accepted` recorded them;
/// none when it was not accepted, or was in a log from before the move recorded them.
fn accepted_changes(history: &[FarikEvent]) -> Option<Vec<String>> {
    history.iter().rev().find_map(|event| match &event.body {
        EventBody::TaskTransitioned(body) if body.to.to_string() == "accepted" => {
            body.changed.clone()
        }
        _ => None,
    })
}

/// The number of a task id, which orders `FRK-9` before `FRK-10`.
fn id_number(contract: &TaskContract) -> u64 {
    contract
        .id
        .as_str()
        .trim_start_matches("FRK-")
        .parse()
        .unwrap_or_default()
}

/// The commit and the branch of the task's last integration, when it has one.
fn integrated(history: &[FarikEvent]) -> Option<(String, String)> {
    history.iter().rev().find_map(|event| match &event.body {
        EventBody::TaskIntegrated(body) => Some((body.sha.clone(), body.into.clone())),
        _ => None,
    })
}

fn branch_diff(
    git: &Git,
    team: &Team,
    contract: &TaskContract,
    history: &[FarikEvent],
) -> Result<String, String> {
    let branch = task_branch(contract);
    // `merge-base x x` answers `x`'s commit, and refuses a name that names none.
    if git.merge_base(&branch, &branch).is_err() {
        return Err(format!(
            "{} has no branch yet: its work starts at assignment",
            contract.id.as_str()
        ));
    }
    let words = |error: crate::GitError| error.to_string();
    match integrated(history) {
        None => {
            let base = integration_branch(team, git).map_err(words)?;
            git.diff(&base, &branch).map_err(words)
        }
        Some((sha, into)) => {
            let second_parent = format!("{sha}^2");
            if git.merge_base(&second_parent, &second_parent).is_ok() {
                git.diff(&format!("{sha}^1"), &branch).map_err(words)
            } else {
                let diff = git.diff(&into, &branch).map_err(words)?;
                if diff.trim().is_empty() {
                    Ok(format!(
                        "{branch} is wholly in {into}; its merge is not a commit farik can diff \
                         against"
                    ))
                } else {
                    Ok(diff)
                }
            }
        }
    }
}

/// `diff` with the files it touches and the lines it adds and removes, counted inside its hunks
/// only, so that neither a file's `---`/`+++` header nor a `# FRK-n` line counts.
fn counted(diff: String) -> TaskDiff {
    let (mut files, mut added, mut removed) = (Vec::<String>::new(), 0, 0);
    let mut in_hunk = false;
    for line in diff.lines() {
        if let Some(names) = line.strip_prefix("diff --git ") {
            in_hunk = false;
            let file = names.rsplit_once(" b/").map_or(names, |(_, file)| file);
            if !files.iter().any(|known| known == file) {
                files.push(file.to_string());
            }
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if line.starts_with("# FRK-") {
            in_hunk = false;
        } else if in_hunk && line.starts_with('+') {
            added += 1;
        } else if in_hunk && line.starts_with('-') {
            removed += 1;
        }
    }
    TaskDiff {
        diff,
        files,
        added,
        removed,
        private_folder: false,
    }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::fixtures::a_contract_wire;
    use farik_core::contract::{TaskContract, validate_contract};
    use farik_core::team::fixtures::a_team_wire;
    use farik_core::team::{Team, validate_team};
    use farik_protocol::event::fixtures::an_event_wire;
    use farik_protocol::event::{EventKind, FarikEvent, event_from_value};
    use serde_json::{Value, json};

    use super::diff_of;
    use crate::baseline::copy_baseline;
    use crate::git::fixtures::TempRepo;

    const FOLDER: &str = ".farik/local/finance";

    /// A Finance Specialist's task FRK-1, in its folder.
    fn a_finance_task() -> TaskContract {
        let mut wire = a_contract_wire();
        wire["assignee_role"] = json!("finance_specialist");
        wire["reviewer_role"] = json!("product_manager");
        wire["allowed_paths"] = json!([".farik/local/finance/**"]);
        wire["exit_criteria"] = json!([{
            "id": "C1",
            "text": "The books exist.",
            "satisfies": ["R1"],
            "verification": { "method": "artifact", "path": "books.xlsx" }
        }]);
        validate_contract(&wire).expect("a contract")
    }

    fn a_team() -> Team {
        validate_team(&a_team_wire()).expect("a team")
    }

    /// FRK-1's move into `accepted`, which integrates nothing, naming `changed` when it is given.
    fn its_acceptance(changed: Option<&[&str]>) -> FarikEvent {
        let mut wire = an_event_wire(EventKind::TaskTransitioned);
        wire["body"] = json!({
            "from": "verifying",
            "to": "accepted",
            "actor": "product_manager",
            "requested_by": "maya-chen",
            "gate": "definition_of_done",
            "effects": ["nothing_to_integrate"],
            "assignee": "fin",
            "reviewer": "pm",
            "iteration": 0
        });
        if let Some(changed) = changed {
            wire["body"]["changed"] = Value::from(changed.to_vec());
        }
        event_from_value(&wire).expect("a schema-valid event")
    }

    fn files_of(repo: &TempRepo, history: &[FarikEvent]) -> Vec<String> {
        let diff =
            diff_of(&repo.adapter(), &a_team(), &a_finance_task(), history, &[]).expect("the diff");
        assert!(diff.private_folder);
        assert_eq!(diff.diff, "");
        diff.files
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_accepted_tasks_changes_stay_its_own() {
        // FRK-1 was accepted with `books.xlsx` changed; FRK-2 then changed `forecast.xlsx` in the
        // same folder, which FRK-1's page does not show as its own.
        let repo = TempRepo::new("diff-accepted-own");
        let folder = repo.path.join(FOLDER);
        std::fs::create_dir_all(&folder).expect("the folder is made");
        std::fs::write(folder.join("books.xlsx"), "books").expect("written");
        copy_baseline(&folder, &"FRK-1".parse().expect("a task id")).expect("the copy");
        std::fs::write(folder.join("books.xlsx"), "edited books").expect("written");
        let accepted = its_acceptance(Some(&["books.xlsx"]));
        std::fs::write(folder.join("forecast.xlsx"), "forecast").expect("written");
        std::fs::write(folder.join("books.xlsx"), "FRK-2's books").expect("written");

        assert_eq!(files_of(&repo, &[accepted]), ["books.xlsx"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_older_acceptance_reads_the_folder() {
        // A log from before the acceptance carried `changed` holds a move to `accepted` with none:
        // it is answered from the folder as it was, and so is a task not accepted yet.
        let repo = TempRepo::new("diff-accepted-older");
        let folder = repo.path.join(FOLDER);
        std::fs::create_dir_all(&folder).expect("the folder is made");
        std::fs::write(folder.join("books.xlsx"), "books").expect("written");
        copy_baseline(&folder, &"FRK-1".parse().expect("a task id")).expect("the copy");
        std::fs::write(folder.join("books.xlsx"), "edited books").expect("written");
        std::fs::write(folder.join("forecast.xlsx"), "forecast").expect("written");

        assert_eq!(
            files_of(&repo, &[its_acceptance(None)]),
            ["books.xlsx", "forecast.xlsx"]
        );
        assert_eq!(files_of(&repo, &[]), ["books.xlsx", "forecast.xlsx"]);
        // An acceptance that changed nothing says so: no file, not the folder's.
        assert_eq!(
            files_of(&repo, &[its_acceptance(Some(&[]))]),
            Vec::<String>::new()
        );
    }
}
