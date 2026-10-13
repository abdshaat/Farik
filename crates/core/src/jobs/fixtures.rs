//! Recorded and hand-written inputs of the jobs readers, for tests in this crate and in others.
//! Secrets are spelled `SECRET-<where>` and prompts `NEVER-KEPT-TEXT`.

/// `.claude/scheduled_tasks.json`: a recurring, a one-time and a permanent task, and a task with
/// nothing but an id.
pub const SCHEDULED_TASKS: &str = include_str!("fixtures/scheduled_tasks.json");

/// A background job's `state.json` (an undocumented file, read fail-soft).
pub const BACKGROUND_JOB: &str = include_str!("fixtures/background_job_state.json");

/// A workflow named `nightly-sync` with two schedules and a manual trigger.
pub const GITHUB_WORKFLOW: &str = include_str!("fixtures/github_workflow.yml");

/// A workflow that runs on `push` alone.
pub const GITHUB_WORKFLOW_UNSCHEDULED: &str =
    include_str!("fixtures/github_workflow_unscheduled.yml");

/// A `vercel.json` with three crons, the last with a key in its query.
pub const VERCEL_JSON: &str = include_str!("fixtures/vercel.json");

/// A Deployment, a `CronJob` `report`, and a `CronJob` with no schedule.
pub const KUBERNETES_MANIFEST: &str = include_str!("fixtures/kubernetes_manifest.yaml");

/// A `render.yaml` with a web service and a cron `daily-report`.
pub const RENDER_YAML: &str = include_str!("fixtures/render.yaml");

/// A Terraform file with two literal schedules, a variable one and a commented one.
pub const TERRAFORM_MAIN_TF: &str = include_str!("fixtures/main.tf");
