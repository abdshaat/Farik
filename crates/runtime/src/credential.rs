//! The model credential, kept where the user put it (ADR 0022): the environment wins, then the
//! OS keychain, then a file only its owner can read, for a computer with no keychain.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::claude::{ClaudeCredential, CredentialKind, Secret, credential_from_env};

/// The refusal of a subscription token that does not start `sk-ant-oat`.
pub const NOT_A_SUBSCRIPTION_TOKEN: &str =
    "that is not a Claude subscription token: a subscription token starts with sk-ant-oat";
/// The refusal of an API key that does not start `sk-ant-api`.
pub const NOT_AN_API_KEY: &str =
    "that is not an Anthropic API key: an API key starts with sk-ant-api";
/// The keychain entry's service.
const SERVICE: &str = "farik";
/// The keychain entry's account, and the provider every credential records.
const PROVIDER: &str = "anthropic";

/// Where a credential was found, or where it was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN`.
    Environment,
    /// The OS keychain.
    Keychain,
    /// `credential.json` in the state folder.
    File,
}

/// Why a store could not load or keep the credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// The computer has no keychain; the next store is tried.
    NoKeychain,
    /// The store answered and refused, or broke; nothing further is tried.
    Failed(String),
}

/// A place the credential can be kept.
pub trait CredentialStore: Send + Sync {
    /// The credential kept here; `Ok(None)` when none is.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    fn load(&self) -> Result<Option<ClaudeCredential>, CredentialError>;
    /// Keeps `credential` here, replacing any other.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn save(&self, credential: &ClaudeCredential) -> Result<(), CredentialError>;
    /// Removes the credential kept here; nothing kept is nothing to remove.
    ///
    /// # Errors
    ///
    /// The store could not be written.
    fn delete(&self) -> Result<(), CredentialError>;
    /// Which source this store is.
    fn source(&self) -> Source;
}

/// The OS keychain: service `farik`, account `anthropic`.
pub struct KeychainStore;

impl CredentialStore for KeychainStore {
    fn load(&self) -> Result<Option<ClaudeCredential>, CredentialError> {
        read_keychain(keyring::Entry::new(SERVICE, PROVIDER).and_then(|entry| entry.get_password()))
    }

    fn save(&self, credential: &ClaudeCredential) -> Result<(), CredentialError> {
        keyring::Entry::new(SERVICE, PROVIDER)
            .and_then(|entry| entry.set_password(&to_json(credential)))
            .map_err(|error| map_keyring_error(&error))
    }

    fn delete(&self) -> Result<(), CredentialError> {
        match keyring::Entry::new(SERVICE, PROVIDER).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(&error)),
        }
    }

    fn source(&self) -> Source {
        Source::Keychain
    }
}

/// `<state_dir>/credential.json`, mode 0600.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// The store writing `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> FileStore {
        FileStore { path }
    }
}

impl CredentialStore for FileStore {
    fn load(&self) -> Result<Option<ClaudeCredential>, CredentialError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => from_json(&text).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(CredentialError::Failed(format!(
                "could not read {}: {error}",
                self.path.display()
            ))),
        }
    }

    fn save(&self, credential: &ClaudeCredential) -> Result<(), CredentialError> {
        crate::write_private(&self.path, to_json(credential).as_bytes()).map_err(|error| {
            CredentialError::Failed(format!("could not write {}: {error}", self.path.display()))
        })
    }

    fn delete(&self) -> Result<(), CredentialError> {
        match std::fs::remove_file(&self.path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(CredentialError::Failed(format!(
                    "could not remove {}: {error}",
                    self.path.display()
                )))
            }
            _ => Ok(()),
        }
    }

    fn source(&self) -> Source {
        Source::File
    }
}

/// A store in memory, standing in for the keychain where nothing real may be touched: tests.
#[derive(Default)]
pub struct MemoryStore {
    held: Mutex<Option<ClaudeCredential>>,
}

impl CredentialStore for MemoryStore {
    fn load(&self) -> Result<Option<ClaudeCredential>, CredentialError> {
        Ok(crate::locked(&self.held).clone())
    }

    fn save(&self, credential: &ClaudeCredential) -> Result<(), CredentialError> {
        *crate::locked(&self.held) = Some(credential.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        *crate::locked(&self.held) = None;
        Ok(())
    }

    fn source(&self) -> Source {
        Source::Keychain
    }
}

/// The credential to run on, and where it came from.
#[must_use]
pub fn load_credential(
    env: &BTreeMap<String, String>,
    stores: &[Arc<dyn CredentialStore>],
) -> Option<(ClaudeCredential, Source)> {
    if let Some(credential) = credential_from_env(env) {
        return Some((credential, Source::Environment));
    }
    // A store that cannot be read is passed over: a locked keychain does not hide the file.
    stores
        .iter()
        .find_map(|store| Some((store.load().ok()??, store.source())))
}

/// Keeps `credential` in the first store that answers.
///
/// # Errors
///
/// A store refused.
pub fn save_credential(
    credential: &ClaudeCredential,
    stores: &[Arc<dyn CredentialStore>],
) -> Result<Source, CredentialError> {
    for store in stores {
        match store.save(credential) {
            Ok(()) => return Ok(store.source()),
            Err(CredentialError::NoKeychain) => {}
            Err(CredentialError::Failed(why)) if store.source() == Source::Keychain => {
                return Err(CredentialError::Failed(format!(
                    "your computer's keychain would not store the key: {why}"
                )));
            }
            Err(error) => return Err(error),
        }
    }
    Err(CredentialError::NoKeychain)
}

/// The credential a pasted `secret` of `kind` is.
///
/// # Errors
///
/// The sentence saying the secret is not of that kind.
pub fn credential_of_kind(kind: CredentialKind, secret: &str) -> Result<ClaudeCredential, String> {
    let secret = secret.trim();
    match kind {
        CredentialKind::ApiKey if secret.starts_with("sk-ant-api") => {
            Ok(ClaudeCredential::ApiKey(Secret::new(secret.to_string())))
        }
        CredentialKind::SubscriptionToken if secret.starts_with("sk-ant-oat") => Ok(
            ClaudeCredential::OauthToken(Secret::new(secret.to_string())),
        ),
        CredentialKind::ApiKey => Err(NOT_AN_API_KEY.to_string()),
        CredentialKind::SubscriptionToken => Err(NOT_A_SUBSCRIPTION_TOKEN.to_string()),
    }
}

/// What a keychain error means for Farik.
/// No store registered, or no secret service on the bus, is no keychain at all.
fn map_keyring_error(error: &keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoDefaultStore => CredentialError::NoKeychain,
        keyring::Error::PlatformFailure(why) if why.to_string().contains("ServiceUnknown") => {
            CredentialError::NoKeychain
        }
        other => CredentialError::Failed(other.to_string()),
    }
}

/// What the keychain's answer to a load means: no entry is nothing stored.
fn read_keychain(
    answer: keyring::Result<String>,
) -> Result<Option<ClaudeCredential>, CredentialError> {
    match answer {
        Ok(text) => from_json(&text).map(Some),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(map_keyring_error(&error)),
    }
}

/// The stored form: `{"provider":"anthropic","kind":…,"secret":…}`.
fn to_json(credential: &ClaudeCredential) -> String {
    let (ClaudeCredential::ApiKey(secret) | ClaudeCredential::OauthToken(secret)) = credential;
    serde_json::json!({
        "provider": PROVIDER,
        "kind": credential.kind(),
        "secret": secret.expose(),
    })
    .to_string()
}

/// The credential a stored form holds. The error never quotes the text, which holds the secret.
fn from_json(text: &str) -> Result<ClaudeCredential, CredentialError> {
    let value: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let secret = |text: &str| Secret::new(text.to_string());
    match (
        value["provider"].as_str(),
        value["kind"].as_str(),
        value["secret"].as_str(),
    ) {
        (Some(PROVIDER), Some("api_key"), Some(text)) => Ok(ClaudeCredential::ApiKey(secret(text))),
        (Some(PROVIDER), Some("subscription_token"), Some(text)) => {
            Ok(ClaudeCredential::OauthToken(secret(text)))
        }
        _ => Err(CredentialError::Failed(
            "the stored credential is not one farik can read".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    struct Refusing(CredentialError);

    impl CredentialStore for Refusing {
        fn load(&self) -> Result<Option<ClaudeCredential>, CredentialError> {
            Err(self.0.clone())
        }

        fn save(&self, _credential: &ClaudeCredential) -> Result<(), CredentialError> {
            Err(self.0.clone())
        }

        fn delete(&self) -> Result<(), CredentialError> {
            Err(self.0.clone())
        }

        fn source(&self) -> Source {
            Source::Keychain
        }
    }

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("farik-credential-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        dir
    }

    fn token() -> ClaudeCredential {
        ClaudeCredential::OauthToken(Secret::new("sk-ant-oat01-token".to_string()))
    }

    fn holding(credential: &ClaudeCredential) -> Arc<dyn CredentialStore> {
        let store = MemoryStore::default();
        store.save(credential).expect("memory keeps it");
        Arc::new(store)
    }

    fn platform(text: &str) -> keyring::Error {
        keyring::Error::PlatformFailure(Box::new(std::io::Error::other(text.to_string())))
    }

    #[test]
    fn reads_the_environment_before_the_stores() {
        let env = BTreeMap::from([(
            "ANTHROPIC_API_KEY".to_string(),
            "sk-ant-api03-key".to_string(),
        )]);
        let found = load_credential(&env, &[holding(&token())]);
        assert!(matches!(
            found,
            Some((ClaudeCredential::ApiKey(ref key), Source::Environment))
                if key.expose() == "sk-ant-api03-key"
        ));
        assert_eq!(
            load_credential(&BTreeMap::new(), &[holding(&token())]),
            Some((token(), Source::Keychain))
        );
    }

    #[test]
    fn falls_back_to_the_file_only_without_a_keychain() {
        let dir = scratch("fallback");
        let file = dir.join("credential.json");
        let stores: [Arc<dyn CredentialStore>; 2] = [
            Arc::new(Refusing(CredentialError::NoKeychain)),
            Arc::new(FileStore::new(file.clone())),
        ];
        assert_eq!(save_credential(&token(), &stores), Ok(Source::File));
        let mode = std::fs::metadata(&file)
            .expect("the file is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let text = std::fs::read_to_string(&file).expect("the file reads");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).expect("the file is JSON"),
            serde_json::json!({
                "provider": "anthropic",
                "kind": "subscription_token",
                "secret": "sk-ant-oat01-token",
            })
        );
        assert_eq!(stores[1].load(), Ok(Some(token())));

        let locked = dir.join("locked.json");
        let stores: [Arc<dyn CredentialStore>; 2] = [
            Arc::new(Refusing(CredentialError::Failed("locked".to_string()))),
            Arc::new(FileStore::new(locked.clone())),
        ];
        assert_eq!(
            save_credential(&token(), &stores),
            Err(CredentialError::Failed(
                "your computer's keychain would not store the key: locked".to_string()
            ))
        );
        assert!(!locked.exists());

        assert_eq!(
            map_keyring_error(&keyring::Error::NoDefaultStore),
            CredentialError::NoKeychain
        );
        assert_eq!(
            map_keyring_error(&platform(
                "org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable"
            )),
            CredentialError::NoKeychain
        );
        assert_eq!(
            map_keyring_error(&platform("ServiceUnknown")),
            CredentialError::NoKeychain
        );
        assert_eq!(
            map_keyring_error(&platform("the collection is locked")),
            CredentialError::Failed("Platform failure: the collection is locked".to_string())
        );
        assert_eq!(
            map_keyring_error(&keyring::Error::NoEntry),
            CredentialError::Failed("No matching credential found".to_string())
        );
    }

    #[test]
    fn treats_no_entry_as_nothing_stored() {
        assert_eq!(read_keychain(Err(keyring::Error::NoEntry)), Ok(None));
        assert_eq!(
            read_keychain(Err(keyring::Error::NoDefaultStore)),
            Err(CredentialError::NoKeychain)
        );
        let kept = r#"{"provider":"anthropic","kind":"api_key","secret":"sk-ant-api03-key"}"#;
        assert_eq!(
            read_keychain(Ok(kept.to_string())),
            Ok(Some(ClaudeCredential::ApiKey(Secret::new(
                "sk-ant-api03-key".to_string()
            ))))
        );

        let empty: Arc<dyn CredentialStore> = Arc::new(MemoryStore::default());
        assert_eq!(empty.load(), Ok(None));
        let dir = scratch("empty");
        let missing = FileStore::new(dir.join("credential.json"));
        assert_eq!(missing.load(), Ok(None));

        let file = dir.join("held.json");
        let held = FileStore::new(file);
        held.save(&token()).expect("the file is written");
        let stores: [Arc<dyn CredentialStore>; 3] = [
            empty,
            Arc::new(Refusing(CredentialError::NoKeychain)),
            Arc::new(held),
        ];
        assert_eq!(
            load_credential(&BTreeMap::new(), &stores),
            Some((token(), Source::File))
        );
        assert_eq!(load_credential(&BTreeMap::new(), &stores[..2]), None);
    }

    #[test]
    fn forgets_the_credential_on_delete() {
        let file = FileStore::new(scratch("delete").join("credential.json"));
        file.save(&token()).expect("the file is written");
        file.delete().expect("the file is removed");
        assert_eq!(file.load(), Ok(None));
        // Nothing kept is nothing to remove.
        file.delete().expect("nothing to remove");
        let memory = MemoryStore::default();
        memory.save(&token()).expect("kept");
        memory.delete().expect("forgotten");
        assert_eq!(memory.load(), Ok(None));
    }

    #[test]
    fn refuses_a_key_of_the_wrong_kind() {
        assert_eq!(
            credential_of_kind(CredentialKind::SubscriptionToken, "sk-ant-api03-key"),
            Err(NOT_A_SUBSCRIPTION_TOKEN.to_string())
        );
        assert_eq!(
            credential_of_kind(CredentialKind::ApiKey, "sk-ant-oat01-token"),
            Err(NOT_AN_API_KEY.to_string())
        );
        assert_eq!(
            credential_of_kind(CredentialKind::ApiKey, "hello"),
            Err(NOT_AN_API_KEY.to_string())
        );
        assert_eq!(
            credential_of_kind(CredentialKind::SubscriptionToken, " sk-ant-oat01-token\n"),
            Ok(token())
        );
        assert!(matches!(
            credential_of_kind(CredentialKind::ApiKey, "sk-ant-api03-key"),
            Ok(ClaudeCredential::ApiKey(ref key)) if key.expose() == "sk-ant-api03-key"
        ));
    }
}
