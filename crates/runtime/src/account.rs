//! Claude Code's account, asked of Claude Code (ADR 0084): whether it is signed in, the
//! connection check, signing in through its own login, and signing out. Catervas keeps and reads
//! no key; each of these starts `claude` and reads only its answer, which `catervas_core::
//! claude_account` parses from strings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use catervas_core::claude_account::{
    AccountKind, AccountStatus, CHECK_ARGS, CheckOutcome, check_outcome, parse_status,
};
use tokio::io::AsyncReadExt as _;

use crate::claude::credential_variable;
use crate::exec::kill_group;

/// Why a sign-in step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInError {
    /// No sign-in is waiting for a code.
    NoSignIn,
    /// Claude Code's login ended in this way.
    Failed(String),
    /// Claude Code gave no sign-in address; the last line it printed.
    NoAddress(String),
}

const STATUS_LIMIT: Duration = Duration::from_secs(15);
const LOGOUT_LIMIT: Duration = Duration::from_secs(15);
const CHECK_LIMIT: Duration = Duration::from_secs(60);
/// The most of a program's output that is read.
const OUTPUT_LIMIT: u64 = 1 << 20;
/// The longest of standard error that a failed check keeps.
const DETAIL_LIMIT: usize = 200;

/// What a finished run printed.
struct Ran {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Runs the `claude` program for the account: status, check, sign-in and sign-out.
pub struct ClaudeAccount {
    program: PathBuf,
    env: BTreeMap<String, String>,
    check_dir: PathBuf,
}

impl ClaudeAccount {
    /// A runner for the program at `program`, run with exactly `env`, in `check_dir`.
    #[must_use]
    pub fn new(
        program: PathBuf,
        env: BTreeMap<String, String>,
        check_dir: PathBuf,
    ) -> ClaudeAccount {
        ClaudeAccount {
            program,
            env,
            check_dir,
        }
    }

    fn command(&self, dir: &Path) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .env_clear()
            .envs(&self.env)
            .current_dir(dir)
            .process_group(0)
            .kill_on_drop(true);
        command
    }

    /// Runs the program with `args` to its end, or kills its group at `limit`. `None` when it
    /// cannot be run or does not end in time.
    async fn run(&self, args: &[&str], limit: Duration) -> Option<Ran> {
        std::fs::create_dir_all(&self.check_dir).ok()?;
        let mut child = self
            .command(&self.check_dir)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        let pid = child.id()?;
        let mut stdout = child.stdout.take()?.take(OUTPUT_LIMIT);
        let mut stderr = child.stderr.take()?.take(OUTPUT_LIMIT);
        let work = async {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let (status, _, _) = tokio::join!(
                child.wait(),
                stdout.read_to_end(&mut out),
                stderr.read_to_end(&mut err)
            );
            (status, out, err)
        };
        if let Ok((status, out, err)) = tokio::time::timeout(limit, work).await {
            Some(Ran {
                success: status.ok()?.success(),
                stdout: String::from_utf8_lossy(&out).into_owned(),
                stderr: String::from_utf8_lossy(&err).into_owned(),
            })
        } else {
            kill_group(pid);
            let _ = child.wait().await;
            None
        }
    }

    /// Asks `claude auth status --json`. A run that fails is not signed in. A key in the user's
    /// environment is named as the kind; its value is never looked at.
    pub async fn status(&self) -> AccountStatus {
        let answered = self
            .run(&["auth", "status", "--json"], STATUS_LIMIT)
            .await
            .and_then(|ran| parse_status(&ran.stdout));
        let mut status = answered.unwrap_or(AccountStatus {
            signed_in: false,
            kind: None,
        });
        if status.signed_in && credential_variable(&self.env).is_some() {
            status.kind = Some(AccountKind::Environment);
        }
        status
    }

    /// The connection check: one tiny request, unless Claude Code is not signed in.
    pub async fn check(&self) -> CheckOutcome {
        if !self.status().await.signed_in {
            return CheckOutcome::NotSignedIn;
        }
        let Some(ran) = self.run(&CHECK_ARGS, CHECK_LIMIT).await else {
            return CheckOutcome::Failed("Claude Code did not answer in time".to_string());
        };
        check_outcome(&ran.stdout).unwrap_or_else(|| {
            CheckOutcome::Failed(first_characters(&ran.stderr, "Claude Code gave no answer"))
        })
    }

    /// Runs `claude auth logout`.
    ///
    /// # Errors
    ///
    /// Claude Code's words when it did not sign out.
    pub async fn sign_out(&self) -> Result<(), String> {
        match self.run(&["auth", "logout"], LOGOUT_LIMIT).await {
            Some(ran) if ran.success => Ok(()),
            Some(ran) => Err(first_characters(
                &ran.stderr,
                "Claude Code did not sign out",
            )),
            None => Err("Claude Code did not answer in time".to_string()),
        }
    }
}

/// The first `DETAIL_LIMIT` characters of `text`, trimmed; `fallback` when it is blank.
fn first_characters(text: &str, fallback: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        fallback.to_string()
    } else {
        text.chars().take(DETAIL_LIMIT).collect()
    }
}
