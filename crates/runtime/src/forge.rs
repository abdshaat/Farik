//! The forge a team's pull requests live on, driven through the `gh` program (`docs/SPEC.md`
//! 5.14): Farik opens a pull request for an accepted task and reads whether the human merged it.
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

/// A pull request on the forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// Its address.
    pub url: String,
    /// Its number on the forge.
    pub number: u64,
}

/// Where a pull request stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestState {
    /// Neither merged nor closed.
    Open,
    /// Merged, leaving this commit on its base branch.
    Merged {
        /// The merge commit.
        sha: String,
    },
    /// Closed without merging.
    Closed,
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

/// One row of `gh pr list --json url,number`.
#[derive(Deserialize)]
struct Listed {
    url: String,
    number: u64,
}

/// What `gh pr view --json state,mergeCommit` answers.
#[derive(Deserialize)]
struct Viewed {
    state: String,
    #[serde(rename = "mergeCommit")]
    merge_commit: Option<MergeCommit>,
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
        let open: Vec<PullRequest> = serde_json::from_str::<Vec<Listed>>(&listed)
            .map_err(|_| unreadable("gh pr list", &listed))?
            .into_iter()
            .map(|listed| PullRequest {
                url: listed.url,
                number: listed.number,
            })
            .collect();
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
        let viewed = self.run(&["pr", "view", url, "--json", "state,mergeCommit"], None)?;
        let read: Viewed =
            serde_json::from_str(&viewed).map_err(|_| unreadable("gh pr view", &viewed))?;
        match (read.state.as_str(), read.merge_commit) {
            ("OPEN", _) => Ok(PullRequestState::Open),
            ("CLOSED", _) => Ok(PullRequestState::Closed),
            ("MERGED", Some(commit)) => Ok(PullRequestState::Merged { sha: commit.oid }),
            _ => Err(unreadable("gh pr view", &viewed)),
        }
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

    use super::{Forge, ForgeError, PullRequest, PullRequestState};
    use crate::orchestrator::fixtures::FakeGh;

    const BODY: &str = "## Intent\nDone.\n";

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(ToString::to_string).collect()
    }

    fn open(forge: &Forge) -> Result<PullRequest, ForgeError> {
        forge.open_pull_request("main", "farik/FRK-1", "FRK-1: Add done", BODY)
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
                    "farik/FRK-1",
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
                    "farik/FRK-1",
                    "--title",
                    "FRK-1: Add done",
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
        gh.answers("view", r#"{"mergeCommit":null,"state":"OPEN"}"#, "", 0);
        assert_eq!(forge.pull_request_state(url), Ok(PullRequestState::Open));
        gh.answers("view", r#"{"mergeCommit":null,"state":"CLOSED"}"#, "", 0);
        assert_eq!(forge.pull_request_state(url), Ok(PullRequestState::Closed));
        assert_eq!(
            gh.calls()[0],
            strings(&["pr", "view", url, "--json", "state,mergeCommit"])
        );
    }

    #[test]
    fn says_when_gh_is_missing() {
        let forge = Forge {
            program: PathBuf::from("/nonexistent/farik/gh"),
            root: std::env::temp_dir(),
        };
        assert_eq!(
            open(&forge),
            Err(ForgeError::Missing {
                program: "/nonexistent/farik/gh".to_string()
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
