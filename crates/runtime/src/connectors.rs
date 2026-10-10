//! A custom connector's keys, kept per agent (ADR 0030): in the OS keychain, or in
//! `connectors.json` in the user's state folder on a computer with no keychain.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use catervas_core::team::{CustomServer, CustomTransport};

use crate::claude::Secret;
use crate::credential::{CredentialError, map_keyring_error, read_keychain};
use crate::sign_in::OAuthGrant;

/// The keychain entry's service.
pub const SERVICE: &str = "catervas";
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
    /// id on this machine, [`local_project_id`], kept in the user's state folder `state`.
    ///
    /// # Errors
    ///
    /// The project's id could not be read or made.
    pub fn of(
        state: &std::path::Path,
        root: &std::path::Path,
        agent: &str,
        server: &str,
    ) -> std::io::Result<SecretAt> {
        Ok(SecretAt {
            project_id: local_project_id(state, root)?,
            agent_id: agent.to_string(),
            server: server.to_string(),
        })
    }

    /// What a mailbox's password is kept under (step 10f): the account of a connector with no
    /// agent, `mailbox:<project_id>:<purpose>`.
    #[must_use]
    pub fn mailbox(project_id: &str, purpose: &str) -> SecretAt {
        SecretAt {
            project_id: project_id.to_string(),
            agent_id: String::new(),
            server: purpose.to_string(),
        }
    }

    /// The keychain account, and the key in `connectors.json`:
    /// `connector:<project_id>:<agent_id>:<server>`; a mailbox's, which has no agent, is
    /// `mailbox:<project_id>:<purpose>`.
    #[must_use]
    pub fn account(&self) -> String {
        if self.agent_id.is_empty() {
            return format!("mailbox:{}:{}", self.project_id, self.server);
        }
        format!(
            "connector:{}:{}:{}",
            self.project_id, self.agent_id, self.server
        )
    }
}

/// The project at `root`'s id on this machine: 32 random hex digits, made the first time it is
/// asked for and kept in the user's state folder `state`, at `projects/<sha256 of the root's
/// canonical path>`. Not the event log's `project_id`, which is the folder's name, so `~/work/app`
/// and `~/clients/app` would share one agent's keys (finding I1); and not a file in the project,
/// which `cp -r app app2` copies, so the copy found the first one's keys (re-review N3). A
/// project moved or copied is another project here, and is connected again.
///
/// # Errors
///
/// The id could not be read, or made and kept, or what is kept is not 32 hex digits.
pub fn local_project_id(
    state: &std::path::Path,
    root: &std::path::Path,
) -> std::io::Result<String> {
    use sha2::Digest as _;
    use std::io::Read as _;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::DirBuilderExt as _;

    let canonical = root.canonicalize()?;
    let file = state
        .join("projects")
        .join(hex(&sha2::Sha256::digest(canonical.as_os_str().as_bytes())));
    match std::fs::read_to_string(&file) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        read => return read.and_then(checked_id),
    }
    let mut random = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    if let Some(folder) = file.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)?;
    }
    keep_id(&file, &hex(&random))
}

/// Keeps `id` at `file` unless an id is there already, and answers the one kept. Written beside and
/// linked into place, which fails when another process got there first: then its id is the one
/// kept, and no reader ever sees a half-written file.
fn keep_id(file: &std::path::Path, id: &str) -> std::io::Result<String> {
    let beside = file.with_extension(format!("{}.tmp", std::process::id()));
    crate::write_private(&beside, id.as_bytes())?;
    let linked = std::fs::hard_link(&beside, file);
    let _ = std::fs::remove_file(&beside);
    match linked {
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => Err(error),
        _ => std::fs::read_to_string(file).and_then(checked_id),
    }
}

/// `id` when it is 32 lowercase hex digits, the only ids Catervas makes: an account name, and a
/// folder's, are made of it.
fn checked_id(id: String) -> std::io::Result<String> {
    if id.len() == 32
        && id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        Ok(id)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the project's id on this computer is not 32 hex digits",
        ))
    }
}

/// `bytes` in lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// What `connect` kept: the hash of the definition it connected, and the keys.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectorEntry {
    /// `catervas_core::team::spec_sha256` of the server as it was connected.
    pub spec_sha256: String,
    /// Each key's name and value.
    pub keys: BTreeMap<String, Secret>,
    /// The agent's sign-in to the service, when it signed in instead of pasting a key.
    pub oauth: Option<OAuthGrant>,
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
            .field("oauth", &self.oauth)
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
        let mut stored = serde_json::json!({ "spec_sha256": self.spec_sha256, "keys": keys });
        if let Some(grant) = &self.oauth {
            stored["oauth"] = grant.to_json();
        }
        stored
    }

    /// The entry a stored form holds. The error never quotes the form, which holds the keys.
    fn from_json(value: &serde_json::Value) -> Result<ConnectorEntry, CredentialError> {
        let unreadable = || {
            CredentialError::Failed(
                "the stored connector keys are not ones catervas can read".to_string(),
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
        let oauth = match &value["oauth"] {
            serde_json::Value::Null => None,
            stored => Some(OAuthGrant::from_json(stored).ok_or_else(unreadable)?),
        };
        Ok(ConnectorEntry {
            spec_sha256: spec_sha256.to_string(),
            keys,
            oauth,
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

/// The OS keychain: service `catervas`, account [`SecretAt::account`].
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
                    "{} is not a file catervas can read",
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
    /// under an exclusive lock on `connectors.json.lock`: `catervas connect` and the daemon may both
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
                "{NO_SECRET_STORE}: this computer has no keychain and no Catervas state folder to keep the keys in"
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
    /// A tool Catervas called itself ([`call_tool`]) answered that it failed: a result marked as an
    /// error, or a JSON-RPC error. The service's own words, cut at 500 characters, which Catervas
    /// passes on only as untrusted text.
    ToolError {
        /// What the service said.
        text: String,
    },
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

/// The variables a server's environment keeps from Catervas's, beside its keys (ADR 0030): never
/// the model credential.
pub const KEPT_ENV: [&str; 4] = ["PATH", "HOME", "LANG", "TMPDIR"];

/// The folder a stdio connector runs in, outside the repository: `connectors/<project id>/<agent>/
/// <server>` in the user's state folder `state`, removed and made again, empty and owner-only, for
/// every listing and launch, and refused when `state` is inside the project at `root`. Never under
/// the project: git checks out a force-added file there on a
/// clone or a pull, and `npx` and `uv` look upward to the repository's root for `.npmrc`,
/// `node_modules/.bin` and `pyproject.toml`, so a pulled commit chose what ran on the host with the
/// agent's keys (finding C1).
///
/// # Errors
///
/// `state` is inside the project, or the folder could not be removed or made.
pub fn working_folder(
    state: &std::path::Path,
    root: &std::path::Path,
    at: &SecretAt,
) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt as _;

    // Re-review 2 m1: a state folder inside the project, by `XDG_CONFIG_HOME` or a project that
    // is home itself, would put the folder back in the repository.
    if state.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(std::io::Error::other(format!(
            "{STATE_INSIDE_PROJECT}: Catervas's settings folder, {}, is inside this project, so a connector would run among \
             the project's files. Keep the project and Catervas's settings apart: unset \
             XDG_CONFIG_HOME, or move the project into a folder of its own.",
            state.display()
        )));
    }
    let folder = state
        .join("connectors")
        .join(&at.project_id)
        .join(&at.agent_id)
        .join(&at.server);
    match std::fs::remove_dir_all(&folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)?;
    Ok(folder)
}

/// The code [`working_folder`] refuses a state folder inside the project with.
pub const STATE_INSIDE_PROJECT: &str = "state_inside_project";

/// What to say when [`working_folder`] fails: its refusal as it is, which leads with its code,
/// or that the folder could not be made.
#[must_use]
pub fn folder_refusal(error: &std::io::Error) -> String {
    let said = error.to_string();
    if said.starts_with(STATE_INSIDE_PROJECT) {
        said
    } else {
        format!("its folder could not be made: {said}")
    }
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
    let CustomTransport::Stdio { command, args, .. } = &server.transport else {
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

/// The headers the http `server` is sent with the keys in `entry`: each template filled, and
/// `Authorization: Bearer <access token>` when the entry holds a sign-in.
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
    let mut sent: BTreeMap<String, Secret> = headers
        .iter()
        .map(|(name, template)| Ok((name.clone(), Secret::new(filled(template, &entry.keys)?))))
        .collect::<Result<_, ConnectorError>>()?;
    if let Some(grant) = &entry.oauth {
        sent.retain(|name, _| !name.eq_ignore_ascii_case("authorization"));
        sent.insert(
            "Authorization".to_string(),
            Secret::new(format!("Bearer {}", grant.access_token.expose())),
        );
    }
    Ok(sent)
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
        .filter(|entry| entry.spec_sha256 == catervas_core::team::spec_sha256(server)))
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

/// The program that starts a stdio server: `catervas` itself for Catervas's own connector (the exact
/// pair [`catervas_roles::is_catervas_connector`] holds for), whatever `catervas` the `PATH` might find,
/// and `command` as given for everything else (ADR 0038).
#[must_use]
pub fn program(command: &str, args: &[String], catervas: &std::path::Path) -> PathBuf {
    if catervas_roles::is_catervas_connector(command, args) {
        catervas.to_path_buf()
    } else {
        PathBuf::from(command)
    }
}

/// What a caller of [`list_tools`] says when Catervas cannot find its own program.
pub const NO_OWN_PROGRAM: &str = "catervas could not find its own program";

/// The program Catervas's own connector runs: `own`, Catervas's own executable as the process found it
/// at its start (`None` when it could not), for the exact pair
/// [`catervas_roles::is_catervas_connector`] holds for. Any other command gets an empty path it never
/// uses, so a failure to find this program stops only a Catervas connector and no `PATH` lookup ever
/// stands in for it (ADR 0038).
///
/// # Errors
///
/// [`NO_OWN_PROGRAM`], when the command is Catervas's own connector and `own` is `None`.
pub fn own_program_for(
    command: &str,
    args: &[String],
    own: Option<&std::path::Path>,
) -> Result<PathBuf, &'static str> {
    if catervas_roles::is_catervas_connector(command, args) {
        own.map(std::path::Path::to_path_buf).ok_or(NO_OWN_PROGRAM)
    } else {
        Ok(PathBuf::new())
    }
}

/// [`own_program_for`] a server's command, for the callers of [`list_tools`].
///
/// # Errors
///
/// As [`own_program_for`].
pub fn own_program(
    server: &CustomServer,
    own: Option<&std::path::Path>,
) -> Result<PathBuf, &'static str> {
    match &server.transport {
        CustomTransport::Stdio { command, args, .. } => own_program_for(command, args, own),
        CustomTransport::Http { .. } => Ok(PathBuf::new()),
    }
}

/// The tools `server` lists when started, or reached, with `keys`, and `bearer` as its `Authorization`. A stdio server runs in `folder`
/// with only [`KEPT_ENV`] and its keys; the whole listing gives up after thirty seconds. `catervas` is
/// the program Catervas's own connector runs ([`program`]); no other server uses it.
///
/// # Errors
///
/// A key is missing, the server could not be started or reached, it did not answer as MCP, or it
/// took longer than thirty seconds.
pub async fn list_tools(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
    bearer: Option<&Secret>,
    folder: &std::path::Path,
    catervas: &std::path::Path,
) -> Result<Vec<ListedTool>, ConnectorError> {
    // What failed, without the server's own words: an MCP error may quote what it was sent, keys
    // among it, and this reaches the person and the RPC reply (carry M12).
    let plain = |what: &str| ConnectorError::Failed(format!("{} {what}", server.name));
    let listing = async {
        let client = connect(server, keys, bearer, folder, catervas).await?;
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

/// The most characters of a service's own words a [`ConnectorError::ToolError`] keeps.
const TOOL_ERROR_CHARACTERS: usize = 500;

/// Calls `tool` of `server` with `arguments`, as Catervas itself, not as an agent: started or reached
/// as [`list_tools`] does it (a stdio server in `folder` with only [`KEPT_ENV`] and its keys, an
/// http one with its headers filled from `keys` and `bearer` as its `Authorization`), one MCP
/// session for the call, opened, called and cancelled, all within thirty seconds. `catervas` is the
/// program Catervas's own connector runs ([`program`]).
///
/// The answer is the result's structured content when it has some, else its first text block read
/// as JSON, else `{ "text": <that text> }`. The arguments are Catervas's own and hold no key.
///
/// # Errors
///
/// As [`list_tools`] for starting, reaching and the thirty seconds; [`ConnectorError::ToolError`]
/// when the result is marked as an error or the service answers with a JSON-RPC error.
pub async fn call_tool(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
    bearer: Option<&Secret>,
    folder: &std::path::Path,
    catervas: &std::path::Path,
    tool: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, ConnectorError> {
    let call = async {
        let client = connect(server, keys, bearer, folder, catervas).await?;
        let answered = client
            .call_tool(
                rmcp::model::CallToolRequestParams::new(tool.to_string()).with_arguments(arguments),
            )
            .await;
        let _ = client.cancel().await;
        match answered {
            Ok(result) => read_result(&result),
            Err(rmcp::service::ServiceError::McpError(error)) => Err(tool_error(&error.message)),
            Err(error) => Err(match rate_limit_words(&error) {
                Some(words) => tool_error(&words),
                None => ConnectorError::Failed(format!("{} did not answer the call", server.name)),
            }),
        }
    };
    tokio::time::timeout(LISTING_TIMEOUT, call)
        .await
        .unwrap_or(Err(ConnectorError::Timeout))
}

/// What a service said when it turned the call away for its rate limit, with an HTTP 429 and its
/// words in the body, which is not an MCP result and so reaches rmcp as a failure of the
/// transport, naming the status and the body. Any other HTTP failure says nothing of what the
/// service means, and is none.
fn rate_limit_words(error: &rmcp::service::ServiceError) -> Option<String> {
    use rmcp::transport::streamable_http_client::StreamableHttpError;

    let rmcp::service::ServiceError::TransportSend(sent) = error else {
        return None;
    };
    let Some(StreamableHttpError::UnexpectedServerResponse(said)) = sent
        .error
        .downcast_ref::<StreamableHttpError<reqwest::Error>>()
    else {
        return None;
    };
    let status = reqwest::StatusCode::TOO_MANY_REQUESTS;
    let body = said
        .strip_prefix(format!("HTTP {status}: ").as_str())?
        .trim();
    Some(if body.is_empty() {
        format!("HTTP {status}")
    } else {
        body.to_string()
    })
}

/// A [`ConnectorError::ToolError`] of the service's own `words`, cut.
fn tool_error(words: &str) -> ConnectorError {
    ConnectorError::ToolError {
        text: words.chars().take(TOOL_ERROR_CHARACTERS).collect(),
    }
}

/// What a tool's result says, as [`call_tool`] answers it.
fn read_result(result: &rmcp::model::CallToolResult) -> Result<serde_json::Value, ConnectorError> {
    let text = result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .map(|block| block.text.as_str());
    if result.is_error == Some(true) {
        return Err(tool_error(text.unwrap_or_default()));
    }
    if let Some(structured) = &result.structured_content {
        return Ok(structured.clone());
    }
    let text = text.unwrap_or_default();
    Ok(serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({ "text": text })))
}

/// Starts or reaches `server` as [`list_tools`] and [`call_tool`] do, and opens an MCP session
/// with it. Not bounded in time: the callers bound it.
async fn connect(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
    bearer: Option<&Secret>,
    folder: &std::path::Path,
    catervas: &std::path::Path,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ()>, ConnectorError> {
    use rmcp::ServiceExt as _;

    let failed = |what: &str, error: &dyn fmt::Display| {
        ConnectorError::Failed(format!("{} {what}: {error}", server.name))
    };
    let plain = |what: &str| ConnectorError::Failed(format!("{} {what}", server.name));
    match &server.transport {
        CustomTransport::Stdio { command, args, .. } => {
            let mut process = tokio::process::Command::new(program(command, args, catervas));
            // rmcp kills the server when the transport is dropped, as on the timeout of its
            // callers; this is the same promise again, should rmcp stop keeping it.
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
        CustomTransport::Http { url, headers, .. } => {
            let mut config = rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url.as_str())
                .custom_headers(filled_headers(headers, keys)?);
            if let Some(bearer) = bearer {
                config = config.auth_header(bearer.expose());
            }
            ().serve(rmcp::transport::StreamableHttpClientTransport::from_config(config)).await
        }
    }
    .map_err(|_| plain("did not answer as an MCP server"))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use keyring_core::api::CredentialStoreApi as _;

    use super::*;

    #[test]
    fn runs_catervas_by_its_own_path() {
        let own = std::path::Path::new("/opt/catervas/bin/catervas-under-test");
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|a| (*a).to_string()).collect() };
        assert_eq!(program("catervas", &args(&["connector", "osv"]), own), own);
        assert_eq!(
            program("catervas", &args(&["serve"]), own),
            PathBuf::from("catervas")
        );
        for other in ["catervasx", "catervas-osv", "CATERVAS", "catervas.exe"] {
            assert_eq!(
                program(other, &args(&["connector", "osv"]), own),
                PathBuf::from(other)
            );
        }
        assert_eq!(
            program("npx", &args(&["connector", "osv"]), own),
            PathBuf::from("npx")
        );
        assert_eq!(
            program("/usr/local/bin/catervas", &args(&["connector", "osv"]), own),
            PathBuf::from("/usr/local/bin/catervas")
        );
    }

    #[test]
    fn finds_its_own_program_for_catervas_s_connector_only() {
        let catervas = CustomServer {
            transport: CustomTransport::Stdio {
                command: "catervas".to_string(),
                args: vec!["connector".to_string(), "osv".to_string()],
                oauth: None,
            },
            ..stdio(&[])
        };
        assert_eq!(
            own_program(&catervas, Some(std::path::Path::new("/opt/catervas"))),
            Ok(PathBuf::from("/opt/catervas"))
        );
        assert_eq!(own_program(&catervas, None), Err(NO_OWN_PROGRAM));
        assert_eq!(own_program(&stdio(&[]), None), Ok(PathBuf::new()));
    }

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
            oauth: None,
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
            std::env::temp_dir().join(format!("catervas-connectors-{}-{test}", std::process::id()));
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
            .build("catervas", "connector:p:theo:github", None)
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

    /// The one id file kept in `state`.
    fn id_file(state: &std::path::Path) -> PathBuf {
        let files: Vec<_> = std::fs::read_dir(state.join("projects"))
            .expect("the ids' folder reads")
            .map(|item| item.expect("an item").path())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        files[0].clone()
    }

    #[test]
    fn two_projects_in_folders_of_one_name_keep_their_keys_apart() {
        // `~/work/app` and `~/clients/app`: a folder's name is no project's address (finding I1).
        let dir = scratch("same-name");
        let state = dir.join("state");
        let (work, clients) = (dir.join("work/app"), dir.join("clients/app"));
        for root in [&work, &clients] {
            std::fs::create_dir_all(root).expect("the project is made");
        }
        let at_work = SecretAt::of(&state, &work, "theo", "github").expect("an address");
        assert!(
            !work.join(".catervas").exists(),
            "nothing is kept in the project"
        );
        let file = id_file(&state);
        let at_clients = SecretAt::of(&state, &clients, "theo", "github").expect("an address");
        assert_ne!(at_work.account(), at_clients.account());
        // The id is made once, kept in the state folder, owner-only, and read back after.
        assert_eq!(
            SecretAt::of(&state, &work, "iris", "github")
                .expect("an address")
                .project_id,
            at_work.project_id
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("the id is kept"),
            at_work.project_id
        );
        let mode = |path: &std::path::Path| {
            std::fs::metadata(path)
                .expect("it is there")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&state.join("projects")), 0o700);
        assert_eq!(at_work.project_id.len(), 32, "{}", at_work.project_id);
    }

    #[test]
    fn a_copied_project_gets_its_own_id() {
        // `cp -r app app2` copied `.catervas/local/project_id`: the copy is another project, and
        // must not find the first one's keys (re-review N3).
        let dir = scratch("copied");
        let state = dir.join("state");
        let (app, copy) = (dir.join("app"), dir.join("app2"));
        std::fs::create_dir_all(&app).expect("the project is made");
        let first = SecretAt::of(&state, &app, "theo", "github").expect("an address");
        assert!(
            std::process::Command::new("cp")
                .args(["-r"])
                .args([&app, &copy])
                .status()
                .expect("cp runs")
                .success()
        );
        let copied = SecretAt::of(&state, &copy, "theo", "github").expect("an address");
        assert_ne!(first.project_id, copied.project_id);
    }

    #[test]
    fn a_project_reached_through_a_link_has_its_targets_id() {
        // Re-review 2 m4 (N3a): setup gives the root with its links followed, the CLI the folder
        // it runs in, so both must find one id.
        let dir = scratch("linked");
        let root = dir.join("app");
        std::fs::create_dir_all(&root).expect("the root is made");
        let link = dir.join("link");
        std::os::unix::fs::symlink(&root, &link).expect("the link is made");
        let state = dir.join("state");
        assert_eq!(
            local_project_id(&state, &link).expect("an id"),
            local_project_id(&state, &root).expect("an id")
        );
    }

    #[test]
    fn an_id_not_of_32_hex_digits_is_refused() {
        let dir = scratch("bad-id");
        let (state, app) = (dir.join("state"), dir.join("app"));
        std::fs::create_dir_all(&app).expect("the project is made");
        SecretAt::of(&state, &app, "theo", "github").expect("an address");
        std::fs::write(id_file(&state), "../../x").expect("written");
        assert!(SecretAt::of(&state, &app, "theo", "github").is_err());
    }

    #[test]
    fn an_id_kept_first_is_never_replaced() {
        // Two processes making the id at once: the one linked into place first is the one both
        // answer, never a second written over it (carry P1).
        let dir = scratch("first-kept");
        std::fs::create_dir_all(&dir).expect("the folder is made");
        let file = dir.join("id");
        let first = "0123456789abcdef0123456789abcdef";
        assert_eq!(keep_id(&file, first).expect("kept"), first);
        assert_eq!(
            keep_id(&file, "fedcba9876543210fedcba9876543210").expect("the first is read"),
            first
        );
        assert_eq!(std::fs::read_to_string(&file).expect("kept"), first);
        let held: Vec<_> = std::fs::read_dir(&dir).expect("reads").collect();
        assert_eq!(held.len(), 1, "nothing is left beside it");
    }

    #[test]
    fn a_connector_folder_holds_nothing_the_repository_put_there() {
        // Finding C1: git checks out a force-added file at the folder the server ran in, so a
        // pulled commit chose what `python -m github_mcp` ran, with the agent's keys.
        let dir = scratch("planted");
        let root = dir.join("app");
        let planted = root.join(".catervas/local/connectors/dev-a/github");
        std::fs::create_dir_all(&planted).expect("the folder is made");
        std::fs::write(planted.join("github_mcp.py"), "print('PLANTED')").expect("planted");
        std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755))
            .expect("the mode is set");
        let state = dir.join("state");
        let at = SecretAt::of(&state, &root, "dev-a", "github").expect("an address");
        let folder = working_folder(&state, &root, &at).expect("the folder is made");
        assert!(!folder.starts_with(&root), "{}", folder.display());
        let empty_and_owner_only = |folder: &std::path::Path| {
            let held: Vec<_> = std::fs::read_dir(folder)
                .expect("the folder reads")
                .map(|item| item.expect("an item").file_name())
                .collect();
            assert!(held.is_empty(), "{} holds {held:?}", folder.display());
            assert_eq!(
                std::fs::metadata(folder)
                    .expect("it is there")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        };
        empty_and_owner_only(&folder);
        // What the last run left, or anything else put there, is gone at the next.
        std::fs::write(folder.join("github_mcp.py"), "print('PLANTED')").expect("planted");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755))
            .expect("the mode is set");
        assert_eq!(
            working_folder(&state, &root, &at).expect("made again"),
            folder
        );
        empty_and_owner_only(&folder);
    }

    #[test]
    fn no_connector_runs_in_a_state_folder_inside_the_project() {
        // Re-review 2 m1: with `XDG_CONFIG_HOME` inside the project, or the project being home,
        // the folder would sit in the repository, and the walk upward would find its root again.
        let dir = scratch("state-inside");
        let root = dir.join("app");
        std::fs::create_dir_all(&root).expect("the root is made");
        let link = dir.join("link");
        std::os::unix::fs::symlink(&root, &link).expect("the link is made");
        for state in [root.join(".cfg/catervas"), link.join(".cfg/catervas")] {
            let at = SecretAt::of(&state, &root, "dev-a", "github").expect("an address");
            let refused = working_folder(&state, &link, &at).expect_err("refused");
            assert!(
                refused
                    .to_string()
                    .starts_with("state_inside_project: Catervas's settings folder"),
                "{refused}"
            );
            assert!(!state.join("connectors").exists(), "nothing is made");
        }
    }

    #[test]
    fn no_folder_above_a_connector_folder_is_the_repositorys() {
        // Finding C1: `npx` and `uv` look upward for `.npmrc`, `node_modules/.bin` and
        // `pyproject.toml`, so the repository's root decided what `npx -y …` and `uv run` ran.
        let dir = scratch("walk-up");
        let root = dir.join("app");
        std::fs::create_dir_all(root.join("node_modules/.bin")).expect("the root is made");
        for (name, text) in [
            (".npmrc", "registry=https://evil.example/\n"),
            ("package.json", "{}"),
            (
                "pyproject.toml",
                "[tool.uv]\nindex-url = \"https://evil.example/\"\n",
            ),
        ] {
            std::fs::write(root.join(name), text).expect("planted");
        }
        let state = dir.join("state");
        let at = SecretAt::of(&state, &root, "dev-a", "github").expect("an address");
        let folder = working_folder(&state, &root, &at).expect("the folder is made");
        assert!(!folder.starts_with(&root), "{}", folder.display());
        for above in folder.ancestors() {
            for name in [".npmrc", "package.json", "pyproject.toml", "node_modules"] {
                assert!(!above.join(name).exists(), "{}", above.join(name).display());
            }
        }
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
        let folder = dir.join("catervas");
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
    const WRITER_FILE: &str = "CATERVAS_TEST_CONNECTORS_FILE";
    const WRITER_AGENT: &str = "CATERVAS_TEST_CONNECTORS_AGENT";
    const SAVES: usize = 40;

    #[test]
    fn two_processes_saving_at_once_lose_no_entry() {
        // `catervas connect` and the daemon may both rewrite `connectors.json`: two processes, each
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
                oauth: None,
            },
            credential_keys: credential_keys.iter().map(ToString::to_string).collect(),
            tools: BTreeMap::new(),
            kit: false,
            allowances: BTreeMap::new(),
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
                oauth: None,
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

    /// An entry that signed in, its tokens recognisable.
    fn signed_in_entry() -> ConnectorEntry {
        ConnectorEntry {
            spec_sha256: "abc".to_string(),
            keys: BTreeMap::new(),
            oauth: Some(OAuthGrant {
                issuer: "https://auth.example".to_string(),
                resource: "https://mcp.example/mcp".to_string(),
                client_id: "client-1".to_string(),
                token_endpoint: "https://auth.example/token".to_string(),
                revocation_endpoint: Some("https://auth.example/revoke".to_string()),
                access_token: Secret::new("access-secret-value".to_string()),
                refresh_token: Some(Secret::new("refresh-secret-value".to_string())),
                issued_at: "2026-10-02T10:00:00Z".parse().expect("a time"),
                expires_at: Some("2026-10-02T11:00:00Z".parse().expect("a time")),
                scopes: vec!["read".to_string(), "offline_access".to_string()],
                lapsed: false,
                app: None,
            }),
        }
    }

    #[test]
    fn an_oauth_entry_round_trips_and_never_prints() {
        let dir = scratch("oauth-round-trip");
        let store = FileConnectorSecrets::new(dir.join("connectors.json"));
        let theo = at("theo", "notion");
        let entry = signed_in_entry();
        store.save(&theo, &entry).expect("kept");
        assert_eq!(store.load(&theo), Ok(Some(entry.clone())));
        // A lapsed grant, and one with neither a refresh token, an expiry nor a revocation endpoint.
        let mut bare = entry.clone();
        if let Some(grant) = &mut bare.oauth {
            grant.lapsed = true;
            grant.refresh_token = None;
            grant.expires_at = None;
            grant.revocation_endpoint = None;
        }
        store.save(&theo, &bare).expect("kept");
        assert_eq!(store.load(&theo), Ok(Some(bare)));
        for printed in [
            format!("{entry:?}"),
            format!("{:?}", entry.oauth),
            format!("{:#?}", entry.oauth),
        ] {
            assert!(printed.contains("***"), "{printed}");
            assert!(!printed.contains("access-secret-value"), "{printed}");
            assert!(!printed.contains("refresh-secret-value"), "{printed}");
        }
    }

    #[test]
    fn an_entry_stored_before_has_no_oauth() {
        let entry = ConnectorEntry::from_text(r#"{"spec_sha256":"abc","keys":{}}"#)
            .expect("an entry from before");
        assert_eq!(entry.oauth, None);
        assert_eq!(entry.spec_sha256, "abc");
        // And one with no sign-in is stored as it was, with no `oauth` member.
        assert_eq!(
            super::ConnectorEntry::to_json(&entry),
            serde_json::json!({ "spec_sha256": "abc", "keys": {} })
        );
    }

    #[test]
    fn launch_headers_send_the_bearer() {
        let server = CustomServer {
            name: "notion".to_string(),
            transport: CustomTransport::Http {
                url: "https://mcp.example/mcp".to_string(),
                headers: BTreeMap::from([("X-Workspace".to_string(), "a".to_string())]),
                oauth: Some(catervas_core::team::OAuthSettings {
                    client_id: None,
                    callback_port: None,
                    scopes: Vec::new(),
                }),
            },
            credential_keys: Vec::new(),
            tools: BTreeMap::new(),
            kit: false,
            allowances: BTreeMap::new(),
        };
        let headers = launch_headers(&server, &signed_in_entry()).expect("headers");
        assert_eq!(
            headers.get("Authorization").map(Secret::expose),
            Some("Bearer access-secret-value")
        );
        assert_eq!(headers.get("X-Workspace").map(Secret::expose), Some("a"));
        assert_eq!(headers.len(), 2);
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
