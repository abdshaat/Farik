//! Farik's wire types: the event envelope, the event kinds, the commands, and the traits that
//! keep the machine's clock and its identifiers out of the crates that decide things.

/// Types generated from `docs/schemas/`.
pub mod generated;

#[cfg(test)]
mod tests {
    use crate::generated::event::EventKind;

    /// Every kind the log holds in this phase, with the wire name the schema gives it.
    const KINDS: [(&str, EventKind); 9] = [
        ("task.created", EventKind::TaskCreated),
        ("request.triaged", EventKind::RequestTriaged),
        ("contract.written", EventKind::ContractWritten),
        ("contract.locked", EventKind::ContractLocked),
        ("contract.unlocked", EventKind::ContractUnlocked),
        ("drift.detected", EventKind::DriftDetected),
        ("project.scanned", EventKind::ProjectScanned),
        ("team.updated", EventKind::TeamUpdated),
        ("criteria.updated", EventKind::CriteriaUpdated),
    ];

    #[test]
    fn names_every_event_kind_as_an_entity_and_a_past_tense_verb() {
        for (wire, kind) in KINDS {
            assert_eq!(kind.to_string(), wire);
            assert_eq!(wire.parse::<EventKind>().expect("a known kind"), kind);
        }
    }
}
