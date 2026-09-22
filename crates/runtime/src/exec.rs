//! A command run on an agent's behalf, wherever its sandbox puts it (`docs/SPEC.md` 8.3).

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

/// How much of each of standard output and standard error a command keeps; the rest is read and
/// discarded, so that the command is never blocked on a full pipe.
pub const OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;

/// What came of a command that ran. A non-zero `exit_code` is a value, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// The exit code, or 128 plus the signal for a command killed by one.
    pub exit_code: i32,
    /// Standard output, its first `OUTPUT_LIMIT_BYTES`, read as UTF-8 with replacement.
    pub stdout: String,
    /// Standard error, likewise.
    pub stderr: String,
    /// Whether the deadline passed before the command finished.
    pub timed_out: bool,
    /// Whether either stream was cut at `OUTPUT_LIMIT_BYTES`.
    pub truncated: bool,
}

/// Why a command did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    /// The process could not be started.
    SpawnFailed {
        /// What the operating system said.
        detail: String,
    },
    /// The task's container is no longer there, or no longer running.
    ContainerGone,
    /// The directory asked for is not inside the workspace.
    OutsideWorkspace {
        /// The directory as it was given.
        cwd: String,
    },
}

impl fmt::Display for ExecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed { detail } => {
                write!(formatter, "the command could not be started: {detail}")
            }
            Self::ContainerGone => write!(formatter, "the task's container is no longer running"),
            Self::OutsideWorkspace { cwd } => {
                write!(formatter, "the directory {cwd} is outside the workspace")
            }
        }
    }
}

impl std::error::Error for ExecError {}

/// Something that runs a command for a task.
pub trait Executor: Send + Sync {
    /// Runs `command` through `sh -c` in `cwd`, a directory relative to the workspace root (`""`
    /// and `"."` are the root), with `env` as its whole added environment, stopping it at
    /// `timeout`. Blocks for up to the timeout; async callers go through `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// `OutsideWorkspace` for a `cwd` that is absolute or climbs out, `SpawnFailed` when the
    /// process cannot be started, and `ContainerGone` when a container sandbox has lost its
    /// container.
    fn run(
        &self,
        command: &str,
        cwd: &str,
        timeout: Duration,
        env: &BTreeMap<String, String>,
    ) -> Result<ExecResult, ExecError>;
}

/// The workspace-relative directory `cwd` names: `None` for the root, or its normalised path.
pub(crate) fn workspace_relative(cwd: &str) -> Result<Option<String>, ExecError> {
    if cwd
        .split('/')
        .all(|segment| segment.is_empty() || segment == ".")
    {
        return Ok(None);
    }
    farik_core::governor::paths::normalise(cwd)
        .map(Some)
        .ok_or_else(|| ExecError::OutsideWorkspace {
            cwd: cwd.to_owned(),
        })
}

#[cfg(unix)]
pub(crate) use supervise::{Finished, supervise};

#[cfg(unix)]
mod supervise {
    use std::io::Read;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{ExecError, OUTPUT_LIMIT_BYTES};

    const POLL: Duration = Duration::from_millis(20);

    /// How long the pipes are read after the process has exited. A process that escaped its group
    /// (`setsid`) can hold a pipe open for as long as it likes; what it wrote by then is kept, and
    /// the rest is not waited for, so that `run` returns by its deadline.
    const READ_GRACE: Duration = Duration::from_millis(500);

    /// A process that has exited, with what it wrote.
    pub(crate) struct Finished {
        pub exit_code: i32,
        pub stdout: String,
        pub stderr: String,
        pub truncated: bool,
        /// Whether `kill_after` passed and `kill` was called.
        pub killed: bool,
        /// How long it ran, on the host clock.
        pub elapsed: Duration,
    }

    /// Runs `command` with both pipes drained on their own threads, calls `kill` once if it is
    /// still running after `kill_after`, and `after_exit` once it has exited, before the readers
    /// are waited for (so a background child still holding a pipe can be ended there). The readers
    /// get `READ_GRACE` after the exit, and no longer.
    pub(crate) fn supervise(
        command: &mut Command,
        kill_after: Duration,
        kill: impl FnOnce(&mut Child),
        after_exit: impl FnOnce(&Child),
    ) -> Result<Finished, ExecError> {
        let started = Instant::now();
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ExecError::SpawnFailed {
                detail: error.to_string(),
            })?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let mut kill = Some(kill);
        let mut killed = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(error) => break Err(error),
            }
            if started.elapsed() >= kill_after
                && let Some(kill) = kill.take()
            {
                kill(&mut child);
                killed = true;
            }
            thread::sleep(POLL);
        };
        after_exit(&child);
        let elapsed = started.elapsed();
        let until = Instant::now() + READ_GRACE;
        let (stdout, stdout_cut) = collect(stdout, until);
        let (stderr, stderr_cut) = collect(stderr, until);
        let status = status.map_err(|error| ExecError::SpawnFailed {
            detail: error.to_string(),
        })?;
        let exit_code = status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .unwrap_or(-1);
        Ok(Finished {
            exit_code,
            stdout,
            stderr,
            truncated: stdout_cut || stderr_cut,
            killed,
            elapsed,
        })
    }

    /// What a pipe's reader has kept so far, and a channel that disconnects when it has finished.
    type Reader = Option<(Arc<Mutex<Kept>>, Receiver<()>)>;

    /// The head of a stream, and whether more followed.
    #[derive(Default)]
    struct Kept {
        bytes: Vec<u8>,
        cut: bool,
    }

    fn drain<Stream: Read + Send + 'static>(stream: Option<Stream>) -> Reader {
        stream.map(|mut stream| {
            let kept = Arc::new(Mutex::new(Kept::default()));
            let (done, finished) = mpsc::channel::<()>();
            let shared = Arc::clone(&kept);
            thread::spawn(move || {
                keep_the_head(&mut stream, &shared);
                drop(done);
            });
            (kept, finished)
        })
    }

    /// Keeps the first `OUTPUT_LIMIT_BYTES` of `stream` in `kept`, and whether more followed;
    /// reads to the end.
    fn keep_the_head(stream: &mut dyn Read, kept: &Mutex<Kept>) {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match stream.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
                    let room = OUTPUT_LIMIT_BYTES - kept.bytes.len();
                    kept.bytes.extend_from_slice(&buffer[..read.min(room)]);
                    kept.cut |= read > room;
                }
            }
        }
    }

    /// What `reader` kept, once it has finished or `until` has passed, whichever is first.
    fn collect(reader: Reader, until: Instant) -> (String, bool) {
        let Some((kept, finished)) = reader else {
            return (String::new(), false);
        };
        let _ = finished.recv_timeout(until.saturating_duration_since(Instant::now()));
        let kept = std::mem::take(&mut *kept.lock().unwrap_or_else(PoisonError::into_inner));
        (String::from_utf8_lossy(&kept.bytes).into_owned(), kept.cut)
    }
}
