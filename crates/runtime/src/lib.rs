//! Catervas's agent runtime: a session is started, read, talked to, and stopped through one trait
//! (`docs/SPEC.md` section 8.2), whatever program or recording is behind it.

/// A kit connector's allowances: the period, the count and what a connect may set.
pub mod allowances;
/// The Catervafication: the epic that has the team document the project, its fixed tasks, and when
/// it starts.
pub(crate) mod catervafication;
/// The team's ceremonies: the facts each is given.
pub mod ceremonies;
/// The team's channel: messages, and whom they mention.
pub mod channel;
/// The user's one-to-one chats with each agent, kept apart from the channel.
pub mod chat;
/// What the human may do about an escalation, in words, one choice suggested.
pub mod choices;
/// The Claude Code program as a runtime: its command line, credential, and version.
#[cfg(unix)]
pub mod claude;
/// What the first-run wizard finds on the computer, and the sandbox image it builds.
#[cfg(unix)]
pub mod computer;
/// A custom connector's keys, per agent: the keychain, or a private file.
#[cfg(unix)]
pub mod connectors;
/// What a session cost, and what each budget has left.
pub mod cost;
/// Where the model credential is kept: the environment, the keychain, or a private file.
#[cfg(unix)]
pub mod credential;
/// Contract exit criteria, run and judged.
pub mod criteria;
/// The local service: the hooks around every tool call, and Catervas's tools over MCP.
#[cfg(unix)]
pub mod daemon;
/// Catervas's own server over eBay's Browse API (ADR 0038, ADR 0043).
pub mod ebay;
/// Commands run on an agent's behalf, and what came of them.
pub mod exec;
/// The forge pull requests live on, driven through the `gh` program.
pub mod forge;
/// Catervas's own server over the central banks' exchange rates (ADR 0038).
pub mod fx;
/// Catervas's own Google Ads connector: the client for Google's API and what each tool sends (ADR
/// 0038, ADR 0042).
#[cfg(unix)]
pub mod google_ads;
/// A stand-in for the Google Ads API, for the tests of the connector.
#[cfg(all(test, unix))]
#[path = "../tests/support/google_ads_fixture.rs"]
mod google_ads_fixture;
/// A mail server in Docker for the tests of the procurement mailbox.
#[cfg(all(test, unix))]
#[path = "../tests/support/greenmail.rs"]
mod greenmail;
/// The procurement mailbox over IMAP and SMTP: its settings, the login, sending and reading.
#[cfg(unix)]
pub mod mailbox;
/// The owner's decisions on marketing plans, the ends that dates bring, and how a plan is worded
/// on the wire.
pub mod marketing;
/// An OAuth authorization server and a protected MCP server for the sign-in tests.
#[cfg(all(test, unix))]
#[path = "../tests/support/oauth_fixture.rs"]
mod oauth_fixture;
/// Catervas running its team: the board read, the next thing on it done, one session at a time.
#[cfg(unix)]
pub mod orchestrator;
/// Catervas's own server over the open vulnerability database (ADR 0038).
pub mod osv;
/// Whether the human has paused the team.
pub mod pause;
#[cfg(all(test, unix))]
#[path = "../tests/support/ports.rs"]
mod ports;
/// The project's preview, and the confined browser beside it.
pub mod preview;
/// The Procurement Specialist's purchase orders and renewals: the locks that number, decide and
/// expire them, and what the clock closes.
pub mod procurement;
/// A session's system prompt, assembled in one fixed order.
pub mod prompt;
/// Catervas's own server over the United States' product and vehicle safety agencies (ADR 0038).
pub mod recalls;
/// Sessions replayed from recorded transcripts.
pub mod recorded;
/// The apps Catervas has registered with a service, and which servers' addresses each serves.
pub mod registered_apps;
/// Where a task's commands run: a container per task, or the host in no-sandbox mode.
pub mod sandbox;
/// What a session is, what it reports, and the traits every runtime implements.
pub mod session;
/// When each session started and how it ended, in the log.
pub mod sessions;
/// Signing in to a connector's service: the grant, and its refresh and revocation.
#[cfg(unix)]
pub mod sign_in;
/// Skills an agent is given beyond its role's: reading, confirming, loading.
#[cfg(unix)]
pub mod skills;
/// An agent asleep until its model provider's limit resets.
pub mod sleep;
/// Starting and ending a sprint.
pub mod sprints;
/// The Claude Code program's `stream-json` lines, read as session events.
pub mod stream;
/// Saved team templates, kept privately in Catervas's state folder.
#[cfg(unix)]
pub mod templates;
/// Catervas's own tools, each checked against the agent's tier and the rule that owns it.
pub mod tools;
/// Transition requests, judged by the governor on the store's facts and recorded either way.
pub mod transitions;

/// A poisoned lock only means a task panicked while holding it; what it guards is still whole.
pub(crate) fn locked<Value>(mutex: &std::sync::Mutex<Value>) -> std::sync::MutexGuard<'_, Value> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes `bytes` to `path` readable by its owner alone, replacing whatever was there, whole. The
/// bytes go to a temporary file beside `path` (mode 0600 from its creation, so it is never
/// readable by others), are synced, and the file is renamed over `path`: a reader sees the whole
/// old file or the whole new one, never a missing or half-written one. On an error the temporary
/// file is removed.
///
/// # Errors
///
/// The error of creating, writing or syncing the temporary file, or of renaming it into place.
#[cfg(unix)]
pub fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

pub use exec::{ExecError, ExecResult, Executor, OUTPUT_LIMIT_BYTES};
#[cfg(unix)]
pub use preview::docker::DockerPreviewFactory;
pub use preview::{
    AVAILABLE_FOR, BLACKHOLE_PROXY, CheckError, CheckTheme, CheckWidth, NoPreviews, PageCheck,
    PolledPreviews, PreviewError, PreviewFactory, RunningPreview, browser_container, check_page,
    connector_server, disallowed_tools,
};
pub use recorded::{RecordedAdapter, Transcript};
#[cfg(unix)]
pub use sandbox::docker::{DockerSandbox, DockerSandboxFactory};
#[cfg(unix)]
pub use sandbox::host::{HostSandbox, HostSandboxFactory};
pub use sandbox::{SANDBOX_IMAGE, Sandbox, SandboxError, SandboxFactory};
pub use session::{
    EndReason, McpServerConfig, McpTransport, RuntimeAdapter, RuntimeError, SessionEvent,
    SessionHandle, SessionPurpose, SessionSpec,
};
pub use stream::StreamParser;
#[cfg(unix)]
pub use templates::{TemplateError, TemplateListing, Templates};
pub use tools::{
    CatervasTool, KitSource, ToolContext, ToolDeps, ToolError, call_tool, tool_descriptors,
};

#[cfg(all(test, unix))]
mod write_private_tests {
    use super::write_private;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("catervas-private-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch folder");
        dir
    }

    #[test]
    fn a_reader_never_sees_a_private_file_half_written() {
        const ROUNDS: usize = 200;
        let dir = scratch("reader");
        let file = dir.join("state.json");
        let values = [
            serde_json::json!({ "last_project": "/one/project" }).to_string(),
            serde_json::json!({ "last_project": "/another/project/entirely" }).to_string(),
        ];
        write_private(&file, values[0].as_bytes()).expect("first write");
        let writer = {
            let (file, values) = (file.clone(), values.clone());
            std::thread::spawn(move || {
                for round in 0..ROUNDS {
                    write_private(&file, values[round % 2].as_bytes()).expect("rewrite");
                }
            })
        };
        for _ in 0..ROUNDS * 2 {
            let text = std::fs::read_to_string(&file).expect("a private file is always there");
            assert!(values.contains(&text), "read a half-written file: {text:?}");
        }
        writer.join().expect("writer finished");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn keeps_the_file_readable_by_its_owner_alone() {
        let dir = scratch("mode");
        let file = dir.join("secret.json");
        std::fs::write(&file, b"old").expect("old file, readable by others");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("mode");
        write_private(&file, b"new").expect("write");
        assert_eq!(std::fs::read(&file).expect("read"), b"new");
        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = scratch("tmp");
        let file = dir.join("state.json");
        for round in 0..5 {
            write_private(&file, format!("{round}").as_bytes()).expect("write");
        }
        let names = |dir: &PathBuf| {
            let mut names: Vec<_> = std::fs::read_dir(dir)
                .expect("list")
                .map(|entry| entry.expect("entry").file_name())
                .collect();
            names.sort();
            names
        };
        assert_eq!(names(&dir), ["state.json"]);

        let blocked = dir.join("blocked");
        std::fs::create_dir(&blocked).expect("a directory where the file goes");
        assert!(write_private(&blocked, b"x").is_err());
        assert_eq!(names(&dir), ["blocked", "state.json"]);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}
