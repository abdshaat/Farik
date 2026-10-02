//! A signed-in connector's grant, kept fresh for the sessions that use it (ADR 0033).

use std::sync::Arc;
use std::time::Duration;

use farik_core::team::CustomServer;

use crate::connectors::{ConnectorEntry, SecretAt, confirmed_entry};
use crate::credential::CredentialError;
use crate::sign_in::{SignInError, refreshed};

use super::DaemonState;

/// Why a signed-in server's entry could not be given to a session or a launch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fresh {
    /// Nothing is kept as the team file has the server now.
    NotConfirmed,
    /// The service ended the sign-in.
    Lapsed,
    /// The grant could not be refreshed, and its access token is no longer good.
    Failed(String),
    /// The store could not be read or written.
    Store(String),
}

/// How long the service may take to answer a refresh. The refresh runs on a task of its own that
/// finishes whether or not anyone waits, so a rotated token is always kept.
const REFRESH_REQUEST: Duration = Duration::from_secs(30);

/// The entry kept for `server` at `at`, its sign-in refreshed when it will not last `valid_for`.
/// Waits at most `wait` for that; the refresh and its save go on after.
///
/// A refresh that fails leaves the entry as it was while its access token still holds; a refused
/// one lapses the grant (ADR 0033).
pub(crate) async fn refreshed_entry(
    state: &Arc<DaemonState>,
    at: &SecretAt,
    server: &CustomServer,
    valid_for: Duration,
    wait: Duration,
) -> Result<ConnectorEntry, Fresh> {
    let (state, at, server) = (Arc::clone(state), at.clone(), server.clone());
    let task =
        tokio::spawn(async move { refresh_under_lock(&state, &at, &server, valid_for).await });
    match tokio::time::timeout(wait, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(Fresh::Failed("the refresh did not finish".to_string())),
        Err(_) => Err(Fresh::Failed(format!(
            "the service did not answer within {} seconds",
            wait.as_secs()
        ))),
    }
}

/// `work` on a thread that may block, as a store does.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Fresh> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Fresh::Store("the store's task failed".to_string()))
}

fn store_failed(error: &CredentialError) -> Fresh {
    Fresh::Store(match error {
        CredentialError::NoKeychain => "this computer has no keychain".to_string(),
        CredentialError::Failed(detail) => detail.clone(),
    })
}

/// Saves `entry` at `at` and remembers what the store now holds.
async fn keep(state: &Arc<DaemonState>, at: &SecretAt, entry: ConnectorEntry) -> Result<(), Fresh> {
    let (held, kept_at) = (Arc::clone(state), at.clone());
    blocking(move || {
        let saved = held.connector_secrets().save(&kept_at, &entry);
        held.read_kept(&kept_at);
        saved
    })
    .await?
    .map(|_| ())
    .map_err(|error| store_failed(&error))
}

async fn refresh_under_lock(
    state: &Arc<DaemonState>,
    at: &SecretAt,
    server: &CustomServer,
    valid_for: Duration,
) -> Result<ConnectorEntry, Fresh> {
    let lock = state.entry_lock(at);
    let _held = lock.lock().await;
    // Read under the lock: an entry removed or refreshed while this waited is seen as it is.
    let (secrets, kept_at, definition) = (state.connector_secrets(), at.clone(), server.clone());
    let mut entry = blocking(move || confirmed_entry(secrets.as_ref(), &kept_at, &definition))
        .await?
        .map_err(|error| store_failed(&error))?
        .ok_or(Fresh::NotConfirmed)?;
    let Some(grant) = entry.oauth.clone() else {
        return Ok(entry);
    };
    if grant.lapsed {
        return Err(Fresh::Lapsed);
    }
    let now = chrono::Utc::now();
    match refreshed(&grant, now, valid_for, REFRESH_REQUEST).await {
        Ok(None) => Ok(entry),
        // The rotated refresh token is saved before the new access token is used.
        Ok(Some(fresh)) => {
            entry.oauth = Some(fresh);
            keep(state, at, entry.clone())
                .await
                .map_err(|_| Fresh::Failed("the new sign-in could not be kept".to_string()))?;
            Ok(entry)
        }
        Err(SignInError::Lapsed) => {
            if let Some(held) = &mut entry.oauth {
                held.lapsed = true;
            }
            keep(state, at, entry).await?;
            Err(Fresh::Lapsed)
        }
        Err(error) => {
            let expired = grant.expires_at.is_some_and(|expires_at| expires_at <= now);
            match error {
                SignInError::Failed(why) if expired => Err(Fresh::Failed(why)),
                _ if expired => Err(Fresh::Failed(
                    "the sign-in could not be refreshed".to_string(),
                )),
                _ => Ok(entry),
            }
        }
    }
}
