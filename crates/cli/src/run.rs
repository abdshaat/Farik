//! `farik run` and `farik plan` (`docs/SPEC.md` 8.2): a process that drives the project until
//! nothing needs doing, a stop, or Ctrl-C, and then says what waits on the human.

use farik_runtime::claude::CredentialKind;
use farik_runtime::credential::Source;
use farik_runtime::orchestrator::{TickReport, TickRules, TickScope};
use farik_store::files::Sandbox;
use serde_json::{Value, json};

use crate::project::Project;
use crate::start::{Driver, StartOptions, runtime, start};
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

/// What `ticks` does when a tick is idle with no agent to wait for.
#[derive(Clone, Copy)]
pub(crate) enum OnIdle {
    /// Return: the run is done.
    Return,
    /// Say why, once until a tick acts or the reason changes, and wait for a command, a stop, or
    /// the recheck: `farik serve`.
    Wait,
}

/// How long an idle `serve` asks to wait; `Orchestrator::wait_until` caps it at its recheck.
const IDLE_WAIT: chrono::Duration = chrono::Duration::hours(24);

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
        let mut driver = match start(project, io, StartOptions::default()).await {
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
        let ended = ticks(
            &mut driver,
            &scope,
            &mut printer,
            &mut presses,
            OnIdle::Return,
            |_| {},
        )
        .await;
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
                "credential": driver.credential.map(credential_name),
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
    if let Some((kind, source)) = driver.credential {
        let what = match kind {
            CredentialKind::ApiKey => "an API key",
            CredentialKind::SubscriptionToken => "a subscription token",
        };
        let name = credential_name((kind, source));
        let line = match source {
            Source::Environment => format!("credential: {name} ({what})"),
            Source::Keychain => format!("credential: {what}, kept in your computer's keychain"),
            Source::File => format!("credential: {what}, kept in farik's credential.json"),
        };
        printer.line(&line, &Value::Null);
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

/// Where the credential came from, as `--json` names it: the variable, or the store.
fn credential_name((kind, source): (CredentialKind, Source)) -> &'static str {
    match (source, kind) {
        (Source::Environment, CredentialKind::ApiKey) => "ANTHROPIC_API_KEY",
        (Source::Environment, CredentialKind::SubscriptionToken) => "CLAUDE_CODE_OAUTH_TOKEN",
        (Source::Keychain, _) => "keychain",
        (Source::File, _) => "file",
    }
}

/// Ticks within `scope` until a tick is idle with no agent to wait for, the run is stopped, or a
/// tick fails, printing each; a tick idle while an agent sleeps is waited out, and says so. An
/// idle tick with no agent to wait for ends the loop under `OnIdle::Return`; under `Wait` it is
/// printed once until its reason changes or a tick acts, and waited out. Ctrl-C is heard between and during ticks.
/// `after` is called after each tick that acted.
pub(crate) async fn ticks(
    driver: &mut Driver,
    scope: &TickScope,
    printer: &mut Printer<'_, '_>,
    presses: &mut u32,
    on_idle: OnIdle,
    mut after: impl FnMut(&mut Printer<'_, '_>),
) -> Ended {
    // The last wait printed, so a wait capped and rechecked (`Orchestrator::wait_until`) prints
    // its line once, not once per recheck; printed again only when the agent or the time changes.
    let mut last_wait: Option<(String, chrono::DateTime<chrono::Utc>)> = None;
    let mut last_idle: Option<String> = None;
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
            Ok(TickReport::Idle {
                why,
                until: Some(until),
            }) => {
                if last_wait.as_ref() != Some(&(why.clone(), until)) {
                    printer.line(
                        &why,
                        &json!({ "waiting": why, "until": until.to_rfc3339() }),
                    );
                    last_wait = Some((why.clone(), until));
                }
                wait_out(driver, printer, presses, orchestrator.wait_until(until)).await;
            }
            Ok(TickReport::Idle { why, until: None }) => {
                if last_idle.as_ref() != Some(&why) || matches!(on_idle, OnIdle::Return) {
                    printer.line(&format!("idle: {why}"), &json!({ "idle": why }));
                    last_idle = Some(why.clone());
                }
                if matches!(on_idle, OnIdle::Return) {
                    return Ended::Idle(why);
                }
                let until = printer.io.clock.now() + IDLE_WAIT;
                wait_out(driver, printer, presses, orchestrator.wait_until(until)).await;
            }
            Ok(TickReport::Acted { task_id, what }) => {
                last_idle = None;
                printer.line(
                    &format!("{}: {what}", task_id.as_str()),
                    &json!({ "task_id": task_id.as_str(), "what": what }),
                );
                after(printer);
            }
            Ok(TickReport::Sprint { sprint_id, what }) => {
                last_idle = None;
                printer.line(
                    &format!("{sprint_id}: {what}"),
                    &json!({ "sprint_id": sprint_id, "what": what }),
                );
                after(printer);
            }
            Ok(
                TickReport::Conversation { agent_id, what } | TickReport::Chat { agent_id, what },
            ) => {
                last_idle = None;
                printer.line(
                    &format!("{agent_id}: {what}"),
                    &json!({ "agent_id": agent_id, "what": what }),
                );
                after(printer);
            }
            Err(error) => return Ended::Failed(error.to_string()),
        }
    }
}

/// Waits for `wait`, hearing Ctrl-C meanwhile.
async fn wait_out(
    driver: &mut Driver,
    printer: &mut Printer<'_, '_>,
    presses: &mut u32,
    wait: impl Future<Output = farik_runtime::orchestrator::Waited>,
) {
    tokio::pin!(wait);
    loop {
        tokio::select! {
            _ = &mut wait => break,
            Some(()) = driver.interrupts.recv() => {
                *presses += 1;
                interrupted(driver, printer, *presses);
            }
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
    waiting(&project.log, &projections, &project.files, &team)
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
