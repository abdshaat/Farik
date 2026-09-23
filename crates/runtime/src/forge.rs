//! The forge a team's pull requests live on, driven through the `gh` program (`docs/SPEC.md`
//! 5.14): Farik opens a pull request for an accepted task and reads whether the human merged it.
//! `gh` runs in the repository root with the user's own environment and sign-in, as the store runs
//! git; no credential of it enters a container (ADR 0012).

use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

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
            Self::Missing { program } => write!(
                formatter,
                "{program} could not be run: install gh and sign in with gh auth login"
            ),
            Self::Failed { detail } => write!(formatter, "gh failed: {detail}"),
        }
    }
}

impl std::error::Error for ForgeError {}

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
        let open = serde_json::from_str::<Value>(&listed)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .ok_or_else(|| unreadable("gh pr list", &listed))?;
        if let Some(first) = open.first() {
            let url = first.get("url").and_then(Value::as_str);
            let number = first.get("number").and_then(Value::as_u64);
            return match (url, number) {
                (Some(url), Some(number)) => Ok(PullRequest {
                    url: url.to_string(),
                    number,
                }),
                _ => Err(unreadable("gh pr list", &listed)),
            };
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
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .unwrap_or_default();
        let number = url
            .rsplit('/')
            .next()
            .and_then(|segment| segment.parse::<u64>().ok())
            .ok_or_else(|| unreadable("gh pr create", &created))?;
        Ok(PullRequest {
            url: url.to_string(),
            number,
        })
    }

    /// Whether the pull request at `url` is open, merged (and as which commit), or closed.
    ///
    /// # Errors
    ///
    /// As `open_pull_request`.
    pub fn pull_request_state(&self, url: &str) -> Result<PullRequestState, ForgeError> {
        let viewed = self.run(&["pr", "view", url, "--json", "state,mergeCommit"], None)?;
        let value: Value =
            serde_json::from_str(&viewed).map_err(|_| unreadable("gh pr view", &viewed))?;
        match value.get("state").and_then(Value::as_str) {
            Some("OPEN") => Ok(PullRequestState::Open),
            Some("CLOSED") => Ok(PullRequestState::Closed),
            Some("MERGED") => value
                .get("mergeCommit")
                .and_then(|commit| commit.get("oid"))
                .and_then(Value::as_str)
                .map(|sha| PullRequestState::Merged {
                    sha: sha.to_string(),
                })
                .ok_or_else(|| unreadable("gh pr view", &viewed)),
            _ => Err(unreadable("gh pr view", &viewed)),
        }
    }

    /// Runs the program with `arguments` in the root, `stdin` on its standard input when given,
    /// and answers its standard output. ponytail: no timeout on gh, as there is none on git; a
    /// `try_wait` loop as step 02's when one hangs.
    fn run(&self, arguments: &[&str], stdin: Option<&str>) -> Result<String, ForgeError> {
        let missing = || ForgeError::Missing {
            program: self.program.display().to_string(),
        };
        let mut command = Command::new(&self.program);
        command
            .args(arguments)
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let mut child = command.spawn().map_err(|_| missing())?;
        if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(text.as_bytes())
                .map_err(|error| ForgeError::Failed {
                    detail: format!("its standard input could not be written: {error}"),
                })?;
        }
        let output: Output = child
            .wait_with_output()
            .map_err(|error| ForgeError::Failed {
                detail: format!("it could not be waited for: {error}"),
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(ForgeError::Failed {
                detail: format!(
                    "gh {} exited with {}: {stderr}",
                    arguments[..2].join(" "),
                    output.status
                ),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// `what` answered `output`, which is not what it answers.
fn unreadable(what: &str, output: &str) -> ForgeError {
    ForgeError::Failed {
        detail: format!("{what} answered what cannot be read: {:?}", output.trim()),
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
