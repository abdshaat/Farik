//! Reading the connectors Claude's settings name into the machine index, never a key. Only a
//! server's name, kind, host and program name are kept; a file is opened only where
//! `docs/SPEC.md` 8.6 says, by its canonical path, and `.credentials.json`, `Claude Extensions
//! Settings/` and `mcp-needs-auth-cache.json` are never opened.

use std::io::Read;
use std::path::{Path, PathBuf};

use catervas_core::claude_services::{
    ConfiguredServer, ServerKind, ServiceSource, extension_name, mcp_servers,
    plugin_mcp_json_servers, read_claude_json, user_plugin_installs,
};

use crate::error::StoreError;
use crate::machine::{ConfiguredRow, MachineIndex};

/// The largest settings file that is read.
pub const SETTINGS_LIMIT: u64 = 16 * 1_048_576;

/// Where this computer keeps Claude's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsPaths {
    /// `~/.claude.json`, or the one under `CLAUDE_CONFIG_DIR`.
    pub claude_json: Option<PathBuf>,
    /// Claude Code's folder, holding `settings.json` and `plugins/`.
    pub claude_dir: PathBuf,
    /// Claude Desktop's folders.
    pub desktop_dirs: Vec<PathBuf>,
}

fn read_text(path: &Path) -> Option<String> {
    let mut text = String::new();
    let size = std::fs::File::open(path)
        .ok()?
        .take(SETTINGS_LIMIT + 1)
        .read_to_string(&mut text)
        .ok()?;
    (size as u64 <= SETTINGS_LIMIT).then_some(text)
}

/// Reads `folder/name` only when it is a regular file whose canonical path is exactly that file
/// inside the canonical folder, so a link pointing elsewhere is not followed.
fn read_exact_inside(folder: &Path, name: &str) -> Option<String> {
    let real = folder.join(name).canonicalize().ok()?;
    if real != folder.canonicalize().ok()?.join(name) || !real.is_file() {
        return None;
    }
    read_text(&real)
}

fn row(source: ServiceSource, found_in: &str, server: ConfiguredServer) -> ConfiguredRow {
    ConfiguredRow {
        source,
        found_in: found_in.to_string(),
        server,
    }
}

fn rows_of(
    source: ServiceSource,
    found_in: &str,
    servers: Vec<ConfiguredServer>,
    rows: &mut Vec<ConfiguredRow>,
) {
    rows.extend(
        servers
            .into_iter()
            .map(|server| row(source, found_in, server)),
    );
}

fn claude_json_rows(path: &Path, roots: &[String], rows: &mut Vec<ConfiguredRow>) {
    let Some(text) = read_text(path) else {
        return;
    };
    let read = read_claude_json(&text);
    rows_of(ServiceSource::ClaudeCode, "", read.user, rows);
    for (project, servers) in read.projects {
        rows_of(ServiceSource::ClaudeCode, &project, servers, rows);
    }
    for name in read.claude_ai {
        let server = ConfiguredServer {
            name,
            kind: ServerKind::Http,
            host: None,
            program: None,
        };
        rows.push(row(ServiceSource::ClaudeAi, "", server));
    }
    for root in roots {
        let Some(text) = read_exact_inside(Path::new(root), ".mcp.json") else {
            continue;
        };
        let disabled = read.disabled_mcp_json.get(root);
        let servers = plugin_mcp_json_servers(&text)
            .into_iter()
            .filter(|server| disabled.is_none_or(|names| !names.contains(&server.name)))
            .collect();
        rows_of(ServiceSource::ClaudeCode, root, servers, rows);
    }
}

fn plugin_rows(claude_dir: &Path, rows: &mut Vec<ConfiguredRow>) {
    let (Some(settings), Some(installed)) = (
        read_text(&claude_dir.join("settings.json")),
        read_text(&claude_dir.join("plugins/installed_plugins.json")),
    ) else {
        return;
    };
    let Ok(plugins) = claude_dir.join("plugins").canonicalize() else {
        return;
    };
    for (id, install_path) in user_plugin_installs(&settings, &installed) {
        let Ok(install) = Path::new(&install_path).canonicalize() else {
            continue;
        };
        if install == plugins || !install.starts_with(&plugins) {
            continue;
        }
        let found_in = format!("plugin:{id}");
        let inside = |relative: &str| -> Option<String> {
            let real = install.join(relative).canonicalize().ok()?;
            if !real.starts_with(&install) || !real.is_file() {
                return None;
            }
            read_text(&real)
        };
        if let Some(text) = inside(".mcp.json") {
            rows_of(
                ServiceSource::ClaudeCode,
                &found_in,
                plugin_mcp_json_servers(&text),
                rows,
            );
        }
        if let Some(text) = inside(".claude-plugin/plugin.json") {
            rows_of(
                ServiceSource::ClaudeCode,
                &found_in,
                mcp_servers(&text),
                rows,
            );
        }
    }
}

fn desktop_rows(folder: &Path, rows: &mut Vec<ConfiguredRow>) {
    let found_in = folder.to_string_lossy();
    if let Some(text) = read_exact_inside(folder, "claude_desktop_config.json") {
        rows_of(
            ServiceSource::ClaudeDesktop,
            &found_in,
            mcp_servers(&text),
            rows,
        );
    }
    let Ok(entries) = std::fs::read_dir(folder.join("Claude Extensions")) else {
        return;
    };
    let mut extensions: Vec<_> = entries.filter_map(Result::ok).collect();
    extensions.sort_by_key(std::fs::DirEntry::file_name);
    for extension in extensions {
        let Some(name) = read_exact_inside(&extension.path(), "manifest.json")
            .and_then(|text| extension_name(&text))
        else {
            continue;
        };
        let server = ConfiguredServer {
            name,
            kind: ServerKind::Stdio,
            host: None,
            program: None,
        };
        let found_in = format!("extension:{}", extension.file_name().to_string_lossy());
        rows.push(row(ServiceSource::ClaudeDesktop, &found_in, server));
    }
}

/// Reads the connectors Claude's settings name and replaces the index's snapshot of them. A file
/// that is missing, cannot be read or is not JSON adds nothing.
///
/// # Errors
///
/// `Sqlite` when the index refuses.
pub fn read_service_settings(
    index: &mut MachineIndex,
    paths: &SettingsPaths,
) -> Result<(), StoreError> {
    let roots = index.job_search()?.roots;
    let mut rows = Vec::new();
    if let Some(path) = &paths.claude_json {
        claude_json_rows(path, &roots, &mut rows);
    }
    plugin_rows(&paths.claude_dir, &mut rows);
    for folder in &paths.desktop_dirs {
        desktop_rows(folder, &mut rows);
    }
    index.replace_configured_servers(&rows)
}

/// Claude Desktop's folders under the Windows users of a WSL machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsClaude {
    /// No Windows user has one.
    None,
    /// Exactly one Windows user has some: their folders, sorted.
    One(Vec<PathBuf>),
    /// More than one user has: none is read, since another person's connectors would count as the
    /// owner's.
    Several,
}

const SHARED_USERS: [&str; 4] = ["Public", "Default", "Default User", "All Users"];

fn holds_claude(folder: &Path) -> bool {
    folder.join("claude_desktop_config.json").is_file() || folder.join("Claude Extensions").is_dir()
}

fn user_claude_dirs(user: &Path) -> Vec<PathBuf> {
    let mut found = vec![user.join("AppData/Roaming/Claude")];
    if let Ok(packages) = std::fs::read_dir(user.join("AppData/Local/Packages")) {
        for package in packages.filter_map(Result::ok) {
            if package.file_name().to_string_lossy().starts_with("Claude_") {
                found.push(package.path().join("LocalCache/Roaming/Claude"));
            }
        }
    }
    found.retain(|folder| holds_claude(folder));
    found.sort();
    found
}

/// Claude Desktop's folders under `users_root` (`/mnt/c/Users`). Only when exactly one Windows user
/// has such a folder are that user's returned.
///
/// ponytail: C: at /mnt/c only; a changed automount root is not followed.
#[must_use]
pub fn windows_claude_dirs(users_root: &Path) -> WindowsClaude {
    let Ok(users) = std::fs::read_dir(users_root) else {
        return WindowsClaude::None;
    };
    let mut having: Vec<Vec<PathBuf>> = users
        .filter_map(Result::ok)
        .filter(|user| !SHARED_USERS.contains(&user.file_name().to_string_lossy().as_ref()))
        .map(|user| user_claude_dirs(&user.path()))
        .filter(|folders| !folders.is_empty())
        .collect();
    match having.len() {
        0 => WindowsClaude::None,
        1 => WindowsClaude::One(having.remove(0)),
        _ => WindowsClaude::Several,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use catervas_core::claude_services::fixtures::{
        a_claude_json_full_of_keys, a_desktop_config_full_of_keys, a_plugin_mcp_json_full_of_keys,
    };
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;
    use serde_json::json;

    use super::*;
    use crate::files::fixtures::TempProject;
    use crate::machine::{MACHINE_DB, MachineIndex, open_machine_index};

    /// A temporary home with `.claude.json`, `.claude/`, a desktop folder, and the index beside it.
    struct Home {
        temp: TempProject,
        base: PathBuf,
    }

    impl Home {
        fn new(name: &str) -> Self {
            let temp = TempProject::new(name);
            let base = temp.root.canonicalize().expect("a real temporary folder");
            for folder in [".claude/plugins/cache", "Claude"] {
                std::fs::create_dir_all(base.join(folder)).expect("a folder");
            }
            Self { temp, base }
        }

        fn claude_dir(&self) -> PathBuf {
            self.base.join(".claude")
        }

        fn desktop(&self) -> PathBuf {
            self.base.join("Claude")
        }

        fn paths(&self) -> SettingsPaths {
            SettingsPaths {
                claude_json: Some(self.base.join(".claude.json")),
                claude_dir: self.claude_dir(),
                desktop_dirs: vec![self.desktop()],
            }
        }

        fn index(&self) -> MachineIndex {
            let now = Utc
                .with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
                .single()
                .expect("a valid time");
            open_machine_index(&self.temp.root.join("state").join(MACHINE_DB), now)
                .expect("the index opens")
        }

        /// A plugin enabled for the user, installed at `install`, written into the settings.
        fn plugins(&self, installs: &[(&str, &Path)]) {
            let enabled: serde_json::Map<String, serde_json::Value> = installs
                .iter()
                .map(|(id, _)| ((*id).to_string(), json!(true)))
                .collect();
            let installed: serde_json::Map<String, serde_json::Value> = installs
                .iter()
                .map(|(id, path)| {
                    (
                        (*id).to_string(),
                        json!([{"scope": "user", "installPath": path}]),
                    )
                })
                .collect();
            write(
                &self.claude_dir().join("settings.json"),
                &json!({"enabledPlugins": enabled}).to_string(),
            );
            write(
                &self.claude_dir().join("plugins/installed_plugins.json"),
                &json!({"version": 2, "plugins": installed}).to_string(),
            );
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
        std::fs::write(path, text).expect("a file");
    }

    fn add_root(index: &MachineIndex, root: &Path) {
        index
            .connection
            .execute(
                "INSERT INTO projects (root, is_git, ownership, present, first_seen_at, checked_at)
                 VALUES (?1, 0, 'yours', 1, '2026-10-12T09:00:00.000Z', '2026-10-12T09:00:00.000Z')",
                (root.to_string_lossy(),),
            )
            .expect("a project");
    }

    const ROWS: &str = "SELECT printf('%s|%s|%s|%s|%s|%s', source, found_in, name, kind, coalesce(host, ''), coalesce(program, '')) FROM configured_servers ORDER BY source, found_in, name";

    fn rows(home: &Home) -> Vec<String> {
        let connection =
            Connection::open(home.temp.root.join("state").join(MACHINE_DB)).expect("the index");
        let mut statement = connection.prepare(ROWS).expect("a statement");
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows")
    }

    fn http(url: &str) -> serde_json::Value {
        json!({"type": "http", "url": url})
    }

    #[test]
    fn reads_every_settings_source() {
        let home = Home::new("reads-every-settings-source");
        let mut index = home.index();
        let shop = home.base.join("shop");
        add_root(&index, &shop);
        write(
            &home.base.join(".claude.json"),
            &json!({
                "mcpServers": {"stripe": http("https://mcp.stripe.com")},
                "projects": {
                    "/p": {"mcpServers": {"acme": http("https://acme.example/mcp")}},
                    shop.to_string_lossy(): {"disabledMcpjsonServers": ["sentry"]},
                },
                "claudeAiMcpEverConnected": ["claude.ai Zoho Books"],
            })
            .to_string(),
        );
        let pm = home.claude_dir().join("plugins/cache/kw/pm/1.0");
        let notion = home.claude_dir().join("plugins/cache/kw/notion/1.0");
        home.plugins(&[("pm@kw", &pm), ("nt@kw", &notion)]);
        write(
            &pm.join(".mcp.json"),
            &json!({"mcpServers": {"slack": http("https://mcp.slack.com/mcp")}}).to_string(),
        );
        write(
            &pm.join(".claude-plugin/plugin.json"),
            &json!({"name": "pm", "mcpServers": {"linear": http("https://mcp.linear.app/mcp")}})
                .to_string(),
        );
        write(
            &notion.join(".mcp.json"),
            &json!({"notion": http("https://mcp.notion.com/mcp")}).to_string(),
        );
        write(
            &shop.join(".mcp.json"),
            &json!({"mcpServers": {
                "vercel": http("https://mcp.vercel.com"),
                "sentry": http("https://mcp.sentry.dev/mcp"),
            }})
            .to_string(),
        );
        write(
            &home.desktop().join("claude_desktop_config.json"),
            &json!({"mcpServers": {"xero": {"command": "/opt/xero-mcp"}}}).to_string(),
        );
        write(
            &home
                .desktop()
                .join("Claude Extensions/ant.zoho/manifest.json"),
            &json!({"name": "zoho-books"}).to_string(),
        );

        read_service_settings(&mut index, &home.paths()).expect("settings are read");
        let shop_name = shop.to_string_lossy();
        let desktop = home.desktop();
        let mut expected = vec![
            "claude_code||stripe|http|mcp.stripe.com|".to_string(),
            "claude_code|/p|acme|http|acme.example|".to_string(),
            "claude_ai||Zoho Books|http||".to_string(),
            "claude_code|plugin:pm@kw|slack|http|mcp.slack.com|".to_string(),
            "claude_code|plugin:pm@kw|linear|http|mcp.linear.app|".to_string(),
            "claude_code|plugin:nt@kw|notion|http|mcp.notion.com|".to_string(),
            format!("claude_code|{shop_name}|vercel|http|mcp.vercel.com|"),
            format!(
                "claude_desktop|{}|xero|stdio||xero-mcp",
                desktop.to_string_lossy()
            ),
            "claude_desktop|extension:ant.zoho|zoho-books|stdio||".to_string(),
        ];
        expected.sort();
        let mut found = rows(&home);
        found.sort();
        assert_eq!(found, expected);

        // A snapshot: what is no longer named is gone.
        write(
            &home.base.join(".claude.json"),
            &json!({"mcpServers": {}}).to_string(),
        );
        read_service_settings(&mut index, &home.paths()).expect("settings are read again");
        assert!(!rows(&home).iter().any(|row| row.contains("|stripe|")));
        assert!(rows(&home).iter().any(|row| row.contains("|slack|")));
    }

    #[test]
    fn keeps_no_key_from_any_settings_file() {
        let home = Home::new("keeps-no-key");
        let mut index = home.index();
        let shop = home.base.join("shop");
        add_root(&index, &shop);
        write(
            &home.base.join(".claude.json"),
            &a_claude_json_full_of_keys(),
        );
        write(
            &home.desktop().join("claude_desktop_config.json"),
            &a_desktop_config_full_of_keys(),
        );
        let plugin = home.claude_dir().join("plugins/cache/kw/pm/1.0");
        home.plugins(&[("pm@kw", &plugin)]);
        write(&plugin.join(".mcp.json"), &a_plugin_mcp_json_full_of_keys());
        write(&shop.join(".mcp.json"), &a_plugin_mcp_json_full_of_keys());
        for never_opened in [
            home.claude_dir().join(".credentials.json"),
            home.desktop()
                .join("Claude Extensions Settings/ant.zoho.json"),
            home.claude_dir().join("mcp-needs-auth-cache.json"),
        ] {
            write(
                &never_opened,
                "{\"mcpServers\":{\"leak\":{\"command\":\"KEY-MARKER-9\"}}}",
            );
        }
        read_service_settings(&mut index, &home.paths()).expect("settings are read");
        let found = rows(&home);
        for name in ["stripe", "acme", "xero", "slack"] {
            assert!(
                found.iter().any(|row| row.contains(&format!("|{name}|"))),
                "{name}"
            );
        }
        assert!(!found.iter().any(|row| row.contains("leak")));
        drop(index);
        let bytes =
            std::fs::read(home.temp.root.join("state").join(MACHINE_DB)).expect("the index file");
        let marker = b"KEY-MARKER";
        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
    }

    #[test]
    fn refuses_plugin_installs_outside_claudes_plugins() {
        let home = Home::new("refuses-plugin-outside");
        let mut index = home.index();
        let outside = home.base.join("outside");
        write(
            &outside.join(".mcp.json"),
            &json!({"mcpServers": {"evil": http("https://e.example")}}).to_string(),
        );
        let linked = home.claude_dir().join("plugins/cache/linked");
        symlink(&outside, &linked).expect("a link");
        home.plugins(&[("out@kw", &outside), ("link@kw", &linked)]);
        read_service_settings(&mut index, &home.paths()).expect("settings are read");
        assert!(rows(&home).is_empty(), "{:?}", rows(&home));
    }

    #[test]
    fn follows_no_link_to_a_settings_file() {
        let home = Home::new("follows-no-link");
        let mut index = home.index();
        let evil = home.base.join("elsewhere.json");
        write(
            &evil,
            &json!({"mcpServers": {"evil": http("https://e.example")}}).to_string(),
        );
        let root = home.base.join("shop");
        add_root(&index, &root);
        std::fs::create_dir_all(&root).expect("a root");
        symlink(&evil, root.join(".mcp.json")).expect("a link");
        std::fs::create_dir_all(home.desktop().join("Claude Extensions/ant.evil"))
            .expect("an extension");
        symlink(&evil, home.desktop().join("claude_desktop_config.json")).expect("a link");
        let manifest = home.base.join("manifest.json");
        write(&manifest, &json!({"name": "evil"}).to_string());
        symlink(
            &manifest,
            home.desktop()
                .join("Claude Extensions/ant.evil/manifest.json"),
        )
        .expect("a link");
        read_service_settings(&mut index, &home.paths()).expect("settings are read");
        assert!(rows(&home).is_empty(), "{:?}", rows(&home));
    }

    #[test]
    fn skips_settings_it_cannot_read() {
        let home = Home::new("skips-unreadable");
        let mut index = home.index();
        write(&home.base.join(".claude.json"), "not json");
        let plugin = home.claude_dir().join("plugins/cache/kw/pm/1.0");
        home.plugins(&[("pm@kw", &plugin)]);
        write(
            &plugin.join(".mcp.json"),
            &json!({"mcpServers": {"slack": http("https://mcp.slack.com/mcp")}}).to_string(),
        );
        let mut paths = home.paths();
        paths.desktop_dirs = vec![home.base.join("no-such-folder")];
        read_service_settings(&mut index, &paths).expect("an unreadable file is not an error");
        assert_eq!(
            rows(&home),
            vec!["claude_code|plugin:pm@kw|slack|http|mcp.slack.com|"]
        );
    }

    #[test]
    fn finds_claude_desktop_under_windows_users() {
        let temp = TempProject::new("finds-desktop-under-windows");
        let users = temp.root.join("Users");
        let config = |relative: &str| {
            let folder = users.join(relative);
            std::fs::create_dir_all(&folder).expect("a folder");
            std::fs::write(folder.join("claude_desktop_config.json"), "{}").expect("a config");
            folder
        };
        let roaming = config("Abd/AppData/Roaming/Claude");
        let store =
            config("Abd/AppData/Local/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude");
        for shared in ["Public", "Default", "Default User", "All Users"] {
            config(&format!("{shared}/AppData/Roaming/Claude"));
        }
        let mut expected = vec![roaming, store];
        expected.sort();
        assert_eq!(windows_claude_dirs(&users), WindowsClaude::One(expected));
        std::fs::create_dir_all(users.join("Bee/AppData/Roaming/Claude/Claude Extensions"))
            .expect("a second user's extensions");
        assert_eq!(windows_claude_dirs(&users), WindowsClaude::Several);
        std::fs::remove_dir_all(users.join("Bee")).expect("removed");
        std::fs::create_dir_all(users.join("Cy/AppData/Roaming/Claude")).expect("an empty folder");
        assert!(matches!(windows_claude_dirs(&users), WindowsClaude::One(_)));
        assert_eq!(
            windows_claude_dirs(&temp.root.join("no-such-users")),
            WindowsClaude::None
        );
    }
}
