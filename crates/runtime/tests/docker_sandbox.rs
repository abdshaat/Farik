//! The Docker sandbox against a real Docker daemon, on `alpine:3.22`. Ignored by default; run by
//! `cargo xtask check --integration`, which CI runs after pulling the image.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use farik_core::contract::TaskId;
use farik_runtime::{DockerSandbox, ExecError, Executor, Sandbox, SandboxError};

const IMAGE: &str = "alpine:3.22";
const SECOND: Duration = Duration::from_secs(1);

fn project(test: &str) -> String {
    format!("farik-test-{}-{test}", std::process::id())
}

fn worktree(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("farik-docker-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temporary directory can be made");
    root
}

fn task() -> TaskId {
    TaskId::try_from("FRK-1").expect("an id")
}

fn create_at(test: &str, root: &std::path::Path, network: bool) -> DockerSandbox {
    DockerSandbox::create(&project(test), &task(), root, network, IMAGE)
        .unwrap_or_else(|error| panic!("the sandbox could not be made: {error}"))
}

fn create(test: &str, network: bool) -> DockerSandbox {
    create_at(test, &worktree(test), network)
}

fn no_env() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("docker runs");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn containers_named(name: &str) -> usize {
    docker(&["ps", "-a", "-q", "--filter", &format!("name=^/{name}$")])
        .lines()
        .count()
}

#[test]
#[ignore = "needs docker"]
fn runs_a_command_in_the_container_at_workspace() {
    let root = worktree("workspace");
    std::fs::write(root.join("marker.txt"), "from the host").expect("the marker is written");
    let sandbox = create_at("workspace", &root, false);
    let result = sandbox
        .run("pwd; cat marker.txt", "", 30 * SECOND, &no_env())
        .expect("the command runs");
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "/workspace\nfrom the host");
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn writes_files_the_user_owns() {
    let root = worktree("owner");
    let sandbox = create_at("owner", &root, false);
    let result = sandbox
        .run("touch made.txt", "", 30 * SECOND, &no_env())
        .expect("the command runs");
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let id = Command::new("id").arg("-u").output().expect("id runs");
    let uid: u32 = String::from_utf8_lossy(&id.stdout)
        .trim()
        .parse()
        .expect("id -u prints a number");
    let made = std::fs::metadata(root.join("made.txt")).expect("the file is on the host");
    assert_eq!(made.uid(), uid);
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn has_no_network_unless_asked() {
    let sandbox = create("network", false);
    let result = sandbox
        .run(
            "wget -q -T 3 -O- http://example.com",
            "",
            30 * SECOND,
            &no_env(),
        )
        .expect("the command runs");
    assert_ne!(result.exit_code, 0);
    let mode = docker(&[
        "inspect",
        "-f",
        "{{.HostConfig.NetworkMode}}",
        sandbox.name(),
    ]);
    assert_eq!(mode.trim(), "none");
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn stops_a_command_at_the_deadline_inside_the_container() {
    let sandbox = create("deadline", false);
    let started = Instant::now();
    let result = sandbox
        .run("sleep 30", "", SECOND, &no_env())
        .expect("the command runs");
    assert!(started.elapsed() < 10 * SECOND);
    assert!(result.timed_out);
    // Busybox's `timeout` leaves its watcher, whose command line still reads `... sleep 30`, for
    // the `-k 2` grace after the command itself is killed; the check waits that out.
    let mut processes = String::new();
    for _ in 0..50 {
        processes = docker(&["exec", sandbox.name(), "ps"]);
        if !processes.contains("sleep 30") {
            break;
        }
        std::thread::sleep(SECOND / 10);
    }
    assert!(!processes.contains("sleep 30"), "{processes}");
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn refuses_an_image_that_is_not_there() {
    let image = "farik/no-such-image:0";
    let refused =
        DockerSandbox::create(&project("image"), &task(), &worktree("image"), false, image);
    assert_eq!(
        refused.err(),
        Some(SandboxError::ImageMissing {
            image: image.to_owned()
        })
    );
    let name = format!("farik-{}-frk-1", project("image"));
    assert_eq!(containers_named(&name), 0);
}

#[test]
#[ignore = "needs docker"]
fn replaces_a_container_left_behind() {
    let _left_behind = create("replace", false);
    let sandbox = create("replace", false);
    assert_eq!(containers_named(sandbox.name()), 1);
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn answers_container_gone_after_discard() {
    let first = create("gone", false);
    let second = create("gone", false);
    Box::new(second)
        .discard()
        .expect("the container is removed");
    assert_eq!(
        first.run("true", "", 30 * SECOND, &no_env()),
        Err(ExecError::ContainerGone)
    );
}

#[test]
#[ignore = "needs docker"]
fn passes_only_the_environment_it_was_given_into_the_container() {
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
    let sandbox = create("environment", false);
    let env = BTreeMap::from([("GIVEN".to_owned(), "yes".to_owned())]);
    let result = sandbox
        .run("env", "", 30 * SECOND, &env)
        .expect("the command runs");
    assert!(!result.stdout.contains("CARGO_MANIFEST_DIR"));
    assert!(result.stdout.lines().any(|line| line == "GIVEN=yes"));
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn names_a_container_docker_accepts_from_any_project_id() {
    let project = format!("My Project/{}", std::process::id());
    let sandbox = DockerSandbox::create(&project, &task(), &worktree("name"), false, IMAGE)
        .unwrap_or_else(|error| panic!("the sandbox could not be made: {error}"));
    let expected = format!("farik-my-project-{}-frk-1", std::process::id());
    assert_eq!(sandbox.name(), expected);
    assert_eq!(containers_named(&expected), 1);
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn discards_a_container_already_gone() {
    let first = create("discarded", false);
    let second = create("discarded", false);
    Box::new(second)
        .discard()
        .expect("the container is removed");
    assert_eq!(Box::new(first).discard(), Ok(()));
}

#[test]
#[ignore = "needs docker"]
fn has_the_network_when_asked() {
    let sandbox = create("network-on", true);
    let mode = docker(&[
        "inspect",
        "-f",
        "{{.HostConfig.NetworkMode}}",
        sandbox.name(),
    ]);
    assert!(!mode.trim().is_empty(), "the container was not inspected");
    assert_ne!(mode.trim(), "none");
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn does_not_report_a_timeout_for_a_command_that_finished() {
    let sandbox = create("finished", false);
    let result = sandbox
        .run("true", "", 30 * SECOND, &no_env())
        .expect("the command runs");
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert!(!result.timed_out);
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}

#[test]
#[ignore = "needs docker"]
fn runs_in_a_subdirectory_of_the_workspace() {
    let root = worktree("subdirectory");
    std::fs::create_dir(root.join("sub")).expect("sub can be made");
    let sandbox = create_at("subdirectory", &root, false);
    let result = sandbox
        .run("pwd", "sub", 30 * SECOND, &no_env())
        .expect("the command runs");
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "/workspace/sub\n");
    Box::new(sandbox)
        .discard()
        .expect("the container is removed");
}
