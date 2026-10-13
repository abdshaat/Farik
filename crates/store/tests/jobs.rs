//! Finding the schedules committed in the owner's repositories, on real repositories.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use catervas_core::jobs::fixtures::{
    GITHUB_WORKFLOW, GITHUB_WORKFLOW_UNSCHEDULED, KUBERNETES_MANIFEST, RENDER_YAML,
    TERRAFORM_MAIN_TF, VERCEL_JSON,
};
use catervas_store::files::fixtures::TempProject;
use catervas_store::git::fixtures::git_in;
use catervas_store::jobs::{JobPlaces, JobsSummary, ProgramAnswer, find_jobs};
use catervas_store::machine::{JobRow, MACHINE_DB, MachineIndex, open_machine_index};
use chrono::{DateTime, TimeZone, Utc};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
        .single()
        .expect("a valid time")
}

struct World {
    temp: TempProject,
    index: MachineIndex,
}

impl World {
    fn new(name: &str) -> Self {
        let temp = TempProject::new(name);
        std::fs::create_dir_all(temp.root.join("home")).expect("a home");
        let index =
            open_machine_index(&temp.root.join("state").join(MACHINE_DB), now()).expect("index");
        Self { temp, index }
    }

    fn home(&self) -> PathBuf {
        self.temp.root.join("home").canonicalize().expect("a home")
    }

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(path, text).expect("a file");
    }

    /// A repository with these files committed, recorded as the owner's root.
    fn repository(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = self.home().join(name);
        std::fs::create_dir_all(&root).expect("a root");
        git_in(&root, &["init", "-q"]);
        for (relative, text) in files {
            Self::write(&root, relative, text);
        }
        git_in(&root, &["add", "-A"]);
        git_in(
            &root,
            &[
                "-c",
                "user.name=me",
                "-c",
                "user.email=me@example.com",
                "commit",
                "-q",
                "-m",
                "files",
            ],
        );
        let connection = rusqlite::Connection::open(self.temp.root.join("state").join(MACHINE_DB))
            .expect("the index file");
        connection
            .execute(
                "INSERT INTO projects (root, is_git, ownership, present, first_seen_at, checked_at)
                 VALUES (?1, 1, 'yours', 1, '2026-10-12T09:00:00.000Z', '2026-10-12T09:00:00.000Z')",
                (root.to_string_lossy(),),
            )
            .expect("a root");
        root
    }

    fn scan(&mut self) -> JobsSummary {
        let places = JobPlaces {
            home: Some(self.home()),
            claude_dir: self.home().join(".claude"),
        };
        let answers: HashMap<String, String> = HashMap::new();
        let run = |program: &str, args: &[&str]| {
            answers
                .get(&format!("{program} {}", args.join(" ")))
                .map(|stdout| ProgramAnswer {
                    succeeded: true,
                    stdout: stdout.clone(),
                })
        };
        find_jobs(&mut self.index, &places, &run, now()).expect("the scan runs")
    }

    fn jobs(&self) -> Vec<JobRow> {
        self.index.found_jobs(now()).expect("jobs")
    }

    fn database(&self) -> Vec<u8> {
        std::fs::read(self.temp.root.join("state").join(MACHINE_DB)).expect("the index file")
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn finds_the_schedules_in_a_repository() {
    let mut world = World::new("finds-repository-schedules");
    let helm = "kind: CronJob\nspec:\n  schedule: {{ .Values.schedule }}\n";
    let shop = world.repository(
        "shop",
        &[
            (".gitignore", "node_modules/\n"),
            (".github/workflows/nightly.yml", GITHUB_WORKFLOW),
            (".github/workflows/ci.yml", GITHUB_WORKFLOW_UNSCHEDULED),
            ("vercel.json", VERCEL_JSON),
            ("deploy/k8s.yaml", KUBERNETES_MANIFEST),
            ("deploy/chart/templates/cron.yaml", helm),
            ("render.yaml", RENDER_YAML),
            ("infra/main.tf", TERRAFORM_MAIN_TF),
            ("node_modules/x/vercel.json", VERCEL_JSON),
        ],
    );
    // `node_modules/` is ignored and `extra/` is never committed.
    World::write(&shop, "extra/vercel.json", VERCEL_JSON);
    let summary = world.scan();
    assert_eq!((summary.repositories, summary.unreadable), (1, 1));
    let root = shop.to_string_lossy().into_owned();
    let mut found: Vec<(String, String, String)> = world
        .jobs()
        .into_iter()
        .map(|job| {
            assert_eq!(job.origin, root);
            assert_eq!(job.project_root.as_deref(), Some(root.as_str()));
            (job.kind.as_wire().to_string(), job.display_name, job.file)
        })
        .collect();
    found.sort();
    let row = |kind: &str, name: &str, file: &str| (kind.into(), name.into(), file.into());
    let mut expected = vec![
        row(
            "github_actions",
            "nightly-sync",
            ".github/workflows/nightly.yml",
        ),
        row("vercel", "/api/cron/digest", "vercel.json"),
        row("vercel", "/api/cron/cleanup", "vercel.json"),
        row("vercel", "/api/cron/x", "vercel.json"),
        row("kubernetes", "report", "deploy/k8s.yaml"),
        row("render", "daily-report", "render.yaml"),
        row(
            "terraform",
            "aws_cloudwatch_event_rule.nightly",
            "infra/main.tf",
        ),
        row(
            "terraform",
            "aws_scheduler_schedule.hourly",
            "infra/main.tf",
        ),
    ];
    expected.sort();
    assert_eq!(found, expected);
    let bytes = world.database();
    assert!(!bytes.windows(14).any(|window| window == b"SECRET-VERCEL"));
}

#[cfg(unix)]
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_tracked_link_out_of_the_repository() {
    let mut world = World::new("refuses-tracked-link");
    let outside = world.home().join("outside.json");
    std::fs::write(
        &outside,
        r#"{"crons":[{"path":"/SECRET-LINK","schedule":"* * * * *"}]}"#,
    )
    .expect("an outside file");
    let shop = world.home().join("shop");
    std::fs::create_dir_all(&shop).expect("a root");
    std::os::unix::fs::symlink(&outside, shop.join("vercel.json")).expect("a link");
    world.repository("shop", &[]);
    world.scan();
    assert!(world.jobs().is_empty());
    let bytes = world.database();
    assert!(!bytes.windows(11).any(|window| window == b"SECRET-LINK"));
}
