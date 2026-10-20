//! `catervas_install_system_packages` (ADR 0081): an agent that holds `execute` adds Debian packages
//! to the team's `sandbox.packages`, and its task's sandbox is made again with them.

use catervas_core::team::{Team, validate_team};
use catervas_protocol::event::{EventBody, TeamUpdatedBody};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Call, ToolError, failed};
use crate::channel::post_system;

/// The most packages one call adds, and the most a team lists in all.
const MOST_PER_CALL: usize = 10;
const MOST_IN_ALL: usize = 40;

/// The most characters of the reason.
const WHY_CHARS: usize = 300;

/// `catervas_install_system_packages`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallInput {
    /// Debian package names, 1 to 10: lower-case letters, digits, `+`, `.` and `-`, starting with a
    /// letter or digit.
    packages: Vec<String>,
    /// Why the task needs them, 1 to 300 characters; the owner reads it in the channel.
    why: String,
}

/// Whether `name` is a Debian package name as the team file takes it.
fn is_a_package(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '+' | '.' | '-')
        })
}

fn invalid(detail: &str) -> ToolError {
    ToolError::InvalidInput {
        detail: detail.to_string(),
    }
}

/// `team` with `names` listed in its sandbox besides the packages it lists already, or why it is
/// not a team then.
fn team_with(team: &Team, names: &[String]) -> Result<Team, ToolError> {
    let mut all = team.sandbox_packages();
    for name in names {
        if !all.contains(name) {
            all.push(name.clone());
        }
    }
    let mut wire = serde_json::to_value(team).map_err(failed)?;
    wire["sandbox"] = json!({ "packages": all });
    validate_team(&wire).map_err(|errors| ToolError::Failed {
        detail: format!(
            "the team with the packages is not a team: {}",
            errors
                .iter()
                .map(|error| format!("{}: {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    })
}

/// Adds the packages not yet listed to the team file, makes the task's sandbox again with them,
/// and tells the channel. The sandbox is made first, so that a name that cannot be installed
/// is not left in the team file for every task to trip on.
pub(super) async fn install(call: &Call<'_>, input: InstallInput) -> Result<Value, ToolError> {
    let why = input.why.trim();
    if input.packages.is_empty() || input.packages.len() > MOST_PER_CALL {
        return Err(invalid("packages: give from 1 to 10 package names"));
    }
    if why.is_empty() || why.chars().count() > WHY_CHARS {
        return Err(invalid("why: give a reason of 1 to 300 characters"));
    }
    if let Some(name) = input.packages.iter().find(|name| !is_a_package(name)) {
        return Err(invalid(&format!(
            "packages: {name} is not a package name (lower-case letters, digits, + . and -)"
        )));
    }
    let task = call.task()?.clone();
    let daemon = call
        .context
        .daemon
        .upgrade()
        .ok_or_else(|| ToolError::Failed {
            detail: "the daemon behind this session is gone".to_string(),
        })?;
    let deps = call.deps();
    let listed = deps.files.read_team().map_err(failed)?.sandbox_packages();
    let mut added: Vec<String> = Vec::new();
    for name in &input.packages {
        if !listed.contains(name) && !added.contains(name) {
            added.push(name.clone());
        }
    }
    if added.is_empty() {
        return Ok(json!({ "added": [], "packages": listed }));
    }
    if listed.len() + added.len() > MOST_IN_ALL {
        return Err(invalid(&format!(
            "packages: the team lists {} and holds at most {MOST_IN_ALL}",
            listed.len()
        )));
    }
    // The sandbox first, off the async workers: a build takes about a minute.
    let rebuilt = team_with(&call.team, &added)?;
    let made = {
        let (daemon, task, team) = (daemon.clone(), task.clone(), rebuilt);
        tokio::task::spawn_blocking(move || daemon.recreate_sandbox(&task, &team))
            .await
            .map_err(failed)?
    };
    if let Some(Err(error)) = made {
        return Err(failed(error));
    }
    // Then the team file, as it is now, under the lock every write of it holds.
    let packages = {
        let _writing = daemon.team_writes();
        let team = team_with(&deps.files.read_team().map_err(failed)?, &added)?;
        deps.files.write_team(&team).map_err(failed)?;
        call.append(
            None,
            EventBody::TeamUpdated(TeamUpdatedBody {
                team_name: team.name.to_string(),
                agent_ids: team
                    .agents
                    .iter()
                    .map(|agent| agent.id.to_string())
                    .collect(),
                updated_by: call.agent_id().to_string(),
                template: None,
                plan_in_sprints: None,
            }),
        )?;
        team.sandbox_packages()
    };
    post_system(
        &deps.log,
        deps.clock.as_ref(),
        &call.ids(Some(&task)),
        Some(task.clone()),
        &format!(
            "{} added {} to the sandbox: {why}",
            call.agent.display_name.as_str(),
            added.join(", ")
        ),
    )
    .map_err(failed)?;
    Ok(json!({ "added": added, "packages": packages }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::json;

    use crate::daemon::DaemonState;
    use crate::orchestrator::fixtures::CountingSandboxFactory;
    use crate::sandbox::SandboxFactory;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three, run};

    const TOOL: &str = "catervas_install_system_packages";

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn installs_a_system_package_for_the_task() {
        let project = TestProject::new(
            "tools-install-packages",
            &a_team_of_three(|wire| wire["agents"][1]["display_name"] = json!("Theo")),
        );
        project.filed("CTV-1", "in_progress", "task", None);
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let daemon = Arc::new(DaemonState::new(Arc::clone(&project.deps)));
        let (factory, root) = (Arc::clone(&sandboxes), project.repo.path.clone());
        assert!(daemon.set_sandbox_recreator(Arc::new(move |task, team| {
            factory
                .create("p", task, &root, false, &[], &team.sandbox_packages())
                .map(|_| ())
        })));
        let mut context = project.context("dev-a", Some("CTV-1"));
        context.daemon = Arc::downgrade(&daemon);

        let answer = run(
            &context,
            TOOL,
            json!({ "packages": ["ffmpeg"], "why": "CTV-19 reads video metadata" }),
        )
        .expect("the package is added");
        assert_eq!(answer["added"], json!(["ffmpeg"]), "{answer}");
        let team = project.deps.files.read_team().expect("the team");
        assert_eq!(team.sandbox_packages(), ["ffmpeg"]);
        let updated = project.events(&[EventKind::TeamUpdated]);
        let EventBody::TeamUpdated(body) = &updated.last().expect("a team.updated").body else {
            panic!("a team.updated");
        };
        assert_eq!(body.updated_by, "dev-a");
        let said: Vec<String> = project
            .events(&[EventKind::MessagePosted])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::MessagePosted(body) => Some(body.text.clone()),
                _ => None,
            })
            .collect();
        assert!(
            said.contains(
                &"Theo added ffmpeg to the sandbox: CTV-19 reads video metadata".to_string()
            ),
            "{said:?}"
        );
        assert_eq!(sandboxes.packages("CTV-1"), [vec!["ffmpeg".to_string()]]);

        // A package already listed changes nothing and builds nothing.
        let events = project.event_count();
        let again = run(
            &context,
            TOOL,
            json!({ "packages": ["ffmpeg"], "why": "again" }),
        )
        .expect("listed already");
        assert_eq!(again["added"], json!([]), "{again}");
        assert_eq!(project.event_count(), events);
        assert_eq!(sandboxes.packages("CTV-1").len(), 1);

        // A name outside the pattern is refused, and a role without `execute` has no such tool.
        let refused = run(
            &context,
            TOOL,
            json!({ "packages": ["rm -rf /"], "why": "x" }),
        );
        assert!(
            matches!(refused, Err(ToolError::InvalidInput { .. })),
            "{refused:?}"
        );
        match project.call("pm", None, TOOL, json!({ "packages": ["vim"], "why": "x" })) {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with("tier_not_granted"), "{reason}");
            }
            other => panic!("the Product Manager does not execute: {other:?}"),
        }
        assert_eq!(
            project
                .deps
                .files
                .read_team()
                .expect("team")
                .sandbox_packages(),
            ["ffmpeg"]
        );
    }
}
