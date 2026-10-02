//! `farik skill` (`docs/SPEC.md` 6.7, ADR 0034): a skill is instructions an agent follows, so it
//! is shown whole before it is added, and added, confirmed or removed through the command the
//! process driving the project handles, or here when nothing drives.

use std::collections::BTreeMap;
use std::io::{BufRead as _, Write as _};
use std::path::Path;

use farik_core::skill::skill_sha256;
use farik_protocol::command::{Command, SkillScope};
use farik_protocol::event::EventKind;
use farik_roles::{
    CheckedSkill, SkillRefusal, check_skill, core_skill_names, declared_name_and_description,
};
use farik_runtime::skills::{
    SkillLevel, SkillState, confirm_skill, confirmed_sentence, confirmed_skills, read_skill_folder,
    remove_skill, removed_sentence, save_skill, saved_sentence, skill_folder, skill_rows,
};
use farik_store::EventQuery;
use serde_json::json;

use crate::printable::printable;
use crate::project::{Project, tool_deps};
use crate::{CliIo, Report, here_or_sent};

/// Whose skill a command is about, as `--team` or `--agent <id>` said.
pub(crate) enum Whom<'a> {
    /// The team's.
    Team,
    /// One agent's.
    Agent(&'a str),
}

impl Whom<'_> {
    fn level(&self) -> SkillLevel {
        match self {
            Self::Team => SkillLevel::Team,
            Self::Agent(agent) => SkillLevel::Agent((*agent).to_string()),
        }
    }

    fn scope(&self) -> SkillScope {
        match self {
            Self::Team => SkillScope::Team,
            Self::Agent(agent) => SkillScope::Agent((*agent).to_string()),
        }
    }

    /// Refuses an agent the team does not have.
    fn exists(&self, project: &Project) -> Result<(), String> {
        match self {
            Self::Agent(agent)
                if !project
                    .team
                    .agents
                    .iter()
                    .any(|held| held.id.as_str() == *agent) =>
            {
                Err(format!("the team has no agent {agent}"))
            }
            _ => Ok(()),
        }
    }
}

/// A state as a person reads it.
fn state_words(state: SkillState) -> &'static str {
    match state {
        SkillState::InUse => "in use",
        SkillState::Replaced => "replaced",
        SkillState::Review => "review",
        SkillState::Missing => "missing",
    }
}

/// `farik skill list [--agent <id>]`: one line per skill, `<level>  <name>  <state>`: with an
/// agent its role's, the team's and its own, in `skill_rows`' order; without, the team's alone.
///
/// # Errors
///
/// A sentence saying the agent is not on the team, or the log could not be read.
pub(crate) fn list(project: &Project, agent: Option<&str>) -> Result<Report, String> {
    if let Some(agent) = agent {
        Whom::Agent(agent).exists(project)?;
    }
    let events = project
        .log
        .read(&EventQuery {
            kinds: vec![
                EventKind::SkillAdded,
                EventKind::SkillChanged,
                EventKind::SkillRemoved,
                EventKind::SkillConfirmed,
            ],
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?;
    let rows = skill_rows(
        &project.root,
        &project.team,
        agent,
        &confirmed_skills(&events),
    );
    let level = |row: &farik_runtime::skills::SkillRow| {
        serde_json::to_value(row.level)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    Ok(Report {
        lines: rows
            .iter()
            .map(|row| {
                format!(
                    "{}  {}  {}",
                    level(row),
                    printable(&row.name),
                    state_words(row.state)
                )
            })
            .collect(),
        json: json!({ "skills": rows }),
        json_lines: None,
    })
}

/// A folder's files, read as `read_skill_folder` reads them, and held to `check_skill` under the
/// name its `SKILL.md` gives it.
fn read_checked(folder: &Path) -> Result<(BTreeMap<String, Vec<u8>>, CheckedSkill), String> {
    let refused = |refusal: SkillRefusal| refusal.to_string();
    let files = read_skill_folder(folder).map_err(refused)?;
    let (name, _) = declared_name_and_description(&files)
        .ok_or_else(|| refused(SkillRefusal::FrontmatterInvalid))?;
    let checked = check_skill(&name, &files).map_err(refused)?;
    Ok((files, checked))
}

/// What `show` and `add` print: the skill's name and description, each file's name and size, what
/// Farik ignores in its frontmatter, every file escaped, and the hash.
fn shown(
    files: &BTreeMap<String, Vec<u8>>,
    checked: &CheckedSkill,
    whom: &Whom<'_>,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Skill {} for {}",
        printable(&checked.name),
        whom.level().whom()
    )];
    lines.push(format!("Description: {}", printable(&checked.description)));
    for (path, bytes) in files {
        lines.push(format!("  {path}  {} bytes", bytes.len()));
    }
    if !checked.ignored_fields.is_empty() {
        lines.push(format!(
            "Farik ignores: {}",
            printable(&checked.ignored_fields.join(", "))
        ));
    }
    for (path, bytes) in files {
        lines.push(String::new());
        lines.push(format!("--- {path} ---"));
        lines.push(
            printable(&String::from_utf8_lossy(bytes))
                .trim_end_matches('\n')
                .to_string(),
        );
    }
    lines.push(String::new());
    lines.push(format!("Hash: {}", skill_sha256(files)));
    lines
}

/// `farik skill show <name> (--team | --agent <id>)`: every file as the folder holds it, escaped,
/// and the hash.
///
/// # Errors
///
/// A sentence saying there is no such skill, or the refusal of its folder.
pub(crate) fn show(project: &Project, name: &str, whom: &Whom<'_>) -> Result<Report, String> {
    let folder = skill_folder(&project.root, &whom.level(), name);
    if std::fs::symlink_metadata(&folder).is_err() {
        return Err(format!("{} has no skill {name}", whom.level().whom()));
    }
    let (files, checked) = read_checked(&folder)?;
    Ok(Report {
        lines: shown(&files, &checked, whom),
        json: json!({
            "files": files
                .iter()
                .map(|(path, bytes)| (path.clone(), String::from_utf8_lossy(bytes).into_owned()))
                .collect::<BTreeMap<_, _>>(),
            "sha256": skill_sha256(&files),
            "ignored_fields": checked.ignored_fields,
        }),
        json_lines: None,
    })
}

/// The sentence for a shipped skill's name used without `--replace`.
fn shipped_sentence(name: &str) -> String {
    format!(
        "{name} is the name of a skill Farik ships. Add --replace to give {name} in its place, \
         which agents then load when they use it"
    )
}

/// `farik skill add <folder> (--team | --agent <id>) [--yes] [--replace]`: the folder read and
/// checked in this process and shown whole; then, at a terminal, asked about; then added.
///
/// # Errors
///
/// A sentence saying the folder is refused, the agent is not on the team, a shipped name needs
/// `--replace`, there is no terminal to ask on and no `--yes`, or the command's refusal.
pub(crate) fn add(
    project: &Project,
    folder: &Path,
    whom: &Whom<'_>,
    yes: bool,
    replace: bool,
    io: &mut CliIo<'_>,
) -> Result<Report, String> {
    whom.exists(project)?;
    let source = io.cwd.join(folder);
    let (files, checked) = read_checked(&source)?;
    let name = checked.name.clone();
    if !replace && core_skill_names().contains(name.as_str()) {
        return Err(shipped_sentence(&name));
    }
    if !yes && !io.stdin_is_terminal {
        return Err(format!(
            "farik skill add shows {name} and asks before it is added, and there is no terminal \
             here to ask on: read it with farik skill show, then give --yes"
        ));
    }
    let mut report_lines = Vec::new();
    if yes {
        report_lines = shown(&files, &checked, whom);
    } else {
        for line in shown(&files, &checked, whom) {
            writeln!(io.stdout, "{line}").map_err(|error| error.to_string())?;
        }
        write!(
            io.stdout,
            "\nAdd {name} for {}? [y/N] ",
            whom.level().whom()
        )
        .and_then(|()| io.stdout.flush())
        .map_err(|error| error.to_string())?;
        let mut answer = String::new();
        std::io::BufReader::new(&mut io.stdin)
            .read_line(&mut answer)
            .map_err(|error| format!("the answer could not be read: {error}"))?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Ok(Report {
                lines: vec![format!("\nNot added: {name} was left as it was.")],
                json: json!({ "added": false }),
                json_lines: None,
            });
        }
        writeln!(io.stdout).map_err(|error| error.to_string())?;
    }
    let level = whom.level();
    let said = here_or_sent(
        project,
        || {
            let deps = tool_deps(project, io)?;
            let saved =
                save_skill(&deps, &level, &files, replace).map_err(|error| error.to_string())?;
            Ok(done(&saved_sentence(&saved, &level), &[saved.event]))
        },
        || {
            Ok(Command::SkillSave {
                scope: whom.scope(),
                files: files
                    .iter()
                    .map(|(path, bytes)| {
                        (path.clone(), String::from_utf8_lossy(bytes).into_owned())
                    })
                    .collect(),
                replace_shipped: replace,
            })
        },
    )?;
    report_lines.extend(said.lines);
    Ok(Report {
        lines: report_lines,
        json: said.json,
        json_lines: None,
    })
}

/// A command's outcome as the command line prints it.
fn done(said: &str, events: &[u64]) -> Report {
    Report {
        lines: vec![said.to_string()],
        json: json!({ "said": said, "events": events }),
        json_lines: None,
    }
}

/// `farik skill remove <name> (--team | --agent <id>)`.
///
/// # Errors
///
/// A sentence saying the agent is not on the team, or the command's refusal.
pub(crate) fn remove(
    project: &Project,
    name: &str,
    whom: &Whom<'_>,
    io: &CliIo<'_>,
) -> Result<Report, String> {
    whom.exists(project)?;
    let level = whom.level();
    here_or_sent(
        project,
        || {
            let deps = tool_deps(project, io)?;
            let event = remove_skill(&deps, &level, name).map_err(|error| error.to_string())?;
            Ok(done(&removed_sentence(name, &level), &[event]))
        },
        || {
            Ok(Command::SkillRemove {
                scope: whom.scope(),
                name: name.to_string(),
            })
        },
    )
}

/// `farik skill confirm <name> (--team | --agent <id>) <hash> [--replace]`: the skill confirmed as
/// its folder is now, given the hash `show` printed: all of it, or its first twelve digits.
///
/// # Errors
///
/// A sentence saying the hash is not one the folder has, a shipped name needs `--replace`, or the
/// command's refusal.
pub(crate) fn confirm(
    project: &Project,
    name: &str,
    whom: &Whom<'_>,
    given: &str,
    replace: bool,
    io: &CliIo<'_>,
) -> Result<Report, String> {
    whom.exists(project)?;
    let level = whom.level();
    let given = given.to_ascii_lowercase();
    let files = read_skill_folder(&skill_folder(&project.root, &level, name))
        .map_err(|refusal| refusal.to_string())?;
    let hash = skill_sha256(&files);
    let is_whole = given.len() == 64 && given == hash;
    let is_prefix = given.len() == 12 && hash.starts_with(&given);
    if !is_whole && !is_prefix {
        return Err(format!(
            "{given} is not the hash of {name} as it is now: give all of the Hash farik skill \
             show prints, or its first 12 digits"
        ));
    }
    if !replace && core_skill_names().contains(name) {
        return Err(shipped_sentence(name));
    }
    here_or_sent(
        project,
        || {
            let deps = tool_deps(project, io)?;
            let event = confirm_skill(&deps, &level, name, &hash, replace)
                .map_err(|error| error.to_string())?;
            Ok(done(&confirmed_sentence(name, &level), &[event]))
        },
        || {
            Ok(Command::SkillConfirm {
                scope: whom.scope(),
                name: name.to_string(),
                sha256: hash.clone(),
                replace_shipped: replace,
            })
        },
    )
}
