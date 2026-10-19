//! Each effect of a change to a team in plain words (`docs/SPEC.md` section 10), shown before the
//! change is saved.

use super::{Agent, Effort, Integration, Team};
use crate::governor::permissions::PermissionTier;

/// One sentence per effect of changing `old` into `new`, in the order: each agent's model, effort
/// and tiers, then the daily limit, integration, and permissions.
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
                Integration::PullRequest | Integration::AutoMerge | Integration::Manual => {
                    "Finished work opens a pull request for you to merge."
                }
            }
            .to_string(),
        );
    }
    if old.merge_when_checks_pass() != new.merge_when_checks_pass() {
        said.push(
            if new.merge_when_checks_pass() {
                "Catervas merges finished work once GitHub's checks pass."
            } else {
                "Finished work opens a pull request for you to merge."
            }
            .to_string(),
        );
    }
    if old.policy.integration_branch != new.policy.integration_branch {
        said.push(match &new.policy.integration_branch {
            Some(branch) => format!("Finished work goes into {}.", branch.as_str()),
            None => "Finished work goes into the repository's main branch.".to_string(),
        });
    }
    let (was, is) = (old.permissions(), permissions);
    if was.run_commands != is.run_commands {
        if is.run_commands {
            said.push("Developers and Architects may run commands.".to_string());
        } else {
            said.push("Developers and Architects may no longer run commands.".to_string());
            said.push(
                "Catervas still runs every check itself; the agents cannot run commands."
                    .to_string(),
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
            Some(Effort::Low) => format!("{name} now works at low effort."),
            Some(Effort::Medium) => format!("{name} now works at medium effort."),
            Some(Effort::High) => format!("{name} now works at high effort."),
            Some(Effort::Xhigh) => format!("{name} now thinks longer."),
            Some(Effort::Max) => format!("{name} now thinks as long as it can."),
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

/// Each model family a person may choose, as its ids start and the word Anthropic names it by.
pub const MODEL_FAMILIES: [(&str, &str); 4] = [
    ("claude-fable-", "Fable"),
    ("claude-opus-", "Opus"),
    ("claude-sonnet-", "Sonnet"),
    ("claude-haiku-", "Haiku"),
];

/// A model as Anthropic names it: its family's word and the version numbers of its id joined by
/// dots ("claude-opus-5-5" is "Opus 5.5"). A part of a thousand or more is a date, not a version.
/// `None` for an id no family names.
#[must_use]
pub fn model_name(id: &str) -> Option<String> {
    let (rest, word) = MODEL_FAMILIES
        .iter()
        .find_map(|(prefix, word)| id.strip_prefix(prefix).map(|rest| (rest, *word)))?;
    let version: Vec<String> = rest
        .split('-')
        .map_while(|part| part.parse::<u32>().ok().filter(|n| *n < 1000))
        .map(|n| n.to_string())
        .collect();
    Some(if version.is_empty() {
        word.to_string()
    } else {
        format!("{word} {}", version.join("."))
    })
}

/// The words a sentence names the model `id` by: its name, the role's when there is none, or the
/// id itself when no family Catervas names it.
fn model_words(id: Option<&str>) -> String {
    match id {
        None => "the role's model".to_string(),
        Some(id) => model_name(id).unwrap_or_else(|| id.to_string()),
    }
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

    use super::{describe_change, model_name};
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
            ["linus now works at low effort."]
        );
        for (wire, said) in [
            ("medium", "linus now works at medium effort."),
            ("high", "linus now works at high effort."),
            ("xhigh", "linus now thinks longer."),
            ("max", "linus now thinks as long as it can."),
        ] {
            let mut to = a_team_wire();
            to["agents"][1]["model"] = json!({ "effort": wire });
            assert_eq!(describe_change(&old, &team(&to)), [said]);
        }
    }

    #[test]
    fn names_a_model_by_its_family_and_version() {
        assert_eq!(model_name("claude-opus-5-5").as_deref(), Some("Opus 5.5"));
        assert_eq!(model_name("claude-fable-5-1").as_deref(), Some("Fable 5.1"));
        assert_eq!(model_name("claude-sonnet-5").as_deref(), Some("Sonnet 5"));
        assert_eq!(
            model_name("claude-haiku-4-5-20251001").as_deref(),
            Some("Haiku 4.5"),
            "a date is not a version"
        );
        assert_eq!(model_name("local-model"), None);
    }

    #[test]
    fn names_models_in_the_words_the_page_uses() {
        let mut old = a_team_wire();
        old["agents"][0]["model"] = json!({ "id": "claude-opus-5", "effort": "high" });
        old["agents"][1]["model"] = json!({ "id": "claude-haiku-4-5-20251001" });
        let old = team(&old);
        let mut new = serde_json::to_value(&old).expect("a team is JSON");
        new["agents"][0]["model"] = json!({ "id": "claude-opus-5-5", "effort": "medium" });
        new["agents"][1]["model"] = json!({ "id": "local-model" });
        assert_eq!(
            describe_change(&old, &team(&new)),
            [
                "ada's model changes from Opus 5 to Opus 5.5.",
                "ada now works at medium effort.",
                "linus's model changes from Haiku 4.5 to local-model.",
            ],
            "a model of no family Catervas names keeps its id"
        );
        let mut fable = serde_json::to_value(&old).expect("a team is JSON");
        fable["agents"][0]["model"] = json!({ "id": "claude-fable-5", "effort": "high" });
        assert_eq!(
            describe_change(&old, &team(&fable)),
            ["ada's model changes from Opus 5 to Fable 5."]
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
        // Every team integrates by pull request (ADR 0061): a change of the answer says nothing.
        new["policy"]["integration"] = json!("auto_merge");
        new["policy"]["permissions"] = json!({ "run_commands": false, "push": true });
        let new = team(&new);
        assert_eq!(
            describe_change(&old, &new),
            [
                "Ada may no longer use the internet.",
                "Linus's model changes from the role's model to Sonnet 5.",
                "Linus now works at high effort.",
                "The team may spend up to $10 a day.",
                "Developers and Architects may no longer run commands.",
                "Catervas still runs every check itself; the agents cannot run commands.",
                "Developers may now push their work and open pull requests.",
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
        back["agents"]
            .as_array_mut()
            .expect("the agents are a list")
            .push(an_agent_wire("sol", "scrum_master"));
        let back = team(&back);
        assert_eq!(
            describe_change(&new, &back),
            [
                "Ada may now use the internet.",
                "Linus now works as its role usually does.",
                "The team may spend up to $20.50 a day.",
                "Developers and Architects may run commands.",
                "Developers may no longer push their work or open pull requests.",
            ]
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
        let cleared = team(&cleared);
        assert_eq!(
            describe_change(&back, &cleared),
            [
                "Linus's model changes from Sonnet 5 to the role's model.",
                "The team has no daily spending limit.",
            ]
        );
    }
}
