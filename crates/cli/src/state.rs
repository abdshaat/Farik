//! What outlives a project: the state folder, and the `state.json` in it that remembers the last
//! project `farik serve` served (`docs/SPEC.md` 8.1).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The state folder: `$XDG_CONFIG_HOME/farik`, else `$HOME/.config/farik`, else `%APPDATA%\farik`,
/// and `None` when none is set, when nothing is remembered. The environment is the harness's, not
/// the process's, so a test sets its own.
pub(crate) fn state_dir(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let set = |name: &str| env.get(name).filter(|value| !value.is_empty());
    set("XDG_CONFIG_HOME")
        .map(|base| PathBuf::from(base).join("farik"))
        .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".config/farik")))
        .or_else(|| set("APPDATA").map(|base| PathBuf::from(base).join("farik")))
}

/// Writes `state.json`, `{ "last_project": "<root>" }`, in `directory`, made with mode 0700 when it
/// is not there, and readable by its owner alone.
///
/// # Errors
///
/// A sentence saying what could not be made or written.
pub(crate) fn remember(directory: &Path, root: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt as _;

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|error| format!("{} cannot be made: {error}", directory.display()))?;
    let file = directory.join("state.json");
    let text = serde_json::json!({ "last_project": root.to_string_lossy() }).to_string();
    farik_runtime::write_private(&file, text.as_bytes())
        .map_err(|error| format!("{} cannot be written: {error}", file.display()))
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
        assert_eq!(state_dir(&all), Some(PathBuf::from("/x/farik")));
        let no_xdg = env(&[("HOME", "/h"), ("APPDATA", "/a")]);
        assert_eq!(state_dir(&no_xdg), Some(PathBuf::from("/h/.config/farik")));
        let only_appdata = env(&[("APPDATA", "/a")]);
        assert_eq!(state_dir(&only_appdata), Some(PathBuf::from("/a/farik")));
        assert_eq!(state_dir(&env(&[])), None);
    }
}
