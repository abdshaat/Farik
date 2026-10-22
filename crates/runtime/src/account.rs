//! Claude Code's account, asked of Claude Code (ADR 0084): whether it is signed in, the
//! connection check, signing in through its own login, and signing out. Catervas keeps and reads
//! no key; each of these starts `claude` and reads only its answer, which `catervas_core::
//! claude_account` parses from strings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use catervas_core::claude_account::{
    AccountKind, AccountStatus, CHECK_ARGS, CheckOutcome, check_outcome, parse_status,
    sign_in_address,
};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

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
/// How long a login may wait for its first address.
const ADDRESS_LIMIT: Duration = Duration::from_secs(20);
/// How long a login without a code is kept.
const LOGIN_LIFE: Duration = Duration::from_secs(600);
/// How long a login is given to take its code and exit.
const CODE_LIMIT: Duration = Duration::from_secs(60);
/// How long the rest of a finished login's output is waited for.
const DRAIN_LIMIT: Duration = Duration::from_secs(1);
const CHECK_LIMIT: Duration = Duration::from_secs(60);
/// The most of a program's output that is read.
const OUTPUT_LIMIT: u64 = 1 << 20;
/// What `parse_answer` says of output that is no answer.
const NO_ANSWER: &str = "Claude Code gave no answer";
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
    login: Mutex<Option<Login>>,
}

/// A sign-in that waits for its code. The code is never stored in it.
struct Login {
    child: Child,
    pid: u32,
    stdin: Option<ChildStdin>,
    /// The login's lines, standard output and error together.
    lines: UnboundedReceiver<String>,
    last_line: String,
    started: Instant,
}

impl Login {
    /// Ends the login's whole group and reaps it.
    async fn end(mut self) {
        kill_group(self.pid);
        let _ = tokio::time::timeout(DRAIN_LIMIT, self.child.wait()).await;
    }

    fn note(&mut self, line: &str) {
        if !line.trim().is_empty() {
            self.last_line = line.trim().to_string();
        }
    }
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
            login: Mutex::new(None),
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

    /// Starts `claude auth login` (with `--console` for an Anthropic Console account, else
    /// `--claudeai`) and answers the sign-in address it prints. Any earlier sign-in is ended.
    ///
    /// # Errors
    ///
    /// `NoAddress` when Claude Code gave no sign-in address in time, with the last line it
    /// printed.
    pub async fn start_sign_in(&self, console: bool) -> Result<String, SignInError> {
        let mut slot = self.login.lock().await;
        if let Some(earlier) = slot.take() {
            earlier.end().await;
        }
        let no_address = |text: String| SignInError::NoAddress(text);
        std::fs::create_dir_all(&self.check_dir).map_err(|error| no_address(error.to_string()))?;
        let mode = if console { "--console" } else { "--claudeai" };
        let mut child = self
            .command(&self.check_dir)
            .args(["auth", "login", mode])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| no_address(error.to_string()))?;
        let (Some(pid), Some(stdin), Some(stdout), Some(stderr)) = (
            child.id(),
            child.stdin.take(),
            child.stdout.take(),
            child.stderr.take(),
        ) else {
            return Err(no_address("Claude Code could not be started".to_string()));
        };
        let (sender, lines) = unbounded_channel();
        let (out_sender, err_sender) = (sender.clone(), sender);
        tokio::spawn(forward_lines(stdout, out_sender));
        tokio::spawn(forward_lines(stderr, err_sender));
        let mut login = Login {
            child,
            pid,
            stdin: Some(stdin),
            lines,
            last_line: String::new(),
            started: Instant::now(),
        };
        let found = tokio::time::timeout(ADDRESS_LIMIT, async {
            while let Some(line) = login.lines.recv().await {
                if let Some(address) = sign_in_address(&line) {
                    return Some(address);
                }
                login.note(&line);
            }
            None
        })
        .await;
        if let Ok(Some(address)) = found {
            *slot = Some(login);
            Ok(address)
        } else {
            let last = login.last_line.clone();
            login.end().await;
            Err(no_address(last))
        }
    }

    /// Hands the code to the waiting login and, when Claude Code accepts it, checks the
    /// connection. The code is written once and kept nowhere.
    ///
    /// # Errors
    ///
    /// `NoSignIn` when none waits (or it waited past ten minutes); `Failed` with the login's
    /// last line when it did not accept the code in time.
    pub async fn finish_sign_in(&self, code: &str) -> Result<CheckOutcome, SignInError> {
        let mut login = self
            .login
            .lock()
            .await
            .take()
            .ok_or(SignInError::NoSignIn)?;
        if login.started.elapsed() > LOGIN_LIFE {
            login.end().await;
            return Err(SignInError::NoSignIn);
        }
        let Some(mut stdin) = login.stdin.take() else {
            login.end().await;
            return Err(SignInError::NoSignIn);
        };
        // A login that has already exited refuses the write; its exit status says the rest.
        let _ = async {
            stdin.write_all(code.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await
        }
        .await;
        drop(stdin);
        let exited = tokio::time::timeout(CODE_LIMIT, login.child.wait()).await;
        // Whatever the login printed on its way out.
        let _ = tokio::time::timeout(DRAIN_LIMIT, async {
            while let Some(line) = login.lines.recv().await {
                login.note(&line);
            }
        })
        .await;
        match exited {
            Ok(Ok(status)) if status.success() => Ok(self.check().await),
            Ok(_) => Err(SignInError::Failed(login.last_line)),
            Err(_) => {
                let last = login.last_line.clone();
                login.end().await;
                Err(SignInError::Failed(last))
            }
        }
    }

    /// Puts one question to Claude Code: `prompt` on its standard input, `system_prompt` as its
    /// system prompt, with Haiku and no tools (`catervas_core::claude_account::ask_args`). Answers
    /// what it said and what it cost.
    ///
    /// # Errors
    ///
    /// Claude Code's words when it answered with an error, or a sentence saying it could not be
    /// run or did not answer in time.
    pub async fn ask(
        &self,
        prompt: &str,
        system_prompt: &str,
        limit: Duration,
    ) -> Result<catervas_core::claude_account::Answer, String> {
        std::fs::create_dir_all(&self.check_dir).map_err(|error| error.to_string())?;
        let mut child = self
            .command(&self.check_dir)
            .args(catervas_core::claude_account::ask_args(system_prompt))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Claude Code could not be started: {error}"))?;
        let (Some(pid), Some(mut stdin), Some(stdout), Some(stderr)) = (
            child.id(),
            child.stdin.take(),
            child.stdout.take(),
            child.stderr.take(),
        ) else {
            return Err("Claude Code could not be started".to_string());
        };
        let (mut stdout, mut stderr) = (stdout.take(OUTPUT_LIMIT), stderr.take(OUTPUT_LIMIT));
        let work = async {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let written = async {
                // A program that exits before it has read everything refuses the write; its
                // output says why.
                let _ = stdin.write_all(prompt.as_bytes()).await;
                drop(stdin);
            };
            let ((), _, _, _) = tokio::join!(
                written,
                child.wait(),
                stdout.read_to_end(&mut out),
                stderr.read_to_end(&mut err)
            );
            (out, err)
        };
        let Ok((out, err)) = tokio::time::timeout(limit, work).await else {
            kill_group(pid);
            let _ = child.wait().await;
            return Err("Claude Code did not answer in time".to_string());
        };
        catervas_core::claude_account::parse_answer(&String::from_utf8_lossy(&out)).map_err(|why| {
            if why == NO_ANSWER {
                first_characters(&String::from_utf8_lossy(&err), &why)
            } else {
                why
            }
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

/// Sends each line of `reader` to `sender` until the pipe closes.
async fn forward_lines<R>(reader: R, sender: tokio::sync::mpsc::UnboundedSender<String>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if sender.send(line).is_err() {
            break;
        }
    }
}

type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What the daemon asks of Claude Code's account. The daemon holds it as a port so that its tests
/// use a fake instead of a program.
pub trait AccountPort: Send + Sync {
    /// Whether Claude Code is signed in, and with what kind of account.
    fn status(&self) -> Boxed<'_, AccountStatus>;
    /// The connection check.
    fn check(&self) -> Boxed<'_, CheckOutcome>;
    /// Starts a sign-in; the address of its page.
    fn start_sign_in(&self, console: bool) -> Boxed<'_, Result<String, SignInError>>;
    /// Gives the sign-in its code, then checks the connection.
    fn finish_sign_in<'a>(&'a self, code: &'a str) -> Boxed<'a, Result<CheckOutcome, SignInError>>;
    /// Signs Claude Code out.
    fn sign_out(&self) -> Boxed<'_, Result<(), String>>;
}

impl AccountPort for ClaudeAccount {
    fn status(&self) -> Boxed<'_, AccountStatus> {
        Box::pin(ClaudeAccount::status(self))
    }

    fn check(&self) -> Boxed<'_, CheckOutcome> {
        Box::pin(ClaudeAccount::check(self))
    }

    fn start_sign_in(&self, console: bool) -> Boxed<'_, Result<String, SignInError>> {
        Box::pin(ClaudeAccount::start_sign_in(self, console))
    }

    fn finish_sign_in<'a>(&'a self, code: &'a str) -> Boxed<'a, Result<CheckOutcome, SignInError>> {
        Box::pin(ClaudeAccount::finish_sign_in(self, code))
    }

    fn sign_out(&self) -> Boxed<'_, Result<(), String>> {
        Box::pin(ClaudeAccount::sign_out(self))
    }
}

/// A fake account for the daemon's tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::Mutex;

    use super::{AccountPort, AccountStatus, Boxed, CheckOutcome, SignInError};
    use crate::locked;

    /// An account whose answers a test sets, and which records each call by name, the code
    /// included.
    pub(crate) struct FakeAccount {
        pub(crate) status: Mutex<AccountStatus>,
        pub(crate) check: Mutex<CheckOutcome>,
        pub(crate) address: Mutex<Result<String, SignInError>>,
        pub(crate) finish: Mutex<Result<CheckOutcome, SignInError>>,
        pub(crate) sign_out: Mutex<Result<(), String>>,
        /// The calls so far: `status`, `check`, `start_sign_in(true)`, `finish_sign_in(<code>)`,
        /// `sign_out`.
        pub(crate) calls: Mutex<Vec<String>>,
    }

    impl FakeAccount {
        /// A signed-out account whose every answer is the plainest one.
        pub(crate) fn new() -> FakeAccount {
            FakeAccount {
                status: Mutex::new(AccountStatus {
                    signed_in: false,
                    kind: None,
                }),
                check: Mutex::new(CheckOutcome::Working),
                address: Mutex::new(Ok("https://claude.com/cai/oauth/authorize?x=1".to_string())),
                finish: Mutex::new(Ok(CheckOutcome::Working)),
                sign_out: Mutex::new(Ok(())),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn record(&self, call: String) {
            locked(&self.calls).push(call);
        }
    }

    impl AccountPort for FakeAccount {
        fn status(&self) -> Boxed<'_, AccountStatus> {
            self.record("status".to_string());
            Box::pin(async { locked(&self.status).clone() })
        }

        fn check(&self) -> Boxed<'_, CheckOutcome> {
            self.record("check".to_string());
            Box::pin(async { locked(&self.check).clone() })
        }

        fn start_sign_in(&self, console: bool) -> Boxed<'_, Result<String, SignInError>> {
            self.record(format!("start_sign_in({console})"));
            Box::pin(async { locked(&self.address).clone() })
        }

        fn finish_sign_in<'a>(
            &'a self,
            code: &'a str,
        ) -> Boxed<'a, Result<CheckOutcome, SignInError>> {
            self.record(format!("finish_sign_in({code})"));
            Box::pin(async { locked(&self.finish).clone() })
        }

        fn sign_out(&self) -> Boxed<'_, Result<(), String>> {
            self.record("sign_out".to_string());
            Box::pin(async { locked(&self.sign_out).clone() })
        }
    }
}
