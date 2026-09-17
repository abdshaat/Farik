//! Farik's wire types: the event envelope, the event kinds, the commands, and the traits that
//! keep the machine's clock and its identifiers out of the crates that decide things.

/// Time and identifiers, injected rather than read from the machine.
pub mod clock;
/// The commands the daemon accepts.
pub mod command;
/// The event envelope, the event bodies, and the reader and writer of the wire form.
pub mod event;
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
        assert_eq!(KINDS.map(|(_, kind)| kind), crate::event::EVERY_KIND);
    }

    #[test]
    fn keeps_the_summary_vocabularies_the_contract_schema_owns() {
        // One schema never references another, so the event schema repeats the contract's kind,
        // status, and risk lists. This is what stops the copy from drifting from the original.
        let event: serde_json::Value =
            serde_json::from_str(include_str!("generated/event.schema.json"))
                .expect("the embedded event schema is valid JSON");
        let contract: serde_json::Value = serde_json::from_str(farik_core::contract::SCHEMA_JSON)
            .expect("the embedded contract schema is valid JSON");
        let summary = &event["$defs"]["contractSummary"]["properties"];
        let fields = &contract["properties"];
        assert_eq!(summary["kind"]["enum"], fields["kind"]["enum"]);
        assert_eq!(summary["status"]["enum"], fields["status"]["enum"]);
        assert_eq!(summary["risk"]["enum"], fields["risk"]["enum"]);
        assert_eq!(summary["parent"]["pattern"], fields["parent"]["pattern"]);
        // Every copy of the task id's pattern, not only the summary's: the envelope's and the
        // command's are what farik_core's TaskId::from_str is then handed.
        let command: serde_json::Value =
            serde_json::from_str(include_str!("generated/command.schema.json"))
                .expect("the embedded command schema is valid JSON");
        for copy in [
            &event["properties"]["task_id"]["pattern"],
            &command["$defs"]["requestTriageBody"]["properties"]["task_id"]["pattern"],
        ] {
            assert_eq!(copy, &fields["id"]["pattern"]);
        }
    }
}
