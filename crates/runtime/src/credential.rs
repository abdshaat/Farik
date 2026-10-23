//! The OS keychain and file errors the stores share, and the one-time removal of the key an earlier
//! Catervas kept (ADR 0084). Catervas keeps and reads no model key: Claude Code holds the AI
//! account.

/// The keychain entry's service, which an earlier version kept the key under.
const SERVICE: &str = "catervas";
/// The keychain entry's account.
const PROVIDER: &str = "anthropic";

/// Why a store could not load or keep a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// The computer has no keychain; the next store is tried.
    NoKeychain,
    /// The store answered and refused, or broke; nothing further is tried.
    Failed(String),
}

/// Removes, unread, the key an earlier version kept (ADR 0084): the keychain entry when
/// `keychain` is true, and `credential.json` in `state_dir`. Neither is ever read: the keychain
/// entry is deleted without `get_password`, and the file is removed without being opened. A
/// computer with no keychain, and nothing kept, are not errors. Answers whether anything was
/// removed.
#[must_use]
pub fn forget_kept_key(state_dir: &std::path::Path, keychain: bool) -> bool {
    let from_keychain = keychain
        && keyring::Entry::new(SERVICE, PROVIDER)
            .and_then(|entry| entry.delete_credential())
            .is_ok();
    let from_file = std::fs::remove_file(state_dir.join("credential.json")).is_ok();
    from_keychain || from_file
}

/// What a keychain error means for Catervas.
/// No store registered, or no secret service on the bus, is no keychain at all.
pub(crate) fn map_keyring_error(error: &keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoDefaultStore => CredentialError::NoKeychain,
        keyring::Error::PlatformFailure(why) if why.to_string().contains("ServiceUnknown") => {
            CredentialError::NoKeychain
        }
        other => CredentialError::Failed(other.to_string()),
    }
}

/// What the keychain's answer to a load means, read by `parse`: no entry is nothing stored.
pub(crate) fn read_keychain<Kept>(
    answer: keyring::Result<String>,
    parse: impl FnOnce(&str) -> Result<Kept, CredentialError>,
) -> Result<Option<Kept>, CredentialError> {
    match answer {
        Ok(text) => parse(&text).map(Some),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(map_keyring_error(&error)),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use super::*;

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("catervas-credential-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        dir
    }

    fn platform(text: &str) -> keyring::Error {
        keyring::Error::PlatformFailure(Box::new(std::io::Error::other(text.to_string())))
    }

    #[test]
    fn forgets_the_kept_key_without_reading_it() {
        let dir = scratch("forget");
        let file = dir.join("credential.json");
        std::fs::write(&file, "not even a key").expect("a kept file");
        // Unreadable: removing it still works, which is the proof that it is not read.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000))
            .expect("the mode is set");
        assert!(forget_kept_key(&dir, false));
        assert!(!file.exists());
        assert!(!forget_kept_key(&dir, false));
    }

    #[test]
    fn maps_a_keychain_error_to_what_it_means() {
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
        let parse = |text: &str| Ok(text.to_string());
        assert_eq!(read_keychain(Err(keyring::Error::NoEntry), parse), Ok(None));
        assert_eq!(
            read_keychain(Err(keyring::Error::NoDefaultStore), parse),
            Err(CredentialError::NoKeychain)
        );
        assert_eq!(
            read_keychain(Ok("kept".to_string()), parse),
            Ok(Some("kept".to_string()))
        );
    }
}
