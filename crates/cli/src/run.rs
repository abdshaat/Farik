//! `catervas run` and `catervas plan` (`docs/SPEC.md` 8.2): a process that drives the project until
//! nothing needs doing, a stop, or Ctrl-C, and then says what waits on the human.

use catervas_runtime::SandboxError;
use catervas_runtime::claude::CredentialKind;
use catervas_runtime::credential::Source;
use catervas_runtime::orchestrator::{
    OrchestratorError, Progress, TickReport, TickRules, TickScope,
};
use catervas_store::files::Sandbox;
use serde_json::{Value, json};

use crate::project::Project;
use crate::start::{Driver, StartOptions, runtime, start};
use crate::waiting::{Waiting, waiting};
use crate::{CliIo, say};

/// The exit code after Ctrl-C, as a shell reports a process that SIGINT ended.
pub(crate) const INTERRUPTED: i32 = 130;
/// What the first Ctrl-C says.
const STOPPING: &str = "stopping after the running sessions; Ctrl-C again aborts them";
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
    /// the recheck: `catervas serve`.
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
    /// A tick failed, in these words, because this computer's sandbox is not ready: Docker does
    /// not answer or the sandbox image is missing, which setup's computer check prepares.
    Unready(String),
}

/// `catervas run` (every rule) or `catervas plan` (the planning rules): start, tick until done, say
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
        printer.line(&credential_line(kind, source), &Value::Null);
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

/// The line that says which credential the start chose and where it is kept. The private file is
/// said plainly: it is where the key goes when this computer's keychain cannot be used (ADR 0022).
fn credential_line(kind: CredentialKind, source: Source) -> String {
    let what = match kind {
        CredentialKind::ApiKey => "an API key",
        CredentialKind::SubscriptionToken => "a subscription token",
    };
    let name = credential_name((kind, source));
    match source {
        Source::Environment => format!("credential: {name} ({what})"),
        Source::Keychain => format!("credential: {what}, kept in your computer's keychain"),
        Source::File => format!(
            "credential: {what}, kept in a private file only you can read, credential.json in \
             catervas's settings folder: catervas could not use this computer's keychain"
        ),
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
        let orchestrator = std::sync::Arc::clone(&driver.orchestrator);
        let next = orchestrator.next(scope);
        tokio::pin!(next);
        let progress = loop {
            tokio::select! {
                progress = &mut next => break progress,
                Some(()) = driver.interrupts.recv() => {
                    *presses += 1;
                    interrupted(driver, printer, *presses);
                }
            }
        };
        let report = match progress {
            None => {
                printer.line("stopped", &json!({ "stopped": true }));
                return Ended::Stopped;
            }
            Some(Err(error)) => {
                let unready = unready(&error);
                let error = error.to_string();
                return match drain(driver, printer, presses, scope, error).await {
                    Ended::Failed(error) if unready => Ended::Unready(error),
                    ended => ended,
                };
            }
            Some(Ok(Progress::Started {
                agent_id,
                task_id,
                purpose,
            })) => {
                last_idle = None;
                print_started(printer, &agent_id, task_id.as_ref(), &purpose);
                continue;
            }
            Some(Ok(Progress::Ticked(report))) => report,
        };
        match report {
            TickReport::Idle {
                why,
                until: Some(until),
            } => {
                if last_wait.as_ref() != Some(&(why.clone(), until)) {
                    printer.line(
                        &why,
                        &json!({ "waiting": why, "until": until.to_rfc3339() }),
                    );
                    last_wait = Some((why.clone(), until));
                }
                wait_out(driver, printer, presses, orchestrator.wait_until(until)).await;
            }
            TickReport::Idle { why, until: None } => {
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
            acted => {
                last_idle = None;
                print_acted(printer, &acted);
                after(printer);
            }
        }
    }
}

/// Says that a session started, and for what.
fn print_started(
    printer: &mut Printer<'_, '_>,
    agent_id: &str,
    task_id: Option<&catervas_core::contract::TaskId>,
    purpose: &str,
) {
    let text = match task_id {
        Some(task) => format!(
            "{}: {agent_id} started its {purpose} session",
            task.as_str()
        ),
        None => format!("{agent_id}: started its {purpose} session"),
    };
    printer.line(
        &text,
        &json!({
            "started": purpose,
            "agent_id": agent_id,
            "task_id": task_id.map(|task| task.as_str()),
        }),
    );
}

/// Says what a tick that did something did; an idle one says nothing here.
fn print_acted(printer: &mut Printer<'_, '_>, report: &TickReport) {
    match report {
        TickReport::Acted { task_id, what } => printer.line(
            &format!("{}: {what}", task_id.as_str()),
            &json!({ "task_id": task_id.as_str(), "what": what }),
        ),
        TickReport::Delivery { delivery_id, what } => printer.line(
            &format!("{delivery_id}: {what}"),
            &json!({ "delivery_id": delivery_id, "what": what }),
        ),
        TickReport::Conversation { agent_id, what } | TickReport::Chat { agent_id, what } => {
            printer.line(
                &format!("{agent_id}: {what}"),
                &json!({ "agent_id": agent_id, "what": what }),
            );
        }
        TickReport::Goal { goal_id, what } => printer.line(
            &format!("{goal_id}: {what}"),
            &json!({ "goal": goal_id, "what": what }),
        ),
        TickReport::FolderChange { change, what } => printer.line(
            &format!("folder-{change}: {what}"),
            &json!({ "folder_change": change, "what": what }),
        ),
        TickReport::Idle { .. } => {}
    }
}

/// A tick failed (`error`): stops the run as the first Ctrl-C does and lets the running sessions
/// end, printing what they do, so that no session is left running when the daemon shuts down
/// (ADR 0064). Ctrl-C is heard meanwhile.
async fn drain(
    driver: &mut Driver,
    printer: &mut Printer<'_, '_>,
    presses: &mut u32,
    scope: &TickScope,
    error: String,
) -> Ended {
    driver.orchestrator.stop();
    let orchestrator = std::sync::Arc::clone(&driver.orchestrator);
    loop {
        let next = orchestrator.next(scope);
        tokio::pin!(next);
        let progress = loop {
            tokio::select! {
                progress = &mut next => break progress,
                Some(()) = driver.interrupts.recv() => {
                    *presses += 1;
                    interrupted(driver, printer, *presses);
                }
            }
        };
        match progress {
            None => return Ended::Failed(error),
            Some(Err(again)) => report_error(printer, &again.to_string()),
            Some(Ok(Progress::Started {
                agent_id,
                task_id,
                purpose,
            })) => print_started(printer, &agent_id, task_id.as_ref(), &purpose),
            Some(Ok(Progress::Ticked(report))) => print_acted(printer, &report),
        }
    }
}

/// Whether `error` is the sandbox not being ready on this computer (`Ended::Unready`).
fn unready(error: &OrchestratorError) -> bool {
    matches!(
        error,
        OrchestratorError::Sandbox(
            SandboxError::DockerUnavailable | SandboxError::ImageMissing { .. }
        )
    )
}

/// Waits for `wait`, hearing Ctrl-C meanwhile.
async fn wait_out(
    driver: &mut Driver,
    printer: &mut Printer<'_, '_>,
    presses: &mut u32,
    wait: impl Future<Output = catervas_runtime::orchestrator::Waited>,
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
            printer.note("aborting the running sessions");
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
        Ended::Failed(error) | Ended::Unready(error) => {
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
        printer.note(&format!("catervas: {error}"));
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

#[cfg(test)]
mod tests {
    use catervas_runtime::claude::CredentialKind;
    use catervas_runtime::credential::Source;

    use super::credential_line;

    #[test]
    fn says_where_the_credential_is_kept() {
        let file = credential_line(CredentialKind::SubscriptionToken, Source::File);
        assert!(file.contains("a private file only you can read"), "{file}");
        assert!(
            file.contains("could not use this computer's keychain"),
            "{file}"
        );
        assert!(!file.contains("catervas's credential.json"), "{file}");
        let keychain = credential_line(CredentialKind::ApiKey, Source::Keychain);
        assert!(
            keychain.contains("kept in your computer's keychain"),
            "{keychain}"
        );
    }
}
