//! Each effect of a change to a team in plain words (`docs/SPEC.md` section 10), shown before the
//! change is saved.

use super::{Agent, Integration, JudgeChoice, JudgmentRequired, Team};
use crate::governor::permissions::PermissionTier;

/// One sentence per effect of changing `old` into `new`, in the order: each agent's model, effort
/// and tiers, then the daily limit, integration, permissions, and the plan check.
///
/// An agent's tiers are compared under the new team's permissions, so a permission answer is said
/// once for the team rather than again for every agent. Who checks plans, and against what, says
/// nothing while no plan is checked. An agent that is new or gone is not described here.
#[must_use]
pub fn describe_change(old: &Team, new: &Team) -> Vec<String> {
    let mut said = Vec::new();
    let permissions = new.permissions();
    for agent in &new.agents {
        let Some(before) = old.agents.iter().find(|one| one.id == agent.id) else {
            continue;
        };
        let name = agent.display_name.as_str();
        if model(before) != model(agent) {
            said.push(match model(agent) {
                Some(id) => format!("{name} now uses {id}."),
                None => format!("{name} now uses the role's model."),
            });
        }
        if effort(before) != effort(agent) {
            said.push(match effort(agent) {
                Some(effort) => format!("{name} now thinks with {effort} effort."),
                None => format!("{name} now thinks with the role's effort."),
            });
        }
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
    said
}

fn model(agent: &Agent) -> Option<String> {
    agent
        .model
        .as_ref()
        .and_then(|model| model.id.as_ref())
        .map(|id| id.as_str().to_string())
}

fn effort(agent: &Agent) -> Option<String> {
    agent
        .model
        .as_ref()
        .and_then(|model| model.effort)
        .map(|effort| effort.to_string())
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

    use super::describe_change;
    use crate::team::fixtures::{a_team_wire, an_agent_wire};
    use crate::team::{Team, validate_team};

    fn team(wire: &Value) -> Team {
        validate_team(wire).expect("the fixture is a team")
    }

    #[test]
    fn says_an_effort_alone_without_a_model() {
        let old = validate_team(&a_team_wire()).expect("a team");
        let mut new = a_team_wire();
        new["agents"][1]["model"] = json!({ "effort": "low" });
        let new = validate_team(&new).expect("an effort without a model is a team");
        assert_eq!(
            describe_change(&old, &new),
            ["linus now thinks with low effort."]
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
        assert_eq!(describe_change(&old, &old), Vec::<String>::new());

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
            describe_change(&old, &new),
            [
                "Ada may no longer use the internet.",
                "Linus now uses claude-sonnet-5.",
                "Linus now thinks with high effort.",
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
            describe_change(&new, &back),
            [
                "Ada may now use the internet.",
                "Linus now thinks with the role's effort.",
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
            describe_change(&back, &cleared),
            [
                "Linus now uses the role's model.",
                "The team has no daily spending limit.",
                "Finished work waits for you to merge it.",
                "Every plan is checked before work starts.",
                "The Architect checks every plan, or the Scrum Master when there is none, or \
                 else the Product Manager.",
            ]
        );
    }
}
