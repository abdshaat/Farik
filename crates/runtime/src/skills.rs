//! Skills an agent is given beyond its role's (`docs/SPEC.md` 6.7, ADR 0034): reading a folder,
//! what this computer has confirmed, and what a session loads.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read as _};
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

pub use crate::session::SessionSkill;
use crate::tools::ToolDeps;
use catervas_core::skill::skill_sha256;
use catervas_core::team::{SkillPin, Team};
use catervas_protocol::event::{CatervasEvent, EventBody};
use catervas_roles::{
    CheckedSkill, SHIPPED_ROLES, SkillRefusal, check_skill, core_skill_names,
    declared_name_and_description, shipped_skill_names,
};

/// Whose skill: the team's, or one agent's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillLevel {
    /// `.catervas/skills/<name>/`.
    Team,
    /// `.catervas/agents/<agent id>/skills/<name>/`.
    Agent(String),
}

impl SkillLevel {
    /// Who it is said to be for in a sentence: "the team", or the agent's id.
    #[must_use]
    pub fn whom(&self) -> String {
        match self {
            Self::Team => "the team".to_string(),
            Self::Agent(agent) => agent.clone(),
        }
    }
}

/// What a save says it did, as the command and the command line both say it.
#[must_use]
pub fn saved_sentence(saved: &SkillSaved, level: &SkillLevel) -> String {
    format!(
        "{} {} for {}.",
        if saved.changed { "Updated" } else { "Added" },
        saved.name,
        level.whom()
    )
}

/// What a removal says it did.
#[must_use]
pub fn removed_sentence(name: &str, level: &SkillLevel) -> String {
    format!("Removed {name} for {}.", level.whom())
}

/// What a confirmation says it did.
#[must_use]
pub fn confirmed_sentence(name: &str, level: &SkillLevel) -> String {
    format!("Confirmed {name} for {}.", level.whom())
}

/// Where a skill's folder is, in the project at `root`.
#[must_use]
pub fn skill_folder(root: &Path, level: &SkillLevel, name: &str) -> PathBuf {
    let catervas = root.join(".catervas");
    match level {
        SkillLevel::Team => catervas.join("skills").join(name),
        SkillLevel::Agent(agent) => catervas
            .join("agents")
            .join(agent)
            .join("skills")
            .join(name),
    }
}

/// `skill_folder`, refused `PathInvalid` when the folder or any folder between the project's root
/// and it (`.catervas`, `skills` / `agents`, `<agent>`, `skills`) is a link: a clone can commit links,
/// and Catervas reads, writes and deletes here.
///
/// # Errors
///
/// `PathInvalid`, naming the first link.
pub fn skill_folder_unlinked(
    root: &Path,
    level: &SkillLevel,
    name: &str,
) -> Result<PathBuf, SkillRefusal> {
    let folder = skill_folder(root, level, name);
    let mut at = root.to_path_buf();
    for part in folder.strip_prefix(root).unwrap_or(&folder).components() {
        at.push(part);
        if std::fs::symlink_metadata(&at).is_ok_and(|meta| meta.file_type().is_symlink()) {
            let shown = at.strip_prefix(root).unwrap_or(&at).display().to_string();
            return Err(SkillRefusal::PathInvalid(shown));
        }
    }
    Ok(folder)
}

/// Where a project's sessions' plugin folders are written, in the user's state folder `state`:
/// `skills/<local project id>` (ADR 0034, in the place ADR 0030 keeps a connector's folder).
#[must_use]
pub fn skills_dir(state: &Path, project_id: &str) -> PathBuf {
    state.join("skills").join(project_id)
}

/// The limits `check_skill` holds a folder to, which the walk applies as it goes so that a folder
/// of a million files, or a file of a gigabyte, is not read before it is refused.
const FILE_MAX: u64 = 64 * 1024;
const TOTAL_MAX: u64 = 256 * 1024;
const FILES_MAX: usize = 16;
const PARTS_MAX: usize = 3;

/// Every file of a skill folder, by path relative to it. A folder that is not there is empty.
///
/// # Errors
///
/// A refusal of the folder's shape, found as the folder is walked: a link, or anything but a plain
/// file and folder, or a path of more than three parts (`PathInvalid`); a file past 64 KiB or a
/// folder past 256 KiB (`TooLarge`, before the file is read whole); more than 16 files
/// (`TooManyFiles`). `check_skill` holds the rest.
pub fn read_skill_folder(folder: &Path) -> Result<BTreeMap<String, Vec<u8>>, SkillRefusal> {
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    let mut pending = vec![(folder.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound && prefix.is_empty() => {
                return Ok(files);
            }
            Err(_) => return Err(SkillRefusal::PathInvalid(prefix)),
        };
        for entry in entries {
            let entry = entry.map_err(|_| SkillRefusal::PathInvalid(prefix.clone()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = format!("{prefix}{name}");
            let metadata = std::fs::symlink_metadata(entry.path())
                .map_err(|_| SkillRefusal::PathInvalid(relative.clone()))?;
            if relative.split('/').count() > PARTS_MAX {
                return Err(SkillRefusal::PathInvalid(relative));
            }
            if metadata.is_dir() {
                pending.push((entry.path(), format!("{relative}/")));
            } else if metadata.is_file() {
                if metadata.len() > FILE_MAX {
                    return Err(SkillRefusal::TooLarge);
                }
                if files.len() >= FILES_MAX {
                    return Err(SkillRefusal::TooManyFiles);
                }
                total += metadata.len();
                if total > TOTAL_MAX {
                    return Err(SkillRefusal::TooLarge);
                }
                let mut bytes = Vec::new();
                std::fs::File::open(entry.path())
                    .and_then(|file| file.take(FILE_MAX + 1).read_to_end(&mut bytes))
                    .map_err(|_| SkillRefusal::PathInvalid(relative.clone()))?;
                if bytes.len() as u64 > FILE_MAX {
                    return Err(SkillRefusal::TooLarge);
                }
                files.insert(relative, bytes);
            } else {
                return Err(SkillRefusal::PathInvalid(relative));
            }
        }
    }
    Ok(files)
}

/// The hash each skill was last confirmed with on this computer, from the log's events oldest
/// first: the newest `skill.added`, `skill.changed` or `skill.confirmed`, cleared by a later
/// `skill.removed`.
#[must_use]
pub fn confirmed_skills(events: &[CatervasEvent]) -> BTreeMap<(SkillLevel, String), String> {
    let mut confirmed = BTreeMap::new();
    for event in events {
        match &event.body {
            EventBody::SkillAdded(body)
            | EventBody::SkillChanged(body)
            | EventBody::SkillConfirmed(body) => {
                let level = match (body.level.to_string().as_str(), &body.agent) {
                    ("team", _) => SkillLevel::Team,
                    (_, Some(agent)) => SkillLevel::Agent(agent.to_string()),
                    _ => continue,
                };
                confirmed.insert((level, body.name.to_string()), body.sha256.to_string());
            }
            EventBody::SkillRemoved(body) => {
                let level = match (body.level.to_string().as_str(), &body.agent) {
                    ("team", _) => SkillLevel::Team,
                    (_, Some(agent)) => SkillLevel::Agent(agent.to_string()),
                    _ => continue,
                };
                confirmed.remove(&(level, body.name.to_string()));
            }
            _ => {}
        }
    }
    confirmed
}

/// What one agent's sessions load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionSkills {
    /// The skills to load on demand.
    pub skills: Vec<SessionSkill>,
    /// The shipped skills a loaded skill of the same name replaces, which leave the prompt.
    pub replaced_role_skills: BTreeSet<String>,
}

/// How a pinned skill stands for one agent (spec 6.7), in the order the states are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillState {
    /// Folder, pin and this computer's confirmation agree: it loads.
    InUse,
    /// Another level's skill of the same name stands in its place.
    Replaced,
    /// Waiting for the person to read and confirm it.
    Review,
    /// Pinned, and its folder is not there.
    Missing,
}

/// A pinned skill's folder read once: its state, the copy a session loads, its size and what its
/// `SKILL.md` says it is for.
pub(crate) struct Evaluated {
    pub(crate) state: SkillState,
    pub(crate) checked: Option<CheckedSkill>,
    pub(crate) bytes: u64,
    pub(crate) description: String,
}

/// Reads the folder once and judges the same bytes it hashes and copies, so that a folder changed
/// between the check and the copy cannot slip in.
pub(crate) fn evaluate(
    root: &Path,
    level: &SkillLevel,
    pin: &SkillPin,
    confirmed: &BTreeMap<(SkillLevel, String), String>,
) -> Evaluated {
    let name = pin.name.as_str();
    let unreadable = |state| Evaluated {
        state,
        checked: None,
        bytes: 0,
        description: String::new(),
    };
    let Ok(folder) = skill_folder_unlinked(root, level, name) else {
        return unreadable(SkillState::Review);
    };
    if matches!(std::fs::symlink_metadata(&folder), Err(error) if error.kind() == io::ErrorKind::NotFound)
    {
        return unreadable(SkillState::Missing);
    }
    let Ok(files) = read_skill_folder(&folder) else {
        return unreadable(SkillState::Review);
    };
    let bytes = files.values().map(|file| file.len() as u64).sum();
    let description = declared_name_and_description(&files)
        .map(|(_, description)| description)
        .unwrap_or_default();
    let hash = skill_sha256(&files);
    let agreed = hash == pin.sha256.as_str()
        && confirmed.get(&(level.clone(), name.to_string())) == Some(&hash);
    match check_skill(name, &files) {
        Ok(checked) if agreed => Evaluated {
            state: SkillState::InUse,
            checked: Some(checked),
            bytes,
            description,
        },
        _ => Evaluated {
            state: SkillState::Review,
            checked: None,
            bytes,
            description,
        },
    }
}

/// The name of every skill Catervas ships: each shipped role's, and each of its kit's from `kits`.
/// What a user's skill may not take without `replace_shipped` (ADR 0034).
#[must_use]
pub fn shipped_names(kits: &crate::tools::KitSource) -> BTreeSet<String> {
    let roles: Vec<_> = SHIPPED_ROLES
        .into_iter()
        .filter_map(|role| catervas_roles::load_role(role).ok())
        .collect();
    let kits: Vec<_> = SHIPPED_ROLES
        .into_iter()
        .filter_map(|role| kits(role).ok())
        .collect();
    shipped_skill_names(&roles, &kits)
}

/// The skills `agent_id`'s sessions load: its own pinned skills, the team's that it has no skill
/// of the same name for, then its role's kit's (`kit_skills`) that neither pins a skill of the
/// name of, each only when its folder, its pin and this computer's confirmations agree; a kit's
/// skill is Catervas's own, neither pinned nor confirmed (ADR 0034).
#[must_use]
pub fn session_skills(
    root: &Path,
    team: &Team,
    agent_id: &str,
    confirmed: &BTreeMap<(SkillLevel, String), String>,
    kit_skills: &[CheckedSkill],
) -> SessionSkills {
    let own: &[SkillPin] = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == agent_id)
        .map_or(&[], |agent| &agent.skills);
    let mut chosen: Vec<(SkillLevel, &SkillPin)> = own
        .iter()
        .map(|pin| (SkillLevel::Agent(agent_id.to_string()), pin))
        .collect();
    chosen.extend(
        team.skills()
            .iter()
            .filter(|pin| !own.iter().any(|mine| mine.name == pin.name))
            .map(|pin| (SkillLevel::Team, pin)),
    );
    // A pin of the name stands for the kit's skill, whatever its state: one in review never falls
    // back silently to the kit's.
    let pinned: Vec<&str> = chosen.iter().map(|(_, pin)| pin.name.as_str()).collect();
    let kit_chosen: Vec<&CheckedSkill> = kit_skills
        .iter()
        .filter(|skill| !pinned.contains(&skill.name.as_str()))
        .collect();
    let shipped = core_skill_names();
    let mut loaded = SessionSkills::default();
    for (level, pin) in chosen {
        let evaluated = evaluate(root, &level, pin, confirmed);
        let (SkillState::InUse, Some(checked)) = (evaluated.state, evaluated.checked) else {
            continue;
        };
        if let Some(name) = shipped.get(checked.name.as_str()) {
            loaded.replaced_role_skills.insert((*name).to_string());
        }
        loaded.skills.push(SessionSkill {
            name: checked.name,
            files: checked.session_files,
        });
    }
    loaded
        .skills
        .extend(kit_chosen.into_iter().map(|skill| SessionSkill {
            name: skill.name.clone(),
            files: skill.session_files.clone(),
        }));
    loaded
}

/// A saved skill: its name, the hash it is pinned with, whether it replaced one of its name, and
/// the sequence number of the event recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSaved {
    /// The skill's name.
    pub name: String,
    /// `skill_sha256` of its folder, which the team file now pins.
    pub sha256: String,
    /// Whether it replaced a skill already pinned under its name (`skill.changed`).
    pub changed: bool,
    /// The sequence number of `skill.added` or `skill.changed`.
    pub event: u64,
}

/// Why a skill command was refused, or failed.
#[derive(Debug)]
pub enum SkillCommandError {
    /// The name is not pinned there.
    Unknown,
    /// The hash sent is not the folder's.
    HashMismatch,
    /// A shipped skill's name, without `replace_shipped`.
    NameTaken,
    /// Twenty skills are pinned already.
    LimitReached,
    /// No such agent.
    AgentUnknown,
    /// `check_skill`'s or `read_skill_folder`'s refusal.
    Refused(SkillRefusal),
    /// The folder, the team file or the log could not be written.
    Io(io::Error),
}

impl SkillCommandError {
    /// The refusal's wire code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown => "skill_unknown",
            Self::HashMismatch => "skill_hash_mismatch",
            Self::NameTaken => "skill_name_taken",
            Self::LimitReached => "skill_limit_reached",
            Self::AgentUnknown => "agent_unknown",
            Self::Refused(refusal) => refusal.code(),
            Self::Io(_) => "skill_failed",
        }
    }
}

impl std::fmt::Display for SkillCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = self.code();
        match self {
            Self::Unknown => write!(formatter, "{code}: no such skill is pinned there."),
            Self::HashMismatch => write!(
                formatter,
                "{code}: the skill's folder is not the one you read; read it again before \
                 confirming."
            ),
            Self::NameTaken => write!(
                formatter,
                "{code}: that is the name of a skill Catervas ships; replacing it needs your say so \
                 (replace)."
            ),
            Self::LimitReached => write!(
                formatter,
                "{code}: twenty skills are given already; remove one first."
            ),
            Self::AgentUnknown => write!(formatter, "{code}: the team has no such agent."),
            Self::Refused(refusal) => write!(formatter, "{refusal}"),
            Self::Io(error) => write!(formatter, "{code}: {error}"),
        }
    }
}

impl std::error::Error for SkillCommandError {}

/// The most skills one list pins.
const SKILLS_MAX: usize = 20;

fn io_other(error: impl std::fmt::Display) -> SkillCommandError {
    SkillCommandError::Io(io::Error::other(error.to_string()))
}

/// The pins of `level`, or `None` when the agent is not on the team.
fn team_pins<'a>(team: &'a Team, level: &SkillLevel) -> Option<&'a [SkillPin]> {
    match level {
        SkillLevel::Team => Some(team.skills()),
        SkillLevel::Agent(id) => team
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == id)
            .map(|agent| agent.skills.as_slice()),
    }
}

/// `team` with `name` pinned to `sha256` at `level`, in place when it is there, or unpinned when
/// there is no hash; held to `validate_team`.
fn with_pin(
    team: &Team,
    level: &SkillLevel,
    name: &str,
    sha256: Option<&str>,
) -> Result<Team, SkillCommandError> {
    let mut wire = serde_json::to_value(team).map_err(io_other)?;
    let list = match level {
        SkillLevel::Team => &mut wire["skills"],
        SkillLevel::Agent(id) => wire["agents"]
            .as_array_mut()
            .and_then(|agents| agents.iter_mut().find(|agent| agent["id"] == id.as_str()))
            .map(|agent| &mut agent["skills"])
            .ok_or(SkillCommandError::AgentUnknown)?,
    };
    let mut pins: Vec<serde_json::Value> = list.as_array().cloned().unwrap_or_default();
    match (sha256, pins.iter_mut().find(|pin| pin["name"] == name)) {
        (Some(sha256), Some(pin)) => pin["sha256"] = serde_json::json!(sha256),
        (Some(sha256), None) => pins.push(serde_json::json!({ "name": name, "sha256": sha256 })),
        (None, _) => pins.retain(|pin| pin["name"] != name),
    }
    *list = serde_json::Value::Array(pins);
    catervas_core::team::validate_team(&wire).map_err(|errors| {
        io_other(format!(
            "the team file would not be valid: {}",
            errors
                .iter()
                .map(|error| format!("{}: {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })
}

/// Records one skill event as the human's and projects it, answering its sequence number.
fn record(tools: &ToolDeps, body: EventBody) -> Result<u64, SkillCommandError> {
    let event = catervas_protocol::event::new_event(body, tools.clock.now(), tools.ids.clone())
        .map_err(|error| io_other(format!("the event cannot be recorded: {error:?}")))?;
    let appended = tools.log.append(&event).map_err(io_other)?;
    tools.projections.apply(&appended).map_err(io_other)?;
    Ok(appended.envelope.seq)
}

/// The event body of a skill at `level`, pinned to `sha256` when there is a hash.
fn skill_body<T: serde::de::DeserializeOwned>(
    level: &SkillLevel,
    name: &str,
    sha256: Option<&str>,
) -> Result<T, SkillCommandError> {
    let mut body = serde_json::json!({ "name": name });
    match level {
        SkillLevel::Team => body["level"] = "team".into(),
        SkillLevel::Agent(agent) => {
            body["level"] = "agent".into();
            body["agent"] = agent.as_str().into();
        }
    }
    if let Some(sha256) = sha256 {
        body["sha256"] = sha256.into();
    }
    serde_json::from_value(body).map_err(io_other)
}

/// Replaces `folder` with `files`: written beside it as `.<name>.new-<random>`, the old one moved
/// to `.<name>.old-<random>`, the new one renamed into its place (`rename(2)` will not replace a
/// folder that is not empty), then the old one deleted.
fn replace_folder(folder: &Path, files: &BTreeMap<String, Vec<u8>>) -> io::Result<()> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let (Some(parent), Some(name)) = (folder.parent(), folder.file_name()) else {
        return Err(io::Error::other("a skill folder has a parent and a name"));
    };
    std::fs::create_dir_all(parent)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let random = format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let name = name.to_string_lossy();
    let fresh = parent.join(format!(".{name}.new-{random}"));
    let old = parent.join(format!(".{name}.old-{random}"));
    let written = (|| {
        for (path, bytes) in files {
            let file = fresh.join(path);
            if let Some(directory) = file.parent() {
                std::fs::create_dir_all(directory)?;
            }
            std::fs::write(file, bytes)?;
        }
        Ok::<(), io::Error>(())
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(error);
    }
    let had_old = std::fs::symlink_metadata(folder).is_ok();
    if had_old {
        std::fs::rename(folder, &old).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&fresh);
        })?;
    }
    if let Err(error) = std::fs::rename(&fresh, folder) {
        if had_old {
            let _ = std::fs::rename(&old, folder);
        }
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(error);
    }
    if had_old {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

/// Adds the skill `files` hold to `level`, or replaces the one of its name: its folder, then its
/// pin in the team file, then `skill.added` or `skill.changed`. The caller holds whatever lock
/// keeps the team file from two writers.
///
/// # Errors
///
/// A refusal of the skill (`check_skill`'s), `NameTaken` for a shipped skill's name without
/// `replace_shipped`, `AgentUnknown`, `LimitReached`, or the failure to write.
pub fn save_skill(
    tools: &ToolDeps,
    level: &SkillLevel,
    files: &BTreeMap<String, Vec<u8>>,
    replace_shipped: bool,
) -> Result<SkillSaved, SkillCommandError> {
    let (name, _) = declared_name_and_description(files)
        .ok_or(SkillCommandError::Refused(SkillRefusal::FrontmatterInvalid))?;
    check_skill(&name, files).map_err(SkillCommandError::Refused)?;
    if !replace_shipped && shipped_names(&tools.kits).contains(name.as_str()) {
        return Err(SkillCommandError::NameTaken);
    }
    let team = tools.files.read_team().map_err(io_other)?;
    let pins = team_pins(&team, level).ok_or(SkillCommandError::AgentUnknown)?;
    let changed = pins.iter().any(|pin| pin.name.as_str() == name);
    if !changed && pins.len() >= SKILLS_MAX {
        return Err(SkillCommandError::LimitReached);
    }
    let sha256 = skill_sha256(files);
    let folder = skill_folder_unlinked(tools.files.root(), level, &name)
        .map_err(SkillCommandError::Refused)?;
    replace_folder(&folder, files).map_err(SkillCommandError::Io)?;
    tools
        .files
        .write_team(&with_pin(&team, level, &name, Some(&sha256))?)
        .map_err(io_other)?;
    let body = skill_body(level, &name, Some(&sha256))?;
    let event = record(
        tools,
        if changed {
            EventBody::SkillChanged(body)
        } else {
            EventBody::SkillAdded(body)
        },
    )?;
    Ok(SkillSaved {
        name,
        sha256,
        changed,
        event,
    })
}

/// Removes the skill `name` of `level`: its folder, its pin, and records `skill.removed`, which
/// answers its sequence number. A skill whose folder is already gone is unpinned all the same.
///
/// # Errors
///
/// `Unknown` for a name not pinned there, `AgentUnknown`, or the failure to write.
pub fn remove_skill(
    tools: &ToolDeps,
    level: &SkillLevel,
    name: &str,
) -> Result<u64, SkillCommandError> {
    let team = tools.files.read_team().map_err(io_other)?;
    let pins = team_pins(&team, level).ok_or(SkillCommandError::AgentUnknown)?;
    if !pins.iter().any(|pin| pin.name.as_str() == name) {
        return Err(SkillCommandError::Unknown);
    }
    let folder = skill_folder_unlinked(tools.files.root(), level, name)
        .map_err(SkillCommandError::Refused)?;
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            return Err(SkillCommandError::Io(error));
        }
        _ => {}
    }
    tools
        .files
        .write_team(&with_pin(&team, level, name, None)?)
        .map_err(io_other)?;
    record(
        tools,
        EventBody::SkillRemoved(skill_body(level, name, None)?),
    )
}

/// Confirms the skill `name` of `level` as its folder is now: the person read the folder that
/// hashes to `sha256`. The pin is rewritten to that hash when the folder or the pin changed outside
/// Catervas, then `skill.confirmed` is recorded, which answers its sequence number.
///
/// # Errors
///
/// `Unknown` for a name not pinned there, `HashMismatch` when the folder is not the one read, the
/// refusal of the folder, `NameTaken`, `AgentUnknown`, or the failure to write.
pub fn confirm_skill(
    tools: &ToolDeps,
    level: &SkillLevel,
    name: &str,
    sha256: &str,
    replace_shipped: bool,
) -> Result<u64, SkillCommandError> {
    let team = tools.files.read_team().map_err(io_other)?;
    let pins = team_pins(&team, level).ok_or(SkillCommandError::AgentUnknown)?;
    let pin = pins
        .iter()
        .find(|pin| pin.name.as_str() == name)
        .ok_or(SkillCommandError::Unknown)?;
    let folder = skill_folder_unlinked(tools.files.root(), level, name)
        .map_err(SkillCommandError::Refused)?;
    let files = read_skill_folder(&folder).map_err(SkillCommandError::Refused)?;
    let hash = skill_sha256(&files);
    if hash != sha256 {
        return Err(SkillCommandError::HashMismatch);
    }
    check_skill(name, &files).map_err(SkillCommandError::Refused)?;
    if !replace_shipped && shipped_names(&tools.kits).contains(name) {
        return Err(SkillCommandError::NameTaken);
    }
    if pin.sha256.as_str() != hash {
        tools
            .files
            .write_team(&with_pin(&team, level, name, Some(&hash))?)
            .map_err(io_other)?;
    }
    record(
        tools,
        EventBody::SkillConfirmed(skill_body(level, name, Some(&hash))?),
    )
}

/// Whose a row of `skills.list` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRowLevel {
    /// A shipped role's.
    Role,
    /// The team's.
    Team,
    /// One agent's.
    Agent,
}

/// One skill of an agent as `skills.list` shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillRow {
    /// Whose.
    pub level: SkillRowLevel,
    /// Its name.
    pub name: String,
    /// When it applies, as its `SKILL.md` says; empty when that cannot be read.
    pub description: String,
    /// How it stands.
    pub state: SkillState,
    /// The folder's total size, 0 when missing; a role's, its `SKILL.md`'s.
    pub bytes: u64,
}

/// The skills `agent_id` has, role rows first, then the team's, then its own, each with how it
/// stands for that agent; without an agent, the team's alone.
#[must_use]
pub fn skill_rows(
    root: &Path,
    team: &Team,
    agent_id: Option<&str>,
    confirmed: &BTreeMap<(SkillLevel, String), String>,
    kit_skills: &[CheckedSkill],
) -> Vec<SkillRow> {
    let agent = agent_id.and_then(|id| team.agents.iter().find(|agent| agent.id.as_str() == id));
    let own: &[SkillPin] = agent.map_or(&[], |agent| &agent.skills);
    let mine = agent.map_or(SkillLevel::Team, |agent| {
        SkillLevel::Agent(agent.id.to_string())
    });
    let team_skills: Vec<(&SkillPin, Evaluated)> = team
        .skills()
        .iter()
        .map(|pin| (pin, evaluate(root, &SkillLevel::Team, pin, confirmed)))
        .collect();
    let own_skills: Vec<(&SkillPin, Evaluated)> = own
        .iter()
        .map(|pin| (pin, evaluate(root, &mine, pin, confirmed)))
        .collect();
    let row = |level, pin: &SkillPin, evaluated: &Evaluated, state| SkillRow {
        level,
        name: pin.name.to_string(),
        description: evaluated.description.clone(),
        state,
        bytes: evaluated.bytes,
    };
    let mut rows = Vec::new();
    if let Some(agent) = agent
        && let Ok(role) = catervas_roles::load_role(catervas_core::contract::Role::from(agent.role))
    {
        // The skill that stands for a name for this agent: its own, else the team's.
        let in_use = |name: &str| {
            own_skills
                .iter()
                .find(|(pin, _)| pin.name.as_str() == name)
                .or_else(|| {
                    team_skills
                        .iter()
                        .find(|(pin, _)| pin.name.as_str() == name)
                })
                .is_some_and(|(_, evaluated)| evaluated.state == SkillState::InUse)
        };
        rows.extend(role.skills.iter().map(|skill| SkillRow {
            level: SkillRowLevel::Role,
            name: skill.name.clone(),
            description: skill.description.clone(),
            state: if in_use(&skill.name) {
                SkillState::Replaced
            } else {
                SkillState::InUse
            },
            bytes: skill.bytes as u64,
        }));
        // Its role's kit's skills follow the role's own, and count as the role's.
        rows.extend(kit_skills.iter().map(|skill| {
            SkillRow {
                level: SkillRowLevel::Role,
                name: skill.name.clone(),
                description: skill.description.clone(),
                state: if in_use(&skill.name) {
                    SkillState::Replaced
                } else {
                    SkillState::InUse
                },
                bytes: skill
                    .session_files
                    .get("SKILL.md")
                    .map_or(0, |text| text.len() as u64),
            }
        }));
    }
    for (pin, evaluated) in &team_skills {
        let state = if own.iter().any(|mine| mine.name == pin.name) {
            SkillState::Replaced
        } else {
            evaluated.state
        };
        rows.push(row(SkillRowLevel::Team, pin, evaluated, state));
    }
    for (pin, evaluated) in &own_skills {
        rows.push(row(SkillRowLevel::Agent, pin, evaluated, evaluated.state));
    }
    rows
}

/// Writes the plugin folder Claude Code loads `skills` from, replacing what was at `plugin_dir`.
/// The folder and every folder in it are 0700 and every file 0600.
///
/// # Errors
///
/// The folder or a file could not be written.
pub fn write_plugin(plugin_dir: &Path, skills: &[SessionSkill]) -> io::Result<()> {
    match std::fs::remove_dir_all(plugin_dir) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let private_folder = |folder: &Path| {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
    };
    private_folder(&plugin_dir.join(".claude-plugin"))?;
    crate::write_private(
        &plugin_dir.join(".claude-plugin/plugin.json"),
        br#"{"name":"catervas"}"#,
    )?;
    for skill in skills {
        for (path, text) in &skill.files {
            let file = plugin_dir.join("skills").join(&skill.name).join(path);
            if let Some(parent) = file.parent() {
                private_folder(parent)?;
            }
            crate::write_private(&file, text.as_bytes())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    use catervas_core::skill::skill_sha256;
    use catervas_core::team::fixtures::a_team_wire;
    use catervas_core::team::{Team, validate_team};
    use catervas_protocol::event::fixtures::an_event_wire;
    use catervas_protocol::event::{CatervasEvent, EventBody, EventKind, event_from_value};
    use std::sync::Arc;

    use catervas_roles::{SkillRefusal, check_skill};
    use serde_json::{Value, json};

    use super::{
        SessionSkill, SkillLevel, confirmed_skills, read_skill_folder, session_skills,
        skill_folder, write_plugin,
    };

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("catervas-skills-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    fn skill_md(name: &str, body: &str) -> String {
        format!("---\nname: {name}\ndescription: Use when {name}.\n---\n{body}")
    }

    /// Writes a skill folder at its place and returns its files and hash.
    fn put(
        root: &Path,
        level: &SkillLevel,
        name: &str,
        body: &str,
    ) -> (BTreeMap<String, Vec<u8>>, String) {
        let folder = skill_folder(root, level, name);
        std::fs::create_dir_all(folder.join("references")).expect("a folder");
        std::fs::write(folder.join("SKILL.md"), skill_md(name, body)).expect("a file");
        std::fs::write(folder.join("references/a.md"), "details").expect("a file");
        let files = read_skill_folder(&folder).expect("a readable folder");
        let sha = skill_sha256(&files);
        (files, sha)
    }

    fn event(kind: EventKind, level: &str, name: &str, sha: &str) -> CatervasEvent {
        let mut wire = an_event_wire(kind);
        let mut body = json!({ "level": level, "name": name });
        if level == "agent" {
            body["agent"] = json!("linus");
        }
        if kind != EventKind::SkillRemoved {
            body["sha256"] = json!(sha);
        }
        wire["body"] = body;
        event_from_value(&wire).expect("a skill event")
    }

    fn team_with(team_pins: &[(&str, &str)], agent_pins: &[(&str, &str)]) -> Team {
        let pins = |list: &[(&str, &str)]| -> Value {
            list.iter()
                .map(|(n, s)| json!({ "name": n, "sha256": s }))
                .collect()
        };
        let mut wire = a_team_wire();
        wire["skills"] = pins(team_pins);
        wire["agents"][1]["skills"] = pins(agent_pins);
        validate_team(&wire).expect("a team")
    }

    fn confirmed(of: &[(SkillLevel, &str, &str)]) -> BTreeMap<(SkillLevel, String), String> {
        of.iter()
            .map(|(level, name, sha)| ((level.clone(), (*name).to_string()), (*sha).to_string()))
            .collect()
    }

    fn agent() -> SkillLevel {
        SkillLevel::Agent("linus".to_string())
    }

    fn names(skills: &super::SessionSkills) -> Vec<&str> {
        skills
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect()
    }

    #[test]
    fn puts_folders_where_the_spec_says() {
        let root = Path::new("/p");
        assert_eq!(
            skill_folder(root, &SkillLevel::Team, "api-style"),
            Path::new("/p/.catervas/skills/api-style")
        );
        assert_eq!(
            skill_folder(root, &agent(), "api-style"),
            Path::new("/p/.catervas/agents/linus/skills/api-style")
        );
    }

    #[test]
    fn reads_a_folder_and_refuses_a_link() {
        let root = scratch("reads");
        let (files, _) = put(&root, &SkillLevel::Team, "api-style", "body");
        let paths: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(paths, ["SKILL.md", "references/a.md"]);
        let folder = skill_folder(&root, &SkillLevel::Team, "api-style");
        assert_eq!(files["references/a.md"], b"details");

        std::os::unix::fs::symlink("/etc/hostname", folder.join("link.md")).expect("a link");
        assert_eq!(
            read_skill_folder(&folder),
            Err(SkillRefusal::PathInvalid("link.md".to_string()))
        );
        std::fs::remove_file(folder.join("link.md")).expect("removed");
        std::os::unix::fs::symlink("/etc", folder.join("references/dir")).expect("a link");
        assert_eq!(
            read_skill_folder(&folder),
            Err(SkillRefusal::PathInvalid("references/dir".to_string()))
        );
        std::fs::remove_file(folder.join("references/dir")).expect("removed");

        // A 70 KiB file is refused as it is found, and a FIFO would not be read at all.
        std::fs::write(folder.join("big.md"), vec![b'x'; 70 * 1024]).expect("a file");
        assert_eq!(read_skill_folder(&folder), Err(SkillRefusal::TooLarge));
        std::fs::remove_file(folder.join("big.md")).expect("removed");
        // Five files of 60 KiB are each within their limit and 300 KiB in all.
        for n in 0..5 {
            std::fs::write(folder.join(format!("m{n}.md")), vec![b'x'; 60 * 1024]).expect("a file");
        }
        assert_eq!(read_skill_folder(&folder), Err(SkillRefusal::TooLarge));
        for n in 0..5 {
            std::fs::remove_file(folder.join(format!("m{n}.md"))).expect("removed");
        }
        // Four levels down is refused without walking further.
        std::fs::create_dir_all(folder.join("a/b/c")).expect("folders");
        std::fs::write(folder.join("a/b/c/d.md"), "x").expect("a file");
        assert_eq!(
            read_skill_folder(&folder),
            Err(SkillRefusal::PathInvalid("a/b/c/d.md".to_string()))
        );
        std::fs::remove_dir_all(folder.join("a")).expect("removed");
        for n in 0..15 {
            std::fs::write(folder.join(format!("f{n}.md")), "x").expect("a file");
        }
        assert_eq!(read_skill_folder(&folder), Err(SkillRefusal::TooManyFiles));
        assert_eq!(
            read_skill_folder(&root.join("nothing-here")),
            Ok(BTreeMap::new()),
            "a folder that is not there is empty"
        );
    }

    #[test]
    fn the_newest_confirmation_wins_and_removal_clears() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let added = event(EventKind::SkillAdded, "agent", "api-style", &a);
        let changed = event(EventKind::SkillChanged, "agent", "api-style", &b);
        let key = (agent(), "api-style".to_string());
        assert_eq!(
            confirmed_skills(std::slice::from_ref(&added)).get(&key),
            Some(&a)
        );
        assert_eq!(
            confirmed_skills(&[added.clone(), changed.clone()]).get(&key),
            Some(&b)
        );
        let confirmed_again = event(EventKind::SkillConfirmed, "agent", "api-style", &a);
        assert_eq!(
            confirmed_skills(&[added.clone(), changed.clone(), confirmed_again.clone()]).get(&key),
            Some(&a)
        );
        let removed = event(EventKind::SkillRemoved, "agent", "api-style", "");
        assert!(confirmed_skills(&[added.clone(), changed.clone(), removed.clone()]).is_empty());
        assert_eq!(
            confirmed_skills(&[added, changed, removed, confirmed_again]).get(&key),
            Some(&a),
            "a confirmation after the removal confirms it again"
        );
        let team_level = event(EventKind::SkillAdded, "team", "api-style", &b);
        assert_eq!(
            confirmed_skills(&[team_level]).get(&(SkillLevel::Team, "api-style".to_string())),
            Some(&b)
        );
    }

    #[test]
    fn a_skill_loads_only_when_pin_folder_and_log_agree() {
        let root = scratch("agree");
        let (_, sha) = put(&root, &agent(), "api-style", "body");
        let team = team_with(&[], &[("api-style", &sha)]);
        let ok = confirmed(&[(agent(), "api-style", &sha)]);
        assert_eq!(
            names(&session_skills(&root, &team, "linus", &ok, &[])),
            ["api-style"]
        );
        let loaded = session_skills(&root, &team, "linus", &ok, &[]);
        assert!(
            loaded.skills[0].files["SKILL.md"].starts_with("---\nname: api-style\ndescription: ")
        );

        // A folder edited after confirming.
        let folder = skill_folder(&root, &agent(), "api-style");
        std::fs::write(folder.join("references/a.md"), "edited").expect("an edit");
        assert!(
            session_skills(&root, &team, "linus", &ok, &[])
                .skills
                .is_empty()
        );

        // A pin changed alone, the folder still what this computer confirmed.
        let (_, current) = put(&root, &agent(), "api-style", "body");
        let repinned = team_with(&[], &[("api-style", &"c".repeat(64))]);
        let current_ok = confirmed(&[(agent(), "api-style", &current)]);
        assert!(
            session_skills(&root, &repinned, "linus", &current_ok, &[])
                .skills
                .is_empty()
        );
        // A pin and folder changed together with no event, as a pull would.
        let (_, new_sha) = put(&root, &agent(), "api-style", "a pulled body");
        let pulled = team_with(&[], &[("api-style", &new_sha)]);
        assert!(
            session_skills(&root, &pulled, "linus", &ok, &[])
                .skills
                .is_empty()
        );
        assert!(
            session_skills(&root, &pulled, "linus", &BTreeMap::new(), &[])
                .skills
                .is_empty(),
            "no event at all"
        );
        let confirmed_new = confirmed(&[(agent(), "api-style", &new_sha)]);
        assert_eq!(
            names(&session_skills(
                &root,
                &pulled,
                "linus",
                &confirmed_new,
                &[]
            )),
            ["api-style"]
        );

        // A pin whose folder is gone.
        std::fs::remove_dir_all(&folder).expect("removed");
        assert!(
            session_skills(&root, &pulled, "linus", &confirmed_new, &[])
                .skills
                .is_empty()
        );

        // A folder a check refuses, even with its pin and confirmation.
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(folder.join("SKILL.md"), skill_md("api-style", "run !`ls`"))
            .expect("a file");
        let sha = skill_sha256(&read_skill_folder(&folder).expect("readable"));
        let team = team_with(&[], &[("api-style", &sha)]);
        let ok = confirmed(&[(agent(), "api-style", &sha)]);
        assert!(
            session_skills(&root, &team, "linus", &ok, &[])
                .skills
                .is_empty()
        );
        // Another agent's skill is not this agent's.
        assert!(
            session_skills(&root, &team, "ada", &ok, &[])
                .skills
                .is_empty()
        );
    }

    #[test]
    fn a_linked_skill_folder_is_review() {
        let root = scratch("linked");
        let elsewhere = scratch("linked-elsewhere");
        let (_, sha) = put(&elsewhere, &SkillLevel::Team, "api-style", "body");
        std::fs::create_dir_all(root.join(".catervas/skills")).expect("a folder");
        std::os::unix::fs::symlink(
            skill_folder(&elsewhere, &SkillLevel::Team, "api-style"),
            root.join(".catervas/skills/api-style"),
        )
        .expect("a link");
        let team = team_with(&[("api-style", &sha)], &[]);
        let ok = confirmed(&[(SkillLevel::Team, "api-style", &sha)]);
        assert!(
            session_skills(&root, &team, "linus", &ok, &[])
                .skills
                .is_empty()
        );
    }

    #[test]
    fn an_agents_skill_replaces_the_teams() {
        let root = scratch("replaces");
        let (_, team_sha) = put(&root, &SkillLevel::Team, "api-style", "the team's");
        let (_, agent_sha) = put(&root, &agent(), "api-style", "the agent's");
        let team = team_with(&[("api-style", &team_sha)], &[("api-style", &agent_sha)]);
        let ok = confirmed(&[
            (SkillLevel::Team, "api-style", &team_sha),
            (agent(), "api-style", &agent_sha),
        ]);
        let loaded = session_skills(&root, &team, "linus", &ok, &[]);
        assert_eq!(names(&loaded), ["api-style"]);
        assert!(loaded.skills[0].files["SKILL.md"].ends_with("the agent's"));
        // Ada has no skill of her own: she gets the team's.
        let ada = session_skills(&root, &team, "ada", &ok, &[]);
        assert!(ada.skills[0].files["SKILL.md"].ends_with("the team's"));
        // The agent's is in review: none, never the team's.
        let only_team = confirmed(&[(SkillLevel::Team, "api-style", &team_sha)]);
        assert!(
            session_skills(&root, &team, "linus", &only_team, &[])
                .skills
                .is_empty()
        );
    }

    /// The kit skill `name` with `body`, as `check_skill` hands it over.
    fn a_kit_skill(name: &str, body: &str) -> catervas_roles::CheckedSkill {
        let files = BTreeMap::from([("SKILL.md".to_string(), skill_md(name, body).into_bytes())]);
        check_skill(name, &files).expect("a skill")
    }

    #[test]
    fn kit_skills_load_after_the_agents_and_the_teams_without_a_pin_naming_them() {
        let root = scratch("kit-order");
        let kit = [a_kit_skill("launch-plans", "the kit's")];
        let none = BTreeMap::new();
        let team = team_with(&[], &[]);
        let loaded = session_skills(&root, &team, "linus", &none, &kit);
        assert_eq!(names(&loaded), ["launch-plans"]);
        assert!(loaded.skills[0].files["SKILL.md"].ends_with("the kit's"));
        // The team's, in use: the team's, not the kit's.
        let (_, team_sha) = put(&root, &SkillLevel::Team, "launch-plans", "the team's");
        let team = team_with(&[("launch-plans", &team_sha)], &[]);
        let ok = confirmed(&[(SkillLevel::Team, "launch-plans", &team_sha)]);
        let loaded = session_skills(&root, &team, "linus", &ok, &kit);
        assert_eq!(names(&loaded), ["launch-plans"]);
        assert!(loaded.skills[0].files["SKILL.md"].ends_with("the team's"));
        // The team's, in review: none, and never the kit's.
        assert!(
            session_skills(&root, &team, "linus", &none, &kit)
                .skills
                .is_empty()
        );
        // The agent's pin shadows both.
        let (_, agent_sha) = put(&root, &agent(), "launch-plans", "the agent's");
        let team = team_with(
            &[("launch-plans", &team_sha)],
            &[("launch-plans", &agent_sha)],
        );
        let ok = confirmed(&[(agent(), "launch-plans", &agent_sha)]);
        let loaded = session_skills(&root, &team, "linus", &ok, &kit);
        assert!(loaded.skills[0].files["SKILL.md"].ends_with("the agent's"));
    }

    #[test]
    fn counts_a_kit_skill_among_the_names_a_command_checks() {
        let kits: crate::tools::KitSource = Arc::new(|role| match role {
            catervas_core::contract::Role::SoftwareDeveloper => Ok(
                crate::tools::fixtures::a_developer_kit(&[("launch-plans", "x")], None),
            ),
            other => catervas_roles::load_kit(other),
        });
        let names = super::shipped_names(&kits);
        assert!(names.contains("launch-plans"));
        assert!(names.contains("writing-task-contracts"));
        assert!(
            !super::shipped_names(&(Arc::new(catervas_roles::load_kit) as crate::tools::KitSource))
                .contains("launch-plans")
        );
    }

    #[test]
    fn a_confirmed_replacement_names_the_role_skill() {
        let root = scratch("role");
        let (_, sha) = put(&root, &SkillLevel::Team, "implementing-a-contract", "mine");
        let team = team_with(&[("implementing-a-contract", &sha)], &[]);
        let ok = confirmed(&[(SkillLevel::Team, "implementing-a-contract", &sha)]);
        let loaded = session_skills(&root, &team, "linus", &ok, &[]);
        assert_eq!(names(&loaded), ["implementing-a-contract"]);
        assert_eq!(
            loaded
                .replaced_role_skills
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["implementing-a-contract"]
        );
        let review = session_skills(&root, &team, "linus", &BTreeMap::new(), &[]);
        assert!(review.skills.is_empty() && review.replaced_role_skills.is_empty());
        let (_, other) = put(&root, &SkillLevel::Team, "api-style", "x");
        let team = team_with(&[("api-style", &other)], &[]);
        let ok = confirmed(&[(SkillLevel::Team, "api-style", &other)]);
        assert!(
            session_skills(&root, &team, "linus", &ok, &[])
                .replaced_role_skills
                .is_empty()
        );
    }

    #[test]
    fn writes_the_plugin_folder() {
        let dir = scratch("plugin").join("session-1");
        let skill = SessionSkill {
            name: "api-style".to_string(),
            files: BTreeMap::from([
                (
                    "SKILL.md".to_string(),
                    "---\nname: api-style\n---\nbody".to_string(),
                ),
                ("references/a.md".to_string(), "details".to_string()),
            ]),
        };
        write_plugin(&dir, std::slice::from_ref(&skill)).expect("written");
        let manifest =
            std::fs::read_to_string(dir.join(".claude-plugin/plugin.json")).expect("a manifest");
        assert_eq!(
            serde_json::from_str::<Value>(&manifest).expect("JSON"),
            json!({ "name": "catervas" })
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("skills/api-style/SKILL.md")).expect("a copy"),
            skill.files["SKILL.md"]
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("skills/api-style/references/a.md")).expect("a copy"),
            "details"
        );
        let mode = std::fs::metadata(&dir)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
        // Writing again replaces the first copy whole.
        let other = SessionSkill {
            name: "other-skill".to_string(),
            files: BTreeMap::from([("SKILL.md".to_string(), "x".to_string())]),
        };
        write_plugin(&dir, &[other]).expect("written again");
        assert!(!dir.join("skills/api-style").exists());
        assert!(dir.join("skills/other-skill/SKILL.md").exists());
    }
    // The commands and the rows, on a project with a team of `pm`, `dev-a` and `dev-b`.

    use super::{
        SkillCommandError, SkillRowLevel, SkillState, confirm_skill, remove_skill, save_skill,
        skill_rows,
    };
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    fn dev_a() -> SkillLevel {
        SkillLevel::Agent("dev-a".to_string())
    }

    fn a_project(name: &str) -> TestProject {
        TestProject::new(name, &a_team_of_three(|_| {}))
    }

    /// The files of a skill `name` with `body`, and a reference.
    fn skill_files(name: &str, body: &str) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("SKILL.md".to_string(), skill_md(name, body).into_bytes()),
            ("references/a.md".to_string(), b"details".to_vec()),
        ])
    }

    fn pins(project: &TestProject, level: &SkillLevel) -> Vec<(String, String)> {
        let team = project.deps.files.read_team().expect("the team");
        let list: Vec<(String, String)> = match level {
            SkillLevel::Team => team
                .skills()
                .iter()
                .map(|pin| (pin.name.to_string(), pin.sha256.to_string()))
                .collect(),
            SkillLevel::Agent(id) => team
                .agents
                .iter()
                .find(|agent| agent.id.as_str() == id)
                .expect("the agent")
                .skills
                .iter()
                .map(|pin| (pin.name.to_string(), pin.sha256.to_string()))
                .collect(),
        };
        list
    }

    fn confirmed_in(project: &TestProject) -> BTreeMap<(SkillLevel, String), String> {
        let events = project.events(&[
            EventKind::SkillAdded,
            EventKind::SkillChanged,
            EventKind::SkillRemoved,
            EventKind::SkillConfirmed,
        ]);
        confirmed_skills(&events)
    }

    fn code(result: Result<impl std::fmt::Debug, SkillCommandError>) -> &'static str {
        result.expect_err("refused").code()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_team_skill_named_for_a_kit_skill_without_replace_shipped() {
        let project = a_project("skills-kit-name-taken");
        project.set_kit(crate::tools::fixtures::a_developer_kit(
            &[("launch-plans", "kit")],
            None,
        ));
        let files = skill_files("launch-plans", "mine");
        assert_eq!(
            code(save_skill(&project.deps, &SkillLevel::Team, &files, false)),
            "skill_name_taken"
        );
        assert!(pins(&project, &SkillLevel::Team).is_empty());
        save_skill(&project.deps, &SkillLevel::Team, &files, true).expect("replaced on purpose");
        let sha = skill_sha256(&files);
        assert_eq!(
            code(confirm_skill(
                &project.deps,
                &SkillLevel::Team,
                "launch-plans",
                &sha,
                false
            )),
            "skill_name_taken"
        );
        // The rows: the kit's skill is the role's, after the role's own, and replaced now.
        let team = project.deps.files.read_team().expect("the team");
        let root = project.repo.path.clone();
        let kit =
            (project.deps.kits)(catervas_core::contract::Role::SoftwareDeveloper).expect("kit");
        let rows = skill_rows(
            &root,
            &team,
            Some("dev-a"),
            &confirmed_in(&project),
            &kit.skills,
        );
        let shown: Vec<(SkillRowLevel, &str, SkillState)> = rows
            .iter()
            .map(|row| (row.level, row.name.as_str(), row.state))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    SkillRowLevel::Role,
                    "implementing-a-contract",
                    SkillState::InUse
                ),
                (SkillRowLevel::Role, "launch-plans", SkillState::Replaced),
                (SkillRowLevel::Team, "launch-plans", SkillState::InUse),
            ]
        );
        let bare = skill_rows(&root, &team, Some("dev-a"), &BTreeMap::new(), &kit.skills);
        assert_eq!(
            bare[1].state,
            SkillState::InUse,
            "no confirmation: the kit's still stands"
        );
        assert!(bare[1].bytes > 0 && !bare[1].description.is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn save_writes_folder_pin_and_event() {
        let project = a_project("skills-save");
        let level = dev_a();
        let files = skill_files("api-style", "first");
        let saved = save_skill(&project.deps, &level, &files, false).expect("saved");
        let sha = skill_sha256(&files);
        assert_eq!(
            (saved.name.as_str(), saved.sha256.as_str(), saved.changed),
            ("api-style", sha.as_str(), false)
        );
        let root = &project.repo.path;
        let folder = root.join(".catervas/agents/dev-a/skills/api-style");
        assert_eq!(read_skill_folder(&folder).expect("a folder"), files);
        assert_eq!(
            pins(&project, &level),
            [("api-style".to_string(), sha.clone())]
        );
        let added = project.events(&[EventKind::SkillAdded]);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].envelope.seq, saved.event);
        let EventBody::SkillAdded(body) = &added[0].body else {
            panic!("a skill.added")
        };
        assert_eq!(
            (
                body.level.to_string(),
                body.agent.as_ref().map(|agent| agent.as_str().to_string()),
                body.name.to_string(),
                body.sha256.to_string()
            ),
            (
                "agent".to_string(),
                Some("dev-a".to_string()),
                "api-style".to_string(),
                sha
            )
        );
        // Saved again, changed, with one file fewer: the folder is replaced whole.
        let changed = BTreeMap::from([(
            "SKILL.md".to_string(),
            skill_md("api-style", "second").into_bytes(),
        )]);
        let again = save_skill(&project.deps, &level, &changed, false).expect("saved again");
        assert!(again.changed);
        let new_sha = skill_sha256(&changed);
        assert_eq!(
            pins(&project, &level),
            [("api-style".to_string(), new_sha.clone())]
        );
        assert_eq!(read_skill_folder(&folder).expect("a folder"), changed);
        let events = project.events(&[EventKind::SkillChanged]);
        let EventBody::SkillChanged(body) = &events[0].body else {
            panic!("a skill.changed")
        };
        assert_eq!(body.sha256.to_string(), new_sha);
        // No staging folder is left.
        let beside: Vec<String> = std::fs::read_dir(folder.parent().expect("a parent"))
            .expect("a folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(beside, ["api-style"]);
        // A team skill goes under .catervas/skills.
        save_skill(
            &project.deps,
            &SkillLevel::Team,
            &skill_files("team-style", "t"),
            false,
        )
        .expect("saved");
        assert!(root.join(".catervas/skills/team-style/SKILL.md").exists());
        let EventBody::SkillAdded(body) = &project.events(&[EventKind::SkillAdded])[1].body else {
            panic!("added")
        };
        assert_eq!(
            (body.level.to_string(), body.agent.is_none()),
            ("team".to_string(), true)
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn save_refuses_what_check_skill_refuses() {
        let project = a_project("skills-save-refuses");
        let before = project.event_count();
        let running = skill_files("api-style", "run !`ls`");
        assert_eq!(
            code(save_skill(&project.deps, &dev_a(), &running, false)),
            "skill_runs_commands"
        );
        assert!(
            !project.repo.path.join(".catervas/agents/dev-a").exists(),
            "nothing was written"
        );
        assert!(pins(&project, &dev_a()).is_empty());
        assert_eq!(project.event_count(), before);
        let no_skill = BTreeMap::from([("other.md".to_string(), b"x".to_vec())]);
        assert_eq!(
            code(save_skill(
                &project.deps,
                &SkillLevel::Team,
                &no_skill,
                false
            )),
            "skill_frontmatter_invalid"
        );

        // A shipped name needs the person to say so.
        let shipped = skill_files("writing-task-contracts", "mine");
        assert_eq!(
            code(save_skill(
                &project.deps,
                &SkillLevel::Team,
                &shipped,
                false
            )),
            "skill_name_taken"
        );
        assert!(pins(&project, &SkillLevel::Team).is_empty());
        save_skill(&project.deps, &SkillLevel::Team, &shipped, true).expect("replaced on purpose");
        assert_eq!(pins(&project, &SkillLevel::Team).len(), 1);

        // No such agent.
        let ghost = SkillLevel::Agent("ghost".to_string());
        assert_eq!(
            code(save_skill(
                &project.deps,
                &ghost,
                &skill_files("a-b", "x"),
                false
            )),
            "agent_unknown"
        );

        // Twenty skills at most; saving one of the twenty again is not a twenty-first.
        for n in 1..20 {
            save_skill(
                &project.deps,
                &SkillLevel::Team,
                &skill_files(&format!("s{n}"), "x"),
                false,
            )
            .expect("saved");
        }
        assert_eq!(pins(&project, &SkillLevel::Team).len(), 20);
        assert_eq!(
            code(save_skill(
                &project.deps,
                &SkillLevel::Team,
                &skill_files("s20", "x"),
                false
            )),
            "skill_limit_reached"
        );
        save_skill(
            &project.deps,
            &SkillLevel::Team,
            &skill_files("s1", "y"),
            false,
        )
        .expect("one of the twenty again");
        assert_eq!(pins(&project, &SkillLevel::Team).len(), 20);
        assert!(!project.repo.path.join(".catervas/skills/s20").exists());
        // The agent's own list is counted apart.
        save_skill(&project.deps, &dev_a(), &skill_files("s20", "x"), false)
            .expect("the agent's own");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn remove_and_save_refuse_a_linked_skills_folder() {
        let project = a_project("skills-linked");
        let root = &project.repo.path;
        save_skill(
            &project.deps,
            &SkillLevel::Team,
            &skill_files("api-style", "x"),
            false,
        )
        .expect("saved");
        let outside =
            std::env::temp_dir().join(format!("catervas-linked-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&outside).expect("a folder");
        std::fs::rename(root.join(".catervas/skills"), &outside).expect("moved out");
        std::os::unix::fs::symlink(&outside, root.join(".catervas/skills")).expect("a link");
        assert_eq!(
            code(remove_skill(&project.deps, &SkillLevel::Team, "api-style")),
            "skill_path_invalid"
        );
        assert!(outside.join("api-style/SKILL.md").exists());
        assert_eq!(pins(&project, &SkillLevel::Team).len(), 1);
        assert_eq!(
            code(save_skill(
                &project.deps,
                &SkillLevel::Team,
                &skill_files("other-skill", "y"),
                false
            )),
            "skill_path_invalid"
        );
        let names: Vec<_> = std::fs::read_dir(&outside)
            .expect("a folder")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(names, ["api-style"], "nothing written outside");
        assert_eq!(
            code(confirm_skill(
                &project.deps,
                &SkillLevel::Team,
                "api-style",
                &skill_sha256(&skill_files("api-style", "x")),
                false
            )),
            "skill_path_invalid"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn remove_deletes_folder_pin_and_records() {
        let project = a_project("skills-remove");
        save_skill(
            &project.deps,
            &dev_a(),
            &skill_files("api-style", "mine"),
            false,
        )
        .expect("saved");
        save_skill(
            &project.deps,
            &SkillLevel::Team,
            &skill_files("api-style", "theirs"),
            false,
        )
        .expect("saved");
        let removed = remove_skill(&project.deps, &dev_a(), "api-style").expect("removed");
        let root = &project.repo.path;
        assert!(
            !root
                .join(".catervas/agents/dev-a/skills/api-style")
                .exists()
        );
        assert!(pins(&project, &dev_a()).is_empty());
        assert_eq!(
            project.events(&[EventKind::SkillRemoved])[0].envelope.seq,
            removed
        );
        // The team's skill of the same name is untouched.
        assert!(root.join(".catervas/skills/api-style/SKILL.md").exists());
        assert_eq!(pins(&project, &SkillLevel::Team).len(), 1);
        assert_eq!(
            code(remove_skill(&project.deps, &dev_a(), "api-style")),
            "skill_unknown"
        );
        assert_eq!(
            code(remove_skill(&project.deps, &SkillLevel::Team, "nothing")),
            "skill_unknown"
        );

        // A folder that cannot be deleted fails the command, with the pin and the log as they were.
        save_skill(
            &project.deps,
            &dev_a(),
            &skill_files("kept-style", "x"),
            false,
        )
        .expect("saved");
        let parent = root.join(".catervas/agents/dev-a/skills");
        std::fs::set_permissions(&parent, std::os::unix::fs::PermissionsExt::from_mode(0o555))
            .expect("read-only");
        let count = project.event_count();
        let failed = remove_skill(&project.deps, &dev_a(), "kept-style");
        std::fs::set_permissions(&parent, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("writable again");
        assert_eq!(code(failed), "skill_failed");
        assert_eq!(pins(&project, &dev_a()).len(), 1);
        assert_eq!(project.event_count(), count);
        // A missing skill is unpinned all the same.
        std::fs::remove_dir_all(root.join(".catervas/skills/api-style")).expect("gone by hand");
        remove_skill(&project.deps, &SkillLevel::Team, "api-style")
            .expect("a missing skill is removed");
        assert!(pins(&project, &SkillLevel::Team).is_empty());
        assert_eq!(project.events(&[EventKind::SkillRemoved]).len(), 2);
        let confirmed = confirmed_in(&project);
        assert!(!confirmed.contains_key(&(SkillLevel::Team, "api-style".to_string())));
        assert!(!confirmed.contains_key(&(dev_a(), "api-style".to_string())));
        assert!(confirmed.contains_key(&(dev_a(), "kept-style".to_string())));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one scenario, read from top to bottom"
    )]
    fn confirm_needs_the_folders_hash() {
        let project = a_project("skills-confirm");
        let files = skill_files("api-style", "first");
        save_skill(&project.deps, &dev_a(), &files, false).expect("saved");
        let old = skill_sha256(&files);
        let folder = project
            .repo
            .path
            .join(".catervas/agents/dev-a/skills/api-style");
        std::fs::write(folder.join("references/a.md"), "changed outside Catervas")
            .expect("an edit");
        let now = skill_sha256(&read_skill_folder(&folder).expect("readable"));
        assert_ne!(now, old);
        let before = project.event_count();
        assert_eq!(
            code(confirm_skill(
                &project.deps,
                &dev_a(),
                "api-style",
                &old,
                false
            )),
            "skill_hash_mismatch"
        );
        assert_eq!(project.event_count(), before, "nothing was recorded");
        assert_eq!(
            pins(&project, &dev_a()),
            [("api-style".to_string(), old)],
            "nor pinned"
        );
        let rows = |p: &TestProject| {
            skill_rows(
                &p.repo.path,
                &p.deps.files.read_team().expect("team"),
                Some("dev-a"),
                &confirmed_in(p),
                &[],
            )
        };
        let state = |p: &TestProject| {
            rows(p)
                .iter()
                .find(|row| row.level == SkillRowLevel::Agent)
                .map(|row| row.state)
        };
        assert_eq!(state(&project), Some(SkillState::Review));
        let seq =
            confirm_skill(&project.deps, &dev_a(), "api-style", &now, false).expect("confirmed");
        assert_eq!(
            pins(&project, &dev_a()),
            [("api-style".to_string(), now.clone())]
        );
        assert_eq!(
            project.events(&[EventKind::SkillConfirmed])[0].envelope.seq,
            seq
        );
        assert_eq!(state(&project), Some(SkillState::InUse));

        // A folder a check refuses is refused with nothing written, though the hash is right.
        std::fs::write(folder.join("SKILL.md"), skill_md("api-style", "run !`ls`"))
            .expect("an edit");
        let running = skill_sha256(&read_skill_folder(&folder).expect("readable"));
        let before = project.event_count();
        assert_eq!(
            code(confirm_skill(
                &project.deps,
                &dev_a(),
                "api-style",
                &running,
                false
            )),
            "skill_runs_commands"
        );
        assert_eq!(project.event_count(), before);
        assert_eq!(pins(&project, &dev_a()), [("api-style".to_string(), now)]);
        // A name that is not pinned there cannot be confirmed, and a shipped name needs the word.
        assert_eq!(
            code(confirm_skill(
                &project.deps,
                &SkillLevel::Team,
                "api-style",
                &running,
                false
            )),
            "skill_unknown"
        );
        save_skill(
            &project.deps,
            &SkillLevel::Team,
            &skill_files("writing-task-contracts", "mine"),
            true,
        )
        .expect("saved");
        let shipped = skill_sha256(
            &read_skill_folder(
                &project
                    .repo
                    .path
                    .join(".catervas/skills/writing-task-contracts"),
            )
            .expect("readable"),
        );
        assert_eq!(
            code(confirm_skill(
                &project.deps,
                &SkillLevel::Team,
                "writing-task-contracts",
                &shipped,
                false
            )),
            "skill_name_taken"
        );
        confirm_skill(
            &project.deps,
            &SkillLevel::Team,
            "writing-task-contracts",
            &shipped,
            true,
        )
        .expect("on purpose");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one scenario, read from top to bottom"
    )]
    fn skills_list_gives_every_level_and_state() {
        let project = a_project("skills-rows");
        let team_level = SkillLevel::Team;
        // A team skill that replaces the shipped one of dev-a's role, and one an agent pins over.
        save_skill(
            &project.deps,
            &team_level,
            &skill_files("implementing-a-contract", "mine"),
            true,
        )
        .expect("saved");
        save_skill(
            &project.deps,
            &team_level,
            &skill_files("api-style", "theirs"),
            false,
        )
        .expect("saved");
        save_skill(
            &project.deps,
            &dev_a(),
            &skill_files("api-style", "mine"),
            false,
        )
        .expect("saved");
        save_skill(&project.deps, &dev_a(), &skill_files("notes", "n"), false).expect("saved");
        save_skill(
            &project.deps,
            &dev_a(),
            &skill_files("vanished", "v"),
            false,
        )
        .expect("saved");
        let root = project.repo.path.clone();
        std::fs::remove_dir_all(root.join(".catervas/agents/dev-a/skills/vanished"))
            .expect("gone by hand");
        // The agent's api-style is edited after it was confirmed: review.
        std::fs::write(
            root.join(".catervas/agents/dev-a/skills/api-style/references/a.md"),
            "edited",
        )
        .expect("an edit");
        let team = project.deps.files.read_team().expect("the team");
        let confirmed = confirmed_in(&project);
        let rows = skill_rows(&root, &team, Some("dev-a"), &confirmed, &[]);
        let shown: Vec<(SkillRowLevel, &str, SkillState)> = rows
            .iter()
            .map(|row| (row.level, row.name.as_str(), row.state))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    SkillRowLevel::Role,
                    "implementing-a-contract",
                    SkillState::Replaced
                ),
                (
                    SkillRowLevel::Team,
                    "implementing-a-contract",
                    SkillState::InUse
                ),
                (SkillRowLevel::Team, "api-style", SkillState::Replaced),
                (SkillRowLevel::Agent, "api-style", SkillState::Review),
                (SkillRowLevel::Agent, "notes", SkillState::InUse),
                (SkillRowLevel::Agent, "vanished", SkillState::Missing),
            ]
        );
        let by_name = |level, name: &str| {
            rows.iter()
                .find(|row| row.level == level && row.name == name)
                .expect("a row")
                .clone()
        };
        let notes = by_name(SkillRowLevel::Agent, "notes");
        assert_eq!(notes.description, "Use when notes.");
        let folder = skill_folder(&root, &dev_a(), "notes");
        let expected: u64 = read_skill_folder(&folder)
            .expect("readable")
            .values()
            .map(|file| file.len() as u64)
            .sum();
        assert_eq!(notes.bytes, expected);
        assert_eq!(by_name(SkillRowLevel::Agent, "vanished").bytes, 0);
        assert!(
            by_name(SkillRowLevel::Agent, "api-style").bytes > 0,
            "a review row has its size"
        );
        let role = by_name(SkillRowLevel::Role, "implementing-a-contract");
        assert_eq!(
            role.bytes,
            catervas_roles::load_role(catervas_core::contract::Role::SoftwareDeveloper)
                .expect("a role")
                .skills[0]
                .bytes as u64
        );
        assert!(!role.description.is_empty());

        // The Product Manager has its own role skill in use, and the team's skills, in use.
        let pm: Vec<(SkillRowLevel, &str, SkillState)> =
            skill_rows(&root, &team, Some("pm"), &confirmed, &[])
                .iter()
                .map(|row| (row.level, row.name.as_str(), row.state))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(level, name, state)| {
                    (
                        level,
                        Box::leak(name.to_string().into_boxed_str()) as &str,
                        state,
                    )
                })
                .collect();
        assert_eq!(
            pm,
            [
                (
                    SkillRowLevel::Role,
                    "writing-task-contracts",
                    SkillState::InUse
                ),
                (
                    SkillRowLevel::Team,
                    "implementing-a-contract",
                    SkillState::InUse
                ),
                (SkillRowLevel::Team, "api-style", SkillState::InUse),
            ]
        );
        // Without an agent, the team's alone, and nothing is replaced.
        let team_rows: Vec<(String, SkillState)> = skill_rows(&root, &team, None, &confirmed, &[])
            .into_iter()
            .map(|row| {
                assert_eq!(row.level, SkillRowLevel::Team);
                (row.name, row.state)
            })
            .collect();
        assert_eq!(
            team_rows,
            [
                ("implementing-a-contract".to_string(), SkillState::InUse),
                ("api-style".to_string(), SkillState::InUse)
            ]
        );
        // A role's skill replaced by a team skill in review is not replaced.
        std::fs::write(
            root.join(".catervas/skills/implementing-a-contract/references/a.md"),
            "edited",
        )
        .expect("an edit");
        let rows = skill_rows(&root, &team, Some("dev-a"), &confirmed, &[]);
        assert_eq!(rows[0].state, SkillState::InUse);
        assert_eq!(rows[1].state, SkillState::Review);
    }
}
