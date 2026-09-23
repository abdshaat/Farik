//! The Claude Code program as a runtime (`docs/SPEC.md` section 8.2): the command line a session
//! is started with, the credential and environment it gets, and the oldest version Farik runs on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use farik_core::governor::permissions::PermissionTier;
use farik_core::team::validate_team;
use farik_store::files::yaml_value;
use serde_json::{Value, json};

use crate::daemon::{DaemonInfo, builtin_tool_tier};
use crate::session::{McpTransport, RuntimeError, SessionSpec};

/// The oldest Claude Code Farik runs on: the one its flags and stream were measured against.
pub const MIN_CLAUDE_VERSION: &str = "2.1.272";

/// Claude Code's built-in tools, as `claude` 2.1.280 lists them. `allowed_builtins` asks
/// `builtin_tool_tier` about each; a name the program does not have would be dropped from
/// `--tools` without a word, so only names it has are here.
pub const BUILTIN_TOOLS: &[&str] = &[
    "Bash",
    "CronCreate",
    "CronDelete",
    "CronList",
    "Edit",
    "EnterWorktree",
    "ExitWorktree",
    "Glob",
    "Grep",
    "NotebookEdit",
    "Read",
    "ScheduleWakeup",
    "SendMessage",
    "Skill",
    "Task",
    "TaskStop",
    "ToolSearch",
    "WebFetch",
    "WebSearch",
    "Write",
];

/// The name Farik's own MCP server has in every session; its tools are `mcp__farik__<name>`.
const FARIK_SERVER: &str = "farik";
/// The one tool ADR 0004 names: a session's shell is `farik_exec`.
const REFUSED_BUILTIN: &str = "Bash";
const API_KEY: &str = "ANTHROPIC_API_KEY";
const OAUTH_TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";
const SYSTEM_PROMPT_FILE: &str = "system-prompt.md";
const MCP_CONFIG_FILE: &str = "mcp.json";

/// A value that must not be printed: its `Debug` says `[redacted]`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Holds `value`.
    #[must_use]
    pub fn new(value: String) -> Secret {
        Secret(value)
    }

    /// The value itself, for the one place that hands it on.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

/// How a session pays: the user's API key, or the token of their subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeCredential {
    /// Passed to the program as `ANTHROPIC_API_KEY`.
    ApiKey(Secret),
    /// Passed to the program as `CLAUDE_CODE_OAUTH_TOKEN`.
    OauthToken(Secret),
}

/// The credential an environment holds: `ANTHROPIC_API_KEY` first, else
/// `CLAUDE_CODE_OAUTH_TOKEN`; a blank value is none.
#[must_use]
pub fn credential_from_env(env: &BTreeMap<String, String>) -> Option<ClaudeCredential> {
    let named = |name: &str| {
        env.get(name)
            .filter(|value| !value.trim().is_empty())
            .map(|value| Secret::new(value.clone()))
    };
    named(API_KEY)
        .map(ClaudeCredential::ApiKey)
        .or_else(|| named(OAUTH_TOKEN).map(ClaudeCredential::OauthToken))
}

/// Where the program is, what its hooks run, and what its sessions are given.
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// The `claude` program.
    pub claude_path: PathBuf,
    /// The `farik` program the hooks run.
    pub hook_command: PathBuf,
    /// The daemon's `daemon.json`, which the hooks read.
    pub daemon_file: PathBuf,
    /// The daemon itself: its port and token go into each session's MCP config.
    pub daemon: DaemonInfo,
    /// `.farik/local/sessions`: each session's prompt and MCP config, under its id.
    pub sessions_dir: PathBuf,
    /// `.farik/team.yaml`, read at each session start for the protected paths.
    pub team_file: PathBuf,
    /// The whole environment the program gets besides its credential; nothing else is inherited.
    pub env: BTreeMap<String, String>,
}

/// The built-in tools `tiers` grant, from `BUILTIN_TOOLS`, in name order. Never `Bash`, which
/// has no tier.
#[must_use]
pub fn allowed_builtins(tiers: &BTreeSet<PermissionTier>) -> Vec<String> {
    let mut allowed: Vec<String> = BUILTIN_TOOLS
        .iter()
        .filter(|tool| builtin_tool_tier(tool).is_some_and(|tier| tiers.contains(&tier)))
        .map(|tool| (*tool).to_string())
        .collect();
    allowed.sort();
    allowed
}

/// The arguments `claude` is run with for `spec`, its files in `session_dir`: a new session, or
/// with `resume` the same line with `--resume <id>` in place of `--session-id <id>`. The settings
/// are built from the team file as it is now, so a protected path added between sessions applies
/// to the next one. No argument holds the daemon's token, which is in the MCP config file.
///
/// # Errors
///
/// `Spawn` when the spec names its own `farik` server, or the team file cannot be read, since a
/// session without its protected paths would not be governed.
pub fn claude_args(
    spec: &SessionSpec,
    config: &ClaudeConfig,
    session_dir: &Path,
    resume: bool,
) -> Result<Vec<String>, RuntimeError> {
    refuse_a_farik_server(spec)?;
    let settings = settings_json(config, &protected_paths(&config.team_file)?);
    let session_flag = if resume { "--resume" } else { "--session-id" };
    let args = [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        session_flag,
        &spec.session_id,
        "--model",
        &spec.model,
        "--effort",
        &spec.effort.to_string(),
        "--append-system-prompt-file",
        &session_dir.join(SYSTEM_PROMPT_FILE).display().to_string(),
        "--tools",
        &spec.builtin_tools.join(","),
        "--disallowedTools",
        REFUSED_BUILTIN,
        "--mcp-config",
        &session_dir.join(MCP_CONFIG_FILE).display().to_string(),
        "--strict-mcp-config",
        "--permission-prompt-tool",
        "mcp__farik__permission",
        "--max-turns",
        &spec.limits.max_tool_calls.saturating_add(1).to_string(),
        "--setting-sources",
        "",
        "--settings",
        &settings.to_string(),
    ];
    Ok(args.iter().map(|arg| (*arg).to_string()).collect())
}

/// Writes `session_dir`'s two files: `system-prompt.md`, the spec's exact prompt, kept after the
/// session as the record of what it was told; and `mcp.json`, mode 0600, naming Farik's server
/// with the daemon's token and the session's id, and the spec's other servers.
///
/// # Errors
///
/// `Spawn` when the spec names its own `farik` server or a file cannot be written.
pub fn write_session_files(
    spec: &SessionSpec,
    config: &ClaudeConfig,
    session_dir: &Path,
) -> Result<(), RuntimeError> {
    refuse_a_farik_server(spec)?;
    let io = |path: &Path, error: std::io::Error| RuntimeError::Spawn {
        detail: format!("{} cannot be written: {error}", path.display()),
    };
    std::fs::create_dir_all(session_dir).map_err(|error| io(session_dir, error))?;
    let prompt = session_dir.join(SYSTEM_PROMPT_FILE);
    std::fs::write(&prompt, &spec.system_prompt).map_err(|error| io(&prompt, error))?;
    let mcp = session_dir.join(MCP_CONFIG_FILE);
    // Removed first, because a mode is only given to a file as it is created.
    match std::fs::remove_file(&mcp) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io(&mcp, error)),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&mcp)
        .and_then(|mut file| {
            file.write_all(mcp_config(spec, &config.daemon).to_string().as_bytes())
        })
        .map_err(|error| io(&mcp, error))
}

/// The program's whole environment: `base`, and the credential's one variable.
#[must_use]
pub fn child_env(
    credential: &ClaudeCredential,
    base: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = base.clone();
    env.remove(API_KEY);
    env.remove(OAUTH_TOKEN);
    let (name, secret) = match credential {
        ClaudeCredential::ApiKey(secret) => (API_KEY, secret),
        ClaudeCredential::OauthToken(secret) => (OAUTH_TOKEN, secret),
    };
    env.insert(name.to_string(), secret.expose().to_string());
    env
}

/// Reads what `claude --version` printed and refuses a version older than `MIN_CLAUDE_VERSION`.
///
/// # Errors
///
/// `VersionTooOld` for an older one; `Spawn` when the text starts with no `major.minor.patch`.
pub fn check_version(output: &str) -> Result<(), RuntimeError> {
    let found = output.split_whitespace().next().unwrap_or_default();
    let parsed = parse_version(found).ok_or_else(|| RuntimeError::Spawn {
        detail: format!("`claude --version` printed {output:?}, which names no version"),
    })?;
    let required = parse_version(MIN_CLAUDE_VERSION).ok_or_else(|| RuntimeError::Spawn {
        detail: "the minimum version is not one".to_string(),
    })?;
    if parsed < required {
        return Err(RuntimeError::VersionTooOld {
            found: found.to_string(),
            required: MIN_CLAUDE_VERSION.to_string(),
        });
    }
    Ok(())
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.').map(|part| part.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

fn refuse_a_farik_server(spec: &SessionSpec) -> Result<(), RuntimeError> {
    if spec
        .mcp_servers
        .iter()
        .any(|server| server.name == FARIK_SERVER)
    {
        return Err(RuntimeError::Spawn {
            detail: "the spec names a server `farik`, which is the name of Farik's own and is \
                     added by the runtime"
                .to_string(),
        });
    }
    Ok(())
}

/// The team's protected paths, the shipped ones among them.
fn protected_paths(team_file: &Path) -> Result<Vec<String>, RuntimeError> {
    let refused = |detail: String| RuntimeError::Spawn {
        detail: format!(
            "{} cannot be read, and a session is not started without its protected paths: {detail}",
            team_file.display()
        ),
    };
    let text = std::fs::read_to_string(team_file).map_err(|error| refused(error.to_string()))?;
    let value = yaml_value(text.strip_prefix('\u{feff}').unwrap_or(&text), "team.yaml")
        .map_err(|error| refused(error.to_string()))?;
    let team = validate_team(&value).map_err(|errors| refused(format!("{errors:?}")))?;
    Ok(team.rules().protected_paths)
}

/// `--settings`: both hooks on every tool, and a `Read` deny rule for each protected path, which
/// Claude Code also holds its search tools to.
fn settings_json(config: &ClaudeConfig, protected: &[String]) -> Value {
    let hook = |event: &str| {
        json!([{
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": format!(
                    "{} hook {event} --daemon {}",
                    shell_quoted(&config.hook_command.display().to_string()),
                    shell_quoted(&config.daemon_file.display().to_string())
                ),
            }],
        }])
    };
    json!({
        "hooks": {
            "PreToolUse": hook("pre-tool-use"),
            "PostToolUse": hook("post-tool-use"),
        },
        "permissions": {
            "deny": protected.iter().map(|path| format!("Read({path})")).collect::<Vec<_>>(),
        },
    })
}

/// `text` as one word to a POSIX shell, which is what runs a hook's command.
fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn mcp_config(spec: &SessionSpec, daemon: &DaemonInfo) -> Value {
    let mut servers = serde_json::Map::new();
    servers.insert(
        FARIK_SERVER.to_string(),
        json!({
            "type": "http",
            "url": format!("http://127.0.0.1:{}/mcp", daemon.port),
            "headers": {
                "Authorization": format!("Bearer {}", daemon.token),
                "X-Farik-Session": spec.session_id,
            },
        }),
    );
    for server in &spec.mcp_servers {
        let entry = match &server.transport {
            McpTransport::Http { url } => {
                json!({ "type": "http", "url": url, "headers": server.headers })
            }
            McpTransport::Stdio { command, args } => {
                json!({ "type": "stdio", "command": command, "args": args })
            }
        };
        servers.insert(server.name.clone(), entry);
    }
    json!({ "mcpServers": servers })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use farik_core::governor::permissions::PermissionTier;
    use farik_store::files::fixtures::{TempProject, a_team};
    use serde_json::Value;

    use super::{
        ClaudeConfig, ClaudeCredential, Secret, allowed_builtins, check_version, child_env,
        claude_args, credential_from_env, write_session_files,
    };
    use crate::daemon::DaemonInfo;
    use crate::recorded::fixtures::a_session_spec;
    use crate::session::{McpServerConfig, McpTransport, RuntimeError, SessionSpec};

    const TOKEN: &str = "0123456789abcdef-the-daemon-token";

    fn config(project: &TempProject) -> ClaudeConfig {
        ClaudeConfig {
            claude_path: PathBuf::from("/usr/local/bin/claude"),
            hook_command: PathBuf::from("/usr/local/bin/farik"),
            daemon_file: project.root.join(".farik/local/daemon.json"),
            daemon: DaemonInfo {
                port: 47_123,
                token: TOKEN.to_string(),
                pid: 1,
            },
            sessions_dir: project.root.join(".farik/local/sessions"),
            team_file: project.root.join(".farik/team.yaml"),
            env: BTreeMap::new(),
        }
    }

    fn a_project(name: &str) -> TempProject {
        let project = TempProject::new(name);
        project
            .files()
            .write_team(&a_team())
            .expect("the team is written");
        project
    }

    fn spec() -> SessionSpec {
        SessionSpec {
            builtin_tools: vec!["Read".to_string(), "Grep".to_string()],
            ..a_session_spec()
        }
    }

    fn session_dir(config: &ClaudeConfig, spec: &SessionSpec) -> PathBuf {
        config.sessions_dir.join(&spec.session_id)
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let at = args
            .iter()
            .position(|arg| arg == flag)
            .unwrap_or_else(|| panic!("{flag} is not in {args:?}"));
        &args[at + 1]
    }

    fn settings(args: &[String]) -> Value {
        serde_json::from_str(value_after(args, "--settings")).expect("the settings are JSON")
    }

    #[test]
    fn builds_the_command_line_claude_code_needs() {
        let project = a_project("claude-args");
        let config = config(&project);
        let spec = spec();
        let dir = session_dir(&config, &spec);
        let args = claude_args(&spec, &config, &dir, false).expect("the args are built");
        let flags: Vec<&str> = args
            .iter()
            .map(String::as_str)
            .filter(|arg| arg.starts_with('-'))
            .collect();
        assert_eq!(
            flags,
            vec![
                "-p",
                "--output-format",
                "--input-format",
                "--verbose",
                "--session-id",
                "--model",
                "--effort",
                "--append-system-prompt-file",
                "--tools",
                "--disallowedTools",
                "--mcp-config",
                "--strict-mcp-config",
                "--permission-prompt-tool",
                "--max-turns",
                "--setting-sources",
                "--settings",
            ]
        );
        assert_eq!(value_after(&args, "--output-format"), "stream-json");
        assert_eq!(value_after(&args, "--input-format"), "stream-json");
        assert_eq!(value_after(&args, "--session-id"), spec.session_id);
        assert_eq!(value_after(&args, "--model"), spec.model);
        assert_eq!(value_after(&args, "--effort"), "high");
        assert_eq!(
            Path::new(value_after(&args, "--append-system-prompt-file")),
            dir.join("system-prompt.md")
        );
        assert_eq!(value_after(&args, "--tools"), "Read,Grep");
        assert_eq!(value_after(&args, "--disallowedTools"), "Bash");
        assert_eq!(
            Path::new(value_after(&args, "--mcp-config")),
            dir.join("mcp.json")
        );
        assert_eq!(
            value_after(&args, "--permission-prompt-tool"),
            "mcp__farik__permission"
        );
        assert_eq!(
            value_after(&args, "--max-turns"),
            (spec.limits.max_tool_calls + 1).to_string()
        );
        assert_eq!(value_after(&args, "--setting-sources"), "");
        assert!(
            args.iter().all(|arg| !arg.contains(TOKEN)),
            "the token is on the command line: {args:?}"
        );
    }

    #[test]
    fn refuses_a_spec_that_names_its_own_farik_server() {
        let project = a_project("claude-farik-server");
        let config = config(&project);
        let spec = SessionSpec {
            mcp_servers: vec![McpServerConfig {
                name: "farik".to_string(),
                transport: McpTransport::Http {
                    url: "http://127.0.0.1:1/mcp".to_string(),
                },
                headers: BTreeMap::new(),
            }],
            ..spec()
        };
        let dir = session_dir(&config, &spec);
        assert!(matches!(
            claude_args(&spec, &config, &dir, false),
            Err(RuntimeError::Spawn { .. })
        ));
        assert!(matches!(
            write_session_files(&spec, &config, &dir),
            Err(RuntimeError::Spawn { .. })
        ));
    }

    #[test]
    fn passes_only_the_base_environment_and_one_credential() {
        let base = BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string())]);
        let api_key = ClaudeCredential::ApiKey(Secret::new("sk-key".to_string()));
        assert_eq!(
            child_env(&api_key, &base),
            BTreeMap::from([
                ("ANTHROPIC_API_KEY".to_string(), "sk-key".to_string()),
                ("PATH".to_string(), "/usr/bin".to_string()),
            ])
        );
        let token = ClaudeCredential::OauthToken(Secret::new("oauth".to_string()));
        assert_eq!(
            child_env(&token, &base),
            BTreeMap::from([
                ("CLAUDE_CODE_OAUTH_TOKEN".to_string(), "oauth".to_string()),
                ("PATH".to_string(), "/usr/bin".to_string()),
            ])
        );
    }

    #[test]
    fn puts_the_session_and_token_in_the_mcp_config_file() {
        let project = a_project("claude-mcp-file");
        let config = config(&project);
        let spec = spec();
        let dir = session_dir(&config, &spec);
        write_session_files(&spec, &config, &dir).expect("the files are written");
        let args = claude_args(&spec, &config, &dir, false).expect("the args are built");
        let path = PathBuf::from(value_after(&args, "--mcp-config"));
        let mode = std::fs::metadata(&path)
            .expect("the file is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("JSON");
        let farik = &file["mcpServers"]["farik"];
        assert_eq!(farik["type"], "http");
        assert_eq!(farik["url"], "http://127.0.0.1:47123/mcp");
        assert_eq!(farik["headers"]["Authorization"], format!("Bearer {TOKEN}"));
        assert_eq!(
            farik["headers"]["X-Farik-Session"],
            spec.session_id.as_str()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("system-prompt.md")).expect("the prompt file"),
            spec.system_prompt
        );
    }

    #[test]
    fn wires_both_hooks_and_the_protected_paths_into_the_settings() {
        let project = a_project("claude-settings");
        let config = config(&project);
        let spec = spec();
        let args = claude_args(&spec, &config, &session_dir(&config, &spec), false)
            .expect("the args are built");
        let settings = settings(&args);
        let daemon_file = config.daemon_file.display().to_string();
        for (event, command) in [
            ("PreToolUse", "pre-tool-use"),
            ("PostToolUse", "post-tool-use"),
        ] {
            let matchers = settings["hooks"][event].as_array().expect("an array");
            assert_eq!(matchers.len(), 1, "{settings}");
            assert_eq!(matchers[0]["matcher"], "*");
            let hooks = matchers[0]["hooks"].as_array().expect("an array");
            assert_eq!(hooks.len(), 1, "{settings}");
            assert_eq!(hooks[0]["type"], "command");
            let line = hooks[0]["command"].as_str().expect("a command");
            assert!(line.contains("/usr/local/bin/farik"), "{line}");
            assert!(line.contains(&format!("hook {command} --daemon")), "{line}");
            assert!(line.contains(&daemon_file), "{line}");
        }
        let deny: Vec<&str> = settings["permissions"]["deny"]
            .as_array()
            .expect("an array")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(deny.contains(&"Read(.env)"), "{deny:?}");
        assert!(deny.contains(&"Read(**/*.pem)"), "{deny:?}");
    }

    #[test]
    fn refuses_a_session_whose_team_file_cannot_be_read() {
        let project = TempProject::new("claude-no-team");
        let config = config(&project);
        let spec = spec();
        assert!(matches!(
            claude_args(&spec, &config, &session_dir(&config, &spec), false),
            Err(RuntimeError::Spawn { .. })
        ));
    }

    #[test]
    fn resumes_with_the_same_line_and_the_resume_flag() {
        let project = a_project("claude-resume");
        let config = config(&project);
        let spec = spec();
        let dir = session_dir(&config, &spec);
        let args = claude_args(&spec, &config, &dir, true).expect("the args are built");
        assert_eq!(value_after(&args, "--resume"), spec.session_id);
        assert!(!args.iter().any(|arg| arg == "--session-id"), "{args:?}");
        let started = claude_args(&spec, &config, &dir, false).expect("the args are built");
        let without = |args: &[String], flag: &str| -> Vec<String> {
            let at = args.iter().position(|arg| arg == flag).expect("the flag");
            let mut rest = args.to_vec();
            rest.drain(at..at + 2);
            rest
        };
        assert_eq!(
            without(&args, "--resume"),
            without(&started, "--session-id")
        );
    }

    #[test]
    fn allows_only_the_builtins_the_tiers_grant() {
        let read = BTreeSet::from([PermissionTier::Read]);
        assert_eq!(
            allowed_builtins(&read),
            vec!["Glob", "Grep", "Read", "ToolSearch"]
        );
        let write = BTreeSet::from([PermissionTier::Read, PermissionTier::WriteWorkspace]);
        assert_eq!(
            allowed_builtins(&write),
            vec![
                "Edit",
                "Glob",
                "Grep",
                "NotebookEdit",
                "Read",
                "ToolSearch",
                "Write"
            ]
        );
        let everything = BTreeSet::from([
            PermissionTier::Read,
            PermissionTier::WriteWorkspace,
            PermissionTier::Network,
            PermissionTier::Execute,
            PermissionTier::GitLocal,
            PermissionTier::GitRemote,
            PermissionTier::ExternalEffect,
        ]);
        let all = allowed_builtins(&everything);
        assert!(all.contains(&"WebFetch".to_string()), "{all:?}");
        assert!(!all.contains(&"Bash".to_string()), "{all:?}");
    }

    #[test]
    fn prefers_the_api_key_and_ignores_blank_values() {
        let env = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect()
        };
        assert!(matches!(
            credential_from_env(&env(&[
                ("ANTHROPIC_API_KEY", "sk-key"),
                ("CLAUDE_CODE_OAUTH_TOKEN", "oauth")
            ])),
            Some(ClaudeCredential::ApiKey(secret)) if secret.expose() == "sk-key"
        ));
        assert!(matches!(
            credential_from_env(&env(&[
                ("ANTHROPIC_API_KEY", "  "),
                ("CLAUDE_CODE_OAUTH_TOKEN", "oauth")
            ])),
            Some(ClaudeCredential::OauthToken(secret)) if secret.expose() == "oauth"
        ));
        assert!(credential_from_env(&env(&[("CLAUDE_CODE_OAUTH_TOKEN", "")])).is_none());
        assert!(credential_from_env(&env(&[])).is_none());
    }

    #[test]
    fn hides_a_secret_when_printed() {
        let credential = ClaudeCredential::ApiKey(Secret::new("sk-very-secret".to_string()));
        let printed = format!("{credential:?}");
        assert!(printed.contains("[redacted]"), "{printed}");
        assert!(!printed.contains("sk-very-secret"), "{printed}");
        let project = TempProject::new("claude-debug");
        let printed = format!("{:?}", config(&project).daemon);
        assert!(printed.contains("[redacted]"), "{printed}");
        assert!(!printed.contains(TOKEN), "{printed}");
    }

    #[test]
    fn refuses_a_claude_code_older_than_the_minimum() {
        assert_eq!(
            check_version("2.1.200 (Claude Code)\n"),
            Err(RuntimeError::VersionTooOld {
                found: "2.1.200".to_string(),
                required: "2.1.272".to_string(),
            })
        );
        assert_eq!(check_version("2.1.280 (Claude Code)\n"), Ok(()));
        assert_eq!(check_version("2.1.272 (Claude Code)"), Ok(()));
        assert_eq!(check_version("3.0.0"), Ok(()));
        assert!(matches!(
            check_version("2.0.999 (Claude Code)"),
            Err(RuntimeError::VersionTooOld { .. })
        ));
        assert!(matches!(
            check_version("not a version"),
            Err(RuntimeError::Spawn { .. })
        ));
    }
}
