//! The `catervas` command line. Everything it does is in the library beside this file, so that the
//! tests run a command without spawning a process.

use std::path::PathBuf;
use std::sync::Arc;

use catervas::ids::{RandomSessionIds, SystemClock};
use catervas::{CliIo, Engine, Interrupts, run_cli};

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
    io.credential_stores = catervas::system_credential_stores(&io.env, true);
    io.connector_secrets = catervas::system_connector_secrets(&io.env);
    io.own_program = std::env::current_exe().ok();
    let code = run_cli(&arguments, &mut io);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Starts the system's opener on `url` and does not wait for it.
fn open_in_browser(url: &str) -> Result<(), String> {
    let wsl = std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists();
    opener(wsl)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The system's opener, given a link as its last argument; `wsl` says Linux runs under Windows'
/// WSL, which has no browser of its own, so Windows' opens it.
fn opener(wsl: bool) -> std::process::Command {
    if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) || wsl {
        let mut command = std::process::Command::new(if wsl { "cmd.exe" } else { "cmd" });
        command.args(["/c", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn opens_the_windows_browser_from_wsl() {
        // WSL has no browser of its own: the link goes to Windows, which keeps its `#` part.
        let command = opener(true);
        assert_eq!(command.get_program(), "cmd.exe");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["/c", "start", ""]);
        assert_eq!(opener(false).get_program(), "xdg-open");
    }
}
