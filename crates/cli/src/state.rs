//! What outlives a project: the state folder, and the `state.json` in it that remembers the last
//! project `catervas serve` served (`docs/SPEC.md` 8.1).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

/// Writes `state.json`, `{ "last_project": "<root>" }`, in `directory`, made with mode 0700 when it
/// is not there, and readable by its owner alone.
///
/// # Errors
///
/// A sentence saying what could not be made or written.
pub(crate) fn remember(directory: &Path, root: &Path) -> Result<(), String> {
    make_state_dir(directory)?;
    let file = directory.join("state.json");
    let text = serde_json::json!({ "last_project": root.to_string_lossy() }).to_string();
    catervas_runtime::write_private(&file, text.as_bytes())
        .map_err(|error| format!("{} cannot be written: {error}", file.display()))
}

/// The project `state.json` in `directory` remembers, when it says one.
pub(crate) fn last_project(directory: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(directory.join("state.json")).ok()?;
    let state: serde_json::Value = serde_json::from_str(&text).ok()?;
    state["last_project"].as_str().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::state_dir;

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
}
