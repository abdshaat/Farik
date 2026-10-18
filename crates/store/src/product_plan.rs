//! A new project and its product plan task, read from the log (`docs/SPEC.md` 4.1 and 4.3).

use catervas_core::contract::{TaskId, TaskStatus};
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind};
use chrono::{DateTime, Utc};

use crate::{EventLog, EventQuery, StoreError, TaskProjection};

/// A project the first run made from the owner's paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    /// The folder's name.
    pub name: String,
    /// The paragraph, as the owner wrote it, trimmed.
    pub description: String,
}

/// The project's `project.started`, when its log holds one.
///
/// # Errors
///
/// What the log refused.
pub fn new_project(log: &EventLog) -> Result<Option<NewProject>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::ProjectStarted],
        limit: Some(1),
        ..EventQuery::default()
    })?;
    Ok(events.into_iter().find_map(|event| match event.body {
        EventBody::ProjectStarted(body) => Some(NewProject {
            name: body.name.to_string(),
            description: body.description.to_string(),
        }),
        _ => None,
    }))
}

/// Every task filed to draft the product plan, oldest first, each with when it was filed.
///
/// # Errors
///
/// What the log refused.
pub fn product_plan_tasks(log: &EventLog) -> Result<Vec<(TaskId, DateTime<Utc>)>, StoreError> {
    Ok(log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            ..EventQuery::default()
        })?
        .into_iter()
        .filter(|event| {
            matches!(&event.body, EventBody::TaskCreated(body) if body.product_plan == Some(true))
        })
        .filter_map(|event| Some((event.envelope.ids.task_id?, event.envelope.recorded_at)))
        .collect())
}

/// Whether a product plan task still drafts the plan: it is neither cancelled nor accepted
/// (Catervas accepts it, and nothing waits for its merge).
#[must_use]
pub fn drafts_the_plan(row: &TaskProjection) -> bool {
    !matches!(row.status, TaskStatus::Cancelled | TaskStatus::Accepted)
}

/// Whether a task's events say it is the product plan's: its `task.created` has `product_plan`.
#[must_use]
pub fn is_the_product_plan(history: &[CatervasEvent]) -> bool {
    history.iter().any(|event| {
        matches!(&event.body, EventBody::TaskCreated(body) if body.product_plan == Some(true))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{NewProject, is_the_product_plan, new_project};
    use crate::waiting::fixtures::{Board, at};

    fn created(board: &Board, task: &str, extra: &serde_json::Value) {
        let mut body = json!({
            "summary": {
                "kind": "task", "title": "Draft the product plan", "status": "draft", "risk": "low"
            },
            "created_by": "human"
        });
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                body[key] = value.clone();
            }
        }
        board.put(at(10, 0), Some(task), None, "task.created", body);
    }

    #[test]
    fn a_task_drafts_the_plan_until_it_is_cancelled_or_accepted() {
        let board = Board::new("product-plan-open");
        created(&board, "CTV-1", &json!({ "product_plan": true }));
        let drafting = |board: &Board| {
            let rows = board.projections.board().expect("the board reads");
            super::drafts_the_plan(&rows[0])
        };
        assert!(drafting(&board));
        board.moved(
            at(11, 0),
            "CTV-1",
            ("draft", "accepted"),
            "human",
            (None, None),
        );
        assert!(!drafting(&board), "accepted, merged or not");
        let cancelled = Board::new("product-plan-cancelled");
        created(&cancelled, "CTV-1", &json!({ "product_plan": true }));
        cancelled.moved(
            at(11, 0),
            "CTV-1",
            ("draft", "cancelled"),
            "human",
            (None, None),
        );
        assert!(!drafting(&cancelled));
    }

    #[test]
    fn reads_the_new_project_and_its_plans_task() {
        let board = Board::new("product-plan");
        assert_eq!(new_project(&board.log).expect("reads"), None);
        board.put(
            at(9, 30),
            None,
            None,
            "project.started",
            json!({
                "name": "little-oak-cakes",
                "description": "A bakery that sells cakes online and bakes to order."
            }),
        );
        assert_eq!(
            new_project(&board.log).expect("reads"),
            Some(NewProject {
                name: "little-oak-cakes".to_string(),
                description: "A bakery that sells cakes online and bakes to order.".to_string(),
            })
        );
        created(&board, "CTV-1", &json!({ "product_plan": true }));
        created(&board, "CTV-2", &json!({}));
        let history = |task: &str| {
            board
                .log
                .read(&crate::EventQuery {
                    task_id: Some(task.parse().expect("a task id")),
                    ..crate::EventQuery::default()
                })
                .expect("reads")
        };
        assert!(is_the_product_plan(&history("CTV-1")));
        assert!(!is_the_product_plan(&history("CTV-2")));
    }
}
