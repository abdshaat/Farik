//! Each effect of a change to a team in plain words (`docs/SPEC.md` section 10), shown before the
//! change is saved.

use super::{Agent, Effort, Integration, JudgeChoice, JudgmentRequired, Team};
use crate::governor::permissions::PermissionTier;

/// The work a switch of the sprint policy touches (ADR 0028).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SprintWork<'a> {
    /// Tasks under way outside the open sprint: in `assigned`, `in_progress`, `blocked`,
    /// `verifying` or `rejected`, without the Backlog mark.
    pub under_way: u32,
    /// The titles of the Backlog's rows with no parent.
    pub in_the_backlog: Vec<&'a str>,
}

/// One sentence per effect of changing `old` into `new`, in the order: each agent's model, effort
/// and tiers, then the daily limit, integration, permissions, the plan check, and planning in
/// sprints, which `work` says what it touches.
///
/// An agent's tiers are compared under the new team's permissions, so a permission answer is said
/// once for the team rather than again for every agent. Who checks plans, and against what, says
/// nothing while no plan is checked. An agent that is new or gone is not described here.
#[must_use]
pub fn describe_change(old: &Team, new: &Team, work: &SprintWork<'_>) -> Vec<String> {
    let mut said = Vec::new();
    let permissions = new.permissions();
    for agent in &new.agents {
        let Some(before) = old.agents.iter().find(|one| one.id == agent.id) else {
            continue;
        };
        let name = agent.display_name.as_str();
        said.extend(model_change(name, before, agent));
        let (was, is) = (before.tiers(&permissions), agent.tiers(&permissions));
        for tier in is.iter().filter(|tier| !was.contains(tier)) {
            said.push(format!("{name} may now {}.", may(*tier)));
        }
        for tier in was.iter().filter(|tier| !is.contains(tier)) {
            said.push(format!("{name} may no longer {}.", may(*tier)));
        }
    }
    if old.budgets.daily_usd != new.budgets.daily_usd {
        said.push(match new.budgets.daily_usd {
            Some(usd) if usd.fract() == 0.0 => format!("The team may spend up to ${usd:.0} a day."),
            Some(usd) => format!("The team may spend up to ${usd:.2} a day."),
            None => "The team has no daily spending limit.".to_string(),
        });
    }
    if old.policy.integration != new.policy.integration {
        said.push(
            match new.policy.integration {
                Integration::AutoMerge => "Finished work is merged on its own.",
                Integration::PullRequest => "Finished work opens a pull request for you.",
                Integration::Manual => "Finished work waits for you to merge it.",
            }
            .to_string(),
        );
    }
    let (was, is) = (old.permissions(), permissions);
    if was.run_commands != is.run_commands {
        if is.run_commands {
            said.push("Developers and Architects may run commands.".to_string());
        } else {
            said.push("Developers and Architects may no longer run commands.".to_string());
            said.push(
                "Farik still runs every check itself; the agents cannot run commands.".to_string(),
            );
        }
    }
    if was.push != is.push {
        said.push(
            if is.push {
                "Developers may now push their work and open pull requests."
            } else {
                "Developers may no longer push their work or open pull requests."
            }
            .to_string(),
        );
    }
    let (was, is) = (old.judgment(), new.judgment());
    if was.required != is.required {
        said.push(
            match is.required {
                JudgmentRequired::Always => "Every plan is checked before work starts.",
                JudgmentRequired::Never => "Plans are no longer checked before work starts.",
            }
            .to_string(),
        );
    }
    if is.required == JudgmentRequired::Always {
        if was.questions != is.questions {
            let count = is.questions.len();
            let noun = if count == 1 { "question" } else { "questions" };
            said.push(format!("Plans are checked against {count} {noun}."));
        }
        if was.judge != is.judge {
            said.push(
                match is.judge {
                    JudgeChoice::Architect => "The Architect checks every plan.",
                    JudgeChoice::ScrumMaster => "The Scrum Master checks every plan.",
                    JudgeChoice::Auto => {
                        "The Architect checks every plan, or the Scrum Master when there is \
                         none, or else the Product Manager."
                    }
                }
                .to_string(),
            );
        }
    }
    said.extend(sprint_change(old, new, work));
    said
}

/// The lines of switching the sprint policy (ADR 0028), with what it does to the work at hand.
fn sprint_change(old: &Team, new: &Team, work: &SprintWork<'_>) -> Vec<String> {
    let mut said = Vec::new();
    if old.plans_in_sprints() == new.plans_in_sprints() {
        return said;
    }
    if new.plans_in_sprints() {
        said.push("Ready work now waits in the Backlog until you start a sprint.".to_string());
        match work.under_way {
            0 => {}
            1 => said.push("The task already under way finishes first.".to_string()),
            count => said.push(format!("The {count} tasks already under way finish first.")),
        }
    } else {
        said.push(
            "Ready work starts as soon as someone is free, without waiting for a sprint."
                .to_string(),
        );
        match work.in_the_backlog.split_last() {
            None => {}
            Some((only, [])) => {
                said.push(format!("{only}, waiting in the Backlog, can start now."));
            }
            Some((last, rest)) => said.push(format!(
                "The {} pieces of work in the Backlog, {} and {last}, can start now.",
                rest.len() + 1,
                rest.join(", ")
            )),
        }
        said.push("You can still start sprints from the Board.".to_string());
    }
    said
}

/// The model and effort lines of `name`, which was `before` and is `agent`, in the Team cards'
/// words.
fn model_change(name: &str, before: &Agent, agent: &Agent) -> Vec<String> {
    let mut said = Vec::new();
    let (was, is) = (model(before), model(agent));
    if was != is {
        let (from, to) = (model_words(was.as_deref()), model_words(is.as_deref()));
        said.push(if from == to {
            format!("{name} moves to another version of {to}.")
        } else {
            format!("{name}'s model changes from {from} to {to}.")
        });
    }
    if effort(before) != effort(agent) {
        said.push(match effort(agent) {
            Some(Effort::Low) => format!("{name} now works quickly."),
            Some(Effort::Medium) => format!("{name} now works in a balanced way."),
            Some(Effort::High) => format!("{name} now works carefully."),
            None => format!("{name} now works as its role usually does."),
        });
    }
    said
}

fn model(agent: &Agent) -> Option<String> {
    agent
        .model
        .as_ref()
        .and_then(|model| model.id.as_ref())
        .map(|id| id.as_str().to_string())
}

fn effort(agent: &Agent) -> Option<Effort> {
    agent.model.as_ref().and_then(|model| model.effort)
}

/// Each model family a person may choose, as its ids start, the words its card shows, and the
/// words a sentence names it by.
pub const MODEL_FAMILIES: [(&str, &str, &str); 4] = [
    (
        "claude-fable-",
        "Most capable model",
        "the most capable model",
    ),
    (
        "claude-opus-",
        "Strongest model, thinks hard",
        "the strongest model",
    ),
    ("claude-sonnet-", "Everyday model", "the everyday model"),
    ("claude-haiku-", "Quick model", "the quick model"),
];

/// The words a sentence names the model `id` by: its family's, the role's when there is none, or
/// the id itself when no family Farik names it.
fn model_words(id: Option<&str>) -> &str {
    let Some(id) = id else {
        return "the role's model";
    };
    MODEL_FAMILIES
        .iter()
        .find(|(prefix, _, _)| id.starts_with(prefix))
        .map_or(id, |(_, _, words)| words)
}

/// What a tier lets an agent do, after "may".
fn may(tier: PermissionTier) -> &'static str {
    match tier {
        PermissionTier::Read => "read the project",
        PermissionTier::WriteWorkspace => "change the files of its task",
        PermissionTier::Execute => "run commands",
        PermissionTier::Network => "use the internet",
        PermissionTier::GitLocal => "save its work on its task's branch",
        PermissionTier::GitRemote => "push its work and open pull requests",
        PermissionTier::ExternalEffect => "act outside the project, with your approval",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{SprintWork, describe_change};
    use crate::team::fixtures::{a_team_wire, an_agent_wire};
    use crate::team::{Team, validate_team};

    fn team(wire: &Value) -> Team {
        validate_team(wire).expect("the fixture is a team")
    }

    #[test]
    fn describes_the_switch() {
        let off = team(&a_team_wire());
        let mut wire = a_team_wire();
        wire["policy"]["plan_in_sprints"] = json!(true);
        let on = team(&wire);
        let none = SprintWork::default();
        let busy = SprintWork {
            under_way: 2,
            in_the_backlog: vec!["Gift cards at checkout", "Sold-out badge on the menu"],
        };
        assert_eq!(
            describe_change(&off, &on, &busy),
            [
                "Ready work now waits in the Backlog until you start a sprint.",
                "The 2 tasks already under way finish first.",
            ]
        );
        assert_eq!(
            describe_change(&off, &on, &none),
            ["Ready work now waits in the Backlog until you start a sprint."]
        );
        let one = SprintWork {
            under_way: 1,
            in_the_backlog: vec!["Gift cards at checkout"],
        };
        assert_eq!(
            describe_change(&off, &on, &one)[1],
            "The task already under way finishes first."
        );
        assert_eq!(
            describe_change(&on, &off, &one)[1],
            "Gift cards at checkout, waiting in the Backlog, can start now."
        );
        let three = SprintWork {
            under_way: 0,
            in_the_backlog: vec!["A", "B", "C"],
        };
        assert_eq!(
            describe_change(&on, &off, &three)[1],
            "The 3 pieces of work in the Backlog, A, B and C, can start now."
        );
        assert_eq!(
            describe_change(&on, &off, &busy),
            [
                "Ready work starts as soon as someone is free, without waiting for a sprint.",
                "The 2 pieces of work in the Backlog, Gift cards at checkout and Sold-out badge \
                 on the menu, can start now.",
                "You can still start sprints from the Board.",
            ],
            "the SettingsSprints mockup's lines"
        );
        assert_eq!(
            describe_change(&on, &off, &none),
            [
                "Ready work starts as soon as someone is free, without waiting for a sprint.",
                "You can still start sprints from the Board.",
            ]
        );
        assert_eq!(describe_change(&on, &on, &busy), Vec::<String>::new());
        assert_eq!(describe_change(&off, &off, &busy), Vec::<String>::new());
    }

    #[test]
    fn says_an_effort_alone_without_a_model() {
        let old = validate_team(&a_team_wire()).expect("a team");
        let mut new = a_team_wire();
        new["agents"][1]["model"] = json!({ "effort": "low" });
        let new = validate_team(&new).expect("an effort without a model is a team");
        assert_eq!(
            describe_change(&old, &new, &SprintWork::default()),
            ["linus now works quickly."]
        );
    }

    #[test]
    fn names_models_in_the_words_the_page_uses() {
        let mut old = a_team_wire();
        old["agents"][0]["model"] = json!({ "id": "claude-opus-5", "effort": "high" });
        old["agents"][1]["model"] = json!({ "id": "claude-haiku-4-5" });
        let old = team(&old);
        let mut new = serde_json::to_value(&old).expect("a team is JSON");
        new["agents"][0]["model"] = json!({ "id": "claude-opus-5-5", "effort": "medium" });
        new["agents"][1]["model"] = json!({ "id": "local-model" });
        assert_eq!(
            describe_change(&old, &team(&new), &SprintWork::default()),
            [
                "ada moves to another version of the strongest model.",
                "ada now works in a balanced way.",
                "linus's model changes from the quick model to local-model.",
            ],
            "a model of no family Farik names keeps its id"
        );
        let mut fable = serde_json::to_value(&old).expect("a team is JSON");
        fable["agents"][0]["model"] = json!({ "id": "claude-fable-5", "effort": "high" });
        assert_eq!(
            describe_change(&old, &team(&fable), &SprintWork::default()),
            ["ada's model changes from the strongest model to the most capable model."]
        );
    }

    #[test]
    fn describes_each_change_in_words() {
        let mut old = a_team_wire();
        old["agents"][0]["display_name"] = json!("Ada");
        old["agents"][1]["display_name"] = json!("Linus");
        old["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .push(an_agent_wire("kai", "architect"));
        let old = team(&old);
        assert_eq!(
            describe_change(&old, &old, &SprintWork::default()),
            Vec::<String>::new()
        );

        let mut new = serde_json::to_value(&old).expect("a team is JSON");
        new["agents"][1]["model"] = json!({ "id": "claude-sonnet-5", "effort": "high" });
        new["agents"][1]["grants"] = json!(["network"]);
        new["agents"][0]["revokes"] = json!(["network"]);
        new["budgets"]["daily_usd"] = json!(10);
        new["policy"]["integration"] = json!("auto_merge");
        new["policy"]["permissions"] = json!({ "run_commands": false, "push": true });
        new["policy"]["judgment"] = json!({ "required": "always", "judge": "architect", "questions": ["Does the task fit its budget?"] });
        let new = team(&new);
        assert_eq!(
            describe_change(&old, &new, &SprintWork::default()),
            [
                "Ada may no longer use the internet.",
                "Linus's model changes from the role's model to the everyday model.",
                "Linus now works carefully.",
                "Linus may now use the internet.",
                "The team may spend up to $10 a day.",
                "Finished work is merged on its own.",
                "Developers and Architects may no longer run commands.",
                "Farik still runs every check itself; the agents cannot run commands.",
                "Developers may now push their work and open pull requests.",
                "Every plan is checked before work starts.",
                "Plans are checked against 1 question.",
                "The Architect checks every plan.",
            ]
        );

        let mut back = serde_json::to_value(&new).expect("a team is JSON");
        back["agents"][1]["model"] = json!({ "id": "claude-sonnet-5" });
        back["agents"][0]
            .as_object_mut()
            .expect("an agent is an object")
            .remove("revokes");
        back["budgets"]["daily_usd"] = json!(20.5);
        back["policy"]["integration"] = json!("pull_request");
        back["policy"]["permissions"] = json!({ "run_commands": true, "push": false });
        back["policy"]["judgment"] = json!({ "required": "never", "judge": "scrum_master" });
        back["agents"]
            .as_array_mut()
            .expect("the agents are a list")
            .push(an_agent_wire("sol", "scrum_master"));
        let back = team(&back);
        assert_eq!(
            describe_change(&new, &back, &SprintWork::default()),
            [
                "Ada may now use the internet.",
                "Linus now works as its role usually does.",
                "The team may spend up to $20.50 a day.",
                "Finished work opens a pull request for you.",
                "Developers and Architects may run commands.",
                "Developers may no longer push their work or open pull requests.",
                "Plans are no longer checked before work starts.",
            ],
            "who checks, and by what, says nothing while nobody checks"
        );

        let mut cleared = serde_json::to_value(&back).expect("a team is JSON");
        cleared["agents"][1]
            .as_object_mut()
            .expect("an agent is an object")
            .remove("model");
        cleared["budgets"]
            .as_object_mut()
            .expect("the budgets are an object")
            .remove("daily_usd");
        cleared["policy"]["integration"] = json!("manual");
        cleared["policy"]["judgment"] = json!({ "required": "always", "judge": "auto" });
        let cleared = team(&cleared);
        assert_eq!(
            describe_change(&back, &cleared, &SprintWork::default()),
            [
                "Linus's model changes from the everyday model to the role's model.",
                "The team has no daily spending limit.",
                "Finished work waits for you to merge it.",
                "Every plan is checked before work starts.",
                "The Architect checks every plan, or the Scrum Master when there is none, or \
                 else the Product Manager.",
            ]
        );
    }
}
