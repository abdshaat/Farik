//! The Docker sandbox against a real Docker daemon, on `alpine:3.22`. Ignored by default; run by
//! `cargo xtask check --integration`, which CI runs after pulling the image.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use catervas_core::contract::TaskId;
use catervas_runtime::{
    DockerSandbox, DockerSandboxFactory, ExecError, Executor, Sandbox, SandboxError, SandboxFactory,
};

const IMAGE: &str = "alpine:3.22";
const SECOND: Duration = Duration::from_secs(1);

fn project(test: &str) -> String {
    format!("catervas-test-{}-{test}", std::process::id())
}

fn worktree(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("catervas-docker-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a temporary directory can be made");
    root
}

fn task() -> TaskId {
    TaskId::try_from("CTV-1").expect("an id")
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
    let image = "catervas/no-such-image:0";
    let refused =
        DockerSandbox::create(&project("image"), &task(), &worktree("image"), false, image);
    assert_eq!(
        refused.err(),
        Some(SandboxError::ImageMissing {
            image: image.to_owned()
        })
    );
    let name = format!("catervas-{}-ctv-1", project("image"));
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
    let expected = format!("catervas-my-project-{}-ctv-1", std::process::id());
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
fn makes_a_base_container_beside_the_task_s_own_with_no_network() {
    let factory = DockerSandboxFactory {
        image: IMAGE.to_owned(),
    };
    let project = project("base");
    let own = factory
        .create(&project, &task(), &worktree("base-own"), true, &[], &[])
        .unwrap_or_else(|error| panic!("the task's sandbox could not be made: {error}"));
    let base = factory
        .create_base(&project, &task(), &worktree("base"))
        .unwrap_or_else(|error| panic!("the base sandbox could not be made: {error}"));
    // `project` is already in Docker's alphabet, so the names are spelled out rather than derived.
    let own_name = format!("catervas-{project}-ctv-1");
    let base_name = format!("{own_name}-base");
    assert_eq!(containers_named(&own_name), 1);
    assert_eq!(containers_named(&base_name), 1, "both run side by side");
    let mode = docker(&["inspect", "-f", "{{.HostConfig.NetworkMode}}", &base_name]);
    assert_eq!(mode.trim(), "none");
    base.discard().expect("the base container is removed");
    assert_eq!(containers_named(&base_name), 0);
    own.discard().expect("the task's container is removed");
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

#[test]
#[ignore = "needs docker"]
fn removes_a_container_by_name() {
    let factory = DockerSandboxFactory {
        image: IMAGE.to_owned(),
    };
    let project = project("remove");
    let _own = factory
        .create(&project, &task(), &worktree("remove-own"), false, &[], &[])
        .unwrap_or_else(|error| panic!("the task's sandbox could not be made: {error}"));
    let _base = factory
        .create_base(&project, &task(), &worktree("remove-base"))
        .unwrap_or_else(|error| panic!("the base sandbox could not be made: {error}"));
    let own_name = format!("catervas-{project}-ctv-1");
    let base_name = format!("{own_name}-base");
    assert_eq!(containers_named(&own_name), 1);
    assert_eq!(containers_named(&base_name), 1);

    factory
        .remove(&project, &task())
        .expect("both containers are removed");

    assert_eq!(containers_named(&own_name), 0);
    assert_eq!(containers_named(&base_name), 0);
    factory
        .remove(&project, &task())
        .expect("removing what is gone is no error");
}

/// An image that has git: Catervas's own sandbox image when this machine has it, else `alpine`
/// with git added, built once and kept by Docker's cache.
fn image_with_git() -> String {
    let sandbox = catervas_runtime::SANDBOX_IMAGE;
    if Command::new("docker")
        .args(["image", "inspect", sandbox])
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return sandbox.to_owned();
    }
    let tag = "catervas-test-alpine-git:1";
    let mut build = Command::new("docker")
        .args(["build", "-t", tag, "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("docker build runs");
    std::io::Write::write_all(
        build.stdin.as_mut().expect("a pipe"),
        b"FROM alpine:3.22\nRUN apk add --no-cache git\n",
    )
    .expect("the Dockerfile is sent");
    assert!(build.wait().expect("the build ends").success());
    tag.to_owned()
}

#[test]
#[ignore = "needs docker and the git program"]
fn a_check_reads_git_and_an_agent_does_not() {
    use catervas_store::git::fixtures::TempRepo;

    let repository = TempRepo::new("docker-git-store");
    let git = repository.adapter();
    let worktree = repository.path.join(".catervas/local/worktrees/CTV-1");
    git.create_worktree(&worktree, "catervas/CTV-1", "main")
        .expect("the worktree is made");
    let store = git.common_dir(&worktree).expect("the store is found");
    let head = |directory: &std::path::Path| {
        String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(directory)
                .output()
                .expect("git runs")
                .stdout,
        )
        .into_owned()
    };
    let before = head(&repository.path);
    let factory = DockerSandboxFactory {
        image: image_with_git(),
    };
    let project = project("git-store");

    let checking = factory
        .create_checking(&project, &task(), &worktree, false, &[], &[], &store)
        .unwrap_or_else(|error| panic!("the checks' sandbox could not be made: {error}"));
    let run = |sandbox: &dyn Sandbox, command: &str| {
        sandbox
            .run(command, ".", 30 * SECOND, &no_env())
            .expect("the command runs")
    };
    assert_eq!(
        run(checking.as_ref(), "git ls-files --error-unmatch README.md").exit_code,
        0
    );
    assert_eq!(
        run(checking.as_ref(), "git check-ignore -q README.md").exit_code,
        1
    );
    let commit = run(
        checking.as_ref(),
        "git -c user.name=x -c user.email=x@x commit --allow-empty -m x",
    );
    assert_ne!(commit.exit_code, 0, "{}", commit.stdout);
    assert_eq!(head(&repository.path), before, "the store is unchanged");
    checking.discard().expect("the container is removed");

    let agent = factory
        .create(&project, &task(), &worktree, false, &[], &[])
        .unwrap_or_else(|error| panic!("the agent's sandbox could not be made: {error}"));
    let status = run(agent.as_ref(), "git status");
    assert_eq!(status.exit_code, 128, "{}", status.stderr);
    assert!(
        status.stderr.contains("not a git repository"),
        "{}",
        status.stderr
    );
    agent.discard().expect("the container is removed");
}
