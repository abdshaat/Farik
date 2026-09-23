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
    let code = run_cli(&arguments, &mut io);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
