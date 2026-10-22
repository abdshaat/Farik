//! The notification Catervas sends through the operating system when a digest is due and no
//! Catervas tab is open (`docs/SPEC.md` 4.4, 8.6; ADR 0059). One program is run with fixed
//! arguments, never a shell; the title and the body reach it as arguments, never as part of a
//! command line that anything parses.

use std::collections::BTreeMap;
use std::sync::Arc;

/// One program run: the program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyCall {
    /// The program.
    pub program: String,
    /// Its arguments.
    pub args: Vec<String>,
}

/// What runs a notification program: the program itself, or a test's recording of it.
pub type NotifyRun = Arc<dyn Fn(&NotifyCall) -> Result<(), String> + Send + Sync>;

/// A notification that could not be sent: the program is missing or it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotSent(pub String);

impl std::fmt::Display for NotSent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for NotSent {}

/// Which system this is, as far as notifications go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    /// `std::env::consts::OS`: `linux`, `macos` or `windows`.
    pub os: String,
    /// Whether this is Linux under WSL (`WSL_DISTRO_NAME` is set), which notifies through Windows.
    pub wsl: bool,
}

impl Platform {
    /// The platform of `os` (`std::env::consts::OS`) in the environment `env`.
    #[must_use]
    pub fn detect(os: &str, env: &BTreeMap<String, String>) -> Self {
        Self {
            os: os.to_string(),
            wsl: os == "linux"
                && env
                    .get("WSL_DISTRO_NAME")
                    .is_some_and(|name| !name.is_empty()),
        }
    }
}

/// Windows PowerShell under WSL.
const WSL_POWERSHELL: &str = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";

/// The fixed script Windows PowerShell runs. It reads three arguments: the title and the body, each
/// the letter `x` and the text in base64 (so that no text is ever parsed as PowerShell), and the port
/// Catervas is served on. It shows a toast that opens `http://127.0.0.1:<port>/needs-you`.
const TOAST_SCRIPT: &str = "& { param($t, $b, $p) \
$d = { param($s) [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($s.Substring(1))) }; \
$title = & $d $t; $body = & $d $b; \
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
$xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
$text = $xml.GetElementsByTagName('text'); \
$text.Item(0).AppendChild($xml.CreateTextNode($title)) | Out-Null; \
$text.Item(1).AppendChild($xml.CreateTextNode($body)) | Out-Null; \
$xml.DocumentElement.SetAttribute('activationType', 'protocol'); \
$xml.DocumentElement.SetAttribute('launch', 'http://127.0.0.1:' + $p + '/needs-you'); \
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Catervas').Show($toast) }";

/// A text as the toast script reads it: `x` and the text in base64.
fn encoded(text: &str) -> String {
    use base64::Engine as _;
    format!(
        "x{}",
        base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
    )
}

/// The program and arguments that show a notification on `platform`. `port` is the one Catervas
/// is served on, for the Windows toast to open.
#[must_use]
pub fn call_for(platform: &Platform, port: u16, title: &str, body: &str) -> NotifyCall {
    if platform.wsl || platform.os == "windows" {
        return NotifyCall {
            program: if platform.wsl {
                WSL_POWERSHELL.to_string()
            } else {
                "powershell.exe".to_string()
            },
            args: vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                TOAST_SCRIPT.to_string(),
                encoded(title),
                encoded(body),
                port.to_string(),
            ],
        };
    }
    if platform.os == "macos" {
        return NotifyCall {
            program: "osascript".to_string(),
            args: [
                "-e",
                "on run argv",
                "-e",
                "display notification (item 1 of argv) with title (item 2 of argv)",
                "-e",
                "end run",
                body,
                title,
            ]
            .map(str::to_string)
            .to_vec(),
        };
    }
    NotifyCall {
        program: "notify-send".to_string(),
        args: vec![
            "--app-name=Catervas".to_string(),
            title.to_string(),
            body.to_string(),
        ],
    }
}

/// Shows `title` and `body` through `run`.
///
/// # Errors
///
/// [`NotSent`] when the program is missing or fails.
pub fn send(
    platform: &Platform,
    port: u16,
    run: &NotifyRun,
    title: &str,
    body: &str,
) -> Result<(), NotSent> {
    run(&call_for(platform, port, title, body)).map_err(NotSent)
}

/// Runs `call` for real: the program with its arguments, no shell, nothing on its standard
/// streams, and its exit status read.
///
/// # Errors
///
/// A sentence when the program could not be run or did not succeed.
pub fn run_program(call: &NotifyCall) -> Result<(), String> {
    let program = std::path::Path::new(&call.program).file_name().map_or_else(
        || call.program.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    let status = std::process::Command::new(&call.program)
        .args(&call.args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("{program} is not installed")
            } else {
                format!("{program} could not be run")
            }
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} did not succeed"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn recording() -> (NotifyRun, Arc<Mutex<Vec<NotifyCall>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        let run: NotifyRun = Arc::new(move |call| {
            kept.lock().expect("calls").push(call.clone());
            Ok(())
        });
        (run, seen)
    }

    fn platform(os: &str, wsl: bool) -> Platform {
        let env: BTreeMap<String, String> = if wsl {
            BTreeMap::from([("WSL_DISTRO_NAME".to_string(), "Ubuntu".to_string())])
        } else {
            BTreeMap::new()
        };
        Platform::detect(os, &env)
    }

    #[test]
    fn notifies_through_the_operating_system() {
        let (run, seen) = recording();

        // Linux: notify-send, the title and the body as separate arguments.
        send(
            &platform("linux", false),
            7420,
            &run,
            "Catervas · 09:00",
            "9 things need you",
        )
        .expect("sent");
        // A title that is a shell command is one argument, unchanged.
        let hostile = "'; rm -rf /";
        send(&platform("linux", false), 7420, &run, hostile, "body").expect("sent");
        // macOS: osascript, the body and title as the script's arguments.
        send(&platform("macos", false), 7420, &run, hostile, "body").expect("sent");
        // WSL: Windows' PowerShell by its path, the body among its arguments.
        send(
            &platform("linux", true),
            7420,
            &run,
            "Catervas · 09:00",
            "9 things need you",
        )
        .expect("sent");

        let calls = seen.lock().expect("calls").clone();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].program, "notify-send");
        assert_eq!(
            calls[0].args,
            [
                "--app-name=Catervas",
                "Catervas · 09:00",
                "9 things need you"
            ]
        );
        assert_eq!(
            calls[1].args,
            ["--app-name=Catervas", hostile, "body"],
            "the title reaches the program as one argument"
        );
        assert_eq!(calls[2].program, "osascript");
        assert_eq!(&calls[2].args[calls[2].args.len() - 2..], ["body", hostile]);
        assert_eq!(calls[3].program, WSL_POWERSHELL);
        assert_eq!(&calls[3].args[..2], ["-NoProfile", "-Command"]);
        assert!(
            calls[3].args[2].contains("ToastNotificationManager"),
            "a fixed script that shows a toast"
        );
        assert!(
            !calls[3].args[2].contains("9 things"),
            "the script holds none of the text"
        );
        let body = calls[3].args.iter().any(|arg| {
            arg.strip_prefix('x').is_some_and(|encoded| {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .is_ok_and(|bytes| bytes == "9 things need you".as_bytes())
            })
        });
        assert!(body, "the body is an argument: {:?}", calls[3].args);

        // A program that is missing is not sent.
        let missing: NotifyRun = Arc::new(|_| Err("notify-send is not installed".to_string()));
        assert_eq!(
            send(&platform("linux", false), 7420, &missing, "t", "b"),
            Err(NotSent("notify-send is not installed".to_string()))
        );
    }
}
