//! The governor's refusals in the words a person reads.
//!
//! `farik-core` answers with values rather than sentences, so that one rule has one answer and every
//! caller says it its own way (`docs/standards/code.md`). This is the command line's way of saying
//! them; the daemon and the app will have their own, and every one of them points at the rule in
//! `docs/SPEC.md` that decided it.

use farik_core::governor::gates::ContractWriteRefusal;
use farik_protocol::event::EventError;

/// Why the governor would not let this write happen (`docs/SPEC.md` sections 5.2, 5.11 and 5.16).
#[must_use]
pub fn contract_write(refusal: &ContractWriteRefusal) -> String {
    match refusal {
        ContractWriteRefusal::ContractLocked => {
            "the contract is held by the human, and a contract's content is the holder's alone \
             (5.11): farik contract unlock gives it back to the team"
                .to_string()
        }
        ContractWriteRefusal::ContractFrozen { fields } => format!(
            "the contract is frozen: once a task leaves refining only its status, assignee, \
             reviewer, iteration, sprint and notes change (5.11), and this would change {}",
            listed(fields)
        ),
        ContractWriteRefusal::TaskTerminal { status } => format!(
            "the task is {status}, and nothing leaves that status (5.2): its notes are all that \
             still change"
        ),
        ContractWriteRefusal::LifecycleFields { fields } => format!(
            "{} {} the governor's, written when it applies a transition (5.2): ask for the \
             transition instead",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::HumansFields { fields } => format!(
            "{} {} the human's alone (5.11)",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::StoresFields { fields } => format!(
            "{} {} the store's: Farik assigns the identifier and the stamps",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::CreationFields { fields } => format!(
            "{} {} fixed when a contract is created (5.16): the triage decides the kind, and a \
             task's epic is the epic that broke it down",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::ContentFields { fields } => format!(
            "a contract's content is the Product Manager's and the human's, and an epic's tasks are \
             its assignee's (5.16, 6.2): this actor does not write {}",
            listed(fields)
        ),
        ContractWriteRefusal::UnknownFields { fields } => format!(
            "{} {} not a field of a contract: every one is listed by name, so a field added to the \
             schema is refused until somebody says who writes it",
            listed(fields),
            is_or_are(fields)
        ),
    }
}

/// Why an event could not be built. Either is this program disagreeing with itself rather than
/// anything the person did, so each says which field was missing.
#[must_use]
pub fn event(error: &EventError) -> String {
    match error {
        // The field is `team_id`, `project_id`, or the body's own field naming who acted, so the
        // sentence names it rather than guessing which of the three it was.
        EventError::BlankId { field } => format!(
            "this event needed a {field} and it is blank, and a blank id names nobody: that is a bug \
             in Farik rather than anything you did"
        ),
        EventError::NoContractNamed { kind } => format!(
            "a {kind} event is about one contract and this one names none, which is a bug in Farik \
             rather than anything you did"
        ),
    }
}

/// A list in the words a person would read it out in.
fn listed(fields: &[String]) -> String {
    match fields {
        [] => "nothing".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Whether the sentence about those fields takes a singular verb.
fn is_or_are(fields: &[String]) -> &'static str {
    if fields.len() == 1 { "is" } else { "are" }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::TaskStatus;
    use farik_core::governor::gates::ContractWriteRefusal;
    use farik_protocol::event::{EventError, EventKind};

    use super::{contract_write, event};

    fn fields(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn says_why_the_governor_would_not_let_a_contract_be_written() {
        // Every refusal a person can meet, in one place, so that a variant added to the governor
        // without a sentence here is a compilation error rather than a Rust value in a terminal.
        assert_eq!(
            [
                contract_write(&ContractWriteRefusal::ContractLocked),
                contract_write(&ContractWriteRefusal::ContractFrozen {
                    fields: fields(&["title", "risk"])
                }),
                contract_write(&ContractWriteRefusal::TaskTerminal {
                    status: TaskStatus::Accepted
                }),
                contract_write(&ContractWriteRefusal::LifecycleFields {
                    fields: fields(&["status"])
                }),
                contract_write(&ContractWriteRefusal::HumansFields {
                    fields: fields(&["locked"])
                }),
                contract_write(&ContractWriteRefusal::StoresFields {
                    fields: fields(&["id", "created_at"])
                }),
                contract_write(&ContractWriteRefusal::CreationFields {
                    fields: fields(&["kind"])
                }),
                contract_write(&ContractWriteRefusal::ContentFields {
                    fields: fields(&["title"])
                }),
                contract_write(&ContractWriteRefusal::UnknownFields {
                    fields: fields(&["colour"])
                }),
            ],
            [
                "the contract is held by the human, and a contract's content is the holder's alone \
                 (5.11): farik contract unlock gives it back to the team",
                "the contract is frozen: once a task leaves refining only its status, assignee, \
                 reviewer, iteration, sprint and notes change (5.11), and this would change title \
                 and risk",
                "the task is accepted, and nothing leaves that status (5.2): its notes are all that \
                 still change",
                "status is the governor's, written when it applies a transition (5.2): ask for the \
                 transition instead",
                "locked is the human's alone (5.11)",
                "id and created_at are the store's: Farik assigns the identifier and the stamps",
                "kind is fixed when a contract is created (5.16): the triage decides the kind, and \
                 a task's epic is the epic that broke it down",
                "a contract's content is the Product Manager's and the human's, and an epic's tasks \
                 are its assignee's (5.16, 6.2): this actor does not write title",
                "colour is not a field of a contract: every one is listed by name, so a field added \
                 to the schema is refused until somebody says who writes it",
            ]
        );
    }

    #[test]
    fn says_which_id_an_event_was_missing() {
        assert_eq!(
            [
                event(&EventError::BlankId {
                    field: "team_id".to_string()
                }),
                event(&EventError::NoContractNamed {
                    kind: EventKind::ContractLocked
                }),
            ],
            [
                "this event needed a team_id and it is blank, and a blank id names nobody: that is \
                 a bug in Farik rather than anything you did",
                "a contract.locked event is about one contract and this one names none, which is a \
                 bug in Farik rather than anything you did",
            ]
        );
    }
}
