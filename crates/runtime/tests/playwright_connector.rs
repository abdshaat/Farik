//! The preview and the Playwright connector against a real Docker daemon: the pinned image's tool
//! list, the browser's confinement barrier by barrier, and nothing left behind. The preview is
//! `alpine:3.22` serving a page with busybox's `httpd`. Ignored by default; run by
//! `cargo xtask check --integration`, which CI runs after pulling both images.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use farik_core::contract::TaskId;
use farik_core::team::Preview;
use farik_roles::{ConnectorDefinition, builtin_connector};
use farik_runtime::{
    DockerPreviewFactory, McpTransport, PreviewFactory, RunningPreview, browser_container,
    connector_server,
};
use serde_json::{Value, json};

const ALPINE: &str = "alpine:3.22";
const PORT: u16 = 4401;
/// How long one answer of the browser may take.
const ANSWER: Duration = Duration::from_secs(90);

fn playwright() -> ConnectorDefinition {
    builtin_connector("playwright").expect("Farik ships it")
}

fn project(test: &str) -> String {
    format!("farik-test-{}-{test}", std::process::id())
}

fn task() -> TaskId {
    TaskId::try_from("FRK-1").expect("an id")
}

fn docker(args: &[&str]) -> (bool, String) {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("docker runs");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

fn user() -> String {
    let id = |flag: &str| {
        let output = Command::new("id").arg(flag).output().expect("id runs");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    format!("{}:{}", id("-u"), id("-g"))
}

fn containers_named(name: &str) -> usize {
    docker(&["ps", "-a", "-q", "--filter", &format!("name=^/{name}$")])
        .1
        .lines()
        .count()
}

/// Removes every container a test labelled with its project, when it ends however it ends.
struct Cleanup(String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let filter = format!("label=farik.project={}", self.0);
        let (_, ids) = docker(&["ps", "-a", "-q", "--filter", &filter]);
        for id in ids.lines() {
            let _ = docker(&["rm", "-f", id]);
        }
    }
}

/// A worktree holding `site/`: a page titled "Preview", the CGI `/cgi-bin/away` answering `302` to
/// `http://example.com/`, and busybox's `httpd`. Alpine 3.22's own busybox leaves `httpd` out, so
/// it is taken from its `busybox-extras` package once, by a container with the network on.
fn worktree(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("farik-preview-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let site = root.join("site");
    std::fs::create_dir_all(site.join("cgi-bin")).expect("the site is made");
    std::fs::write(
        site.join("index.html"),
        "<!doctype html><title>Preview</title><h1>Hello from the preview</h1>\n",
    )
    .expect("the page is written");
    let away = site.join("cgi-bin/away");
    std::fs::write(
        &away,
        "#!/bin/sh\nprintf 'Status: 302 Found\\r\\nLocation: http://example.com/\\r\\n\\r\\n'\n",
    )
    .expect("the redirect is written");
    make_executable(&away);
    let mount = format!("type=bind,src={},dst=/out", root.display());
    let (fetched, output) = docker(&[
        "run",
        "--rm",
        "--mount",
        &mount,
        ALPINE,
        "sh",
        "-c",
        &format!(
            "apk add -q --no-cache busybox-extras && cp /bin/busybox-extras /out/busybox \
             && chown {} /out/busybox",
            user()
        ),
    ]);
    assert!(fetched, "busybox's httpd could not be fetched: {output}");
    root
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn serving() -> Preview {
    Preview {
        prepare: Some("cp busybox site/.httpd".to_owned()),
        start: format!("./busybox httpd -f -p {PORT} -h site"),
        port: PORT,
        path: "/".to_owned(),
    }
}

/// An MCP client over a child's standard streams, one JSON message a line.
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next: u64,
}

impl Mcp {
    fn start(command: &str, args: &[String]) -> Mcp {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take().expect("a stdin");
        let stdout = child.stdout.take().expect("a stdout");
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut mcp = Mcp {
            child,
            stdin,
            lines,
            next: 0,
        };
        mcp.ask(
            "initialize",
            &json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "farik-test", "version": "0" }
            }),
        );
        mcp.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        mcp
    }

    fn of(server: &farik_runtime::McpServerConfig) -> Mcp {
        let McpTransport::Stdio { command, args } = &server.transport else {
            panic!("the browser is a child process");
        };
        Mcp::start(command, args)
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("the server reads");
        self.stdin.flush().expect("flushed");
    }

    fn ask(&mut self, method: &str, params: &Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let line = self
                .lines
                .recv_timeout(ANSWER)
                .unwrap_or_else(|_| panic!("no answer to {method}"));
            let message: Value = serde_json::from_str(&line).expect("a JSON line");
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    /// The text of `browser_navigate`'s answer for `url`.
    fn navigate(&mut self, url: &str) -> String {
        let answer = self.ask(
            "tools/call",
            &json!({ "name": "browser_navigate", "arguments": { "url": url } }),
        );
        answer["result"]["content"].as_array().map_or_else(
            || answer.to_string(),
            |parts| {
                parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "needs docker"]
fn the_pinned_image_lists_the_pinned_tools() {
    let definition = playwright();
    let mut args: Vec<String> = ["run", "--rm", "-i", "--network", "none"]
        .map(String::from)
        .to_vec();
    args.push(definition.image.clone());
    args.extend(definition.args.iter().cloned());
    let mut mcp = Mcp::start("docker", &args);
    let listed = mcp.ask("tools/list", &json!({}));
    let tools: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("a list of tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(tools.contains(&"browser_navigate"), "{tools:?}");
    let untagged: Vec<&&str> = tools
        .iter()
        .filter(|tool| !definition.tools.contains_key(**tool))
        .collect();
    assert!(
        untagged.is_empty(),
        "the pinned image lists tools playwright.yaml does not tag; add each as denied: \
         {untagged:?}"
    );
}

/// A preview container the test runs itself, on the bridge network, where `example.com` is the
/// sink's address, so that a request that leaves the namespace can be counted.
struct BridgePreview {
    name: String,
    project: String,
}

impl RunningPreview for BridgePreview {
    fn origin(&self) -> String {
        format!("http://localhost:{PORT}")
    }

    fn container(&self) -> String {
        self.name.clone()
    }

    fn labels(&self) -> Vec<String> {
        vec![format!("farik.project={}", self.project)]
    }

    fn user(&self) -> String {
        user()
    }

    fn stop(&self, _reason: &str) -> Result<(), farik_runtime::PreviewError> {
        let _ = docker(&["rm", "-f", &self.name, &browser_container(&self.name)]);
        Ok(())
    }
}

/// How many requests the sink has answered.
fn sink_hits(sink: &str) -> usize {
    docker(&["logs", sink])
        .1
        .lines()
        .filter(|line| line.contains("url:"))
        .count()
}

/// The connector's arguments with one confinement flag, and its value, left out.
fn without(
    server: &farik_runtime::McpServerConfig,
    flags: &[&str],
) -> farik_runtime::McpServerConfig {
    let McpTransport::Stdio { command, args } = &server.transport else {
        panic!("the browser is a child process");
    };
    let mut kept = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if flags.contains(&arg.as_str()) {
            skip = true;
        } else {
            kept.push(arg.clone());
        }
    }
    farik_runtime::McpServerConfig {
        transport: McpTransport::Stdio {
            command: command.clone(),
            args: kept,
        },
        ..server.clone()
    }
}

const DIRECT: &str = "http://example.com/";

fn redirect() -> String {
    format!("http://localhost:{PORT}/cgi-bin/away")
}

/// What the browser answers, with the flags `left_out` left out of its connector, to the preview,
/// to `http://example.com/` directly, and to the preview's redirect there, in that order.
fn browse(
    preview: &dyn RunningPreview,
    output: &Path,
    left_out: &[&str],
) -> (String, String, String) {
    let server = connector_server(&playwright(), preview, output);
    let mut browser = Mcp::of(&without(&server, left_out));
    let answers = (
        browser.navigate(&format!("http://localhost:{PORT}/")),
        browser.navigate(DIRECT),
        browser.navigate(&redirect()),
    );
    drop(browser);
    preview.stop("the test ended").expect("stopped");
    answers
}

/// The sink, which stands in for the internet: on the bridge network, answering any request.
/// Answers its name and the `--add-host` that makes `example.com` it.
fn a_sink(root: &Path, project: &str) -> (String, String) {
    let sink = format!("farik-sink-{project}");
    let label = format!("farik.project={project}");
    let site = format!("type=bind,src={},dst=/w", root.join("site").display());
    let bin = format!("type=bind,src={},dst=/b", root.display());
    let (ran, said) = docker(&[
        "run",
        "-d",
        "--name",
        &sink,
        "--label",
        &label,
        "--network",
        "bridge",
        "--mount",
        &site,
        "--mount",
        &bin,
        "-w",
        "/w",
        ALPINE,
        "/b/busybox",
        "httpd",
        "-f",
        "-vv",
        "-p",
        "80",
        "-h",
        "/w",
    ]);
    assert!(ran, "the sink did not start: {said}");
    let (_, address) = docker(&[
        "inspect",
        "-f",
        "{{.NetworkSettings.Networks.bridge.IPAddress}}",
        &sink,
    ]);
    (sink, format!("example.com:{}", address.trim()))
}

/// A preview the test runs itself on the bridge network, `example` making `example.com` the sink,
/// so that only the browser's own flags stand between it and the sink.
fn on_the_bridge(root: &Path, project: &str, barrier: &str, example: &str) -> BridgePreview {
    let name = format!("farik-preview-{project}-{barrier}");
    let label = format!("farik.project={project}");
    let mount = format!("type=bind,src={},dst=/workspace", root.display());
    let (ran, said) = docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "--label",
        &label,
        "--network",
        "bridge",
        "--add-host",
        example,
        "--user",
        &user(),
        "--mount",
        &mount,
        "-w",
        "/workspace",
        ALPINE,
        "./busybox",
        "httpd",
        "-f",
        "-p",
        &PORT.to_string(),
        "-h",
        "site",
    ]);
    assert!(ran, "the preview did not start: {said}");
    BridgePreview {
        name,
        project: project.to_owned(),
    }
}

#[test]
#[ignore = "needs docker"]
fn the_browser_reaches_only_the_preview() {
    let root = worktree("confined");
    let project = project("confined");
    let _cleanup = Cleanup(project.clone());
    let output = root.join("output");
    std::fs::create_dir_all(&output).expect("the output folder is made");
    let (sink, example) = a_sink(&root, &project);
    let preview_page = |page: &str, run: &str| {
        assert!(
            page.contains("Page Title: Preview"),
            "({run}) preview: {page}"
        );
    };

    // (a) The preview's network is off, and the browser has neither the proxy nor the origins.
    let preview = DockerPreviewFactory {
        image: ALPINE.to_owned(),
    }
    .start(&project, &task(), &root, &serving(), "tree")
    .unwrap_or_else(|error| panic!("the preview did not start: {error}"));
    let (page, direct, away) = browse(
        preview.as_ref(),
        &output,
        &["--proxy-server", "--proxy-bypass", "--allowed-origins"],
    );
    preview_page(&page, "a");
    assert!(
        direct.contains("ERR_INTERNET_DISCONNECTED"),
        "(a) direct: {direct}"
    );
    assert!(
        away.contains("ERR_INTERNET_DISCONNECTED"),
        "(a) redirect: {away}"
    );

    // (b) The bridge, and the proxy alone.
    let preview = on_the_bridge(&root, &project, "b", &example);
    let (page, direct, away) = browse(&preview, &output, &["--allowed-origins"]);
    preview_page(&page, "b");
    assert!(
        direct.contains("ERR_PROXY_CONNECTION_FAILED"),
        "(b) direct: {direct}"
    );
    assert!(
        away.contains("ERR_PROXY_CONNECTION_FAILED"),
        "(b) redirect: {away}"
    );
    assert_eq!(sink_hits(&sink), 0, "(b) a request left the namespace");

    // (c) The bridge, and `--allowed-origins` alone: it blocks the direct navigation, and not the
    // redirect, which reaches example.com, as the server's README says. That is why the proxy is.
    let preview = on_the_bridge(&root, &project, "c", &example);
    let (page, direct, _) = browse(&preview, &output, &["--proxy-server", "--proxy-bypass"]);
    preview_page(&page, "c");
    assert!(
        direct.contains("ERR_BLOCKED_BY_CLIENT"),
        "(c) direct: {direct}"
    );
    assert_eq!(
        sink_hits(&sink),
        1,
        "(c) the redirect, and only the redirect, reached example.com"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
#[ignore = "needs docker"]
fn stop_leaves_no_container() {
    let definition = playwright();
    let root = worktree("stop");
    let project = project("stop");
    let _cleanup = Cleanup(project.clone());
    let output = root.join("output");
    std::fs::create_dir_all(&output).expect("the output folder is made");
    let factory = DockerPreviewFactory {
        image: ALPINE.to_owned(),
    };
    let preview = factory
        .start(&project, &task(), &root, &serving(), "tree")
        .unwrap_or_else(|error| panic!("the preview did not start: {error}"));
    assert!(
        root.join("site/.httpd").exists(),
        "prepare ran in the worktree"
    );
    let server = connector_server(&definition, preview.as_ref(), &output);
    let mut browser = Mcp::of(&server);
    let page = browser.navigate(&format!("http://localhost:{PORT}/"));
    assert!(page.contains("Page Title: Preview"), "{page}");
    let (preview_name, browser_name) =
        (preview.container(), browser_container(&preview.container()));
    assert_eq!(containers_named(&preview_name), 1);
    assert_eq!(containers_named(&browser_name), 1);

    preview.stop("the session ended").expect("stopped");

    assert_eq!(containers_named(&preview_name), 0);
    assert_eq!(containers_named(&browser_name), 0);
    drop(browser);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
#[ignore = "needs docker"]
fn the_task_cleanup_removes_the_preview() {
    use farik_runtime::{DockerSandboxFactory, SandboxFactory};

    let root = worktree("cleanup");
    let project = project("cleanup");
    let _cleanup = Cleanup(project.clone());
    let preview = DockerPreviewFactory {
        image: ALPINE.to_owned(),
    }
    .start(&project, &task(), &root, &serving(), "tree")
    .unwrap_or_else(|error| panic!("the preview did not start: {error}"));
    let name = preview.container();
    assert_eq!(containers_named(&name), 1);

    // A killed session never stops its preview; the task's cleanup removes it by name.
    DockerSandboxFactory {
        image: ALPINE.to_owned(),
    }
    .remove(&project, &task())
    .expect("the task's containers are removed");

    assert_eq!(containers_named(&name), 0);
    let _ = std::fs::remove_dir_all(&root);
}
