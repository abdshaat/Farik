//! What outlives a project: the state folder, and the `state.json` in it that remembers the last
//! project `catervas serve` served (`docs/SPEC.md` 8.1).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::CliIo;

/// The state folder: `$XDG_CONFIG_HOME/catervas`, else `$HOME/.config/catervas`, else `%APPDATA%\catervas`,
/// and `None` when none is set, when nothing is remembered. The environment is the harness's, not
/// the process's, so a test sets its own.
pub(crate) fn state_dir(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let set = |name: &str| env.get(name).filter(|value| !value.is_empty());
    set("XDG_CONFIG_HOME")
        .map(|base| PathBuf::from(base).join("catervas"))
        .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".config/catervas")))
        .or_else(|| set("APPDATA").map(|base| PathBuf::from(base).join("catervas")))
}

/// Where Claude Code keeps its history: `CLAUDE_CONFIG_DIR` when it is set and not empty, as Claude
/// Code itself honours it, else `$HOME/.claude`; `None` when neither is set.
pub(crate) fn claude_dir(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let set = |name: &str| env.get(name).filter(|value| !value.is_empty());
    set("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".claude")))
}

/// Where Claude Code keeps its settings file: `.claude.json` under `CLAUDE_CONFIG_DIR` when that is
/// set and not empty, else `$HOME/.claude.json`; `None` when neither is set.
pub(crate) fn claude_json(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let set = |name: &str| env.get(name).filter(|value| !value.is_empty());
    set("CLAUDE_CONFIG_DIR")
        .map(|dir| PathBuf::from(dir).join(".claude.json"))
        .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".claude.json")))
}

/// Where Claude Desktop keeps its settings on `os` (`std::env::consts::OS`): Linux
/// `$XDG_CONFIG_HOME/Claude`, else `$HOME/.config/Claude`; macOS
/// `$HOME/Library/Application Support/Claude`; Windows `%APPDATA%\Claude`. None when the variable
/// is not set.
pub(crate) fn claude_desktop_dirs(env: &BTreeMap<String, String>, os: &str) -> Vec<PathBuf> {
    let set = |name: &str| env.get(name).filter(|value| !value.is_empty());
    let dir = match os {
        "linux" => set("XDG_CONFIG_HOME")
            .map(|base| PathBuf::from(base).join("Claude"))
            .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".config/Claude"))),
        "macos" => {
            set("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/Claude"))
        }
        "windows" => set("APPDATA").map(|base| PathBuf::from(format!("{base}\\Claude"))),
        _ => None,
    };
    dir.into_iter().collect()
}

/// What the command line says when it removes the key an earlier version kept.
const KEPT_KEY_REMOVED: &str = "Catervas no longer keeps your AI account's key, so it removed the one it kept. \
    Sign in through Claude Code: in the web app, or with claude auth login.";

/// Removes, unread, the key an earlier version kept (ADR 0084) and says so once on standard
/// error: a start that finds nothing kept says nothing.
pub(crate) fn forget_kept_key(io: &mut CliIo<'_>) {
    let Some(directory) = state_dir(&io.env) else {
        return;
    };
    if catervas_runtime::credential::forget_kept_key(&directory, io.forget_keychain) {
        io.kept_key_removed = true;
        crate::say(&mut io.stderr, KEPT_KEY_REMOVED);
    }
}

/// Home: `HOME` when it is set and not empty, else where the command was run. A project folder's
/// member repositories, and the folders the first-run wizard browses, are relative to it.
pub(crate) fn home_of(io: &CliIo<'_>) -> PathBuf {
    io.env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map_or_else(|| io.cwd.clone(), PathBuf::from)
}

/// Makes the state folder `directory` with mode 0700, when it is not there, and sets 0700 on one
/// that is.
///
/// # Errors
///
/// A sentence saying it cannot be made.
pub(crate) fn make_state_dir(directory: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

    let unmade = |error: std::io::Error| format!("{} cannot be made: {error}", directory.display());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(unmade)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).map_err(unmade)
}

/// What `state.json` holds: every team's project, the one served last, and the computer's two
/// limits (ADR 0059).
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ServeState {
    /// The project remembered last.
    pub(crate) last_project: Option<PathBuf>,
    /// Every team's project, in the order they were taken on.
    pub(crate) projects: Vec<PathBuf>,
    /// How many agent sessions run at once across the teams, when it is limited.
    pub(crate) agents_at_once: Option<usize>,
    /// The most the teams together spend on a UTC day, when it is capped.
    pub(crate) daily_cap_usd: Option<f64>,
    /// Whether the owner paused finding projects (`finding_paused`).
    pub(crate) finding_paused: bool,
    /// Whether the owner dismissed the card that offers claude.ai's export (`export_card` is
    /// `"dismissed"`).
    pub(crate) export_dismissed: bool,
}

/// What `state.json` in `directory` holds; an unreadable or absent file reads as empty. A file
/// with only `last_project`, as earlier versions wrote it, lists that project as the one team.
pub(crate) fn read_state(directory: &Path) -> ServeState {
    let Ok(text) = std::fs::read_to_string(directory.join("state.json")) else {
        return ServeState::default();
    };
    let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else {
        return ServeState::default();
    };
    let path = |value: &serde_json::Value| value.as_str().map(PathBuf::from);
    let last_project = path(&state["last_project"]);
    let mut projects: Vec<PathBuf> = state["projects"]
        .as_array()
        .map(|listed| listed.iter().filter_map(path).collect())
        .unwrap_or_default();
    if state["projects"].is_null()
        && let Some(last) = &last_project
    {
        projects.push(last.clone());
    }
    ServeState {
        last_project,
        projects,
        agents_at_once: state["agents_at_once"]
            .as_u64()
            .and_then(|limit| usize::try_from(limit).ok()),
        daily_cap_usd: state["daily_cap_usd"].as_f64(),
        finding_paused: state["finding_paused"].as_bool().unwrap_or(false),
        export_dismissed: state["export_card"].as_str() == Some("dismissed"),
    }
}

/// Writes `state` as `state.json` in `directory`, made with mode 0700 when it is not there, and
/// readable by its owner alone.
///
/// # Errors
///
/// A sentence saying what could not be made or written.
pub(crate) fn write_state(directory: &Path, state: &ServeState) -> Result<(), String> {
    make_state_dir(directory)?;
    let file = directory.join("state.json");
    let mut text = serde_json::Map::new();
    if let Some(last) = &state.last_project {
        text.insert("last_project".to_string(), last.to_string_lossy().into());
    }
    text.insert(
        "projects".to_string(),
        state
            .projects
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .into(),
    );
    if let Some(limit) = state.agents_at_once {
        text.insert("agents_at_once".to_string(), limit.into());
    }
    if let Some(cap) = state.daily_cap_usd {
        text.insert("daily_cap_usd".to_string(), cap.into());
    }
    if state.finding_paused {
        text.insert("finding_paused".to_string(), true.into());
    }
    if state.export_dismissed {
        text.insert("export_card".to_string(), "dismissed".into());
    }
    catervas_runtime::write_private(
        &file,
        serde_json::Value::Object(text).to_string().as_bytes(),
    )
    .map_err(|error| format!("{} cannot be written: {error}", file.display()))
}

/// `root`, followed through its links when it can be, else as given.
fn canonical(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Adds `root` to the projects `state.json` lists, once, and makes it the last project.
///
/// # Errors
///
/// A sentence saying what could not be made or written.
pub(crate) fn remember(directory: &Path, root: &Path) -> Result<(), String> {
    let mut state = read_state(directory);
    let root = canonical(root);
    if !state.projects.contains(&root) {
        state.projects.push(root.clone());
    }
    state.last_project = Some(root);
    write_state(directory, &state)
}

/// Takes `root` out of the projects `state.json` lists; the last project too, when it was that.
///
/// # Errors
///
/// A sentence saying what could not be written.
pub(crate) fn forget(directory: &Path, root: &Path) -> Result<(), String> {
    let mut state = read_state(directory);
    let root = canonical(root);
    state.projects.retain(|listed| *listed != root);
    if state.last_project.as_ref() == Some(&root) {
        state.last_project = state.projects.last().cloned();
    }
    write_state(directory, &state)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{
        ServeState, claude_desktop_dirs, claude_dir, claude_json, forget, read_state, remember,
        state_dir,
    };

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn state_dir_follows_xdg_then_home_then_appdata() {
        let all = env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h"), ("APPDATA", "/a")]);
        assert_eq!(state_dir(&all), Some(PathBuf::from("/x/catervas")));
        let no_xdg = env(&[("HOME", "/h"), ("APPDATA", "/a")]);
        assert_eq!(
            state_dir(&no_xdg),
            Some(PathBuf::from("/h/.config/catervas"))
        );
        let only_appdata = env(&[("APPDATA", "/a")]);
        assert_eq!(state_dir(&only_appdata), Some(PathBuf::from("/a/catervas")));
        assert_eq!(state_dir(&env(&[])), None);
    }

    #[test]
    fn claude_dir_prefers_claude_config_dir() {
        let both = env(&[("CLAUDE_CONFIG_DIR", "/c"), ("HOME", "/h")]);
        assert_eq!(claude_dir(&both), Some(PathBuf::from("/c")));
        let empty = env(&[("CLAUDE_CONFIG_DIR", ""), ("HOME", "/h")]);
        assert_eq!(claude_dir(&empty), Some(PathBuf::from("/h/.claude")));
        assert_eq!(claude_dir(&env(&[])), None);
    }

    #[test]
    fn claude_json_follows_claude_config_dir() {
        let both = env(&[("CLAUDE_CONFIG_DIR", "/c"), ("HOME", "/h")]);
        assert_eq!(claude_json(&both), Some(PathBuf::from("/c/.claude.json")));
        let empty = env(&[("CLAUDE_CONFIG_DIR", ""), ("HOME", "/h")]);
        assert_eq!(claude_json(&empty), Some(PathBuf::from("/h/.claude.json")));
        assert_eq!(claude_json(&env(&[])), None);
    }

    #[test]
    fn finds_claude_desktop_per_system() {
        let one = |dir: &str| vec![PathBuf::from(dir)];
        let xdg = env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]);
        assert_eq!(claude_desktop_dirs(&xdg, "linux"), one("/x/Claude"));
        let home = env(&[("HOME", "/h")]);
        assert_eq!(
            claude_desktop_dirs(&home, "linux"),
            one("/h/.config/Claude")
        );
        assert_eq!(
            claude_desktop_dirs(&home, "macos"),
            one("/h/Library/Application Support/Claude")
        );
        let appdata = env(&[("APPDATA", "C:\\A")]);
        assert_eq!(
            claude_desktop_dirs(&appdata, "windows"),
            one("C:\\A\\Claude")
        );
        for os in ["linux", "macos", "windows"] {
            assert_eq!(
                claude_desktop_dirs(&env(&[]), os),
                Vec::<PathBuf>::new(),
                "{os}"
            );
        }
    }

    /// A state folder of its own under the system's temporary folder.
    fn a_state_dir(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("catervas-state-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn remembers_every_team_and_reads_an_old_state() {
        let directory = a_state_dir("remembers");
        std::fs::create_dir_all(&directory).expect("made");
        std::fs::write(directory.join("state.json"), r#"{"last_project":"/x"}"#).expect("written");
        assert_eq!(read_state(&directory).projects, [PathBuf::from("/x")]);

        remember(&directory, &PathBuf::from("/y")).expect("remembers");
        remember(&directory, &PathBuf::from("/x")).expect("remembers");
        let state = read_state(&directory);
        assert_eq!(state.projects, [PathBuf::from("/x"), PathBuf::from("/y")]);
        assert_eq!(state.last_project, Some(PathBuf::from("/x")));

        forget(&directory, &PathBuf::from("/x")).expect("forgets");
        assert_eq!(read_state(&directory).projects, [PathBuf::from("/y")]);

        std::fs::write(
            directory.join("state.json"),
            r#"{"projects":["/a"],"agents_at_once":3,"daily_cap_usd":12.5}"#,
        )
        .expect("written");
        remember(&directory, &PathBuf::from("/b")).expect("remembers");
        let state = read_state(&directory);
        assert_eq!(state.agents_at_once, Some(3));
        assert_eq!(state.daily_cap_usd, Some(12.5));
        assert_eq!(state.projects, [PathBuf::from("/a"), PathBuf::from("/b")]);

        std::fs::write(directory.join("state.json"), "not json").expect("written");
        assert_eq!(read_state(&directory), ServeState::default());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
