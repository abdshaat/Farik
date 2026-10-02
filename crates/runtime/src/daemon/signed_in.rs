//! A signed-in connector's grant, kept fresh for the sessions that use it (ADR 0033).

use std::sync::Arc;
use std::time::Duration;

use farik_core::team::{CustomServer, CustomTransport, OAuthSettings};

use crate::claude::Secret;
use crate::connectors::{ConnectorEntry, ConnectorSecrets, SecretAt, confirmed_entry};
use crate::credential::CredentialError;
use crate::sign_in::{OAuthGrant, SIGN_IN_WINDOW, SignInError, refreshed, revoke, start_sign_in};

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
/// one lapses the grant (ADR 0033). With `fallback_when_valid`, a refresh still going when `wait`
/// passes also leaves the entry as it was while its access token holds: a session's setup, which
/// has no one waiting on it; the launch route answers that it did not finish.
pub(crate) async fn refreshed_entry(
    state: &Arc<DaemonState>,
    at: &SecretAt,
    server: &CustomServer,
    valid_for: Duration,
    wait: Duration,
    fallback_when_valid: bool,
) -> Result<ConnectorEntry, Fresh> {
    let (held, kept_at, definition) = (Arc::clone(state), at.clone(), server.clone());
    let task =
        tokio::spawn(
            async move { refresh_under_lock(&held, &kept_at, &definition, valid_for).await },
        );
    match tokio::time::timeout(wait, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(Fresh::Failed("the refresh did not finish".to_string())),
        Err(_) => {
            if fallback_when_valid && let Some(entry) = unexpired_entry(state, at, server).await {
                return Ok(entry);
            }
            Err(Fresh::Failed(format!(
                "the service did not answer within {} seconds",
                wait.as_secs()
            )))
        }
    }
}

/// The entry as the store has it, read without the lock the refresh holds, when its sign-in is not
/// lapsed and its access token has not expired.
async fn unexpired_entry(
    state: &Arc<DaemonState>,
    at: &SecretAt,
    server: &CustomServer,
) -> Option<ConnectorEntry> {
    let (secrets, kept_at, definition) = (state.connector_secrets(), at.clone(), server.clone());
    let entry = blocking(move || confirmed_entry(secrets.as_ref(), &kept_at, &definition))
        .await
        .ok()?
        .ok()??;
    let grant = entry.oauth.as_ref()?;
    let holds = !grant.lapsed
        && grant
            .expires_at
            .is_some_and(|expires_at| expires_at > chrono::Utc::now());
    holds.then_some(entry)
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

/// Whether the store still holds `grant` for `server` at `at`: read again, just before a save.
async fn still_holds(
    state: &Arc<DaemonState>,
    at: &SecretAt,
    server: &CustomServer,
    grant: &OAuthGrant,
) -> bool {
    let (secrets, kept_at, definition) = (state.connector_secrets(), at.clone(), server.clone());
    let read = blocking(move || confirmed_entry(secrets.as_ref(), &kept_at, &definition)).await;
    let Ok(Ok(Some(ConnectorEntry {
        oauth: Some(now), ..
    }))) = read
    else {
        return false;
    };
    now.access_token.expose() == grant.access_token.expose()
        && now.refresh_token.as_ref().map(Secret::expose)
            == grant.refresh_token.as_ref().map(Secret::expose)
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
    // A server that signs in and is kept without a grant is not handed out bare.
    let Some(grant) = entry.oauth.clone() else {
        return Err(Fresh::NotConfirmed);
    };
    if grant.lapsed {
        return Err(Fresh::Lapsed);
    }
    let now = chrono::Utc::now();
    match refreshed(&grant, now, valid_for, REFRESH_REQUEST).await {
        Ok(None) => Ok(entry),
        // The rotated refresh token is saved before the new access token is used.
        Ok(Some(fresh)) => {
            // The command line changes the store without this lock: an entry gone or changed
            // since it was read is not written over, and the token just rotated is given back.
            if !still_holds(state, at, server, &grant).await {
                tokio::spawn(async move { revoke(&fresh).await });
                return Err(Fresh::NotConfirmed);
            }
            entry.oauth = Some(fresh);
            keep(state, at, entry.clone())
                .await
                .map_err(|_| Fresh::Failed("the new sign-in could not be kept".to_string()))?;
            Ok(entry)
        }
        Err(SignInError::Lapsed) => {
            if !still_holds(state, at, server, &grant).await {
                return Err(Fresh::NotConfirmed);
            }
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

// ---- Sign-in attempts ----

/// How a sign-in attempt stands.
enum Outcome {
    Waiting,
    SignedIn(Box<OAuthGrant>),
    Failed(SignInError),
}

/// A sign-in under way, or finished and not yet used: bound to the agent and the server it was
/// made for, in the daemon's memory only, and gone ten minutes after it began.
pub(crate) struct Attempt {
    agent: String,
    server: String,
    url: String,
    oauth: OAuthSettings,
    issuer: String,
    started: tokio::time::Instant,
    outcome: Outcome,
    task: tokio::task::JoinHandle<()>,
}

/// What an attempt is bound to: the agent, and the server's name, address and sign-in settings.
pub(crate) struct Binding<'a> {
    pub(crate) agent: &'a str,
    pub(crate) server: &'a CustomServer,
}

impl Binding<'_> {
    fn is(&self, attempt: &Attempt) -> bool {
        let CustomTransport::Http {
            url,
            oauth: Some(oauth),
            ..
        } = &self.server.transport
        else {
            return false;
        };
        attempt.agent == self.agent
            && attempt.server == self.server.name
            && attempt.url == *url
            && attempt.oauth == *oauth
    }
}

/// 32 random hex digits.
fn random_hex() -> std::io::Result<String> {
    use std::fmt::Write as _;
    use std::io::Read as _;

    let mut random = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    Ok(random.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    }))
}

fn unknown(why: &str) -> String {
    format!("sign_in_unknown: {why}")
}

/// A refusal's words for why a sign-in could not start.
fn refusal_of(error: &SignInError) -> String {
    match error {
        SignInError::NotOffered => {
            "sign_in_not_offered: this server does not offer signing in".to_string()
        }
        SignInError::NotSupported => {
            "sign_in_not_supported: this server offers signing in, but Farik cannot register with it"
                .to_string()
        }
        SignInError::PkceNotSupported => {
            "pkce_not_supported: this server's sign-in is not one Farik will use".to_string()
        }
        SignInError::Failed(why) => format!("sign_in_failed: {why}"),
        SignInError::Denied(_) | SignInError::Mismatch | SignInError::TimedOut | SignInError::Lapsed => {
            "sign_in_failed: the sign-in could not be started".to_string()
        }
    }
}

impl DaemonState {
    /// Drops every attempt that has lasted ten minutes, and stops its listener.
    fn purge_sign_ins(map: &mut std::collections::BTreeMap<String, Attempt>) {
        map.retain(|_, attempt| {
            let live = attempt.started.elapsed() <= SIGN_IN_WINDOW;
            if !live {
                attempt.task.abort();
            }
            live
        });
    }

    /// Ends `agent`'s attempt at `server`, if any, and answers its task, which is waited for so
    /// that its listener is closed, and a fixed port free, when the next attempt starts.
    fn end_sign_in_of(
        map: &mut std::collections::BTreeMap<String, Attempt>,
        agent: &str,
        server: &str,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let ended: Vec<String> = map
            .iter()
            .filter(|(_, attempt)| attempt.agent == agent && attempt.server == server)
            .map(|(id, _)| id.clone())
            .collect();
        ended
            .into_iter()
            .filter_map(|id| map.remove(&id))
            .map(|attempt| {
                attempt.task.abort();
                attempt.task
            })
            .collect()
    }

    /// Starts signing `agent` in to `server`'s service. A new attempt ends the old one of the same
    /// agent and server first, so a fixed callback port is free again.
    ///
    /// # Errors
    /// The refusal, as `code: sentence`.
    pub(crate) async fn begin_sign_in(
        self: &Arc<Self>,
        agent: &str,
        server: &CustomServer,
    ) -> Result<(String, String, String), String> {
        let CustomTransport::Http {
            url,
            oauth: Some(oauth),
            ..
        } = &server.transport
        else {
            return Err("sign_in_failed: this server does not ask to sign in".to_string());
        };
        let ended = {
            let mut map = crate::locked(&self.sign_ins);
            Self::purge_sign_ins(&mut map);
            Self::end_sign_in_of(&mut map, agent, &server.name)
        };
        for task in ended {
            let _ = task.await;
        }
        let sign_in = start_sign_in(url, oauth, chrono::Utc::now())
            .await
            .map_err(|error| refusal_of(&error))?;
        let id = random_hex()
            .map_err(|_| "sign_in_failed: no random number was available".to_string())?;
        let (authorize_url, issuer) = (
            sign_in.authorize_url().to_string(),
            sign_in.issuer().to_string(),
        );
        let mut map = crate::locked(&self.sign_ins);
        // Inserted with the lock held, so the task's own answer finds it.
        let (state, attempt_id) = (Arc::clone(self), id.clone());
        let task = tokio::spawn(async move {
            let outcome = match sign_in.finish().await {
                Ok(grant) => Outcome::SignedIn(Box::new(grant)),
                Err(error) => Outcome::Failed(error),
            };
            if let Some(attempt) = crate::locked(&state.sign_ins).get_mut(&attempt_id) {
                attempt.outcome = outcome;
            }
        });
        map.insert(
            id.clone(),
            Attempt {
                agent: agent.to_string(),
                server: server.name.clone(),
                url: url.clone(),
                oauth: oauth.clone(),
                issuer: issuer.clone(),
                started: tokio::time::Instant::now(),
                outcome: Outcome::Waiting,
                task,
            },
        );
        Ok((id, authorize_url, issuer))
    }

    /// How the attempt `id` is going: `waiting`, `signed_in`, or `failed` with its reason's code
    /// and Farik's words. Never a token.
    ///
    /// # Errors
    /// The attempt is not known, was used, ended or expired.
    pub(crate) fn sign_in_status(&self, id: &str) -> Result<serde_json::Value, String> {
        let mut map = crate::locked(&self.sign_ins);
        Self::purge_sign_ins(&mut map);
        let attempt = map
            .get(id)
            .ok_or_else(|| unknown("there is no such sign-in under way"))?;
        let host = reqwest::Url::parse(&attempt.issuer)
            .ok()
            .and_then(|url| url.host_str().map(ToString::to_string))
            .unwrap_or_else(|| "the service".to_string());
        Ok(match &attempt.outcome {
            Outcome::Waiting => serde_json::json!({ "state": "waiting" }),
            Outcome::SignedIn(_) => serde_json::json!({ "state": "signed_in" }),
            Outcome::Failed(error) => {
                let (code, message) = match error {
                    SignInError::Denied(_) => {
                        ("access_denied", format!("You said no on {host}'s page."))
                    }
                    SignInError::TimedOut => (
                        "sign_in_timed_out",
                        "The sign-in took longer than 10 minutes.".to_string(),
                    ),
                    SignInError::Mismatch => (
                        "sign_in_mismatch",
                        format!("Something did not match on the way back from {host}."),
                    ),
                    SignInError::Failed(why) => ("sign_in_failed", why.clone()),
                    _ => (
                        "sign_in_failed",
                        format!("{host} did not accept the sign-in."),
                    ),
                };
                serde_json::json!({ "state": "failed", "reason": { "code": code, "message": message } })
            }
        })
    }

    /// The grant of the finished attempt `id`, which was made for `binding`; the attempt stays.
    /// One bound to another agent or server is ended.
    ///
    /// # Errors
    /// The attempt is unknown, unfinished, failed, expired, or another server's.
    pub(crate) fn peek_sign_in(
        &self,
        id: &str,
        binding: &Binding<'_>,
    ) -> Result<OAuthGrant, String> {
        let mut map = crate::locked(&self.sign_ins);
        Self::purge_sign_ins(&mut map);
        let attempt = map
            .get(id)
            .ok_or_else(|| unknown("there is no such sign-in under way"))?;
        if !binding.is(attempt) {
            if let Some(ended) = map.remove(id) {
                ended.task.abort();
            }
            return Err(unknown("that sign-in was for another server"));
        }
        match &attempt.outcome {
            Outcome::SignedIn(grant) => Ok((**grant).clone()),
            _ => Err(unknown("that sign-in is not finished")),
        }
    }

    /// Uses up the finished attempt `id`, which was made for `binding`: its grant and its issuer,
    /// and the attempt, to be put back with [`DaemonState::restore_sign_in`] if what it was taken
    /// for fails before the grant is kept.
    ///
    /// # Errors
    /// As [`DaemonState::peek_sign_in`].
    pub(crate) fn take_sign_in(
        &self,
        id: &str,
        binding: &Binding<'_>,
    ) -> Result<(OAuthGrant, String, Attempt), String> {
        let grant = self.peek_sign_in(id, binding)?;
        let attempt = crate::locked(&self.sign_ins)
            .remove(id)
            .ok_or_else(|| unknown("that sign-in was used"))?;
        let issuer = attempt.issuer.clone();
        Ok((grant, issuer, attempt))
    }

    /// Puts back an attempt [`DaemonState::take_sign_in`] took, whose use failed.
    pub(crate) fn restore_sign_in(&self, id: &str, attempt: Attempt) {
        crate::locked(&self.sign_ins).insert(id.to_string(), attempt);
    }
}

/// Deletes the entry at `at` and, in a task of its own, asks the service to forget the sign-in it
/// held.
pub(super) fn delete_and_revoke(secrets: &Arc<dyn ConnectorSecrets>, at: &SecretAt) {
    let grant = secrets
        .load(at)
        .ok()
        .flatten()
        .and_then(|entry| entry.oauth);
    let _ = secrets.delete(at);
    if let Some(grant) = grant
        && let Ok(runtime) = tokio::runtime::Handle::try_current()
    {
        runtime.spawn(async move { revoke(&grant).await });
    }
}
