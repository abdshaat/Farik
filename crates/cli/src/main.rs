//! The `farik` command line. Everything it does is in the library beside this file, so that the
//! tests run a command without spawning a process.

use std::path::PathBuf;

use chrono::Utc;
use farik::{CliIo, run_cli};
use farik_protocol::clock::Clock;

/// The wall clock, which is the only thing the binary has that a test does not want.
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        Utc::now()
    }
}

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    let mut io = CliIo {
        stdin: Box::new(std::io::stdin()),
        stdout: Box::new(std::io::stdout()),
        stderr: Box::new(std::io::stderr()),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        clock: Box::new(SystemClock),
    };
    let code = run_cli(&arguments, &mut io);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
