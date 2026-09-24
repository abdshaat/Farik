//! `farik run` and `farik plan` (`docs/SPEC.md` 8.2): a process that drives the project until
//! nothing needs doing, a stop, or Ctrl-C, and then says what waits on the human.

use farik_runtime::orchestrator::{TickReport, TickRules, TickScope};
use farik_store::files::Sandbox;
use serde_json::{Value, json};

use crate::project::Project;
use crate::start::{Driver, runtime, start};
use crate::waiting::{Waiting, waiting};
use crate::{CliIo, say};

/// The exit code after Ctrl-C, as a shell reports a process that SIGINT ended.
pub(crate) const INTERRUPTED: i32 = 130;
/// What the first Ctrl-C says.
const STOPPING: &str = "stopping after the current session; Ctrl-C again aborts it";
/// Why the second Ctrl-C stops the running sessions.
const INTERRUPTED_BY: &str = "interrupted by Ctrl-C";

/// Where a driving process writes: lines a person reads, or one JSON object per line.
pub(crate) struct Printer<'p, 'a> {
    pub(crate) io: &'p mut CliIo<'a>,
    pub(crate) as_json: bool,
    /// Whether `--json` prints a line per thing, as `run` does, or nothing until the end, as
    /// `contract new` does.
    pub(crate) json_lines: bool,
}

impl Printer<'_, '_> {
    /// One line: `text` for a person, `value` for a script, on standard output.
    pub(crate) fn line(&mut self, text: &str, value: &Value) {
        if self.as_json {
            if self.json_lines {
                say(&mut self.io.stdout, &value.to_string());
            }
        } else {
            say(&mut self.io.stdout, text);
        }
    }

    /// A line on standard error, whatever the format.
    pub(crate) fn note(&mut self, text: &str) {
        say(&mut self.io.stderr, text);
    }
}

/// How a loop of ticks ended.
pub(crate) enum Ended {
    /// A tick was idle, for this reason.
    Idle(String),
    /// The run was stopped.
    Stopped,
    /// A tick failed, in these words.
    Failed(String),
}

/// `farik run` (every rule) or `farik plan` (the planning rules): start, tick until done, say
/// what waits on the human, shut down. Answers the exit code.
pub(crate) fn drive(project: &Project, rules: TickRules, io: &mut CliIo<'_>, as_json: bool) -> i32 {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => return refuse(io, as_json, &error),
    };
    runtime.block_on(async {
        let mut driver = match start(project, io).await {
            Ok(driver) => driver,
            Err(error) => return refuse(io, as_json, &error),
        };
        let mut printer = Printer {
            io,
            as_json,
            json_lines: true,
        };
        started(&mut printer, &driver);
        let scope = TickScope {
            task_id: None,
            rules,
        };
        let mut presses = 0;
        let ended = ticks(&mut driver, &scope, &mut printer, &mut presses, |_| {}).await;
        finish(project, driver, &mut printer, ended, presses).await
    })
}

/// Prints what the start chose: the credential, and what recovery did.
pub(crate) fn started(printer: &mut Printer<'_, '_>, driver: &Driver) {
    let recovered = &driver.recovered;
    if printer.as_json {
        printer.line(
            "",
            &json!({
                "credential": driver.credential,
                "sandbox": match driver.sandbox {
                    Sandbox::Docker => "docker",
                    Sandbox::None => "none",
                },
                "recovered": {
                    "sessions_interrupted": recovered.sessions_interrupted,
                    "worktrees_removed": recovered.worktrees_removed,
                    "tasks_resumed": recovered.tasks_resumed,
                },
            }),
        );
        return;
    }
    match driver.credential {
        Some("ANTHROPIC_API_KEY") => {
            printer.line("credential: ANTHROPIC_API_KEY (an API key)", &Value::Null);
        }
        Some(name) => printer.line(
            &format!("credential: {name} (a subscription token)"),
            &Value::Null,
        ),
        None => {}
    }
    if recovered.sessions_interrupted + recovered.worktrees_removed + recovered.tasks_resumed > 0 {
        printer.line(
            &format!(
                "recovered: sessions interrupted {}, worktrees removed {}, tasks resumed {}",
                recovered.sessions_interrupted,
                recovered.worktrees_removed,
                recovered.tasks_resumed
            ),
            &Value::Null,
        );
    }
}

/// Ticks within `scope` until a tick is idle, the run is stopped, or a tick fails, printing each;
/// Ctrl-C is heard between and during ticks. `after` is called after each tick that acted.
pub(crate) async fn ticks(
    driver: &mut Driver,
    scope: &TickScope,
    printer: &mut Printer<'_, '_>,
    presses: &mut u32,
    mut after: impl FnMut(&mut Printer<'_, '_>),
) -> Ended {
    loop {
        if driver.orchestrator.is_stopped() {
            printer.line("stopped", &json!({ "stopped": true }));
            return Ended::Stopped;
        }
        let orchestrator = std::sync::Arc::clone(&driver.orchestrator);
        let tick = orchestrator.tick_within(scope);
        tokio::pin!(tick);
        let report = loop {
            tokio::select! {
                report = &mut tick => break report,
                Some(()) = driver.interrupts.recv() => {
                    *presses += 1;
                    interrupted(driver, printer, *presses);
                }
            }
        };
        match report {
            Ok(TickReport::Idle { why }) => {
                printer.line(&format!("idle: {why}"), &json!({ "idle": why }));
                return Ended::Idle(why);
            }
            Ok(TickReport::Acted { task_id, what }) => {
                printer.line(
                    &format!("{}: {what}", task_id.as_str()),
                    &json!({ "task_id": task_id.as_str(), "what": what }),
                );
                after(printer);
            }
            Ok(TickReport::Sprint { sprint_id, what }) => {
                printer.line(
                    &format!("{sprint_id}: {what}"),
                    &json!({ "sprint_id": sprint_id, "what": what }),
                );
                after(printer);
            }
            Err(error) => return Ended::Failed(error.to_string()),
        }
    }
}

/// What a press of Ctrl-C does: the first stops the run after its session, the second stops the
/// sessions running now, which the session loop aborts; the task keeps its status and the next
/// run resumes it (5.15). Later ones say so.
pub(crate) fn interrupted(driver: &Driver, printer: &mut Printer<'_, '_>, presses: u32) {
    match presses {
        1 => {
            driver.orchestrator.stop();
            printer.note(STOPPING);
        }
        2 => {
            for session in driver.daemon.session_ids() {
                driver.daemon.request_stop(&session, INTERRUPTED_BY);
            }
            printer.note("aborting the current session");
        }
        _ => printer.note("already stopping"),
    }
}

/// Says what waits on the human, shuts the daemon down and gives the lock back, and answers the
/// exit code: 130 after Ctrl-C, 1 after a failed tick, 0 otherwise.
pub(crate) async fn finish(
    project: &Project,
    driver: Driver,
    printer: &mut Printer<'_, '_>,
    ended: Ended,
    presses: u32,
) -> i32 {
    let mut code = match &ended {
        Ended::Failed(error) => {
            report_error(printer, error);
            1
        }
        Ended::Idle(_) | Ended::Stopped => 0,
    };
    match waiting_now(project) {
        Ok(waiting) => print_waiting(printer, &waiting),
        Err(error) => {
            report_error(printer, &error);
            code = 1;
        }
    }
    if let Err(error) = driver.finish().await {
        report_error(printer, &error);
        code = 1;
    }
    if presses > 0 { INTERRUPTED } else { code }
}

/// Shuts the driver down, and answers `code`, or 1 when the shutdown failed.
pub(crate) async fn finish_quietly(
    driver: Driver,
    printer: &mut Printer<'_, '_>,
    code: i32,
) -> i32 {
    match driver.finish().await {
        Ok(()) => code,
        Err(error) => {
            report_error(printer, &error);
            1
        }
    }
}

/// What waits on the human now, read from the project's board.
pub(crate) fn waiting_now(project: &Project) -> Result<Vec<Waiting>, String> {
    let team = project
        .files
        .read_team()
        .map_err(|error| error.to_string())?;
    let projections = project.projections()?;
    waiting(&project.log, &projections, &team)
}

/// Prints what waits on the human: a person's lines, or one `{"waiting_on_you"}` object.
pub(crate) fn print_waiting(printer: &mut Printer<'_, '_>, waiting: &[Waiting]) {
    if printer.as_json {
        printer.line(
            "",
            &json!({ "waiting_on_you": waiting.iter().map(Waiting::json).collect::<Vec<_>>() }),
        );
        return;
    }
    for item in waiting {
        for line in item.lines() {
            printer.line(&line, &Value::Null);
        }
    }
}

/// A failure, as every refusal is written.
pub(crate) fn report_error(printer: &mut Printer<'_, '_>, error: &str) {
    if printer.as_json {
        printer.note(&json!({ "error": error }).to_string());
    } else {
        printer.note(&format!("farik: {error}"));
    }
}

/// A start that refused.
pub(crate) fn refuse(io: &mut CliIo<'_>, as_json: bool, error: &str) -> i32 {
    let mut printer = Printer {
        io,
        as_json,
        json_lines: true,
    };
    report_error(&mut printer, error);
    1
}
