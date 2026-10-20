//! Whether Catervas may merge its pull requests on this computer (ADR 0073).

use catervas_core::team::Team;
use catervas_protocol::event::EventKind;

use crate::{EventLog, EventQuery, StoreError};

/// Whether Catervas may merge: the team file says so (`policy.merge_when_checks_pass`) and the
/// newest of `merge_permission.granted` and `merge_permission.withdrawn` in this project's log is
/// the grant. The file travels with the repository and the log does not, so a clone that brings
/// `true` merges nothing until its owner confirms the warning here.
///
/// # Errors
///
/// What the log refused.
pub fn merging_allowed(log: &EventLog, team: &Team) -> Result<bool, StoreError> {
    if !team.merge_when_checks_pass() {
        return Ok(false);
    }
    let newest = log.read(&EventQuery {
        kinds: vec![
            EventKind::MergePermissionGranted,
            EventKind::MergePermissionWithdrawn,
        ],
        limit: Some(1),
        newest_first: true,
        ..EventQuery::default()
    })?;
    Ok(newest
        .first()
        .is_some_and(|event| event.body.kind() == EventKind::MergePermissionGranted))
}

#[cfg(test)]
mod tests {
    use catervas_core::team::validate_team;
    use serde_json::json;

    use super::merging_allowed;
    use crate::waiting::fixtures::{Board, a_team, at};

    #[test]
    fn merging_needs_this_computer_s_permission() {
        let board = Board::new("merging-allowed");
        let mut wire = serde_json::to_value(a_team()).expect("a team is JSON");
        wire["policy"]["merge_when_checks_pass"] = json!(true);
        let on = validate_team(&wire).expect("a team");

        assert!(
            !merging_allowed(&board.log, &on).expect("the log reads"),
            "the file alone permits nothing"
        );

        board.put(
            at(9, 1),
            None,
            None,
            "merge_permission.granted",
            json!({ "by": "human", "into": "main" }),
        );
        assert!(merging_allowed(&board.log, &on).expect("the log reads"));
        assert!(
            !merging_allowed(&board.log, &a_team()).expect("the log reads"),
            "a grant in the log without the key permits nothing"
        );

        board.put(
            at(9, 2),
            None,
            None,
            "merge_permission.withdrawn",
            json!({ "by": "human", "into": "main" }),
        );
        assert!(!merging_allowed(&board.log, &on).expect("the log reads"));
    }
}
