//! Taking the team to another project (ADR 0053): what the project being left leaves for the one
//! taken on, so that its team, pictures, pinned skills' folders, sandbox setting, connector keys and
//! mailbox are there when it opens. Criteria are not: the new repository keeps the library its own
//! `init` seeded from its scan. Nothing in the old project is changed.

use std::path::Path;

use catervas_core::team::carried_team;
use catervas_protocol::event::EventBody;
use catervas_protocol::generated::event::TeamUpdatedBody;
use catervas_runtime::connectors::{ConnectorSecrets, copy_keys, write_keys_copied};
use catervas_runtime::procurement::{carry_mailbox, mailbox_connected};
use catervas_runtime::skills::{SkillLevel, skill_folder, skill_folder_unlinked};
use catervas_store::files::ProjectFiles;
use chrono::{DateTime, Utc};

use crate::HUMAN;
use crate::project::Project;

/// Where the mailbox's files are kept, which a carry that fails half way removes again.
const MAIL: &str = ".catervas/local/procurement/mail";

/// Carries the team of the project at `from` to `to`, a project `init` has just made: the team
/// without its retired agents, each other agent active, then the files and keys that go with it,
/// recorded as `team.updated` (and `mailbox.connected` when the mailbox came).
///
/// # Errors
///
/// A sentence saying what could not be carried.
pub(crate) fn carry(
    from: &Path,
    to: &Project,
    secrets: &dyn ConnectorSecrets,
    state: Option<&Path>,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let old = ProjectFiles::open(from.to_path_buf());
    let team = old.read_team().map_err(|error| error.to_string())?;
    let team = carried_team(&team).map_err(|errors| {
        let said: Vec<String> = errors.into_iter().map(|error| error.message).collect();
        format!("the team cannot be carried: {}", said.join("; "))
    })?;
    to.files
        .write_team(&team)
        .map_err(|error| error.to_string())?;
    to.files
        .write_settings(&old.read_settings().map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    copy_tree(
        &from.join(".catervas/team/avatars"),
        &to.root.join(".catervas/team/avatars"),
    )?;
    for pin in team.skills() {
        copy_skill(from, &to.root, &SkillLevel::Team, pin.name.as_str())?;
    }
    for agent in &team.agents {
        for pin in &agent.skills {
            let level = SkillLevel::Agent(agent.id.to_string());
            copy_skill(from, &to.root, &level, pin.name.as_str())?;
        }
    }

    let copied = match state {
        Some(state) => copy_keys(secrets, state, from, &to.root, &team)
            .map_err(|error| format!("the connector keys cannot be copied: {error}"))?,
        None => Vec::new(),
    };
    // The address is checked as the event is built, so that a mailbox that cannot be recorded is
    // taken out again and said, not left half moved.
    let mailbox = if copied
        .iter()
        .any(|at| at.agent_id.is_empty() && at.server == "procurement")
    {
        carry_mailbox(from, &to.root)
            .map_err(|error| error.to_string())
            .and_then(|address| {
                address
                    .map(|address| mailbox_connected(&address))
                    .transpose()
            })
            .map_err(|why| {
                let _ = std::fs::remove_dir_all(to.root.join(MAIL));
                format!("the mailbox could not be carried: {why}")
            })?
    } else {
        None
    };
    write_keys_copied(&to.root, from, &copied)
        .map_err(|error| format!("the copied keys cannot be noted: {error}"))?;

    let updated = EventBody::TeamUpdated(TeamUpdatedBody {
        agent_ids: team.agents.iter().map(|a| a.id.to_string()).collect(),
        team_name: team.name.to_string(),
        updated_by: HUMAN.to_string(),
        template: None,
        plan_in_sprints: Some(team.plans_in_sprints()),
    });
    for body in std::iter::once(updated).chain(mailbox) {
        let event = to.event(body, now, None)?;
        to.append(&event)?;
    }
    Ok(())
}

/// Copies one pinned skill's folder, unless the old one is not there or is, or lies under, a link.
fn copy_skill(from: &Path, to: &Path, level: &SkillLevel, name: &str) -> Result<(), String> {
    match skill_folder_unlinked(from, level, name) {
        Ok(source) => copy_tree(&source, &skill_folder(to, level, name)),
        Err(_) => Ok(()),
    }
}

/// Copies the folder `source` to `target`, files and folders alone: a link anywhere in it is left
/// behind, never followed. Nothing is copied from a `source` that is not there.
fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    let unmade = |error: std::io::Error| format!("{} cannot be copied: {error}", source.display());
    let Ok(meta) = std::fs::symlink_metadata(source) else {
        return Ok(());
    };
    if !meta.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(target).map_err(unmade)?;
    for entry in std::fs::read_dir(source).map_err(unmade)? {
        let entry = entry.map_err(unmade)?;
        let (path, kind) = (entry.path(), entry.file_type().map_err(unmade)?);
        let there = target.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&path, &there)?;
        } else if kind.is_file() {
            std::fs::copy(&path, &there).map_err(unmade)?;
        }
    }
    Ok(())
}
