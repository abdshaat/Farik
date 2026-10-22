//! The owner's GitHub (`docs/SPEC.md` 4.4, 6.7, 8.6; ADR 0059, ADR 0044): their repositories pushed
//! to lately, and the branches and pull requests Claude Code on the web made in them. Read only.
//! The client speaks to one fixed address, follows no redirect and uses no proxy, and every answer
//! is read for the few fields named here and no others.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::claude::Secret;

/// GitHub's address. Only the tests give [`GithubClient::new_at`] another.
pub const GITHUB_API: &str = "https://api.github.com";

/// A repository the owner pushes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    /// `owner/name`.
    pub full_name: String,
    /// Whether it is private.
    pub private: bool,
    /// When it was last pushed to.
    pub pushed_at: DateTime<Utc>,
    /// Its default branch.
    pub default_branch: String,
}

/// An open pull request from a branch named `claude/…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudePull {
    /// Its number.
    pub number: u64,
    /// Its title.
    pub title: String,
    /// Whether it is a draft.
    pub draft: bool,
    /// The branch it comes from.
    pub branch: String,
    /// When it last changed.
    pub updated_at: String,
}

/// Why GitHub could not be asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubError {
    /// GitHub did not accept the token.
    Refused,
    /// GitHub's rate limit is used up until `reset`, when it says when.
    RateLimited {
        /// When the limit resets.
        reset: Option<DateTime<Utc>>,
    },
    /// Anything else, in a sentence that holds none of GitHub's own words.
    Failed(String),
}

impl fmt::Display for GithubError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused => write!(formatter, "GitHub did not accept the token"),
            Self::RateLimited { reset: Some(reset) } => write!(
                formatter,
                "GitHub's limit is used up until {}",
                reset.format("%H:%M UTC")
            ),
            Self::RateLimited { reset: None } => write!(formatter, "GitHub's limit is used up"),
            Self::Failed(why) => write!(formatter, "{why}"),
        }
    }
}

impl std::error::Error for GithubError {}

/// The client, speaking to one address.
#[derive(Clone)]
pub struct GithubClient {
    base: String,
    client: reqwest::Client,
}

impl GithubClient {
    /// A client for GitHub itself.
    ///
    /// # Errors
    ///
    /// The web client could not be made.
    pub fn new() -> Result<Self, GithubError> {
        Self::new_at(GITHUB_API)
    }

    /// A client for `base`: the tests give a fixture's address.
    ///
    /// # Errors
    ///
    /// The web client could not be made.
    pub fn new_at(base: &str) -> Result<Self, GithubError> {
        let client = reqwest::Client::builder()
            // Nothing GitHub answers sends Catervas anywhere else, and nothing goes through a proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| failed("the web client could not be made"))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// One GET of `path` under the fixed address, answered as JSON.
    async fn get(&self, token: &Secret, path: &str) -> Result<Value, GithubError> {
        let mut response = self
            .client
            .get(format!("{}{path}", self.base))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .header("user-agent", "catervas")
            .bearer_auth(token.expose())
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    failed("GitHub did not answer in time")
                } else {
                    failed("GitHub could not be reached")
                }
            })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GithubError::Refused);
        }
        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            let header = |name: &str| {
                response
                    .headers()
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
            };
            if header("x-ratelimit-remaining").as_deref() == Some("0")
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            {
                let reset = header("x-ratelimit-reset")
                    .and_then(|epoch| epoch.parse::<i64>().ok())
                    .and_then(|epoch| DateTime::from_timestamp(epoch, 0));
                return Err(GithubError::RateLimited { reset });
            }
            return Err(failed("GitHub would not show that to this token"));
        }
        if status.is_redirection() {
            return Err(failed(
                "GitHub answered with a redirect, which Catervas does not follow",
            ));
        }
        if !status.is_success() {
            return Err(failed(&format!("GitHub answered {}", status.as_u16())));
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| failed("GitHub's answer was cut off"))?
        {
            if chunk.len() > MAX_BODY - bytes.len() {
                return Err(failed("GitHub's answer is too large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| failed("GitHub's answer was not understood"))
    }

    /// The login the token belongs to (`GET /user`).
    ///
    /// # Errors
    ///
    /// [`GithubError`].
    pub async fn login(&self, token: &Secret) -> Result<String, GithubError> {
        let user = self.get(token, "/user").await?;
        user["login"]
            .as_str()
            .filter(|login| !login.is_empty())
            .map(str::to_string)
            .ok_or_else(|| failed("GitHub's answer was not understood"))
    }

    /// The repositories pushed to in the last 90 days, most recent first, the first page only.
    ///
    /// # Errors
    ///
    /// [`GithubError`].
    pub async fn repositories(
        &self,
        token: &Secret,
        now: DateTime<Utc>,
    ) -> Result<Vec<Repository>, GithubError> {
        let rows = self
            .get(
                token,
                "/user/repos?sort=pushed&per_page=100&affiliation=owner,collaborator",
            )
            .await?;
        let since = now - chrono::Duration::days(RECENT_DAYS);
        Ok(rows
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|row| {
                let pushed_at = DateTime::parse_from_rfc3339(row["pushed_at"].as_str()?)
                    .ok()?
                    .with_timezone(&Utc);
                Some(Repository {
                    full_name: valid_full_name(row["full_name"].as_str()?)?.to_string(),
                    private: row["private"].as_bool().unwrap_or(true),
                    pushed_at,
                    default_branch: row["default_branch"].as_str().unwrap_or("main").to_string(),
                })
            })
            .filter(|repo| repo.pushed_at >= since)
            .collect())
    }

    /// The branches of `repo` (`owner/name`) named `claude/…`.
    ///
    /// # Errors
    ///
    /// [`GithubError`].
    pub async fn claude_branches(
        &self,
        token: &Secret,
        repo: &str,
    ) -> Result<Vec<String>, GithubError> {
        let repo = valid_full_name(repo).ok_or_else(|| failed("that is not a repository"))?;
        let rows = self
            .get(token, &format!("/repos/{repo}/branches?per_page=100"))
            .await?;
        Ok(rows
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|row| row["name"].as_str())
            .filter(|name| name.starts_with(CLAUDE_PREFIX))
            .map(str::to_string)
            .collect())
    }

    /// The open pull requests of `repo` from branches named `claude/…`.
    ///
    /// # Errors
    ///
    /// [`GithubError`].
    pub async fn claude_pulls(
        &self,
        token: &Secret,
        repo: &str,
    ) -> Result<Vec<ClaudePull>, GithubError> {
        let repo = valid_full_name(repo).ok_or_else(|| failed("that is not a repository"))?;
        let rows = self
            .get(
                token,
                &format!("/repos/{repo}/pulls?state=open&per_page=100"),
            )
            .await?;
        Ok(rows
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|row| {
                let branch = row["head"]["ref"].as_str()?;
                branch.starts_with(CLAUDE_PREFIX).then(|| ClaudePull {
                    number: row["number"].as_u64().unwrap_or(0),
                    title: row["title"]
                        .as_str()
                        .unwrap_or_default()
                        .chars()
                        .take(200)
                        .collect(),
                    draft: row["draft"].as_bool().unwrap_or(false),
                    branch: branch.to_string(),
                    updated_at: row["updated_at"].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect())
    }
}

/// How long one call to GitHub may take.
const TIMEOUT: Duration = Duration::from_secs(15);
/// The most of an answer Catervas reads.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// How many days back a push still counts as lately.
const RECENT_DAYS: i64 = 90;
/// What the branches Claude Code on the web makes start with.
const CLAUDE_PREFIX: &str = "claude/";

fn failed(words: &str) -> GithubError {
    GithubError::Failed(words.to_string())
}

/// `name` when it is `owner/name` in the characters GitHub allows, so that it is safe in a path.
#[must_use]
pub fn valid_full_name(name: &str) -> Option<&str> {
    let (owner, repo) = name.split_once('/')?;
    let fine = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (fine(owner) && fine(repo)).then_some(name)
}

/// Runs `future` to its end on a thread that is not an async task's: the scan and the hub's blocking
/// calls are such threads.
///
/// # Errors
///
/// A sentence when the runtime could not be made.
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|runtime| runtime.block_on(future))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github_fixture::Fixture;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
            .expect("a moment")
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn reads_repositories_and_claude_s_work_from_github() {
        let fixture = Fixture::start("ghp_invented", now()).await;
        let client = GithubClient::new_at(&fixture.address).expect("a client");
        let token = Secret::new("ghp_invented".to_string());

        assert_eq!(client.login(&token).await.expect("a login"), "octo");
        let repos = client.repositories(&token, now()).await.expect("repos");
        let names: Vec<&str> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(
            names,
            ["octo/shop", "octo/notes"],
            "the old one is left out"
        );
        assert!(repos[0].private);
        assert_eq!(repos[1].default_branch, "trunk");

        let branches = client
            .claude_branches(&token, "octo/shop")
            .await
            .expect("branches");
        assert!(
            branches.contains(&"claude/fix-login".to_string()),
            "{branches:?}"
        );
        assert!(!branches.contains(&"main".to_string()));
        let pulls = client
            .claude_pulls(&token, "octo/shop")
            .await
            .expect("pulls");
        assert_eq!(pulls.len(), 1, "{pulls:?}");
        assert_eq!(
            (pulls[0].number, pulls[0].draft, pulls[0].branch.as_str()),
            (12, true, "claude/fix-login")
        );

        let requests = fixture.requests();
        assert!(requests.len() >= 4);
        for request in &requests {
            assert_eq!(
                request.headers.get("authorization").map(String::as_str),
                Some("Bearer ghp_invented")
            );
            assert_eq!(
                request
                    .headers
                    .get("x-github-api-version")
                    .map(String::as_str),
                Some("2022-11-28")
            );
            assert_eq!(
                request.headers.get("accept").map(String::as_str),
                Some("application/vnd.github+json")
            );
        }
        assert!(
            requests
                .iter()
                .any(|r| r.path
                    == "/user/repos?sort=pushed&per_page=100&affiliation=owner,collaborator"),
            "{requests:?}"
        );

        // A wrong token is refused, and its words are never passed on.
        let wrong = Secret::new("ghp_wrong".to_string());
        assert_eq!(client.login(&wrong).await, Err(GithubError::Refused));

        // No rate left is a rate limit with its reset.
        fixture.script.lock().expect("script").rate_limited_until = Some(1_792_000_000);
        let limited = client.repositories(&token, now()).await;
        assert_eq!(
            limited,
            Err(GithubError::RateLimited {
                reset: DateTime::from_timestamp(1_792_000_000, 0)
            })
        );
        // A 403 with rate left is neither.
        {
            let mut script = fixture.script.lock().expect("script");
            script.rate_limited_until = None;
            script.forbidden = true;
        }
        assert!(matches!(
            client.repositories(&token, now()).await,
            Err(GithubError::Failed(_))
        ));
    }
}
