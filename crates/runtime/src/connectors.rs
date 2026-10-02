//! A custom connector's keys, kept per agent (ADR 0030): in the OS keychain, or in
//! `connectors.json` in the user's state folder on a computer with no keychain.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use farik_core::team::{CustomServer, CustomTransport};

use crate::claude::Secret;
use crate::credential::{CredentialError, map_keyring_error, read_keychain};

/// The keychain entry's service.
pub const SERVICE: &str = "farik";
/// The refusal when there is neither a keychain nor a state folder to keep the keys in.
pub const NO_SECRET_STORE: &str = "no_secret_store";

/// Whose keys, for which server: one entry per project, agent and server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretAt {
    /// The project's id on this machine ([`local_project_id`]): two projects can both have an
    /// agent `theo`.
    pub project_id: String,
    /// The agent the server was given to.
    pub agent_id: String,
    /// The server's name in the team file.
    pub server: String,
}

impl SecretAt {
    /// Where `agent`'s keys for `server` are kept in the project at `root`: under the project's
    /// id on this machine, [`local_project_id`].
    ///
    /// # Errors
    ///
    /// The project's id could not be read or made.
    pub fn of(root: &std::path::Path, agent: &str, server: &str) -> std::io::Result<SecretAt> {
        Ok(SecretAt {
            project_id: local_project_id(root)?,
            agent_id: agent.to_string(),
            server: server.to_string(),
        })
    }

    /// The keychain account, and the key in `connectors.json`:
    /// `connector:<project_id>:<agent_id>:<server>`.
    #[must_use]
    pub fn account(&self) -> String {
        format!(
            "connector:{}:{}:{}",
            self.project_id, self.agent_id, self.server
        )
    }
}

/// The project at `root`'s id on this machine: 32 random hex digits, made the first time it is
/// asked for and kept in `.farik/local/project_id`, which is never committed. Not the event log's
/// `project_id`, which is the folder's name, so `~/work/app` and `~/clients/app` would share one
/// agent's keys, and a clone into a folder of the same name would find them (finding I1).
///
/// # Errors
///
/// The id could not be read, or made and kept.
pub fn local_project_id(root: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read as _;

    let file = root.join(".farik/local/project_id");
    match std::fs::read_to_string(&file) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        read => return read,
    }
    let mut random = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let id = random.iter().fold(String::new(), |mut id, byte| {
        use std::fmt::Write as _;
        let _ = write!(id, "{byte:02x}");
        id
    });
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    // Written beside and linked into place, which fails when another process got there first:
    // then its id is the one kept, and no reader ever sees a half-written file.
    let beside = file.with_extension(format!("{}.tmp", std::process::id()));
    crate::write_private(&beside, id.as_bytes())?;
    let linked = std::fs::hard_link(&beside, &file);
    let _ = std::fs::remove_file(&beside);
    match linked {
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => Err(error),
        _ => std::fs::read_to_string(&file),
    }
}

/// What `connect` kept: the hash of the definition it connected, and the keys.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectorEntry {
    /// `farik_core::team::spec_sha256` of the server as it was connected.
    pub spec_sha256: String,
    /// Each key's name and value.
    pub keys: BTreeMap<String, Secret>,
}

impl fmt::Debug for ConnectorEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: BTreeMap<&str, &str> = self
            .keys
            .keys()
            .map(|name| (name.as_str(), "***"))
            .collect();
        formatter
            .debug_struct("ConnectorEntry")
            .field("spec_sha256", &self.spec_sha256)
            .field("keys", &keys)
            .finish()
    }
}

impl ConnectorEntry {
    /// The stored form: `{"spec_sha256":…,"keys":{"<NAME>":"<value>"}}`.
    fn to_json(&self) -> serde_json::Value {
        let keys: serde_json::Map<String, serde_json::Value> = self
            .keys
            .iter()
            .map(|(name, secret)| (name.clone(), secret.expose().into()))
            .collect();
        serde_json::json!({ "spec_sha256": self.spec_sha256, "keys": keys })
    }

    /// The entry a stored form holds. The error never quotes the form, which holds the keys.
    fn from_json(value: &serde_json::Value) -> Result<ConnectorEntry, CredentialError> {
        let unreadable = || {
            CredentialError::Failed(
                "the stored connector keys are not ones farik can read".to_string(),
            )
        };
        let spec_sha256 = value["spec_sha256"].as_str().ok_or_else(unreadable)?;
        let keys = value["keys"]
            .as_object()
            .ok_or_else(unreadable)?
            .iter()
            .map(|(name, secret)| {
                let secret = secret.as_str().ok_or_else(unreadable)?;
                Ok((name.clone(), Secret::new(secret.to_string())))
            })
            .collect::<Result<_, CredentialError>>()?;
        Ok(ConnectorEntry {
            spec_sha256: spec_sha256.to_string(),
            keys,
        })
    }

    /// The entry a stored text holds.
    fn from_text(text: &str) -> Result<ConnectorEntry, CredentialError> {
        ConnectorEntry::from_json(&serde_json::from_str(text).unwrap_or_default())
    }
}

/// Where an entry was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStore {
    /// The OS keychain.
    Keychain,
    /// `connectors.json` in the state folder.
    File,
}

/// A place connector entries can be kept.
pub trait ConnectorSecrets: Send + Sync {
    /// The entry kept for `at`; `Ok(None)` when none is.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError>;
    /// The entry kept for `at`, and the store it is in; a store that is one place says so.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    fn locate(
        &self,
        at: &SecretAt,
    ) -> Result<Option<(ConnectorEntry, SecretStore)>, CredentialError> {
        Ok(self.load(at)?.map(|entry| (entry, SecretStore::Keychain)))
    }
    /// Keeps `entry` for `at`, replacing any other, and says where.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError>;
    /// Removes the entry kept for `at`, and no other; nothing kept is nothing to remove.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError>;
}

/// Opens the keychain entry of a service and account.
type OpenEntry = Arc<dyn Fn(&str, &str) -> keyring::Result<keyring::Entry> + Send + Sync>;

/// The OS keychain: service `farik`, account [`SecretAt::account`].
pub struct KeychainConnectorSecrets {
    open: OpenEntry,
}

impl Default for KeychainConnectorSecrets {
    fn default() -> KeychainConnectorSecrets {
        KeychainConnectorSecrets {
            open: Arc::new(keyring::Entry::new),
        }
    }
}

impl ConnectorSecrets for KeychainConnectorSecrets {
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError> {
        read_keychain(
            (self.open)(SERVICE, &at.account()).and_then(|entry| entry.get_password()),
            ConnectorEntry::from_text,
        )
    }

    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError> {
        (self.open)(SERVICE, &at.account())
            .and_then(|kept| kept.set_password(&entry.to_json().to_string()))
            .map(|()| SecretStore::Keychain)
            .map_err(|error| map_keyring_error(&error))
    }

    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError> {
        match (self.open)(SERVICE, &at.account()).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(&error)),
        }
    }
}

/// `connectors.json`: one object keyed by account, the file 0600 in a folder 0700.
pub struct FileConnectorSecrets {
    path: PathBuf,
}

impl FileConnectorSecrets {
    /// The store writing `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> FileConnectorSecrets {
        FileConnectorSecrets { path }
    }
}

impl FileConnectorSecrets {
    /// Every entry in the file, by account; none when there is no file.
    fn read_all(&self) -> Result<serde_json::Map<String, serde_json::Value>, CredentialError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(serde_json::Value::Object(entries)) => Ok(entries),
                _ => Err(CredentialError::Failed(format!(
                    "{} is not a file farik can read",
                    self.path.display()
                ))),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(serde_json::Map::new())
            }
            Err(error) => Err(self.failed("read", &error)),
        }
    }

    /// Writes `entries` beside the file, owner-only in an owner-only folder, and renames it over.
    fn write_all(
        &self,
        entries: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), CredentialError> {
        let beside = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        let text = serde_json::Value::Object(entries).to_string();
        crate::write_private(&beside, text.as_bytes())
            .and_then(|()| std::fs::rename(&beside, &self.path))
            .map_err(|error| {
                let _ = std::fs::remove_file(&beside);
                self.failed("write", &error)
            })
    }

    /// Reads the entries, lets `change` change them, and writes them back when it says so, all
    /// under an exclusive lock on `connectors.json.lock`: `farik connect` and the daemon may both
    /// rewrite the file, and neither may lose the other's entry.
    fn rewrite(
        &self,
        change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> bool,
    ) -> Result<(), CredentialError> {
        use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};

        let folder = self.path.parent().unwrap_or(std::path::Path::new("."));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .and_then(|()| std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)))
            .map_err(|error| self.failed("make the folder of", &error))?;
        // A file beside, not `connectors.json` itself, which the rename replaces.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(self.path.with_extension("json.lock"))
            .and_then(|lock| lock.lock().map(|()| lock))
            .map_err(|error| self.failed("lock", &error))?;
        let mut entries = self.read_all()?;
        let written = if change(&mut entries) {
            self.write_all(entries)
        } else {
            Ok(())
        };
        drop(lock);
        written
    }

    fn failed(&self, what: &str, error: &std::io::Error) -> CredentialError {
        CredentialError::Failed(format!("could not {what} {}: {error}", self.path.display()))
    }
}

impl ConnectorSecrets for FileConnectorSecrets {
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError> {
        // No lock: the file is only ever replaced whole, by a rename.
        self.read_all()?
            .get(&at.account())
            .map(ConnectorEntry::from_json)
            .transpose()
    }

    fn locate(
        &self,
        at: &SecretAt,
    ) -> Result<Option<(ConnectorEntry, SecretStore)>, CredentialError> {
        Ok(self.load(at)?.map(|entry| (entry, SecretStore::File)))
    }

    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError> {
        self.rewrite(|entries| {
            entries.insert(at.account(), entry.to_json());
            true
        })
        .map(|()| SecretStore::File)
    }

    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError> {
        self.rewrite(|entries| entries.remove(&at.account()).is_some())
    }
}

/// The keychain, then the file: `save` uses the file only when there is no keychain, and `load`
/// looks in both, so an entry saved to the file before a keychain appeared is still found.
pub struct ConnectorSecretStores {
    keychain: Arc<dyn ConnectorSecrets>,
    file: Option<FileConnectorSecrets>,
}

impl ConnectorSecretStores {
    /// The stores: `keychain`, then the file at `file` when there is a state folder.
    #[must_use]
    pub fn new(
        keychain: Arc<dyn ConnectorSecrets>,
        file: Option<PathBuf>,
    ) -> ConnectorSecretStores {
        ConnectorSecretStores {
            keychain,
            file: file.map(FileConnectorSecrets::new),
        }
    }
}

impl ConnectorSecrets for ConnectorSecretStores {
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError> {
        Ok(self.locate(at)?.map(|(entry, _)| entry))
    }

    fn locate(
        &self,
        at: &SecretAt,
    ) -> Result<Option<(ConnectorEntry, SecretStore)>, CredentialError> {
        match self.keychain.locate(at) {
            Ok(None) | Err(CredentialError::NoKeychain) => match &self.file {
                Some(file) => file.locate(at),
                None => Ok(None),
            },
            kept => kept,
        }
    }

    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError> {
        match (self.keychain.save(at, entry), &self.file) {
            (Err(CredentialError::NoKeychain), Some(file)) => file.save(at, entry),
            (Err(CredentialError::NoKeychain), None) => Err(CredentialError::Failed(format!(
                "{NO_SECRET_STORE}: this computer has no keychain and no Farik state folder to keep the keys in"
            ))),
            (kept, _) => kept,
        }
    }

    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError> {
        match self.keychain.delete(at) {
            Ok(()) | Err(CredentialError::NoKeychain) => {}
            Err(error) => return Err(error),
        }
        self.file.as_ref().map_or(Ok(()), |file| file.delete(at))
    }
}

/// Entries in memory, standing in for the keychain where nothing real may be touched: tests.
#[derive(Default)]
pub struct MemoryConnectorSecrets {
    held: Mutex<BTreeMap<String, ConnectorEntry>>,
}

impl ConnectorSecrets for MemoryConnectorSecrets {
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError> {
        Ok(crate::locked(&self.held).get(&at.account()).cloned())
    }

    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError> {
        crate::locked(&self.held).insert(at.account(), entry.clone());
        Ok(SecretStore::Keychain)
    }

    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError> {
        crate::locked(&self.held).remove(&at.account());
        Ok(())
    }
}

/// Why a server's tools could not be listed, or its launch not described.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorError {
    /// The server did not answer within thirty seconds.
    Timeout,
    /// A key the server names has no value kept for it.
    KeyMissing(String),
    /// Anything else, said in a sentence.
    Failed(String),
}

/// A tool a server listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedTool {
    /// Its name, as the server gave it.
    pub name: String,
    /// What the server says it does.
    pub description: String,
    /// Whether Claude Code would call it by this name.
    pub usable: bool,
}

/// How to start a stdio server: the program, its arguments, and its keys.
pub struct LaunchSpec {
    /// The program.
    pub command: String,
    /// Its arguments.
    pub args: Vec<String>,
    /// Each key's name and value.
    pub env: BTreeMap<String, Secret>,
}

impl fmt::Debug for LaunchSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env: BTreeMap<&str, &str> =
            self.env.keys().map(|name| (name.as_str(), "***")).collect();
        formatter
            .debug_struct("LaunchSpec")
            .field("command", &self.command)
            .field("args", &self.args)
            .field("env", &env)
            .finish()
    }
}

/// The variables a server's environment keeps from Farik's, beside its keys (ADR 0030): never
/// the model credential.
pub const KEPT_ENV: [&str; 4] = ["PATH", "HOME", "LANG", "TMPDIR"];

/// The folder a stdio connector runs in: `.farik/local/connectors/<agent>/<server>` under the
/// project `root`, made owner-only when it is not there. Never a session's worktree, which agents
/// write to: there `npx` would run a planted `node_modules/.bin`, `python -m` a planted module, and
/// a relative argument a planted script, on the host with the agent's keys (finding C1).
///
/// # Errors
///
/// The folder could not be made.
pub fn working_folder(
    root: &std::path::Path,
    agent: &str,
    server: &str,
) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt as _;

    let folder = root
        .join(".farik/local/connectors")
        .join(agent)
        .join(server);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)?;
    Ok(folder)
}

/// How long a server has to list its tools.
const LISTING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// How to start the stdio `server` with the keys in `entry`: each key it names, and no other.
///
/// # Errors
///
/// `server` is reached at a web address, or a key it names has no value in `entry`.
pub fn launch_spec(
    server: &CustomServer,
    entry: &ConnectorEntry,
) -> Result<LaunchSpec, ConnectorError> {
    let CustomTransport::Stdio { command, args } = &server.transport else {
        return Err(ConnectorError::Failed(format!(
            "{} is reached at a web address, not started",
            server.name
        )));
    };
    Ok(LaunchSpec {
        command: command.clone(),
        args: args.clone(),
        env: named_keys(server, &entry.keys)?,
    })
}

/// The headers the http `server` is sent with the keys in `entry`: each template filled.
///
/// # Errors
///
/// `server` is started, not reached at a web address, or a key a header names has no value in
/// `entry`.
pub fn launch_headers(
    server: &CustomServer,
    entry: &ConnectorEntry,
) -> Result<BTreeMap<String, Secret>, ConnectorError> {
    let CustomTransport::Http { headers, .. } = &server.transport else {
        return Err(ConnectorError::Failed(format!(
            "{} is started, not reached at a web address",
            server.name
        )));
    };
    headers
        .iter()
        .map(|(name, template)| Ok((name.clone(), Secret::new(filled(template, &entry.keys)?))))
        .collect()
}

/// The entry kept for `server` at `at`, when it was connected as the team file has it now: its
/// `spec_sha256` is the server's (ADR 0030). `Ok(None)` when none is kept, or it hashes
/// differently, so the server runs nothing and is sent no key.
///
/// # Errors
///
/// The store could not be read.
pub fn confirmed_entry(
    secrets: &dyn ConnectorSecrets,
    at: &SecretAt,
    server: &CustomServer,
) -> Result<Option<ConnectorEntry>, CredentialError> {
    Ok(secrets
        .load(at)?
        .filter(|entry| entry.spec_sha256 == farik_core::team::spec_sha256(server)))
}

/// Each key `server` names, with its value from `keys`.
fn named_keys(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
) -> Result<BTreeMap<String, Secret>, ConnectorError> {
    server
        .credential_keys
        .iter()
        .map(|name| {
            keys.get(name)
                .map(|value| (name.clone(), value.clone()))
                .ok_or_else(|| ConnectorError::KeyMissing(name.clone()))
        })
        .collect()
}

/// Each header template with every `{KEY}` replaced by its value.
fn filled_headers(
    templates: &BTreeMap<String, String>,
    keys: &BTreeMap<String, Secret>,
) -> Result<
    std::collections::HashMap<axum::http::HeaderName, axum::http::HeaderValue>,
    ConnectorError,
> {
    templates
        .iter()
        .map(|(name, template)| {
            // The error never quotes the value, which holds a key.
            let invalid =
                || ConnectorError::Failed(format!("the header {name} is not one HTTP can send"));
            Ok((
                axum::http::HeaderName::try_from(name.as_str()).map_err(|_| invalid())?,
                axum::http::HeaderValue::try_from(filled(template, keys)?)
                    .map_err(|_| invalid())?,
            ))
        })
        .collect()
}

/// `template` with each `{KEY}` replaced by its value, in one pass, so a value is never read as
/// a template itself.
fn filled(template: &str, keys: &BTreeMap<String, Secret>) -> Result<String, ConnectorError> {
    let mut filled = String::new();
    let mut rest = template;
    while let Some((before, after)) = rest.split_once('{') {
        let Some((key, after)) = after.split_once('}') else {
            break;
        };
        let value = keys
            .get(key)
            .ok_or_else(|| ConnectorError::KeyMissing(key.to_string()))?;
        filled.push_str(before);
        filled.push_str(value.expose());
        rest = after;
    }
    filled.push_str(rest);
    Ok(filled)
}

/// Whether Claude Code calls a tool by this name: it rewrites any other character, so the hook's
/// lookup and `--disallowedTools` would miss it.
fn usable_tool_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The tools `server` lists when started, or reached, with `keys`. A stdio server runs in `folder`
/// with only [`KEPT_ENV`] and its keys; the whole listing gives up after thirty seconds.
///
/// # Errors
///
/// A key is missing, the server could not be started or reached, it did not answer as MCP, or it
/// took longer than thirty seconds.
pub async fn list_tools(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
    folder: &std::path::Path,
) -> Result<Vec<ListedTool>, ConnectorError> {
    use rmcp::ServiceExt as _;

    let failed = |what: &str, error: &dyn fmt::Display| {
        ConnectorError::Failed(format!("{} {what}: {error}", server.name))
    };
    // What failed, without the server's own words: an MCP error may quote what it was sent, keys
    // among it, and this reaches the person and the RPC reply (carry M12).
    let plain = |what: &str| ConnectorError::Failed(format!("{} {what}", server.name));
    let listing = async {
        let client = match &server.transport {
            CustomTransport::Stdio { command, args } => {
                let mut process = tokio::process::Command::new(command);
                // rmcp kills the server when the transport is dropped, as on the timeout below;
                // this is the same promise again, should rmcp stop keeping it.
                process
                    .args(args)
                    .current_dir(folder)
                    .env_clear()
                    .kill_on_drop(true);
                for name in KEPT_ENV {
                    if let Some(value) = std::env::var_os(name) {
                        process.env(name, value);
                    }
                }
                for (name, value) in named_keys(server, keys)? {
                    process.env(name, value.expose());
                }
                let transport = rmcp::transport::TokioChildProcess::builder(process)
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|error| failed("could not be started", &error))?
                    .0;
                ().serve(transport).await
            }
            CustomTransport::Http { url, headers } => {
                let config = rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url.as_str())
                    .custom_headers(filled_headers(headers, keys)?);
                ().serve(rmcp::transport::StreamableHttpClientTransport::from_config(config)).await
            }
        }
        .map_err(|_| plain("did not answer as an MCP server"))?;
        let tools = client
            .list_all_tools()
            .await
            .map_err(|_| plain("could not list its tools"));
        let _ = client.cancel().await;
        Ok(tools?
            .into_iter()
            .map(|tool| ListedTool {
                usable: usable_tool_name(&tool.name),
                name: tool.name.into_owned(),
                description: tool
                    .description
                    .map(std::borrow::Cow::into_owned)
                    .unwrap_or_default(),
            })
            .collect())
    };
    tokio::time::timeout(LISTING_TIMEOUT, listing)
        .await
        .unwrap_or(Err(ConnectorError::Timeout))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use keyring_core::api::CredentialStoreApi as _;

    use super::*;

    fn at(agent: &str, server: &str) -> SecretAt {
        SecretAt {
            project_id: "p".to_string(),
            agent_id: agent.to_string(),
            server: server.to_string(),
        }
    }

    fn entry(hash: &str) -> ConnectorEntry {
        ConnectorEntry {
            spec_sha256: hash.to_string(),
            keys: BTreeMap::from([(
                "API_KEY".to_string(),
                Secret::new("ghp-secret-value".to_string()),
            )]),
        }
    }

    /// The keychain code over keyring's in-memory mock store, never the computer's keychain.
    fn mock_keychain() -> (KeychainConnectorSecrets, Arc<keyring_core::mock::Store>) {
        let store = keyring_core::mock::Store::new().expect("the mock store");
        let held = Arc::clone(&store);
        let keychain = KeychainConnectorSecrets {
            open: Arc::new(move |service: &str, account: &str| {
                held.build(service, account, None)
                    .map(|inner| keyring::Entry { inner })
            }),
        };
        (keychain, store)
    }

    fn no_keychain() -> KeychainConnectorSecrets {
        KeychainConnectorSecrets {
            open: Arc::new(|_: &str, _: &str| Err(keyring::Error::NoDefaultStore)),
        }
    }

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("farik-connectors-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn keeps_keys_under_the_project_agent_and_server() {
        let theo = at("theo", "github");
        assert_eq!(theo.account(), "connector:p:theo:github");
        let (keychain, store) = mock_keychain();
        assert_eq!(keychain.load(&theo), Ok(None));
        assert_eq!(
            keychain.save(&theo, &entry("abc")),
            Ok(SecretStore::Keychain)
        );
        assert_eq!(keychain.load(&theo), Ok(Some(entry("abc"))));

        let kept = store
            .build("farik", "connector:p:theo:github", None)
            .expect("the entry")
            .get_password()
            .expect("the entry holds the keys");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&kept).expect("the entry is JSON"),
            serde_json::json!({ "spec_sha256": "abc", "keys": { "API_KEY": "ghp-secret-value" } })
        );

        let other_project = SecretAt {
            project_id: "q".to_string(),
            ..theo.clone()
        };
        assert_eq!(other_project.account(), "connector:q:theo:github");
        assert_eq!(keychain.load(&other_project), Ok(None));
    }

    #[test]
    fn two_projects_in_folders_of_one_name_keep_their_keys_apart() {
        // `~/work/app` and `~/clients/app`: a folder's name is no project's address (finding I1).
        let dir = scratch("same-name");
        let (work, clients) = (dir.join("work/app"), dir.join("clients/app"));
        for root in [&work, &clients] {
            std::fs::create_dir_all(root).expect("the project is made");
        }
        let at_work = SecretAt::of(&work, "theo", "github").expect("an address");
        let at_clients = SecretAt::of(&clients, "theo", "github").expect("an address");
        assert_ne!(at_work.account(), at_clients.account());
        // The id is made once, kept in `.farik/local/`, owner-only, and read back after.
        assert_eq!(
            SecretAt::of(&work, "iris", "github")
                .expect("an address")
                .project_id,
            at_work.project_id
        );
        let file = work.join(".farik/local/project_id");
        assert_eq!(
            std::fs::read_to_string(&file).expect("the id is kept"),
            at_work.project_id
        );
        assert_eq!(
            std::fs::metadata(&file)
                .expect("it is there")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(at_work.project_id.len(), 32, "{}", at_work.project_id);
    }

    #[test]
    fn deleting_one_agents_keys_leaves_anothers() {
        let (keychain, _store) = mock_keychain();
        let dir = scratch("delete");
        let stores: [Box<dyn ConnectorSecrets>; 3] = [
            Box::new(keychain),
            Box::new(FileConnectorSecrets::new(dir.join("connectors.json"))),
            Box::new(MemoryConnectorSecrets::default()),
        ];
        for store in &stores {
            store
                .save(&at("theo", "github"), &entry("t"))
                .expect("kept");
            store
                .save(&at("iris", "github"), &entry("i"))
                .expect("kept");
            assert_eq!(store.load(&at("theo", "github")), Ok(Some(entry("t"))));
            store.delete(&at("theo", "github")).expect("removed");
            assert_eq!(store.load(&at("theo", "github")), Ok(None));
            assert_eq!(store.load(&at("iris", "github")), Ok(Some(entry("i"))));
            // Nothing kept is nothing to remove.
            store
                .delete(&at("theo", "github"))
                .expect("nothing to remove");
        }
    }

    #[test]
    fn maps_no_keychain_to_no_keychain() {
        let keychain = no_keychain();
        assert_eq!(
            keychain.load(&at("theo", "github")),
            Err(CredentialError::NoKeychain)
        );
        assert_eq!(
            keychain.save(&at("theo", "github"), &entry("abc")),
            Err(CredentialError::NoKeychain)
        );
        assert_eq!(
            keychain.delete(&at("theo", "github")),
            Err(CredentialError::NoKeychain)
        );
    }

    #[test]
    fn falls_back_to_the_private_file_without_a_keychain() {
        let dir = scratch("fallback");
        let file = dir.join("connectors.json");
        let stores = ConnectorSecretStores::new(Arc::new(no_keychain()), Some(file.clone()));
        assert_eq!(
            stores.save(&at("theo", "github"), &entry("abc")),
            Ok(SecretStore::File)
        );
        assert_eq!(stores.load(&at("theo", "github")), Ok(Some(entry("abc"))));
        let text = std::fs::read_to_string(&file).expect("the file reads");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).expect("the file is JSON"),
            serde_json::json!({ "connector:p:theo:github": {
                "spec_sha256": "abc", "keys": { "API_KEY": "ghp-secret-value" }
            } })
        );

        // A keychain that appears later is tried first, and the file's entry is still found.
        let keychain = Arc::new(MemoryConnectorSecrets::default());
        let stores = ConnectorSecretStores::new(keychain.clone(), Some(file.clone()));
        assert_eq!(stores.load(&at("theo", "github")), Ok(Some(entry("abc"))));
        assert_eq!(
            stores.save(&at("iris", "github"), &entry("i")),
            Ok(SecretStore::Keychain)
        );
        assert_eq!(keychain.load(&at("iris", "github")), Ok(Some(entry("i"))));
        stores.delete(&at("theo", "github")).expect("removed");
        assert_eq!(stores.load(&at("theo", "github")), Ok(None));

        // A keychain that refuses is not passed over.
        let refusing = KeychainConnectorSecrets {
            open: Arc::new(|_: &str, _: &str| {
                Err(keyring::Error::PlatformFailure(Box::new(
                    std::io::Error::other("the collection is locked"),
                )))
            }),
        };
        let stores = ConnectorSecretStores::new(Arc::new(refusing), Some(file));
        assert!(matches!(
            stores.save(&at("theo", "github"), &entry("abc")),
            Err(CredentialError::Failed(_))
        ));
    }

    #[test]
    fn connectors_json_is_owner_only() {
        let dir = scratch("modes");
        let folder = dir.join("farik");
        let file = folder.join("connectors.json");
        let store = FileConnectorSecrets::new(file.clone());
        store
            .save(&at("theo", "github"), &entry("abc"))
            .expect("kept");
        let mode = |path: &std::path::Path| {
            std::fs::metadata(path)
                .expect("it is there")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&folder.join("connectors.json.lock")), 0o600);
        // Written beside and renamed over: nothing else is left in the folder but the lock.
        let mut names: Vec<_> = std::fs::read_dir(&folder)
            .expect("the folder reads")
            .map(|item| item.expect("an item").file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["connectors.json", "connectors.json.lock"]);
    }

    #[test]
    fn refuses_with_no_store_at_all() {
        let stores = ConnectorSecretStores::new(Arc::new(no_keychain()), None);
        assert!(matches!(
            stores.save(&at("theo", "github"), &entry("abc")),
            Err(CredentialError::Failed(why)) if why.starts_with(NO_SECRET_STORE)
        ));
        assert_eq!(stores.load(&at("theo", "github")), Ok(None));
    }

    /// Set in the children `two_processes_saving_at_once_lose_no_entry` runs itself as: the
    /// file both write, and the agent whose entries this one saves.
    const WRITER_FILE: &str = "FARIK_TEST_CONNECTORS_FILE";
    const WRITER_AGENT: &str = "FARIK_TEST_CONNECTORS_AGENT";
    const SAVES: usize = 40;

    #[test]
    fn two_processes_saving_at_once_lose_no_entry() {
        // `farik connect` and the daemon may both rewrite `connectors.json`: two processes, each
        // with its own handle on the file.
        if let (Some(file), Some(agent)) = (
            std::env::var_os(WRITER_FILE),
            std::env::var(WRITER_AGENT).ok(),
        ) {
            let store = FileConnectorSecrets::new(PathBuf::from(file));
            for index in 0..SAVES {
                store
                    .save(&at(&agent, &format!("s{index}")), &entry("h"))
                    .expect("kept");
            }
            return;
        }
        let dir = scratch("two-writers");
        let file = dir.join("connectors.json");
        let writers: Vec<_> = ["theo", "iris"]
            .map(|agent| {
                std::process::Command::new(std::env::current_exe().expect("the test binary"))
                    .args([
                        "--exact",
                        "connectors::tests::two_processes_saving_at_once_lose_no_entry",
                    ])
                    .env(WRITER_FILE, &file)
                    .env(WRITER_AGENT, agent)
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .expect("the test runs itself")
            })
            .into();
        for mut writer in writers {
            assert!(writer.wait().expect("the writer ends").success());
        }
        let store = FileConnectorSecrets::new(file);
        for agent in ["theo", "iris"] {
            for index in 0..SAVES {
                let server = format!("s{index}");
                assert_eq!(
                    store.load(&at(agent, &server)),
                    Ok(Some(entry("h"))),
                    "{agent} {server}"
                );
            }
        }
    }

    fn stdio(credential_keys: &[&str]) -> CustomServer {
        CustomServer {
            name: "github".to_string(),
            transport: CustomTransport::Stdio {
                command: "github-mcp".to_string(),
                args: vec!["stdio".to_string()],
            },
            credential_keys: credential_keys.iter().map(ToString::to_string).collect(),
            tools: BTreeMap::new(),
        }
    }

    #[test]
    fn a_launch_spec_never_prints() {
        let spec = launch_spec(&stdio(&["API_KEY"]), &entry("abc")).expect("a stdio server");
        assert_eq!(spec.command, "github-mcp");
        assert_eq!(spec.args, ["stdio"]);
        assert_eq!(
            spec.env.get("API_KEY").map(Secret::expose),
            Some("ghp-secret-value")
        );
        let printed = format!("{spec:?}");
        assert!(printed.contains("API_KEY"), "{printed}");
        assert!(printed.contains("***"), "{printed}");
        assert!(!printed.contains("ghp-secret-value"), "{printed}");

        // Only the keys the server names, and each of them.
        assert_eq!(
            launch_spec(&stdio(&[]), &entry("abc")).map(|spec| spec.env.len()),
            Ok(0)
        );
        assert_eq!(
            launch_spec(&stdio(&["API_KEY", "TOKEN"]), &entry("abc")).err(),
            Some(ConnectorError::KeyMissing("TOKEN".to_string()))
        );
        let http = CustomServer {
            transport: CustomTransport::Http {
                url: "https://x.example/mcp".to_string(),
                headers: BTreeMap::new(),
            },
            ..stdio(&[])
        };
        assert!(matches!(
            launch_spec(&http, &entry("abc")),
            Err(ConnectorError::Failed(_))
        ));
    }

    #[test]
    fn fills_a_header_in_one_pass() {
        // A value holding `{TOKEN}` is sent as it is, never read as a template itself (carry R8).
        let keys = BTreeMap::from([
            ("API_KEY".to_string(), Secret::new("{TOKEN}".to_string())),
            ("TOKEN".to_string(), Secret::new("t".to_string())),
        ]);
        assert_eq!(
            filled("Bearer {API_KEY} {TOKEN}", &keys),
            Ok("Bearer {TOKEN} t".to_string())
        );
    }

    #[test]
    fn a_secret_never_prints() {
        let printed = format!("{:?}", entry("abc"));
        assert!(printed.contains("API_KEY"), "{printed}");
        assert!(printed.contains("***"), "{printed}");
        assert!(printed.contains("abc"), "{printed}");
        assert!(!printed.contains("ghp-secret-value"), "{printed}");
    }
}
