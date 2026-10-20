//! The forge a team's pull requests live on, driven through the `gh` program (`docs/SPEC.md`
//! 5.14): Catervas opens a pull request for an accepted task and reads whether the human merged it.
//! `gh` runs in the repository root with the user's own environment and sign-in, as the store runs
//! git; no credential of it enters a container (ADR 0012).

use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Deserialize;

/// The `gh` program, and the repository it acts for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forge {
    /// The program, `gh` on the path outside tests.
    pub program: PathBuf,
    /// The repository root it runs in.
    pub root: PathBuf,
}

/// A pull request on the forge, as `gh pr list --json url,number` spells one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullRequest {
    /// Its address.
    pub url: String,
    /// Its number on the forge.
    pub number: u64,
}

/// An issue on the forge, as `catervas contract new --from` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    /// Its title.
    pub title: String,
    /// Its text.
    pub body: String,
    /// Its address.
    pub url: String,
}

/// Where a pull request stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestState {
    /// Neither merged nor closed.
    Open {
        /// Whether it would merge now, as the forge last worked it out.
        mergeable: Mergeable,
        /// What stands between it and a merge (`mergeStateStatus`).
        merge_state: MergeState,
        /// The commit its branch is at (`headRefOid`), which a merge is pinned to.
        head: String,
        /// Where its checks stand.
        checks: Checks,
    },
    /// Merged, leaving this commit on its base branch.
    Merged {
        /// The merge commit.
        sha: String,
    },
    /// Closed without merging.
    Closed,
}

/// Whether an open pull request merges into its base, as the forge says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mergeable {
    /// It merges.
    Mergeable,
    /// Its base changed the same lines: it does not merge.
    Conflicting,
    /// The forge has not worked it out yet (it does after a push), or said nothing.
    Unknown,
}

/// What stands between an open pull request and its merge, as `mergeStateStatus` says; absent or
/// unknown text is `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeState {
    /// It merges and nothing blocks it.
    Clean,
    /// Its branch is behind its base.
    Behind,
    /// A rule of the branch blocks it (a review, say).
    Blocked,
    /// It conflicts with its base.
    Dirty,
    /// It merges, but a check that is not required failed.
    Unstable,
    /// It merges, and the repository has pre-receive hooks.
    HasHooks,
    /// It is a draft.
    Draft,
    /// The forge has not worked it out, or said something else.
    Unknown,
}

/// Where the checks of a pull request stand (`statusCheckRollup`): every check run and every
/// status the forge lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checks {
    /// None is configured: the rollup is empty or absent.
    None,
    /// None failed and some have not finished.
    Pending,
    /// All finished and passed.
    Passed,
    /// At least one failed.
    Failed {
        /// Each failed check's name, in the rollup's order.
        names: Vec<String>,
    },
}

/// How `gh pr merge` merges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    /// A merge commit (`--merge`).
    Merge,
    /// A squash (`--squash`).
    Squash,
    /// A rebase (`--rebase`).
    Rebase,
}

impl MergeMethod {
    /// The wire word, which `pull_request.merged` records.
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

/// Why the forge could not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeError {
    /// The program could not be run at all.
    Missing {
        /// The program asked for.
        program: String,
    },
    /// It ran and refused, or answered what cannot be read.
    Failed {
        /// Its words, or what could not be read.
        detail: String,
    },
}

impl fmt::Display for ForgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { program } => write!(formatter, "{program} could not be run"),
            Self::Failed { detail } => write!(formatter, "{detail}"),
        }
    }
}

impl std::error::Error for ForgeError {}

/// What `gh pr view --json state,mergeCommit,mergeable` answers.
#[derive(Deserialize)]
struct Viewed {
    state: String,
    #[serde(rename = "mergeCommit")]
    merge_commit: Option<MergeCommit>,
    #[serde(default)]
    mergeable: Option<String>,
    #[serde(default, rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    #[serde(default, rename = "headRefOid")]
    head_ref_oid: Option<String>,
    #[serde(default, rename = "statusCheckRollup")]
    status_check_rollup: Option<Vec<RollupItem>>,
}

/// One entry of `statusCheckRollup`: a check run or a status, whichever `__typename` says.
#[derive(Deserialize)]
struct RollupItem {
    #[serde(default, rename = "__typename")]
    typename: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

/// What `gh api repos/{owner}/{repo}` says of the merge methods; a missing field counts as allowed.
#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "allow_merge_commit")]
    merge_commit: Option<bool>,
    #[serde(rename = "allow_squash_merge")]
    squash_merge: Option<bool>,
    #[serde(rename = "allow_rebase_merge")]
    rebase_merge: Option<bool>,
}

/// How one rollup entry stands: passed, still pending, or failed under this name.
enum Standing {
    Passed,
    Pending,
    Failed(String),
}

fn standing(item: &RollupItem) -> Standing {
    let held = |text: &Option<String>| text.as_deref().unwrap_or_default().to_string();
    match item.typename.as_deref() {
        Some("CheckRun") => match (item.status.as_deref(), item.conclusion.as_deref()) {
            (Some("COMPLETED"), Some("SUCCESS" | "NEUTRAL" | "SKIPPED")) => Standing::Passed,
            (Some("COMPLETED"), _) => Standing::Failed(held(&item.name)),
            _ => Standing::Pending,
        },
        Some("StatusContext") => match item.state.as_deref() {
            Some("SUCCESS") => Standing::Passed,
            Some("PENDING" | "EXPECTED") => Standing::Pending,
            _ => Standing::Failed(held(&item.context)),
        },
        // Anything else is not read as passed.
        _ => Standing::Failed(
            item.name
                .clone()
                .or_else(|| item.context.clone())
                .unwrap_or_else(|| "a check GitHub did not describe".to_string()),
        ),
    }
}

fn checks_of(rollup: &[RollupItem]) -> Checks {
    let mut failed = Vec::new();
    let mut pending = false;
    for item in rollup {
        match standing(item) {
            Standing::Passed => {}
            Standing::Pending => pending = true,
            Standing::Failed(name) => failed.push(name),
        }
    }
    if !failed.is_empty() {
        Checks::Failed { names: failed }
    } else if pending {
        Checks::Pending
    } else if rollup.is_empty() {
        Checks::None
    } else {
        Checks::Passed
    }
}

/// A merged pull request's commit, `null` until the merge.
#[derive(Deserialize)]
struct MergeCommit {
    oid: String,
}

impl Forge {
    /// The open pull request from `head` into `base`, reused when there is one, so that a run
    /// stopped between opening it and recording it opens no second one; otherwise a new one with
    /// `title` and `body`, the body given on standard input.
    ///
    /// # Errors
    ///
    /// `Missing` when the program cannot be run; `Failed` when it exits non-zero, with its standard
    /// error, or when its answer is not a pull request.
    pub fn open_pull_request(
        &self,
        base: &str,
        head: &str,
        title: &str,
        body: &str,
    ) -> Result<PullRequest, ForgeError> {
        let listed = self.run(
            &[
                "pr",
                "list",
                "--head",
                head,
                "--base",
                base,
                "--state",
                "open",
                "--json",
                "url,number",
                "--limit",
                "1",
            ],
            None,
        )?;
        let open: Vec<PullRequest> =
            serde_json::from_str(&listed).map_err(|_| unreadable("gh pr list", &listed))?;
        if let Some(found) = open.into_iter().next() {
            return Ok(found);
        }
        let created = self.run(
            &[
                "pr",
                "create",
                "--base",
                base,
                "--head",
                head,
                "--title",
                title,
                "--body-file",
                "-",
            ],
            Some(body),
        )?;
        let url = created
            .lines()
            .last()
            .unwrap_or_default()
            .trim()
            .to_string();
        let number = url
            .rsplit('/')
            .next()
            .and_then(|segment| segment.parse().ok())
            .ok_or_else(|| unreadable("gh pr create", &created))?;
        Ok(PullRequest { url, number })
    }

    /// Where the pull request at `url` stands.
    ///
    /// # Errors
    ///
    /// As `open_pull_request`.
    pub fn pull_request_state(&self, url: &str) -> Result<PullRequestState, ForgeError> {
        let viewed = self.run(
            &[
                "pr",
                "view",
                url,
                "--json",
                "state,mergeCommit,mergeable,mergeStateStatus,headRefOid,statusCheckRollup",
            ],
            None,
        )?;
        let read: Viewed =
            serde_json::from_str(&viewed).map_err(|_| unreadable("gh pr view", &viewed))?;
        let mergeable = match read.mergeable.as_deref() {
            Some("MERGEABLE") => Mergeable::Mergeable,
            Some("CONFLICTING") => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        };
        let merge_state = match read.merge_state_status.as_deref() {
            Some("CLEAN") => MergeState::Clean,
            Some("BEHIND") => MergeState::Behind,
            Some("BLOCKED") => MergeState::Blocked,
            Some("DIRTY") => MergeState::Dirty,
            Some("UNSTABLE") => MergeState::Unstable,
            Some("HAS_HOOKS") => MergeState::HasHooks,
            Some("DRAFT") => MergeState::Draft,
            _ => MergeState::Unknown,
        };
        match (read.state.as_str(), read.merge_commit) {
            ("OPEN", _) => Ok(PullRequestState::Open {
                mergeable,
                merge_state,
                head: read.head_ref_oid.unwrap_or_default(),
                checks: checks_of(&read.status_check_rollup.unwrap_or_default()),
            }),
            ("CLOSED", _) => Ok(PullRequestState::Closed),
            ("MERGED", Some(commit)) => Ok(PullRequestState::Merged { sha: commit.oid }),
            _ => Err(unreadable("gh pr view", &viewed)),
        }
    }

    /// The merge methods the repository allows, in the order merge commit, squash, rebase:
    /// `gh api repos/{owner}/{repo}`, whose braces `gh` fills in from the repository it runs in.
    ///
    /// # Errors
    ///
    /// As `open_pull_request`.
    pub fn merge_methods(&self) -> Result<Vec<MergeMethod>, ForgeError> {
        let answer = self.run(&["api", "repos/{owner}/{repo}"], None)?;
        let read: Repository =
            serde_json::from_str(&answer).map_err(|_| unreadable("gh api", &answer))?;
        Ok([
            (MergeMethod::Merge, read.merge_commit),
            (MergeMethod::Squash, read.squash_merge),
            (MergeMethod::Rebase, read.rebase_merge),
        ]
        .into_iter()
        .filter(|(_, allowed)| allowed.unwrap_or(true))
        .map(|(method, _)| method)
        .collect())
    }

    /// Merges the pull request at `url` with `method`, only while its branch is still at `head`
    /// (`--match-head-commit`), so a push made after the checks were read is never merged
    /// unchecked. It never passes `--admin` or `--auto`, and it deletes no branch.
    ///
    /// # Errors
    ///
    /// As `open_pull_request`: `Failed` carries what `gh` said when it refused.
    pub fn merge_pull_request(
        &self,
        url: &str,
        method: MergeMethod,
        head: &str,
    ) -> Result<(), ForgeError> {
        let flag = match method {
            MergeMethod::Merge => "--merge",
            MergeMethod::Squash => "--squash",
            MergeMethod::Rebase => "--rebase",
        };
        self.run(
            &["pr", "merge", url, flag, "--match-head-commit", head],
            None,
        )
        .map(|_| ())
    }

    /// The issue at `url`: `gh issue view <url> --json title,body,url`.
    ///
    /// # Errors
    ///
    /// `Missing` when the program cannot be run; `Failed` when it exits non-zero, with its
    /// standard error, or when its answer is not an issue.
    pub fn issue(&self, url: &str) -> Result<Issue, ForgeError> {
        let viewed = self.run(&["issue", "view", url, "--json", "title,body,url"], None)?;
        serde_json::from_str(&viewed).map_err(|_| unreadable("gh issue view", &viewed))
    }

    /// Runs the program in the root with `arguments`, giving it `stdin` (or nothing), and answers
    /// its standard output.
    fn run(&self, arguments: &[&str], stdin: Option<&str>) -> Result<String, ForgeError> {
        let mut child = Command::new(&self.program)
            .args(arguments)
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| ForgeError::Missing {
                program: self.program.display().to_string(),
            })?;
        if let Some(mut input) = child.stdin.take() {
            // A program that exits without reading its input is answered by its exit, below.
            let _ = input.write_all(stdin.unwrap_or_default().as_bytes());
        }
        let output = child
            .wait_with_output()
            .map_err(|error| ForgeError::Failed {
                detail: error.to_string(),
            })?;
        if !output.status.success() {
            return Err(ForgeError::Failed {
                detail: format!(
                    "gh {} failed: {}",
                    arguments.first().copied().unwrap_or_default(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// `Failed`, naming what `command` answered that could not be read.
fn unreadable(command: &str, output: &str) -> ForgeError {
    ForgeError::Failed {
        detail: format!(
            "{command} answered what could not be read: {}",
            output.trim()
        ),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;

    use super::{
        Checks, Forge, ForgeError, Issue, MergeMethod, MergeState, Mergeable, PullRequest,
        PullRequestState,
    };
    use crate::orchestrator::fixtures::FakeGh;

    const BODY: &str = "## Intent\nDone.\n";

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(ToString::to_string).collect()
    }

    fn open(forge: &Forge) -> Result<PullRequest, ForgeError> {
        forge.open_pull_request("main", "catervas/CTV-1", "CTV-1: Add done", BODY)
    }

    #[test]
    fn opens_a_pull_request_with_gh() {
        let gh = FakeGh::new("opens");
        gh.answers("list", "[]\n", "", 0);
        gh.answers(
            "create",
            "Creating...\nhttps://github.com/o/r/pull/7\n",
            "",
            0,
        );
        let forge = gh.forge(&std::env::temp_dir());

        assert_eq!(
            open(&forge),
            Ok(PullRequest {
                url: "https://github.com/o/r/pull/7".to_string(),
                number: 7,
            })
        );
        assert_eq!(
            gh.calls(),
            vec![
                strings(&[
                    "pr",
                    "list",
                    "--head",
                    "catervas/CTV-1",
                    "--base",
                    "main",
                    "--state",
                    "open",
                    "--json",
                    "url,number",
                    "--limit",
                    "1",
                ]),
                strings(&[
                    "pr",
                    "create",
                    "--base",
                    "main",
                    "--head",
                    "catervas/CTV-1",
                    "--title",
                    "CTV-1: Add done",
                    "--body-file",
                    "-",
                ]),
            ]
        );
        assert_eq!(gh.stdin_of("create"), BODY);
    }

    #[test]
    fn reuses_an_open_pull_request_for_its_branch() {
        let gh = FakeGh::new("reuses");
        gh.answers(
            "list",
            r#"[{"url":"https://github.com/o/r/pull/7","number":7}]"#,
            "",
            0,
        );
        let forge = gh.forge(&std::env::temp_dir());

        assert_eq!(
            open(&forge),
            Ok(PullRequest {
                url: "https://github.com/o/r/pull/7".to_string(),
                number: 7,
            })
        );
        assert!(
            !gh.calls().iter().any(|call| call[1] == "create"),
            "{:?}",
            gh.calls()
        );
    }

    #[test]
    fn reads_a_pull_requests_state() {
        let gh = FakeGh::new("state");
        let forge = gh.forge(&std::env::temp_dir());
        let url = "https://github.com/o/r/pull/7";

        gh.answers(
            "view",
            r#"{"state":"MERGED","mergeCommit":{"oid":"abc"}}"#,
            "",
            0,
        );
        assert_eq!(
            forge.pull_request_state(url),
            Ok(PullRequestState::Merged {
                sha: "abc".to_string()
            })
        );
        let open = |mergeable| {
            Ok(PullRequestState::Open {
                mergeable,
                merge_state: MergeState::Unknown,
                head: String::new(),
                checks: Checks::None,
            })
        };
        gh.answers(
            "view",
            r#"{"mergeCommit":null,"state":"OPEN","mergeable":"CONFLICTING"}"#,
            "",
            0,
        );
        assert_eq!(forge.pull_request_state(url), open(Mergeable::Conflicting));
        gh.answers(
            "view",
            r#"{"mergeCommit":null,"state":"OPEN","mergeable":"MERGEABLE"}"#,
            "",
            0,
        );
        assert_eq!(forge.pull_request_state(url), open(Mergeable::Mergeable));
        gh.answers(
            "view",
            r#"{"mergeCommit":null,"state":"OPEN","mergeable":"UNKNOWN"}"#,
            "",
            0,
        );
        assert_eq!(forge.pull_request_state(url), open(Mergeable::Unknown));
        gh.answers("view", r#"{"mergeCommit":null,"state":"OPEN"}"#, "", 0);
        assert_eq!(forge.pull_request_state(url), open(Mergeable::Unknown));
        gh.answers("view", r#"{"mergeCommit":null,"state":"CLOSED"}"#, "", 0);
        assert_eq!(forge.pull_request_state(url), Ok(PullRequestState::Closed));
        assert_eq!(
            gh.calls()[0],
            strings(&[
                "pr",
                "view",
                url,
                "--json",
                "state,mergeCommit,mergeable,mergeStateStatus,headRefOid,statusCheckRollup"
            ])
        );
    }

    #[test]
    fn reads_the_checks_and_merges_with_the_allowed_method() {
        let gh = FakeGh::new("checks");
        let forge = gh.forge(&std::env::temp_dir());
        let url = "https://github.com/o/r/pull/7";
        let viewed = |rollup: &str| {
            format!(
                r#"{{"state":"OPEN","mergeCommit":null,"mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","headRefOid":"abc","statusCheckRollup":{rollup}}}"#
            )
        };
        let pass = r#"{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"}"#;
        let fail = r#"{"__typename":"StatusContext","context":"ci/lint","state":"FAILURE"}"#;
        let waits =
            r#"{"__typename":"CheckRun","name":"slow","status":"IN_PROGRESS","conclusion":""}"#;
        let checks = |rollup: &str| {
            gh.answers("view", &viewed(rollup), "", 0);
            match forge.pull_request_state(url) {
                Ok(PullRequestState::Open {
                    merge_state,
                    head,
                    checks,
                    ..
                }) => {
                    assert_eq!((merge_state, head.as_str()), (MergeState::Clean, "abc"));
                    checks
                }
                other => panic!("expected an open pull request, got {other:?}"),
            }
        };
        assert_eq!(
            checks(&format!("[{pass},{fail}]")),
            Checks::Failed {
                names: vec!["ci/lint".to_string()]
            }
        );
        assert_eq!(checks(&format!("[{pass},{waits}]")), Checks::Pending);
        assert_eq!(checks(&format!("[{pass}]")), Checks::Passed);
        assert_eq!(checks("[]"), Checks::None);

        gh.answers(
            "repos/{owner}/{repo}",
            r#"{"allow_merge_commit":false,"allow_squash_merge":true}"#,
            "",
            0,
        );
        assert_eq!(
            forge.merge_methods(),
            Ok(vec![MergeMethod::Squash, MergeMethod::Rebase])
        );

        gh.answers("merge", "", "", 0);
        assert_eq!(
            forge.merge_pull_request(url, MergeMethod::Squash, "abc"),
            Ok(())
        );
        assert_eq!(
            gh.calls().last(),
            Some(&strings(&[
                "pr",
                "merge",
                url,
                "--squash",
                "--match-head-commit",
                "abc"
            ]))
        );
    }

    #[test]
    fn reads_an_issue_with_gh() {
        let gh = FakeGh::new("issue");
        gh.answers("view", r#"{"title":"T","body":"B","url":"u"}"#, "", 0);
        let forge = gh.forge(&std::env::temp_dir());
        let url = "https://github.com/o/r/issues/3";

        assert_eq!(
            forge.issue(url),
            Ok(Issue {
                title: "T".to_string(),
                body: "B".to_string(),
                url: "u".to_string(),
            })
        );
        assert_eq!(
            gh.calls(),
            vec![strings(&["issue", "view", url, "--json", "title,body,url"])]
        );

        gh.answers("view", "", "not found", 1);
        let detail = failed(forge.issue(url));
        assert!(detail.contains("not found"), "{detail}");
    }

    #[test]
    fn says_when_gh_is_missing() {
        let forge = Forge {
            program: PathBuf::from("/nonexistent/catervas/gh"),
            root: std::env::temp_dir(),
        };
        assert_eq!(
            open(&forge),
            Err(ForgeError::Missing {
                program: "/nonexistent/catervas/gh".to_string()
            })
        );
    }

    fn failed(result: Result<impl std::fmt::Debug, ForgeError>) -> String {
        match result {
            Err(ForgeError::Failed { detail }) => detail,
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn reports_what_gh_said_when_it_fails() {
        let gh = FakeGh::new("fails");
        let forge = gh.forge(&std::env::temp_dir());

        gh.answers("list", "", "not logged in", 1);
        let detail = failed(open(&forge));
        assert!(detail.contains("not logged in"), "{detail}");

        gh.answers("view", "nonsense", "", 0);
        let detail = failed(forge.pull_request_state("https://github.com/o/r/pull/7"));
        assert!(detail.contains("nonsense"), "{detail}");

        gh.answers("list", "[]", "", 0);
        gh.answers("create", "nonsense", "", 0);
        let detail = failed(open(&forge));
        assert!(detail.contains("nonsense"), "{detail}");
    }
}
