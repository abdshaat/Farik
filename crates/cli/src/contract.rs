//! `farik contract lock` and `farik contract unlock`: contract ownership (`docs/SPEC.md` section
//! 5.11).

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_core::governor::gates::{ContractWriteActor, check_contract_write};
use farik_core::governor::transition_table::TransitionActor;
use farik_protocol::event::EventBody;
use farik_protocol::generated::event::{ContractLockedBody, ContractUnlockedBody};
use serde_json::json;

use crate::project::Project;
use crate::triage::status_of;
use crate::{HUMAN, Report};

/// Takes a contract, or gives it back.
///
/// Whether the write is allowed is `check_contract_write`'s answer, asked with `locked` as the only
/// field changing and the human as the actor: the lock is the human's field alone, locking is not a
/// content change and so does not send the task back to `refining`, and a task that is `accepted` or
/// `cancelled` takes no write but a note.
///
/// The status comes from the log, which is what decides a task's status (8.4), and the kind and the
/// lock from the file, which is what decides a contract's content (8.4, and the project plan's note
/// on step 07). On a project where the two disagree — which is what `farik doctor` is for — the
/// governor is asked about the status the log knows, because that is the one the transition table
/// answers for. The gate is asked before the contract is found to be held already, so that a task
/// nothing can be written to says so rather than answering about the lock.
///
/// # Errors
///
/// A sentence saying there is no such task, that the contract is already held or already the
/// team's, the governor's own refusal, or what could not be written.
pub fn hold(
    project: &Project,
    task_id: &str,
    held: bool,
    now: DateTime<Utc>,
) -> Result<Report, String> {
    let task_id: TaskId = task_id
        .parse()
        .map_err(|error| format!("{task_id} is not a task id: {error}"))?;
    let contract = project
        .files
        .read_contract(&task_id)
        .map_err(|error| error.to_string())?;
    let status = status_of(project, &task_id)?;
    check_contract_write(
        contract.kind,
        status,
        contract.locked,
        &ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        },
        &["locked".to_string()],
    )
    .map_err(|refusal| crate::refusal::contract_write(&refusal))?;
    if contract.locked == held {
        return Err(format!(
            "{} is already {}",
            task_id.as_str(),
            if held {
                "yours: farik contract unlock gives it back"
            } else {
                "the team's"
            }
        ));
    }

    let mut written = contract;
    written.locked = held;
    written.updated_at = Some(now);
    project
        .files
        .write_contract(&written)
        .map_err(|error| error.to_string())?;

    let body = if held {
        EventBody::ContractLocked(ContractLockedBody {
            locked_by: HUMAN.to_string(),
        })
    } else {
        EventBody::ContractUnlocked(ContractUnlockedBody {
            unlocked_by: HUMAN.to_string(),
        })
    };
    let event = project.event(body, now, Some(task_id.clone()))?;
    let seq = project.append(&event)?;

    Ok(Report {
        lines: vec![if held {
            format!(
                "{} is yours: agents may record criterion results and write notes, and nothing \
                 else",
                task_id.as_str()
            )
        } else {
            format!("{} is the team's again", task_id.as_str())
        }],
        json: json!({
            "task_id": task_id.to_string(),
            "locked": held,
            "events": [seq],
        }),
        json_lines: None,
    })
}
