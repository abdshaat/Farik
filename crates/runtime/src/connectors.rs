//! A custom connector's keys, kept per agent (ADR 0030): in the OS keychain, or in
//! `connectors.json` in the user's state folder on a computer with no keychain.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::claude::Secret;
use crate::credential::{CredentialError, map_keyring_error, read_keychain};

/// The keychain entry's service.
pub const SERVICE: &str = "farik";
/// The refusal when there is neither a keychain nor a state folder to keep the keys in.
pub const NO_SECRET_STORE: &str = "no_secret_store";

/// Whose keys, for which server: one entry per project, agent and server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretAt {
    /// The project's id: two projects can both have an agent `theo`.
    pub project_id: String,
    /// The agent the server was given to.
    pub agent_id: String,
    /// The server's name in the team file.
    pub server: String,
}

impl SecretAt {
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

/// Held while `connectors.json` is read and rewritten, so two saves in one process do not lose
/// one another's entry.
// ponytail: one lock per process; a `farik connect` and the daemon saving at the same moment can
// still race, a file lock if that is ever seen.
static FILE_LOCK: Mutex<()> = Mutex::new(());

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
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

        let folder = self.path.parent().unwrap_or(std::path::Path::new("."));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .and_then(|()| std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)))
            .map_err(|error| self.failed("make the folder of", &error))?;
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

    fn failed(&self, what: &str, error: &std::io::Error) -> CredentialError {
        CredentialError::Failed(format!("could not {what} {}: {error}", self.path.display()))
    }
}

impl ConnectorSecrets for FileConnectorSecrets {
    fn load(&self, at: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError> {
        let _held = crate::locked(&FILE_LOCK);
        self.read_all()?
            .get(&at.account())
            .map(ConnectorEntry::from_json)
            .transpose()
    }

    fn save(&self, at: &SecretAt, entry: &ConnectorEntry) -> Result<SecretStore, CredentialError> {
        let _held = crate::locked(&FILE_LOCK);
        let mut entries = self.read_all()?;
        entries.insert(at.account(), entry.to_json());
        self.write_all(entries).map(|()| SecretStore::File)
    }

    fn delete(&self, at: &SecretAt) -> Result<(), CredentialError> {
        let _held = crate::locked(&FILE_LOCK);
        let mut entries = self.read_all()?;
        if entries.remove(&at.account()).is_none() {
            return Ok(());
        }
        self.write_all(entries)
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
        match self.keychain.load(at) {
            Ok(None) | Err(CredentialError::NoKeychain) => match &self.file {
                Some(file) => file.load(at),
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
        // Written beside and renamed over: nothing else is left in the folder.
        let names: Vec<_> = std::fs::read_dir(&folder)
            .expect("the folder reads")
            .map(|item| item.expect("an item").file_name())
            .collect();
        assert_eq!(names, ["connectors.json"]);
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

    #[test]
    fn a_secret_never_prints() {
        let printed = format!("{:?}", entry("abc"));
        assert!(printed.contains("API_KEY"), "{printed}");
        assert!(printed.contains("***"), "{printed}");
        assert!(printed.contains("abc"), "{printed}");
        assert!(!printed.contains("ghp-secret-value"), "{printed}");
    }
}
