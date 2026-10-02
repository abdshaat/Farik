//! Skills an agent is given beyond its role's (`docs/SPEC.md` 6.7, ADR 0034): reading a folder,
//! what this computer has confirmed, and what a session loads.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read as _};
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

use farik_core::skill::skill_sha256;
use farik_core::team::{SkillPin, Team};
use farik_protocol::event::{EventBody, FarikEvent};
use farik_roles::{CheckedSkill, SkillRefusal, check_skill, core_skill_names};

/// Whose skill: the team's, or one agent's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillLevel {
    /// `.farik/skills/<name>/`.
    Team,
    /// `.farik/agents/<agent id>/skills/<name>/`.
    Agent(String),
}

/// Where a skill's folder is, in the project at `root`.
#[must_use]
pub fn skill_folder(root: &Path, level: &SkillLevel, name: &str) -> PathBuf {
    let farik = root.join(".farik");
    match level {
        SkillLevel::Team => farik.join("skills").join(name),
        SkillLevel::Agent(agent) => farik.join("agents").join(agent).join("skills").join(name),
    }
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
pub fn confirmed_skills(events: &[FarikEvent]) -> BTreeMap<(SkillLevel, String), String> {
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

/// One skill a session loads on demand: its name and the session copy of each file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSkill {
    /// The skill's name.
    pub name: String,
    /// Every file as text, `SKILL.md`'s frontmatter rewritten to `name` and `description`.
    pub files: BTreeMap<String, String>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// A pinned skill's folder read once: its state, the copy a session loads, and its size.
pub(crate) struct Evaluated {
    pub(crate) state: SkillState,
    pub(crate) checked: Option<CheckedSkill>,
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
    let folder = skill_folder(root, level, name);
    if matches!(std::fs::symlink_metadata(&folder), Err(error) if error.kind() == io::ErrorKind::NotFound)
    {
        return Evaluated {
            state: SkillState::Missing,
            checked: None,
        };
    }
    let review = Evaluated {
        state: SkillState::Review,
        checked: None,
    };
    let Ok(files) = read_skill_folder(&folder) else {
        return review;
    };
    let hash = skill_sha256(&files);
    let agreed = hash == pin.sha256.as_str()
        && confirmed.get(&(level.clone(), name.to_string())) == Some(&hash);
    match check_skill(name, &files) {
        Ok(checked) if agreed => Evaluated {
            state: SkillState::InUse,
            checked: Some(checked),
        },
        _ => review,
    }
}

/// The skills `agent_id`'s sessions load: its own pinned skills, and the team's that it has no
/// skill of the same name for, each only when its folder, its pin and this computer's
/// confirmations agree.
#[must_use]
pub fn session_skills(
    root: &Path,
    team: &Team,
    agent_id: &str,
    confirmed: &BTreeMap<(SkillLevel, String), String>,
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
        br#"{"name":"farik"}"#,
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

    use farik_core::skill::skill_sha256;
    use farik_core::team::fixtures::a_team_wire;
    use farik_core::team::{Team, validate_team};
    use farik_protocol::event::fixtures::an_event_wire;
    use farik_protocol::event::{EventKind, FarikEvent, event_from_value};
    use farik_roles::SkillRefusal;
    use serde_json::{Value, json};

    use super::{
        SessionSkill, SkillLevel, confirmed_skills, read_skill_folder, session_skills,
        skill_folder, write_plugin,
    };

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("farik-skills-{}-{test}", std::process::id()));
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

    fn event(kind: EventKind, level: &str, name: &str, sha: &str) -> FarikEvent {
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
            Path::new("/p/.farik/skills/api-style")
        );
        assert_eq!(
            skill_folder(root, &agent(), "api-style"),
            Path::new("/p/.farik/agents/linus/skills/api-style")
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
            names(&session_skills(&root, &team, "linus", &ok)),
            ["api-style"]
        );
        let loaded = session_skills(&root, &team, "linus", &ok);
        assert!(
            loaded.skills[0].files["SKILL.md"].starts_with("---\nname: api-style\ndescription: ")
        );

        // A folder edited after confirming.
        let folder = skill_folder(&root, &agent(), "api-style");
        std::fs::write(folder.join("references/a.md"), "edited").expect("an edit");
        assert!(session_skills(&root, &team, "linus", &ok).skills.is_empty());

        // A pin changed alone, the folder still what this computer confirmed.
        let (_, current) = put(&root, &agent(), "api-style", "body");
        let repinned = team_with(&[], &[("api-style", &"c".repeat(64))]);
        let current_ok = confirmed(&[(agent(), "api-style", &current)]);
        assert!(
            session_skills(&root, &repinned, "linus", &current_ok)
                .skills
                .is_empty()
        );
        // A pin and folder changed together with no event, as a pull would.
        let (_, new_sha) = put(&root, &agent(), "api-style", "a pulled body");
        let pulled = team_with(&[], &[("api-style", &new_sha)]);
        assert!(
            session_skills(&root, &pulled, "linus", &ok)
                .skills
                .is_empty()
        );
        assert!(
            session_skills(&root, &pulled, "linus", &BTreeMap::new())
                .skills
                .is_empty(),
            "no event at all"
        );
        let confirmed_new = confirmed(&[(agent(), "api-style", &new_sha)]);
        assert_eq!(
            names(&session_skills(&root, &pulled, "linus", &confirmed_new)),
            ["api-style"]
        );

        // A pin whose folder is gone.
        std::fs::remove_dir_all(&folder).expect("removed");
        assert!(
            session_skills(&root, &pulled, "linus", &confirmed_new)
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
        assert!(session_skills(&root, &team, "linus", &ok).skills.is_empty());
        // Another agent's skill is not this agent's.
        assert!(session_skills(&root, &team, "ada", &ok).skills.is_empty());
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
        let loaded = session_skills(&root, &team, "linus", &ok);
        assert_eq!(names(&loaded), ["api-style"]);
        assert!(loaded.skills[0].files["SKILL.md"].ends_with("the agent's"));
        // Ada has no skill of her own: she gets the team's.
        let ada = session_skills(&root, &team, "ada", &ok);
        assert!(ada.skills[0].files["SKILL.md"].ends_with("the team's"));
        // The agent's is in review: none, never the team's.
        let only_team = confirmed(&[(SkillLevel::Team, "api-style", &team_sha)]);
        assert!(
            session_skills(&root, &team, "linus", &only_team)
                .skills
                .is_empty()
        );
    }

    #[test]
    fn a_confirmed_replacement_names_the_role_skill() {
        let root = scratch("role");
        let (_, sha) = put(&root, &SkillLevel::Team, "implementing-a-contract", "mine");
        let team = team_with(&[("implementing-a-contract", &sha)], &[]);
        let ok = confirmed(&[(SkillLevel::Team, "implementing-a-contract", &sha)]);
        let loaded = session_skills(&root, &team, "linus", &ok);
        assert_eq!(names(&loaded), ["implementing-a-contract"]);
        assert_eq!(
            loaded
                .replaced_role_skills
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["implementing-a-contract"]
        );
        let review = session_skills(&root, &team, "linus", &BTreeMap::new());
        assert!(review.skills.is_empty() && review.replaced_role_skills.is_empty());
        let (_, other) = put(&root, &SkillLevel::Team, "api-style", "x");
        let team = team_with(&[("api-style", &other)], &[]);
        let ok = confirmed(&[(SkillLevel::Team, "api-style", &other)]);
        assert!(
            session_skills(&root, &team, "linus", &ok)
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
            json!({ "name": "farik" })
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
}
