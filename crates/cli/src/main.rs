//! The `farik` command line. Everything it does is in the library beside this file, so that the
//! tests run a command without spawning a process.

use std::path::PathBuf;
use std::sync::Arc;

use farik::ids::{RandomSessionIds, SystemClock};
use farik::{CliIo, Engine, Interrupts, run_cli};

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    let mut io = CliIo::new(
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        Box::new(std::io::stdout()),
        Box::new(std::io::stderr()),
        Arc::new(SystemClock),
    );
    io.stdin = Box::new(std::io::stdin());
    io.stdin_is_terminal = std::io::IsTerminal::is_terminal(&std::io::stdin());
    io.env = std::env::vars_os()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    io.engine = Engine::Claude;
    io.interrupts = Interrupts::CtrlC;
    io.session_ids = Arc::new(RandomSessionIds);
    io.open_url = Arc::new(open_in_browser);
    io.credential_stores = farik::system_credential_stores(&io.env, true);
    io.connector_secrets = farik::system_connector_secrets(&io.env);
    let code = run_cli(&arguments, &mut io);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Starts the system's opener on `url` and does not wait for it.
fn open_in_browser(url: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
