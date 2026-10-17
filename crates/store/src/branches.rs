//! The branch names a new task's branch may not take (ADR 0066).

use catervas_core::contract::TaskId;

use crate::error::StoreError;
use crate::event_log::EventLog;
use crate::files::ProjectFiles;
use crate::folder_docs::folder_docs;
use crate::git::Git;

/// The branch names a new task's or folder change's branch may not take: `git`'s branches and
/// every folder change's `branch` and every contract's recorded `branch` but `except`'s. A contract that cannot be read is passed
/// over.
///
/// # Errors
///
/// `StoreError::Io` with git's or the files' words.
pub fn taken_branches(
    files: &ProjectFiles,
    log: &EventLog,
    git: &Git,
    except: Option<&TaskId>,
) -> Result<Vec<String>, StoreError> {
    let mut taken = git.branch_names().map_err(|error| StoreError::Io {
        detail: error.to_string(),
    })?;
    let ids = files.list_contracts().map_err(|error| StoreError::Io {
        detail: error.to_string(),
    })?;
    for id in ids.iter().filter(|id| Some(*id) != except) {
        if let Ok(contract) = files.read_contract(id)
            && let Some(branch) = &contract.branch
        {
            taken.push(branch.as_str().to_string());
        }
    }
    taken.extend(
        folder_docs(log)?
            .changes
            .into_iter()
            .map(|change| change.branch),
    );
    Ok(taken)
}
