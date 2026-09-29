//! `farik serve` (`docs/SPEC.md` 8.1): `farik run` that keeps driving when the board is idle, until
//! a stop or Ctrl-C, and remembers the project it serves.

use farik_runtime::daemon::PortChoice;
use farik_runtime::orchestrator::{TickRules, TickScope};
use serde_json::json;

use crate::project::Project;
use crate::run::{OnIdle, Printer, finish, refuse, started, ticks};
use crate::start::{StartOptions, runtime, start};
use crate::state::{remember, state_dir};
use crate::{CliIo, say};

/// The port `serve` asks for when it is not told one.
const DEFAULT_PORT: u16 = 7420;

/// Drives `project` with every rule until a stop or Ctrl-C, whatever the board. Answers the exit
/// code: 130 after Ctrl-C, 1 after a refusal or a failed tick, 0 after `farik stop`.
pub(crate) fn serve(
    project: &Project,
    port: Option<u16>,
    no_open: bool,
    io: &mut CliIo<'_>,
) -> i32 {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => return refuse(io, false, &error),
    };
    runtime.block_on(async {
        let options = StartOptions {
            port: PortChoice::Preferred(port.unwrap_or(DEFAULT_PORT)),
            web: true,
        };
        let mut driver = match start(project, io, options).await {
            Ok(driver) => driver,
            Err(error) => return refuse(io, false, &error),
        };
        match state_dir(&io.env) {
            Some(directory) => {
                if let Err(error) = remember(&directory, &project.root) {
                    say(&mut io.stderr, &format!("warning: {error}"));
                }
            }
            None => say(
                &mut io.stderr,
                "warning: XDG_CONFIG_HOME, HOME, and APPDATA are all unset, so the project is not \
                 remembered",
            ),
        }
        let mut printer = Printer {
            io,
            as_json: false,
            json_lines: true,
        };
        started(&mut printer, &driver);
        printer.line(
            &format!(
                "serving {} on 127.0.0.1:{}",
                project.root.display(),
                driver.port()
            ),
            &json!({}),
        );
        if let Some(code) = &driver.connect_code {
            let link = format!("http://127.0.0.1:{}/connect#{code}", driver.port());
            printer.line(&format!("open {link} in your browser"), &json!({}));
            if !no_open && let Err(why) = (printer.io.open_url)(&link) {
                say(
                    &mut printer.io.stderr,
                    &format!("could not open a browser: {why}; open the link above yourself"),
                );
            }
        }
        let scope = TickScope {
            task_id: None,
            rules: TickRules::All,
        };
        let mut presses = 0;
        let ended = ticks(
            &mut driver,
            &scope,
            &mut printer,
            &mut presses,
            OnIdle::Wait,
            |_| {},
        )
        .await;
        finish(project, driver, &mut printer, ended, presses).await
    })
}
