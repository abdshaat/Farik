//! What the human may do about an escalation, in words: each choice says what happens next,
//! names the agent, and exactly one is the team's suggestion (ADR 0062). The web app and the
//! command line read this one list.

use std::str::FromStr as _;

use catervas_core::contract::{Role, TaskId, TaskStatus};
use catervas_core::team::Team;
use catervas_protocol::event::{EventBody, EventKind};
use catervas_store::activity::status_in_words;
use catervas_store::files::ProjectFiles;
use catervas_store::waiting::name_of;
use catervas_store::{EventLog, EventQuery, Projections, StoreError};
use serde_json::{Value, json};

use crate::ceremonies::open_escalations;
use crate::transitions::last_move_into;

/// What the choices of one open escalation are built from.
pub struct Offer<'a> {
    /// The task.
    pub task_id: &'a str,
    /// The reason of its escalation, as the log writes it.
    pub reason: &'a str,
    /// The status the task left for `escalated`, when known.
    pub before: Option<TaskStatus>,
    /// Who holds the task now: a display name.
    pub who: &'a str,
    /// The active Product Manager's display name.
    pub product_manager: &'a str,
    /// The contract's sessions, which "Give it more room" gives again.
    pub max_sessions: u64,
    /// The contract's dollars, which "Give it more room" gives again.
    pub max_cost_usd: f64,
}

/// What a choice does when it is picked.
#[derive(Debug, Clone, PartialEq)]
pub enum ChoiceAction {
    /// Resolve the escalation by moving the task to `to`.
    Resolve {
        /// Where the task goes.
        to: TaskStatus,
        /// More tries, for an `iterations` escalation.
        extra_tries: Option<u8>,
        /// The contract's own sessions and dollars again, for a spent task.
        extra_room: bool,
    },
    /// Open the pull request of an accepted task again.
    Integrate,
    /// Go to a page of the web app.
    Open {
        /// The page's path.
        path: String,
    },
}

/// One thing the human may do.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// The button's words.
    pub label: String,
    /// What happens next, in a sentence that names the agent.
    pub next: String,
    /// Whether it is the team's suggestion.
    pub suggested: bool,
    /// What picking it does.
    pub action: ChoiceAction,
}

impl Choice {
    /// `{label, next, suggested, kind, body?, path?}` as `escalationChoicesResult` holds one.
    #[must_use]
    pub fn wire(&self, task_id: &str) -> Value {
        let mut wire = json!({
            "label": self.label,
            "next": self.next,
            "suggested": self.suggested,
        });
        match &self.action {
            ChoiceAction::Resolve {
                to,
                extra_tries,
                extra_room,
            } => {
                let mut body = json!({ "task_id": task_id, "to": to.to_string() });
                if let Some(tries) = extra_tries {
                    body["extra_tries"] = json!(tries);
                }
                if *extra_room {
                    body["extra_room"] = json!(true);
                }
                wire["kind"] = json!("command");
                wire["body"] = json!({ "command": "escalation_resolve", "body": body });
            }
            ChoiceAction::Integrate => {
                wire["kind"] = json!("command");
                wire["body"] =
                    json!({ "command": "task_integrate", "body": { "task_id": task_id } });
            }
            ChoiceAction::Open { path } => {
                wire["kind"] = json!("open");
                wire["path"] = json!(path);
            }
        }
        wire
    }

    /// The command line that makes this choice, as the end of a run prints it.
    #[must_use]
    pub fn command_line(&self, task_id: &str) -> String {
        match &self.action {
            ChoiceAction::Resolve {
                to,
                extra_tries,
                extra_room,
            } => {
                let more = match (extra_tries, extra_room) {
                    (Some(tries), _) => format!(" --extra-tries {tries}"),
                    (None, true) => " --extra-room".to_string(),
                    (None, false) => String::new(),
                };
                format!("catervas resolve {task_id} {to}{more} \"<your message>\"")
            }
            ChoiceAction::Integrate => format!("catervas integrate {task_id}"),
            ChoiceAction::Open { path } => format!("in the web app: {path}"),
        }
    }
}

fn choice(label: &str, next: String, action: ChoiceAction) -> Choice {
    Choice {
        label: label.to_string(),
        next,
        suggested: false,
        action,
    }
}

const fn resolve(to: TaskStatus, extra_tries: Option<u8>, extra_room: bool) -> ChoiceAction {
    ChoiceAction::Resolve {
        to,
        extra_tries,
        extra_room,
    }
}

fn cancel(offer: &Offer<'_>) -> Choice {
    choice(
        "Cancel the task",
        format!(
            "{} stops for good. Its branch is kept, and nothing is merged.",
            offer.task_id
        ),
        resolve(TaskStatus::Cancelled, None, false),
    )
}

fn change(offer: &Offer<'_>) -> Choice {
    choice(
        "Change the plan",
        rewrite(offer),
        resolve(TaskStatus::Refining, None, false),
    )
}

fn rewrite(offer: &Offer<'_>) -> String {
    format!(
        "{} rewrites the plan of {}; it is assigned again once it is ready.",
        offer.product_manager, offer.task_id
    )
}

/// Going back to where the task was, when that helps: not into planning, and never into
/// `escalated` itself.
fn carry_on(offer: &Offer<'_>, then: &str) -> Option<Choice> {
    offer
        .before
        .filter(|before| !matches!(before, TaskStatus::Refining | TaskStatus::Escalated))
        .map(|before| {
            choice(
                "Carry on",
                format!(
                    "{} goes back to {} and {} {then}.",
                    offer.task_id,
                    status_in_words(before).to_lowercase(),
                    offer.who
                ),
                resolve(before, None, false),
            )
        })
}

fn more_room(offer: &Offer<'_>) -> Vec<Choice> {
    let mut spent = Vec::new();
    if let Some(before) = offer.before {
        spent.push(choice(
            "Give it more room",
            format!(
                "{} carries on with {}, with up to {} more sessions and ${:.2} more to spend.",
                offer.who, offer.task_id, offer.max_sessions, offer.max_cost_usd
            ),
            resolve(before, None, true),
        ));
    }
    spent.push(cancel(offer));
    spent
}

fn fix_the_preview(offer: &Offer<'_>) -> Vec<Choice> {
    let id = offer.task_id;
    let mut offered = vec![choice(
        "Fix how your app opens",
        format!(
            "You tell Catervas in Settings how to start your app; then come back to {id} and choose Carry on."
        ),
        ChoiceAction::Open {
            path: "/settings/your-app".to_string(),
        },
    )];
    offered.extend(carry_on(offer, "opens your app again"));
    offered.push(cancel(offer));
    offered
}

/// The choices of an escalation, suggested first.
#[must_use]
pub fn escalation_choices(offer: &Offer<'_>) -> Vec<Choice> {
    let (id, who, pm) = (offer.task_id, offer.who, offer.product_manager);
    let mut choices = match offer.reason {
        "iterations" => vec![
            choice(
                "Give 2 more tries",
                format!("{who} picks up {id} again with 2 more tries."),
                resolve(TaskStatus::InProgress, Some(2), false),
            ),
            choice(
                &format!("Ask {pm} to change the plan"),
                rewrite(offer),
                resolve(TaskStatus::Refining, None, false),
            ),
            cancel(offer),
        ],
        "budget" | "sessions" => more_room(offer),
        "blocker_age" => vec![
            choice(
                "Answer and carry on",
                format!("{who} reads your answer and carries on with {id}."),
                resolve(TaskStatus::InProgress, None, false),
            ),
            change(offer),
            cancel(offer),
        ],
        "permission" | "explicit_request" => {
            let mut offered: Vec<Choice> = carry_on(offer, "is told what you wrote")
                .into_iter()
                .collect();
            offered.extend([change(offer), cancel(offer)]);
            offered
        }
        "readiness_failures" => vec![
            choice(
                &format!("Ask {pm} to try the plan again"),
                format!("{pm} writes the plan of {id} again with your note."),
                resolve(TaskStatus::Refining, None, false),
            ),
            cancel(offer),
        ],
        "approval" | "risk_gate" => vec![
            choice(
                "Review the plan",
                format!("You read the plan of {id} and approve it or send it back."),
                ChoiceAction::Open {
                    path: format!("/tasks/{id}/plan"),
                },
            ),
            cancel(offer),
        ],
        "integration" => vec![choice(
            "Open the pull request again",
            format!(
                "Catervas pushes the branch of {id} and opens its pull request again, for you to merge."
            ),
            ChoiceAction::Integrate,
        )],
        "preview" => fix_the_preview(offer),
        _ => vec![change(offer), cancel(offer)],
    };
    if let Some(first) = choices.first_mut() {
        first.suggested = true;
    }
    choices
}

/// What the task's open escalation offers: its reason, `before`, and the names, read from the log
/// and the board; `None` when nothing is open for the task.
///
/// # Errors
///
/// When the log, the board or the contract cannot be read.
pub fn offer_for(
    log: &EventLog,
    projections: &Projections,
    files: &ProjectFiles,
    team: &Team,
    task_id: &TaskId,
) -> Result<Option<Vec<Choice>>, StoreError> {
    let Some(open) = open_escalations(log, projections)?
        .into_iter()
        .find(|open| &open.task_id == task_id)
    else {
        return Ok(None);
    };
    let history = log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TaskTransitioned],
        ..EventQuery::default()
    })?;
    let before =
        last_move_into(&history, TaskStatus::Escalated).and_then(|event| match &event.body {
            EventBody::TaskTransitioned(body) => TaskStatus::from_str(&body.from.to_string()).ok(),
            _ => None,
        });
    let who = projections
        .task(task_id)?
        .and_then(|row| row.assignee_id)
        .map_or_else(|| "The team".to_string(), |id| name_of(team, &id));
    let product_manager = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
        .map_or_else(
            || "The Product Manager".to_string(),
            |agent| name_of(team, agent.id.as_str()),
        );
    let contract = files
        .read_contract(task_id)
        .map_err(|error| StoreError::Io {
            detail: error.to_string(),
        })?;
    Ok(Some(escalation_choices(&Offer {
        task_id: task_id.as_str(),
        reason: &open.reason,
        before,
        who: &who,
        product_manager: &product_manager,
        max_sessions: contract.budget.max_sessions.get(),
        max_cost_usd: contract.budget.max_cost_usd,
    })))
}

#[cfg(test)]
mod tests {
    use catervas_core::contract::TaskStatus;

    use super::{Choice, ChoiceAction, Offer, escalation_choices, resolve};

    const REASONS: [&str; 12] = [
        "iterations",
        "budget",
        "sessions",
        "blocker_age",
        "permission",
        "readiness_failures",
        "approval",
        "risk_gate",
        "integration",
        "explicit_request",
        "preview",
        "something_new",
    ];

    fn offer(reason: &str, before: Option<TaskStatus>) -> Offer<'_> {
        Offer {
            task_id: "CTV-14",
            reason,
            before,
            who: "Theo",
            product_manager: "Mira",
            max_sessions: 4,
            max_cost_usd: 5.0,
        }
    }

    fn words(choice: &Choice) -> (&str, &str) {
        (&choice.label, &choice.next)
    }

    #[test]
    fn every_reason_offers_one_suggested_choice_that_says_what_happens() {
        for reason in REASONS {
            let choices = escalation_choices(&offer(reason, Some(TaskStatus::InProgress)));
            assert!(!choices.is_empty(), "{reason}");
            assert_eq!(
                choices.iter().filter(|choice| choice.suggested).count(),
                1,
                "{reason}"
            );
            assert!(choices[0].suggested, "{reason}");
            for (at, choice) in choices.iter().enumerate() {
                assert!(!choice.next.is_empty(), "{reason}");
                assert!(choice.next.contains("CTV-14"), "{reason}: {}", choice.next);
                assert!(
                    choices[..at]
                        .iter()
                        .all(|other| other.action != choice.action),
                    "{reason}: two choices do the same"
                );
            }
        }

        let iterations = escalation_choices(&offer("iterations", Some(TaskStatus::InProgress)));
        assert_eq!(
            iterations
                .iter()
                .map(|choice| (words(choice), choice.action.clone()))
                .collect::<Vec<_>>(),
            vec![
                (
                    (
                        "Give 2 more tries",
                        "Theo picks up CTV-14 again with 2 more tries."
                    ),
                    resolve(TaskStatus::InProgress, Some(2), false)
                ),
                (
                    (
                        "Ask Mira to change the plan",
                        "Mira rewrites the plan of CTV-14; it is assigned again once it is ready."
                    ),
                    resolve(TaskStatus::Refining, None, false)
                ),
                (
                    (
                        "Cancel the task",
                        "CTV-14 stops for good. Its branch is kept, and nothing is merged."
                    ),
                    resolve(TaskStatus::Cancelled, None, false)
                ),
            ]
        );

        for reason in ["sessions", "budget"] {
            let spent = escalation_choices(&offer(reason, Some(TaskStatus::InProgress)));
            assert_eq!(spent.len(), 2, "{reason}");
            assert_eq!(
                words(&spent[0]),
                (
                    "Give it more room",
                    "Theo carries on with CTV-14, with up to 4 more sessions and $5.00 more to spend."
                )
            );
            assert_eq!(spent[0].action, resolve(TaskStatus::InProgress, None, true));
            assert_eq!(spent[1].action, resolve(TaskStatus::Cancelled, None, false));
        }

        let blocked = escalation_choices(&offer("blocker_age", Some(TaskStatus::Blocked)));
        assert_eq!(
            blocked[0].action,
            resolve(TaskStatus::InProgress, None, false)
        );

        let integration = escalation_choices(&offer("integration", None));
        assert_eq!(integration.len(), 1);
        assert_eq!(integration[0].action, ChoiceAction::Integrate);
        assert_eq!(integration[0].label, "Open the pull request again");

        for (reason, path) in [
            ("approval", "/tasks/CTV-14/plan"),
            ("preview", "/settings/your-app"),
        ] {
            let choices = escalation_choices(&offer(reason, Some(TaskStatus::InProgress)));
            assert_eq!(
                choices[0].action,
                ChoiceAction::Open {
                    path: path.to_string()
                }
            );
        }

        let refining = escalation_choices(&offer("permission", Some(TaskStatus::Refining)));
        assert!(refining.iter().all(|choice| choice.label != "Carry on"));

        let schema = item_schema();
        for reason in REASONS {
            for choice in escalation_choices(&offer(reason, Some(TaskStatus::InProgress))) {
                let wire = choice.wire("CTV-14");
                assert!(schema.is_valid(&wire), "{reason}: {wire}");
            }
        }
    }

    fn item_schema() -> jsonschema::Validator {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/schemas/rpc.schema.json"))
                .expect("the schema parses");
        let item =
            schema["$defs"]["escalationChoicesResult"]["properties"]["choices"]["items"].clone();
        jsonschema::validator_for(&item).expect("the item schema compiles")
    }
}
